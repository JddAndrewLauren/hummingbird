//! Route templating and the `X-Hummingbird-*` correlation headers.
//!
//! [`route_template`] is a pure function over the *route* — an
//! already-known path string — never a built URL string-munged apart, per
//! the brief: a caller with just `"/api/items/a-1"` in hand, no URL object,
//! can call this directly and a test can assert its output without ever
//! constructing a request.

/// The four correlation headers every observed sync HTTP call carries.
/// Since #711, the authority's own request boundary
/// (`server/authority/src/diagnostics.rs`, a separate Rust crate in a
/// separate Cargo workspace — it cannot call back into this module) checks
/// `X-Hummingbird-Cycle-Id`/`-Request-Id` against the identical
/// `[A-Za-z0-9_-]{1,80}` pattern [`is_valid_header_value`] enforces here,
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

/// `[A-Za-z0-9_-]{1,80}` — the shape every one of the four header values
/// must satisfy. A pure predicate; [`sanitize_header_value`] is the
/// enforcement point that actually calls it on every attached value.
pub fn is_valid_header_value(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 80
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
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

/// Reduces a concrete path to its route template: every path segment that
/// is not made up entirely of ASCII letters and underscores is replaced
/// with `:id` — **except** the one segment after `settings`, kept concrete
/// as `sync::write::paths::setting`'s own docs name it: a settings key
/// (`race-series`, `question-enabled-race`, `theme`, ...) is drawn from a
/// small, fixed, non-secret vocabulary, not a per-instance entity id, so
/// redacting it destroys diagnostic detail (which setting failed to sync)
/// for no privacy benefit — and because several settings keys are
/// hyphenated, the general "letters and underscores only" rule would
/// otherwise template `/api/settings/race-series` while leaving
/// `/api/settings/theme` untouched, degrading exactly the setting whose key
/// most needs to stay visible to debug. Every other static segment in this
/// API's own routes (`api`, `items`, `changes`, `sweep`, `blocked_by`,
/// `fog`, `grills`, ...) really is just lowercase letters and underscores;
/// every entity id this app mints (`sweep.py`'s `deterministic_v4`, and the
/// uuids the authority assigns) contains a hyphen or a digit, so the
/// general rule still draws the boundary correctly everywhere else,
/// without a route table to keep in sync as new resources are added.
///
/// **The known limit:** a purely alphabetic id would be left concrete,
/// indistinguishable from a static segment. No id this app mints is purely
/// alphabetic (see the sentence above), so the limit is unreachable today —
/// a future id format without a digit or a hyphen would need a route table
/// here rather than a tweak to this rule.
pub fn route_template(path: &str) -> String {
    let segments: Vec<&str> = path.split('/').collect();
    segments
        .iter()
        .enumerate()
        .map(|(index, segment)| {
            let previous_is_settings = index > 0 && segments[index - 1] == "settings";
            if previous_is_settings
                || segment.is_empty()
                || segment.chars().all(|c| c.is_ascii_alphabetic() || c == '_')
            {
                segment.to_string()
            } else {
                ":id".to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_collection_path_is_unchanged() {
        assert_eq!(route_template("/api/items"), "/api/items");
    }

    #[test]
    fn a_single_entity_id_is_templated() {
        assert_eq!(route_template("/api/items/a-1"), "/api/items/:id");
    }

    #[test]
    fn two_entity_ids_in_one_path_are_both_templated() {
        assert_eq!(
            route_template("/api/blocked_by/a-1/a-2"),
            "/api/blocked_by/:id/:id"
        );
    }

    #[test]
    fn a_recorded_route_never_contains_the_concrete_entity_id() {
        let template = route_template("/api/items/9f1c2e40-aaaa-4b2b-8c3d-000000000001");
        assert!(!template.contains("9f1c2e40"));
        assert_eq!(template, "/api/items/:id");
    }

    #[test]
    fn purely_alphabetic_route_words_survive_untouched() {
        assert_eq!(route_template("/api/sweep"), "/api/sweep");
        assert_eq!(route_template("/api/changes"), "/api/changes");
    }

    /// Pins the exact case review round 1 found broken: a hyphenated
    /// settings key must survive concrete, the same as an unhyphenated one
    /// — `sync::write::paths::setting("race-series")` and
    /// `setting("question-enabled-race")` are both real keys in this tree
    /// today (`lib.rs`'s settings handlers).
    #[test]
    fn a_hyphenated_settings_key_survives_concrete_same_as_an_unhyphenated_one() {
        assert_eq!(route_template("/api/settings/race-series"), "/api/settings/race-series");
        assert_eq!(
            route_template("/api/settings/question-enabled-race"),
            "/api/settings/question-enabled-race"
        );
        assert_eq!(route_template("/api/settings/theme"), "/api/settings/theme");
    }

    /// An entity id one level *past* the settings key is still templated —
    /// the exemption is exactly one segment wide, not "everything under
    /// `/api/settings`".
    #[test]
    fn only_the_segment_immediately_after_settings_is_exempt() {
        assert_eq!(
            route_template("/api/settings/race-series/a-1"),
            "/api/settings/race-series/:id"
        );
    }

    #[test]
    fn a_valid_header_value_accepts_letters_digits_underscore_and_hyphen() {
        assert!(is_valid_header_value("cycle-1_ABC123"));
    }

    #[test]
    fn an_empty_header_value_is_rejected() {
        assert!(!is_valid_header_value(""));
    }

    #[test]
    fn a_header_value_over_eighty_characters_is_rejected() {
        let too_long = "a".repeat(81);
        assert!(!is_valid_header_value(&too_long));
    }

    #[test]
    fn a_header_value_with_a_disallowed_character_is_rejected() {
        assert!(!is_valid_header_value("has a space"));
        assert!(!is_valid_header_value("has/a/slash"));
    }

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
