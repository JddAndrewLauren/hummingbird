//! Route templating and the `X-Hummingbird-*` correlation headers.
//!
//! [`route_template`] is a pure function over the *route* — an
//! already-known path string — never a built URL string-munged apart, per
//! the brief: a caller with just `"/api/items/a-1"` in hand, no URL object,
//! can call this directly and a test can assert its output without ever
//! constructing a request.

/// The four correlation headers every observed sync HTTP call carries. The
/// authority validates each value against [`is_valid_header_value`]'s same
/// pattern — `[A-Za-z0-9_-]{1,80}`.
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
/// must satisfy, checked here rather than trusted, since a caller-minted
/// cycle id or a host-supplied build string is exactly the kind of value
/// that can carry something unexpected (a stray `/`, an empty string) if
/// this crate never checks it.
pub fn is_valid_header_value(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 80
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Reduces a concrete path to its route template: every path segment that
/// is not made up entirely of ASCII letters and underscores is replaced
/// with `:id`. Every static segment in this API's own routes (`api`,
/// `items`, `changes`, `sweep`, `blocked_by`, `fog`, `grills`, ...) is
/// lowercase letters and underscores; every entity id this app mints
/// (`sweep.py`'s `deterministic_v4`, and the uuids the authority assigns)
/// contains a hyphen or a digit, so this rule draws the boundary exactly
/// where "is this an entity id" needs it drawn, without a route table to
/// keep in sync as new resources are added.
pub fn route_template(path: &str) -> String {
    path.split('/')
        .map(|segment| {
            if segment.is_empty()
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
}
