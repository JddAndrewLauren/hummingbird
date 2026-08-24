//! Route templating and the `X-Hummingbird-*` correlation headers.
//!
//! [`route_template`] moved to `hummingbird_domain::diagnostics` in #711 —
//! it is re-exported here unchanged (same signature, same tests moved with
//! it) so every existing call site in this crate is unchanged; see that
//! module's docs for why (the authority's request boundary, #711, needs the
//! identical function from a different Cargo workspace, and a second
//! hand-copied implementation there is exactly the drift a shared home
//! exists to rule out). [`is_valid_header_value`] moved the same way — the
//! authority's own correlation-id validator
//! (`hummingbird_authority::diagnostics::is_valid_correlation_id`) is a
//! thin wrapper over the identical shared predicate, not a second pattern.

pub use hummingbird_domain::diagnostics::{is_valid_header_value, route_template};

/// The four correlation headers every observed sync HTTP call carries.
/// Since #711, the authority's own request boundary
/// (`server/authority/src/diagnostics.rs`, a separate Rust crate in a
/// separate Cargo workspace — it cannot call back into this module) checks
/// `X-Hummingbird-Cycle-Id`/`-Request-Id` against the identical
/// `[A-Za-z0-9_-]{1,80}` pattern [`is_valid_header_value`] enforces here
/// (now the very same function, imported from `hummingbird-domain`),
/// dropping an invalid cycle id and replacing an invalid/missing request id
/// with a server-generated one rather than trusting either header value.
/// `-Client-Platform`/`-Client-Build` are not independently validated
/// server-side — the authority never branches on their content, only logs
/// alongside the two that are — so this crate's [`sanitize_header_value`]
/// stays their only enforcement point.
/// [`sanitize_header_value`] is what
/// actually enforces that pattern on the client side, at the one place
/// (`DiagnosticsContext::correlation_headers`) every attached value passes
/// through — see that function's docs for why a bare `is_valid_header_value`
/// check with no call site would be a claim this module could not back up.
pub const HEADER_CYCLE_ID: &str = "X-Hummingbird-Cycle-Id";
pub const HEADER_REQUEST_ID: &str = "X-Hummingbird-Request-Id";
pub const HEADER_CLIENT_PLATFORM: &str = "X-Hummingbird-Client-Platform";
pub const HEADER_CLIENT_BUILD: &str = "X-Hummingbird-Client-Build";

/// The four correlation values one HTTP call needs to attach as headers.
/// `cycle_id`/`request_id` vary per call within an observed cycle;
/// `platform`/`build` are host identity, constant for the process's life —
/// bundled together anyway because every transport that attaches headers
/// needs all four at once. See [`crate::diagnostics::context::DiagnosticsContext`]
/// for where `cycle_id`/`request_id` come from.
#[derive(Debug, Clone, Copy)]
pub struct CorrelationHeaders<'a> {
    pub cycle_id: &'a str,
    pub request_id: &'a str,
    pub platform: &'a str,
    pub build: &'a str,
}

/// The one enforcement point: returns `value` unchanged when it already
/// satisfies [`is_valid_header_value`], or the fixed sentinel `"invalid"`
/// (itself a valid header value) otherwise. A caller-minted cycle id or a
/// host-supplied build string is exactly the kind of value that can carry
/// something unexpected (a stray `/`, an empty string, a crash-reporter's
/// stack trace pasted into a build field by accident) — substituting a
/// sentinel rather than dropping the header keeps all four headers present
/// on every request, which is what #711's authority-side validation can
/// rely on, at the cost of a `"invalid"` that is honestly less useful than
/// the real value would have been.
/// [`crate::diagnostics::context::DiagnosticsContext::correlation_headers`]
/// is this function's only call site, and every header a transport ever
/// attaches goes through it.
pub fn sanitize_header_value(value: &str) -> &str {
    if is_valid_header_value(value) {
        value
    } else {
        "invalid"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // `route_template`/`is_valid_header_value` themselves are tested in
    // `hummingbird_domain::diagnostics` now — this file only tests what is
    // still local: the sanitize wrapper.

    #[test]
    fn sanitize_passes_a_valid_value_through_unchanged() {
        assert_eq!(sanitize_header_value("cycle-1"), "cycle-1");
    }

    #[test]
    fn sanitize_replaces_an_invalid_value_with_the_sentinel() {
        assert_eq!(sanitize_header_value("1.2.3 (dev build)"), "invalid");
        assert_eq!(sanitize_header_value(""), "invalid");
        assert!(is_valid_header_value(sanitize_header_value("1.2.3 (dev build)")));
    }
}
