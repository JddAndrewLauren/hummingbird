//! `hummingbird-authority-worker`: the thin `workers-rs` shim over
//! `hummingbird-authority` (ADR-0008) — one Worker routing `/api/*` to one
//! SQLite-backed Durable Object, which maps each request onto the pure
//! crate's `handle()` and its [`hummingbird_authority::Sql`] seam.
//!
//! Everything here is `wasm32`-only, the `client/ffi-web` pattern: the
//! runtime interop has nothing to test natively, so the native test run
//! compiles this crate empty and the `wasm32` build in CI gates it. All
//! behaviour lives in `hummingbird-authority`, fixture-tested there.

#[cfg(target_arch = "wasm32")]
mod fcm;

#[cfg(target_arch = "wasm32")]
mod shim {
    use std::cell::Cell;
    use std::collections::HashMap;

    use hummingbird_authority::{
        handle, init_schema, revoke_dead_target, ApiRequest, Entropy, HandleContext, Row, SendVerdict,
        Sql, SqlError, SqlValue,
    };
    use hummingbird_domain::ApiError;
    use worker::*;

    use crate::fcm::FcmSender;

    /// [`Entropy`] over the platform CSPRNG (`crypto.getRandomValues` via
    /// `getrandom`'s js backend) — token minting's randomness source.
    struct WorkersEntropy;

    impl Entropy for WorkersEntropy {
        fn fill(&self, buf: &mut [u8]) {
            getrandom::getrandom(buf).expect("the platform CSPRNG is available");
        }
    }

    /// [`Sql`] over the Durable Object's synchronous SQLite storage.
    struct WorkersSql {
        sql: SqlStorage,
    }

    impl Sql for WorkersSql {
        fn exec(&self, sql: &str, params: &[SqlValue]) -> Result<Vec<Row>, SqlError> {
            let bindings: Vec<SqlStorageValue> = params
                .iter()
                .map(|p| match p {
                    SqlValue::Null => SqlStorageValue::Null,
                    SqlValue::Integer(n) => SqlStorageValue::Integer(*n),
                    SqlValue::Real(f) => SqlStorageValue::Float(*f),
                    SqlValue::Text(s) => SqlStorageValue::String(s.clone()),
                })
                .collect();
            let cursor = self
                .sql
                .exec(sql, bindings)
                .map_err(|e| SqlError { message: e.to_string() })?;
            let rows: Vec<HashMap<String, serde_json::Value>> = cursor
                .to_array()
                .map_err(|e| SqlError { message: e.to_string() })?;
            Ok(rows
                .into_iter()
                .map(|row| {
                    row.into_iter()
                        .map(|(column, value)| (column, json_to_sql(value)))
                        .collect()
                })
                .collect())
        }
    }

    fn json_to_sql(value: serde_json::Value) -> SqlValue {
        match value {
            serde_json::Value::Null => SqlValue::Null,
            serde_json::Value::Bool(b) => SqlValue::Integer(b as i64),
            serde_json::Value::Number(n) => match n.as_i64() {
                Some(i) => SqlValue::Integer(i),
                None => SqlValue::Real(n.as_f64().unwrap_or(0.0)),
            },
            serde_json::Value::String(s) => SqlValue::Text(s),
            // SQLite yields only scalars; anything else would be a BLOB,
            // which no S0 column carries.
            _ => SqlValue::Null,
        }
    }

    /// The one workspace singleton (ADR-0008): every `/api/*` request lands
    /// on the instance named by [`WORKSPACE`].
    const WORKSPACE: &str = "workspace";

    #[durable_object]
    pub struct Authority {
        state: State,
        schema_ready: Cell<bool>,
        /// `ADMIN_SECRET` Worker secret; absent (e.g. unset in `wrangler
        /// dev`) means the admin routes fail closed with a 401.
        admin_secret: Option<String>,
        /// The FCM send leg (#219), over the `FCM_SERVICE_ACCOUNT` secret;
        /// absent means no push can send, and `alarm()` logs each
        /// transition it had to drop.
        fcm: Option<FcmSender>,
    }

    impl DurableObject for Authority {
        fn new(state: State, env: Env) -> Self {
            Authority {
                state,
                schema_ready: Cell::new(false),
                admin_secret: env.secret("ADMIN_SECRET").map(|s| s.to_string()).ok(),
                fcm: FcmSender::from_env(&env),
            }
        }

        async fn fetch(&self, mut req: Request) -> Result<Response> {
            let sql = WorkersSql {
                sql: self.state.storage().sql(),
            };
            // Idempotent, and in fetch rather than the constructor so a
            // failure surfaces as a clean 500 instead of a poisoned object.
            if !self.schema_ready.get() {
                if let Err(e) = init_schema(&sql) {
                    let body = serde_json::to_string(&ApiError {
                        error: "internal".to_string(),
                        message: e.message,
                    })
                    .expect("ApiError serializes");
                    return json_response(500, body);
                }
                self.schema_ready.set(true);
            }
            // The DO alarm sweep (#138): make sure the recurring tick is
            // actually scheduled. Deliberately **not** folded into the
            // `schema_ready` gate above — `get_alarm` is cheap and
            // idempotent (the normal case, `Some`, is a no-op), so this
            // runs on every request rather than only the first one per
            // instance wake. Gating it behind `schema_ready` would wedge
            // the clock silently forever: if this call ever failed on that
            // one first request, `schema_ready` would already be `true`,
            // every later request would skip the whole block, and nothing
            // would ever retry scheduling the alarm.
            ensure_alarm_scheduled(&self.state.storage()).await?;

            let method = req.method().to_string().to_uppercase();
            let url = req.url()?;
            let authorization = req.headers().get("authorization")?;
            let body = req.text().await?;
            let api = handle(
                &ApiRequest {
                    method: &method,
                    path: url.path(),
                    query: url.query(),
                    body: if body.is_empty() { None } else { Some(&body) },
                    authorization: authorization.as_deref(),
                },
                &HandleContext {
                    now_ms: Date::now().as_millis() as i64,
                    admin_secret: self.admin_secret.as_deref(),
                    entropy: &WorkersEntropy,
                },
                &sql,
            );
            json_response(api.status, api.body)
        }

        /// The DO alarm handler (#138): evaluates every item already held
        /// in the authority against every enabled rule, through
        /// [`hummingbird_authority::sweep_tick`] — the pure crate owns
        /// every decision (which items, which rules, mint-or-ratchet,
        /// dedupe); this shim only supplies the clock, drives the actual
        /// FCM send for whatever `sweep_tick` decided is `Logged`, and
        /// reschedules the next tick.
        ///
        /// **Rescheduling happens unconditionally, even if the tick itself
        /// errors** — a single failed tick must never silently stop the
        /// clock; the next tick gets another chance.
        async fn alarm(&self) -> Result<Response> {
            let sql = WorkersSql {
                sql: self.state.storage().sql(),
            };
            if !self.schema_ready.get() {
                init_schema(&sql).map_err(|e| Error::RustError(e.message))?;
                self.schema_ready.set(true);
            }

            let now_ms = Date::now().as_millis() as i64;
            let tick_result = hummingbird_authority::sweep_tick(&sql, now_ms);

            self.state
                .storage()
                .set_alarm(hummingbird_authority::ALARM_INTERVAL_MS)
                .await?;

            let matches = tick_result.map_err(|e| Error::RustError(e.message))?;
            for tick_match in matches {
                let hummingbird_authority::DeliveryOutcome::Logged {
                    delivery_id,
                    targets,
                    notification,
                } = tick_match.outcome
                else {
                    continue;
                };

                let Some(sender) = self.fcm.as_ref() else {
                    // The credential is missing, and the claim row is
                    // already committed — this transition will never ring.
                    // Loud, and per delivery, because it is silent data
                    // loss on the operator's highest-trust channel.
                    console_error!(
                        "delivery {delivery_id} (alert {}) is logged but unsendable: \
                         FCM_SERVICE_ACCOUNT is not configured",
                        notification.alert_id,
                    );
                    continue;
                };

                for target in targets {
                    // Sequential rather than concurrent: an alarm sends to
                    // a handful of the operator's own devices, and one
                    // shared access token minted on the first send is
                    // reused by the rest.
                    match sender.send(&notification, &target, now_ms).await {
                        SendVerdict::Delivered => {}
                        SendVerdict::TokenDead => {
                            console_warn!(
                                "push target {} ({}) is UNREGISTERED with FCM; revoking",
                                target.id,
                                target.name,
                            );
                            // A revoke that itself fails must not abandon
                            // the remaining targets: the next dead-token
                            // response revokes it again (the write is
                            // idempotent), and the alternative is a live
                            // device never hearing this alert.
                            if let Err(e) = revoke_dead_target(&sql, &target.id, now_ms) {
                                console_error!(
                                    "could not revoke dead push target {}: {}",
                                    target.id,
                                    e.message,
                                );
                            }
                        }
                        SendVerdict::Failed { detail } => {
                            // Never retried: `deliver` committed the claim
                            // before this send, so a retry would risk the
                            // double-ring ADR-0012 rules out.
                            console_error!(
                                "delivery {delivery_id} to push target {} failed, not retried: {detail}",
                                target.id,
                            );
                        }
                    }
                }
            }
            Response::empty()
        }
    }

    /// Schedules the first tick if this instance has none pending. A
    /// restart/eviction never loses the pending alarm itself (Cloudflare
    /// persists it), so this only ever fires real work on a truly fresh
    /// object.
    async fn ensure_alarm_scheduled(storage: &Storage) -> Result<()> {
        if storage.get_alarm().await?.is_none() {
            storage.set_alarm(hummingbird_authority::ALARM_INTERVAL_MS).await?;
        }
        Ok(())
    }

    fn json_response(status: u16, body: String) -> Result<Response> {
        // The 401/403 contract: a clean status, no body, no content-type.
        if body.is_empty() {
            return Ok(Response::empty()?.with_status(status));
        }
        let headers = Headers::new();
        headers.set("content-type", "application/json")?;
        Ok(Response::ok(body)?.with_status(status).with_headers(headers))
    }

    #[event(fetch)]
    async fn fetch(req: Request, env: Env, _ctx: Context) -> Result<Response> {
        if !req.path().starts_with("/api/") {
            return Response::error("not found", 404);
        }
        env.durable_object("AUTHORITY")?
            .id_from_name(WORKSPACE)?
            .get_stub()?
            .fetch_with_request(req)
            .await
    }
}
