//! `hummingbird-authority`: the pure handler logic of the owned authority
//! (ADR-0008) — routing, parsing, validation, CAS writes and version-gated
//! reads — over the [`Sql`] seam. No runtime, no bindings: the `workers-rs`
//! shim (`hummingbird-authority-worker`) supplies a [`Sql`] over the Durable
//! Object's SQLite and forwards requests; tests supply rusqlite. The same
//! discipline as `client/core`'s transport seam, in sync form (the DO is
//! single-threaded and its SQLite API synchronous).

mod codec;
mod delivery;
pub mod diagnostics;
mod entropy;
mod fcm;
mod google_calendar;
mod google_oauth;
mod handlers;
mod schema;
mod skills;
mod sql;
mod sweep;

pub use delivery::{deliver, DeliveryOutcome, PushNotification, SuppressReason};
pub use entropy::Entropy;
pub use fcm::{
    assemble_assertion, assertion_signing_input, classify_response, message_json,
    pkcs8_der_from_pem, revoke_dead_target, send_url, token_request_body, FcmConfigError,
    SendVerdict, ServiceAccount, FCM_SCOPE,
};
pub use google_calendar::{
    calendar_invalid_grant, calendar_refresh_grant_body, calendar_secrets_unset,
    calendar_token_success, calendar_unreachable, calendar_upstream_status,
    calendar_write_invalid_grant, calendar_write_refresh_grant_body, calendar_write_secrets_unset,
    is_invalid_grant, token_is_fresh,
};
pub use google_oauth::{parse_access_token, AccessToken, OAUTH_TOKEN_URL};
pub use handlers::{handle, ApiRequest, ApiResponse, HandleContext};
pub use schema::{init_schema, SCHEMA_VERSION};
pub use skills::{
    credential_rejected, forwardable, run_url, unconfigured, unreachable, upstream_status,
    ProxyFailure,
};
pub use sql::{Row, Sql, SqlError, SqlValue};
pub use sweep::{tick as sweep_tick, TickMatch, ALARM_INTERVAL_MS};

#[cfg(test)]
mod tests {
    /// The pure crate must stay natively testable forever: no bindings, no
    /// runtime (the `client/core/src/lib.rs` guard, extended server-side).
    /// rusqlite is the sole exception, dev-only.
    #[test]
    fn cargo_toml_has_no_binding_or_runtime_dependencies() {
        let manifest = include_str!("../Cargo.toml");
        let dependencies = manifest
            .split("[dev-dependencies]")
            .next()
            .expect("split always yields one piece");
        for forbidden in ["uniffi", "wasm-bindgen", "wasm_bindgen", "js-sys", "\nworker"] {
            assert!(
                !dependencies.contains(forbidden),
                "server/authority/Cargo.toml must not depend on `{forbidden}` — \
                 handler logic is runtime-agnostic and natively testable",
            );
        }
    }

    /// #711 review round 1, finding 2: `hummingbird-authority-worker`'s
    /// `fetch` used to have a bare `.await?` after logging
    /// `request.received` (`ensure_alarm_scheduled(...).await?`) — on
    /// failure, that propagated the error straight out of `fetch` with
    /// `request.finished` never logged, manufacturing the exact
    /// "server received, never finished" false-stall signature #712's
    /// interpretation table would misread as an authority stall. Fixed by
    /// replacing every bare `.await?` after the received log with an
    /// explicit `match`/`if let Err` that logs `request.finished` before
    /// returning the error.
    ///
    /// `server/worker` has no test harness of its own (CLAUDE.md's
    /// thin-shim rule), so this is a source-text scan from the one crate
    /// that *is* natively tested — the same technique
    /// `cargo_toml_has_no_binding_or_runtime_dependencies` above already
    /// uses on a sibling file, just aimed at `../worker/src/lib.rs`
    /// instead of this crate's own `Cargo.toml`. It scans only the body of
    /// `fetch` (from its own opening brace to `alarm`'s, the next method
    /// in the same `impl` block) and only the part of that body *after*
    /// the `request.received` log — a bare `.await?` before that point is
    /// fine, since no `request.received` line exists yet to leave orphaned.
    ///
    /// **Exactly what is asserted:** that the substring `.await?` does not
    /// occur at all in that slice, once whole-line comments are dropped
    /// from it. Nothing narrower — review round 2 broke an earlier version
    /// of this test that matched only `.await?;` and `.await?\n`, by
    /// reintroducing the bug in the fully idiomatic chained form
    /// `req.text().await?.trim().to_string()`, which that version let
    /// through. The blunt substring is the point: any suffix (`.await?.`,
    /// `.await?)`, `.await?,`) is caught. Comment *lines* are filtered
    /// because both guarded call sites carry prose naming the bare
    /// `.await?` they replaced; a trailing comment on a line of code is
    /// not filtered, so the filter hides no code from the scan.
    ///
    /// **What it still does not catch,** stated rather than implied: a
    /// plain `?` on a *synchronous* `Result` introduced below the log
    /// point, and a panic. Neither is banned here because neither is
    /// currently expressible in `fetch` — every fallible step after the log
    /// is `async` — but a future edit that adds a synchronous fallible call
    /// there would slip past this scan. Panics are out of scope for the
    /// whole shim (nothing `catch_unwind`s in it, pre-existing).
    ///
    /// **Mutation-tested:** reverting either fixed call site
    /// (`ensure_alarm_scheduled` or `req.text()`) back to a bare
    /// `.await?;` reproduces this test's failure, and so does review round
    /// 2's chained `req.text().await?.trim().to_string()`. Each reverted
    /// from a file copy before landing.
    #[test]
    fn no_bare_await_question_mark_follows_the_shims_request_received_log() {
        let source = include_str!("../../worker/src/lib.rs");
        let fetch_marker = "async fn fetch(&self, mut req: Request) -> Result<Response> {";
        let fetch_start = source
            .find(fetch_marker)
            .expect("the shim's fetch method exists at this signature");
        let after_fetch_start = &source[fetch_start..];
        let alarm_marker = "async fn alarm(&self) -> Result<Response> {";
        let fetch_body_end = after_fetch_start
            .find(alarm_marker)
            .expect("alarm is the next method in the same impl block, after fetch");
        let fetch_body = &after_fetch_start[..fetch_body_end];

        let received_marker = "self.log_received(";
        let received_at = fetch_body
            .find(received_marker)
            .expect("fetch logs request.received via self.log_received");
        // Whole-line comments are dropped before the scan: the fix at both
        // guarded call sites *describes* the bare `.await?` it replaced, in
        // prose, so an unfiltered substring scan would flag its own
        // explanation. Only lines that are nothing but a comment go — a
        // trailing comment on a line of code stays, so no code is hidden
        // from the scan by this.
        let after_received: String = fetch_body[received_at..]
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");

        assert!(
            !after_received.contains(".await?"),
            "a bare `.await?` after the `request.received` log can propagate an \
             error without ever emitting `request.finished` — the exact false \
             \"received, never finished\" stall signature #711's review round 1 \
             found (ensure_alarm_scheduled's own `.await?`). Wrap it in an \
             explicit match/if-let that logs `request.finished` (self.log_finished) \
             before returning the error instead.",
        );
    }

    /// The scan above is only as good as the slice it reads — pin that it
    /// really found `fetch`'s own body (not the whole file, not an empty
    /// string) so a future edit that renames `fetch`/`alarm` or reorders
    /// them cannot silently reduce the test above to scanning nothing.
    #[test]
    fn the_scanned_fetch_body_contains_both_await_points_it_is_meant_to_guard() {
        let source = include_str!("../../worker/src/lib.rs");
        let fetch_start = source
            .find("async fn fetch(&self, mut req: Request) -> Result<Response> {")
            .unwrap();
        let after_fetch_start = &source[fetch_start..];
        let fetch_body_end = after_fetch_start
            .find("async fn alarm(&self) -> Result<Response> {")
            .unwrap();
        let fetch_body = &after_fetch_start[..fetch_body_end];
        assert!(fetch_body.contains("ensure_alarm_scheduled"));
        assert!(fetch_body.contains("req.text()"));
        assert!(fetch_body.contains("self.log_received("));
        assert!(fetch_body.contains("self.log_finished("));
    }

    /// The serde strings of the domain enums are byte-for-byte the DDL
    /// CHECK literals — the equivalence every write and read leans on.
    #[test]
    fn enum_strings_appear_in_the_ddl() {
        use crate::schema::{CREATE_GRILLS, CREATE_ITEMS, CREATE_PUSH_TARGETS, CREATE_RULES, CREATE_TOKENS};
        use hummingbird_domain::{Energy, GrillVerdict, Platform, Scope, Size, Stage, Tier};

        for stage in Stage::ALL {
            assert!(
                CREATE_ITEMS.contains(&format!("'{}'", stage.as_str())),
                "stage `{}` missing from the items DDL CHECK",
                stage.as_str(),
            );
            assert!(
                CREATE_GRILLS.contains(&format!("'{}'", stage.as_str())),
                "stage `{}` missing from the grills.resulting_stage DDL CHECK",
                stage.as_str(),
            );
        }
        for verdict in [GrillVerdict::Resolved, GrillVerdict::FogRemains] {
            let wire = serde_json::to_string(&verdict).unwrap();
            let wire = wire.trim_matches('"');
            assert!(
                CREATE_GRILLS.contains(&format!("'{wire}'")),
                "verdict `{wire}` missing from the grills DDL CHECK",
            );
        }
        for size in Size::ALL {
            assert!(CREATE_ITEMS.contains(&format!("'{}'", size.as_str())));
        }
        for energy in Energy::ALL {
            assert!(CREATE_ITEMS.contains(&format!("'{}'", energy.as_str())));
        }
        for scope in Scope::ALL {
            assert!(
                CREATE_TOKENS.contains(&format!("'{}'", scope.as_str())),
                "scope `{}` missing from the tokens DDL CHECK",
                scope.as_str(),
            );
        }
        for tier in Tier::ALL {
            assert!(
                CREATE_RULES.contains(&format!("'{}'", tier.as_str())),
                "tier `{}` missing from the rules DDL CHECK",
                tier.as_str(),
            );
        }
        for platform in Platform::ALL {
            assert!(
                CREATE_PUSH_TARGETS.contains(&format!("'{}'", platform.as_str())),
                "platform `{}` missing from the push_targets DDL CHECK",
                platform.as_str(),
            );
        }
        // event_kind is deliberately absent from this test: ADR-0013 drops
        // its CHECK, and asserting a closed set here would reintroduce the
        // frozen registry the amendment removed.
    }
}
