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
mod calendar;
#[cfg(target_arch = "wasm32")]
mod fcm;
#[cfg(target_arch = "wasm32")]
mod http;

#[cfg(target_arch = "wasm32")]
mod shim {
    use std::cell::Cell;
    use std::collections::HashMap;
    use std::rc::Rc;

    use hummingbird_authority::diagnostics::{
        accept_cycle_id, accept_request_id, classify_auth_result, route_template,
        RequestFinished, RequestReceived,
    };
    use hummingbird_authority::{
        calendar_secrets_unset, calendar_write_secrets_unset, credential_rejected, forwardable,
        handle, init_schema, revoke_dead_target, run_url, unconfigured, unreachable,
        upstream_status, ApiRequest, DeliveryOutcome, Entropy, HandleContext, ProxyFailure, Row,
        SendVerdict, Sql, SqlError, SqlValue,
    };
    use hummingbird_domain::ApiError;
    use wasm_bindgen::JsValue;
    use worker::*;

    use crate::calendar::CalendarMinter;
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
        /// absent means no push can send, and `alarm()`/`fetch()` log each
        /// transition they had to drop. `Rc`-wrapped (#255) so `fetch()`'s
        /// webhook-delivery hook can clone an owned handle into the
        /// `'static` future `State::wait_until` requires — the alarm's own
        /// `alarm()` handler stays fully `.await`ed and never needs the
        /// clone, since nothing there returns before sending completes.
        fcm: Option<Rc<FcmSender>>,
        /// The calendar-token mint (#577/#582), over the three
        /// `GOOGLE_CALENDAR_*` secrets; absent means the lane fails closed
        /// with a 503. Holds its own cache, so no `Rc` is needed here —
        /// unlike `fcm`, nothing calling into it ever needs to outlive this
        /// `fetch()` inside a `wait_until` future.
        calendar: Option<CalendarMinter>,
        /// The write-scoped calendar mint (ADR-0031), over the three
        /// `GOOGLE_CALENDAR_WRITE_*` secrets; absent means that lane fails
        /// closed with a 503 while the readonly one above goes on working.
        /// A minter of its own, so its cached bearer can never be handed to
        /// a caller of the readonly route.
        calendar_write: Option<CalendarMinter>,
    }

    impl DurableObject for Authority {
        fn new(state: State, env: Env) -> Self {
            Authority {
                state,
                schema_ready: Cell::new(false),
                admin_secret: env.secret("ADMIN_SECRET").map(|s| s.to_string()).ok(),
                fcm: FcmSender::from_env(&env).map(Rc::new),
                calendar: CalendarMinter::from_env(&env),
                calendar_write: CalendarMinter::write_from_env(&env),
            }
        }

        async fn fetch(&self, mut req: Request) -> Result<Response> {
            // #711: correlation id resolution and the `request.received`
            // log happen before any of this function's awaited work
            // (schema init, alarm scheduling, reading the body, `handle()`
            // itself) — the same "write the start event before the awaited
            // operation" shape the client's own `DiagnosticsContext` uses
            // (`client/core/src/diagnostics/context.rs`'s `http.started`),
            // so an incomplete span survives a hang anywhere below this
            // point. `request_id` is resolved exactly once, here, using
            // this instance's entropy: it is never re-derived with fresh
            // entropy further down (`ApiRequest::request_id` below is
            // fed this already-resolved value, and `handle()`'s own
            // `accept_request_id` call short-circuits on an already-valid
            // string without drawing entropy again), so the `received`
            // line, the echoed response header and the `finished` line all
            // name the exact same request.
            let start_ms = Date::now().as_millis() as i64;
            let method = req.method().to_string().to_uppercase();
            let url = req.url()?;
            let path = url.path().to_string();
            let authorization = req.headers().get("authorization")?;
            let cycle_id_header = req.headers().get("x-hummingbird-cycle-id")?;
            let request_id_header = req.headers().get("x-hummingbird-request-id")?;
            let route = route_template(&path);
            let cycle_id = accept_cycle_id(cycle_id_header.as_deref());
            let request_id = accept_request_id(request_id_header.as_deref(), &WorkersEntropy);
            console_log!(
                "{}",
                serde_json::to_string(&RequestReceived::new(
                    cycle_id.clone(),
                    request_id.clone(),
                    route.clone(),
                    method.clone(),
                ))
                .expect("RequestReceived serializes")
            );

            let sql = WorkersSql {
                sql: self.state.storage().sql(),
            };
            // Idempotent, and in fetch rather than the constructor so a
            // failure surfaces as a clean 500 instead of a poisoned object.
            if !self.schema_ready.get() {
                if let Err(e) = init_schema(&sql, Date::now().as_millis() as i64) {
                    let body = serde_json::to_string(&ApiError {
                        error: "internal".to_string(),
                        message: e.message,
                    })
                    .expect("ApiError serializes");
                    log_request_finished(
                        cycle_id, request_id.clone(), route, method, &path, 500, start_ms,
                        body.len(), None,
                    );
                    return with_request_id_header(json_response(500, body), &request_id);
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

            let body = req.text().await?;
            let now_ms = Date::now().as_millis() as i64;
            let mut api = handle(
                &ApiRequest {
                    method: &method,
                    path: &path,
                    query: url.query(),
                    body: if body.is_empty() { None } else { Some(&body) },
                    authorization: authorization.as_deref(),
                    cycle_id: cycle_id_header.as_deref(),
                    request_id: Some(&request_id),
                },
                &HandleContext {
                    now_ms,
                    admin_secret: self.admin_secret.as_deref(),
                    entropy: &WorkersEntropy,
                },
                &sql,
            );

            // #255: `POST /api/alerts`'s inline evaluate-then-deliver hook
            // may have logged deliveries above — the sync-decide half,
            // exactly like `sweep_tick`'s. The actual FCM sends are async
            // and must not hold this response hostage (the whole value of
            // a pushed source is its latency), so they run in the
            // background via `State::wait_until` instead of being awaited
            // here. `sql` is moved into that future rather than cloned —
            // nothing below this point needs it again.
            let deliveries = std::mem::take(&mut api.deliveries);
            if !deliveries.is_empty() {
                let fcm = self.fcm.clone();
                self.state.wait_until(async move {
                    send_logged_deliveries(fcm.as_deref(), &sql, now_ms, deliveries).await;
                });
            }

            // The calendar-token mint's authorization verdict (#577/#582):
            // `handle()`'s 204 means "device-scoped, proceed" — the actual
            // exchange (or cache hit) is async, so it happens here rather
            // than inside `handle()`. Unlike the skill-runner proxy this
            // needs no second subrequest to reach the DO: this *is* the DO,
            // already inside its own `fetch()`, and there is no cycle
            // forcing the exchange above it.
            if path == "/api/google/calendar_token" && api.status == 204 {
                let (status, body) = match &self.calendar {
                    Some(minter) => minter.token_response(now_ms).await,
                    None => calendar_secrets_unset(),
                };
                log_request_finished(
                    cycle_id, request_id.clone(), route, method, &path, status, start_ms,
                    body.len(), api.principal_id.clone(),
                );
                return with_request_id_header(calendar_json_response(status, body), &request_id);
            }

            // The write mint (ADR-0031), the same shape over the write
            // credential. `handle()`'s 204 here means more than "device
            // scope": it means the calling token's *id* is on the
            // allowed-holder list, which is the whole gate — a device token
            // that is not the agent's was already answered 403 above, and
            // never reaches this minter.
            if path == "/api/google/calendar_write_token" && api.status == 204 {
                let (status, body) = match &self.calendar_write {
                    Some(minter) => minter.token_response(now_ms).await,
                    None => calendar_write_secrets_unset(),
                };
                log_request_finished(
                    cycle_id, request_id.clone(), route, method, &path, status, start_ms,
                    body.len(), api.principal_id.clone(),
                );
                return with_request_id_header(calendar_json_response(status, body), &request_id);
            }

            log_request_finished(
                cycle_id, request_id.clone(), route, method, &path, api.status, start_ms,
                api.body.len(), api.principal_id.clone(),
            );
            with_request_id_header(json_response(api.status, api.body), &request_id)
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
                init_schema(&sql, Date::now().as_millis() as i64)
                    .map_err(|e| Error::RustError(e.message))?;
                self.schema_ready.set(true);
            }

            let now_ms = Date::now().as_millis() as i64;
            let tick_result = hummingbird_authority::sweep_tick(&sql, now_ms);

            self.state
                .storage()
                .set_alarm(hummingbird_authority::ALARM_INTERVAL_MS)
                .await?;

            let matches = tick_result.map_err(|e| Error::RustError(e.message))?;
            let outcomes = matches.into_iter().map(|tick_match| tick_match.outcome).collect();
            // Awaited directly, unlike `fetch()`'s own call below: `alarm()`
            // has no response to hold hostage — it already runs to
            // completion before the runtime considers the alarm handled.
            send_logged_deliveries(self.fcm.as_deref(), &sql, now_ms, outcomes).await;
            Response::empty()
        }
    }

    /// Attempts the FCM send for every [`DeliveryOutcome::Logged`] among
    /// `outcomes`, sequentially — shared by `alarm()` (awaited inline, no
    /// response to hold hostage) and `fetch()`'s `POST /api/alerts` hook
    /// (#255, awaited inside a `State::wait_until` future instead, so the
    /// send never delays the ingest response). Neither caller re-derives
    /// this loop; `deliver` already decided what to send and to whom —
    /// this only drives the send itself and reacts to what FCM says.
    async fn send_logged_deliveries(
        fcm: Option<&FcmSender>,
        sql: &dyn Sql,
        now_ms: i64,
        outcomes: Vec<DeliveryOutcome>,
    ) {
        for outcome in outcomes {
            let DeliveryOutcome::Logged { delivery_id, targets, notification } = outcome else {
                continue;
            };

            let Some(sender) = fcm else {
                // The credential is missing, and the claim row is already
                // committed — this transition will never ring. Loud, and
                // per delivery, because it is silent data loss on the
                // operator's highest-trust channel.
                console_error!(
                    "delivery {delivery_id} (alert {}) is logged but unsendable: \
                     FCM_SERVICE_ACCOUNT is not configured",
                    notification.alert_id,
                );
                continue;
            };

            for target in targets {
                // Sequential rather than concurrent: a caller sends to a
                // handful of the operator's own devices, and one shared
                // access token minted on the first send is reused by the
                // rest.
                match sender.send(&notification, &target, now_ms).await {
                    SendVerdict::Delivered => {}
                    SendVerdict::TokenDead => {
                        console_warn!(
                            "push target {} ({}) is UNREGISTERED with FCM; revoking",
                            target.id,
                            target.name,
                        );
                        // A revoke that itself fails must not abandon the
                        // remaining targets: the next dead-token response
                        // revokes it again (the write is idempotent), and
                        // the alternative is a live device never hearing
                        // this alert.
                        if let Err(e) = revoke_dead_target(sql, &target.id, now_ms) {
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

    /// The calendar-token route's response shape (#577/#582): JSON, like
    /// [`json_response`], plus `cache-control: no-store` on every status —
    /// the bearer this route can carry, and the fact of whether the lane is
    /// even provisioned, are both things a shared cache must never retain.
    fn calendar_json_response(status: u16, body: String) -> Result<Response> {
        let headers = Headers::new();
        headers.set("content-type", "application/json")?;
        headers.set("cache-control", "no-store")?;
        Ok(Response::ok(body)?.with_status(status).with_headers(headers))
    }

    /// #711: the accepted (or generated) request id, echoed on every
    /// response this fetch handles — success, error, and the two calendar
    /// mints alike — so a client can bind its own span to the server's
    /// regardless of what the call ended up returning.
    fn with_request_id_header(resp: Result<Response>, request_id: &str) -> Result<Response> {
        let mut resp = resp?;
        resp.headers_mut()
            .set("X-Hummingbird-Request-Id", request_id)?;
        Ok(resp)
    }

    /// The `request.finished` line (#711) — everything decidable
    /// (`AuthResult` classification, the event's JSON shape) lives in
    /// `hummingbird_authority::diagnostics`; this only supplies the
    /// duration (measured from `start_ms`, the instant `fetch()` began)
    /// and calls `console_log!`. Takes the concrete `path` (not the
    /// already-templated `route`) only because that is what
    /// `classify_auth_result` is defined over; every literal segment it
    /// checks (`admin`) survives templating unchanged either way.
    #[allow(clippy::too_many_arguments)]
    fn log_request_finished(
        cycle_id: Option<String>,
        request_id: String,
        route: String,
        method: String,
        path: &str,
        status: u16,
        start_ms: i64,
        response_bytes: usize,
        token_id: Option<String>,
    ) {
        let duration_ms = Date::now().as_millis() as i64 - start_ms;
        let auth_result = classify_auth_result(path, status);
        console_log!(
            "{}",
            serde_json::to_string(&RequestFinished::new(
                cycle_id,
                request_id,
                route,
                method,
                status,
                duration_ms,
                response_bytes,
                token_id,
                auth_result,
            ))
            .expect("RequestFinished serializes")
        );
    }

    // ------------------------------------------- the skill-runner proxy

    /// The one route answered **above** the Durable Object (#273,
    /// ADR-0018): `POST /api/skills/run` is proxied to the cloud runner's
    /// `POST /run` and its NDJSON stream is returned to the browser
    /// unchanged.
    ///
    /// Two reasons the egress cannot happen inside the DO. The decisive one
    /// is a **cycle**: `microtask.apply` calls back into
    /// `hb.twinion.net/api/steps` with the runner's own token, so a run
    /// dispatched through the DO would await a subrequest only that same
    /// object can answer. The second is cost: every DO `fetch` runs
    /// `init_schema` + `ensure_alarm_scheduled` first, and an open request
    /// accrues duration and blocks hibernation for the length of a model
    /// call.
    ///
    /// Authorization still needs the `tokens` table, which is inside the
    /// DO. So this sends a **bodiless preflight** — a fresh minimal
    /// `Request` carrying the same URL, method and `authorization` and
    /// nothing else. Never `req.clone()`: that would buffer the body inside
    /// the DO and consume the stream this function still has to forward.
    ///
    /// Order is load-bearing: **verdict first, secrets second.** Checking
    /// the secrets first would tell an unauthenticated caller whether the
    /// lane is provisioned at all.
    ///
    /// Every user-visible word and every status below comes from
    /// `hummingbird_authority`'s `skills` module, because this file has no
    /// test harness; the only literals here are `console_error!` lines,
    /// which no client ever sees.
    async fn proxy_skill_run(mut req: Request, env: Env) -> Result<Response> {
        let stub = env
            .durable_object("AUTHORITY")?
            .id_from_name(WORKSPACE)?
            .get_stub()?;

        let verdict_headers = Headers::new();
        if let Some(authorization) = req.headers().get("authorization")? {
            verdict_headers.set("authorization", &authorization)?;
        }
        let mut verdict_init = RequestInit::new();
        verdict_init
            .with_method(Method::Post)
            .with_headers(verdict_headers);
        let verdict = stub
            .fetch_with_request(Request::new_with_init(req.url()?.as_ref(), &verdict_init)?)
            .await?;
        // 401/403/405 are already the answer, in the shape the client's
        // contract expects (empty for the first two, the pure crate's JSON
        // for the third).
        if verdict.status_code() != 204 {
            return Ok(verdict);
        }

        let (Ok(base_url), Ok(bearer)) = (
            env.secret("RUNNER_BASE_URL"),
            env.secret("RUNNER_BEARER_TOKEN"),
        ) else {
            // Names the gap in the log, never in the body.
            console_error!(
                "POST /api/skills/run: RUNNER_BASE_URL and/or RUNNER_BEARER_TOKEN is unset; \
                 the skill-runner lane fails closed",
            );
            return ndjson_failure(unconfigured());
        };

        let body = req.text().await?;
        let headers = Headers::new();
        headers.set("content-type", "application/json")?;
        headers.set("authorization", &format!("Bearer {bearer}"))?;
        let mut init = RequestInit::new();
        init.with_method(Method::Post)
            .with_headers(headers)
            .with_body(Some(JsValue::from_str(&body)));

        let runner_url = run_url(&base_url.to_string());
        let response = match Fetch::Request(Request::new_with_init(&runner_url, &init)?)
            .send()
            .await
        {
            Ok(response) => response,
            Err(e) => {
                console_error!("POST /api/skills/run: the runner subrequest failed: {e}");
                return ndjson_failure(unreachable());
            }
        };

        let status = response.status_code();
        if forwardable(status) {
            // Returned directly, which streams: a `worker::Response` holds
            // a `ResponseBody::Stream`, and the immutability trap that
            // applies to a `Request` does not apply here.
            return Ok(response);
        }
        if status == 401 {
            console_error!(
                "POST /api/skills/run: the runner rejected RUNNER_BEARER_TOKEN — it was \
                 rotated on one side only (`fly secrets set` and `wrangler secret put` \
                 are now both required)",
            );
            return ndjson_failure(credential_rejected());
        }
        console_error!("POST /api/skills/run: the runner answered {status}");
        ndjson_failure(upstream_status(status))
    }

    /// A proxy-generated failure as its one NDJSON line — the same
    /// content-type the runner's own stream carries, so the client has one
    /// parser and never branches on a body shape.
    fn ndjson_failure(failure: ProxyFailure) -> Result<Response> {
        let headers = Headers::new();
        headers.set("content-type", "application/x-ndjson")?;
        Ok(Response::ok(failure.body)?
            .with_status(failure.status)
            .with_headers(headers))
    }

    #[event(fetch)]
    async fn fetch(req: Request, env: Env, _ctx: Context) -> Result<Response> {
        if !req.path().starts_with("/api/") {
            return Response::error("not found", 404);
        }
        if req.method() == Method::Post && req.path() == "/api/skills/run" {
            return proxy_skill_run(req, env).await;
        }
        env.durable_object("AUTHORITY")?
            .id_from_name(WORKSPACE)?
            .get_stub()?
            .fetch_with_request(req)
            .await
    }
}
