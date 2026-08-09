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
mod shim {
    use std::cell::Cell;
    use std::collections::HashMap;

    use hummingbird_authority::{handle, init_schema, ApiRequest, Row, Sql, SqlError, SqlValue};
    use worker::*;

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
    }

    impl DurableObject for Authority {
        fn new(state: State, _env: Env) -> Self {
            Authority {
                state,
                schema_ready: Cell::new(false),
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
                    return json_response(500, format!("{{\"error\":\"internal\",\"message\":{}}}", serde_json::Value::from(e.message)));
                }
                self.schema_ready.set(true);
            }

            let method = req.method().to_string().to_uppercase();
            let url = req.url()?;
            let body = req.text().await?;
            let api = handle(
                &ApiRequest {
                    method: &method,
                    path: url.path(),
                    query: url.query(),
                    body: if body.is_empty() { None } else { Some(&body) },
                },
                Date::now().as_millis() as i64,
                &sql,
            );
            json_response(api.status, api.body)
        }
    }

    fn json_response(status: u16, body: String) -> Result<Response> {
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
