//! Reading the committed `services.json` — the reviewable manifest ADR-0017
//! decision 4 makes the whole point of this slice. Changing what a service
//! is *supposed* to be doing is a one-line diff in this file, reviewed like
//! any other PR, on the same "editing the file is the whole override
//! gesture" posture CLAUDE.md documents for `VERSION`.
//!
//! **No per-service interval field, deliberately** (ADR-0017 decision 6):
//! the workflow that carries every declared service probes them hourly, as
//! one unit — the whole point of that decision is that per-service cadence
//! would let someone independently speed the runner's probe back up to the
//! wake-up-heavy interval decision 6 explicitly rejects.
//!
//! **No sweeper line.** ADR-0017 decision 4 excludes it from this axis
//! entirely: it never opens a listener, so no truthful
//! `url`/`method`/`expect_status` triple can be written for it, and neither
//! `expected` value would describe it honestly. `services.json` therefore
//! carries exactly three services — `authority`, `web`, `runner` — all
//! `expected: "on"`.

use serde::Deserialize;

/// Whether a service is *supposed* to be answering HTTP at its declared
/// `url` right now (ADR-0017 decision 4's one meaning for `expected`).
/// `"off"` is the mechanism for deliberate downtime — a service taken down
/// on purpose (the runner suspended for a rebuild is the ADR's own worked
/// example) — never a stand-in for "never HTTP-reachable" (that reading is
/// what the sweeper's exclusion refuses to let this manifest say).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Expected {
    On,
    Off,
}

/// One declared service: `id`/`url`/`method`/`expect_status`/`expected`,
/// and nothing else — no per-service interval (see this module's header).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Service {
    /// The `context_snapshots.key` this service's row is written under.
    pub id: String,
    pub url: String,
    pub method: String,
    pub expect_status: u16,
    pub expected: Expected,
}

/// The committed manifest, embedded at compile time — this poller ships
/// with its own manifest rather than reading it off disk at runtime, since
/// nothing about it is environment-specific (unlike `kimi-balance`'s host,
/// which genuinely varies per which key was minted where).
pub const SERVICES_JSON: &str = include_str!("../services.json");

/// Reads a `services.json`-shaped document into its declared services, or
/// the reason it could not — a malformed manifest must fail the poller's
/// run loudly (`main.rs` treats this as a hard stop) rather than silently
/// probing nothing.
pub fn parse_manifest(contents: &str) -> Result<Vec<Service>, String> {
    serde_json::from_str(contents).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_committed_manifest_parses() {
        let services = parse_manifest(SERVICES_JSON).expect("services.json parses");
        assert!(!services.is_empty());
    }

    /// The acceptance fact this whole slice exists to ship (ADR-0017
    /// decision 4's "the manifest #315 ships therefore carries three
    /// services"): authority, web, runner, all `expected: "on"`, and no
    /// sweeper line at all.
    #[test]
    fn the_committed_manifest_declares_exactly_the_three_corrected_services() {
        let services = parse_manifest(SERVICES_JSON).expect("services.json parses");
        let ids: Vec<&str> = services.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, vec!["authority", "web", "runner"]);
        for service in &services {
            assert_eq!(
                service.expected,
                Expected::On,
                "{} must be expected on; the manifest never claims a live service is deliberately down",
                service.id
            );
        }
        assert!(
            !ids.contains(&"sweeper"),
            "the sweeper never opens a listener — no truthful triple exists for it (ADR-0017 decision 4)"
        );
    }

    #[test]
    fn the_authority_is_probed_as_an_authenticated_route_expecting_401() {
        let services = parse_manifest(SERVICES_JSON).expect("services.json parses");
        let authority = services.iter().find(|s| s.id == "authority").expect("authority is declared");
        assert_eq!(authority.expect_status, 401, "the right refusal, not a 200 — decision 3");
        assert!(authority.url.contains("hb.twinion.net"));
    }

    #[test]
    fn the_web_origin_is_probed_expecting_a_plain_200() {
        let services = parse_manifest(SERVICES_JSON).expect("services.json parses");
        let web = services.iter().find(|s| s.id == "web").expect("web is declared");
        assert_eq!(web.expect_status, 200);
    }

    #[test]
    fn the_runner_is_probed_unauthenticated_expecting_401() {
        let services = parse_manifest(SERVICES_JSON).expect("services.json parses");
        let runner = services.iter().find(|s| s.id == "runner").expect("runner is declared");
        assert_eq!(runner.expect_status, 401, "runner/src/server.js:43 gates POST /run before dispatch");
        assert_eq!(runner.method, "POST");
    }

    #[test]
    fn no_service_carries_a_per_service_interval_field() {
        // Structural: `Service` has no such field at all, so a manifest
        // that tried to add one would simply be ignored by `serde` rather
        // than rejected — this test exists to name that fact rather than to
        // exercise a code path, on `city_waste_v1_key`'s own "the absence is
        // the point" reasoning.
        let services = parse_manifest(SERVICES_JSON).expect("services.json parses");
        assert_eq!(services.len(), 3);
    }

    /// `expected: "off"` is the deliberate-downtime mechanism this module's
    /// header describes, and nothing in the committed manifest uses it — so
    /// without this test the `Expected::Off` deserialize arm is never
    /// exercised at all, and a rename of the serde spelling would only be
    /// caught the day someone actually took a service down on purpose.
    #[test]
    fn a_service_declared_deliberately_down_parses_as_expected_off() {
        let contents = r#"[{"id":"runner","url":"https://hummingbird-runner.fly.dev/run","method":"POST","expect_status":401,"expected":"off"}]"#;
        let services = parse_manifest(contents).expect("an expected-off manifest parses");
        assert_eq!(services.len(), 1);
        assert_eq!(services[0].expected, Expected::Off);
    }

    #[test]
    fn an_unrecognised_expected_value_refuses_to_parse() {
        let bad = r#"[{"id":"x","url":"https://example.com","method":"GET","expect_status":200,"expected":"maybe"}]"#;
        assert!(parse_manifest(bad).is_err());
    }

    #[test]
    fn a_malformed_manifest_is_refused_not_defaulted() {
        assert!(parse_manifest("not json").is_err());
        assert!(parse_manifest("{}").is_err());
    }
}
