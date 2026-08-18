//! What a client-supplied entity id may contain.
//!
//! Every such id is addressed later as a **path segment** — `PATCH
//! /api/items/:id`, `GET /api/grills/:id`, `DELETE /api/push_targets/:id` —
//! and the authority matches segments exactly as received, with no
//! percent-decoding (`handlers::ApiRequest::path`, ADR-0008). Those two
//! facts together mean an id outside this charset is stored fine and then
//! addressable by nothing: `%20` does not match a stored space, and a
//! literal space cannot appear in an HTTP request line. #548 is the row
//! that proved it — minted, live on every client, and unreachable by any
//! caller for the rest of time, since there is no DELETE route for items
//! either.
//!
//! So the create routes reject it at the door instead. The charset is RFC
//! 3986's *unreserved* set, the characters that survive a URL round-trip
//! verbatim in any encoder's hands. Every id every producer mints already
//! sits inside it — the sweeper's and the alert lane's are hex digests,
//! the clients' are uuid-shaped — and a sweep of production on 2026-08-18
//! found exactly one violation across all 63 items, 31 steps, 10 alerts and
//! 4 rules: #548's own row.

/// True when `s` may be both stored as an entity id and matched back as a
/// path segment. Empty is false — an empty id is no id. `.` and `..` are
/// false despite being spelled entirely in unreserved characters: they are
/// the dot segments, which any normalizing intermediary rewrites away
/// before the authority ever sees them (RFC 3986 §5.2.4).
pub fn is_url_safe_id(s: &str) -> bool {
    !s.is_empty()
        && s != "."
        && s != ".."
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_' | '~'))
}

#[cfg(test)]
mod tests {
    use super::is_url_safe_id;

    #[test]
    fn accepts_the_shapes_producers_actually_mint() {
        // uuid-shaped (clients, sweeper), hex digest (alert lane), and the
        // remaining unreserved characters.
        assert!(is_url_safe_id("3f2a1b4c-5d6e-4f70-8a9b-0c1d2e3f4a5b"));
        assert!(is_url_safe_id("9d41ab7c0e2f4b8a9d41ab7c0e2f4b8a"));
        assert!(is_url_safe_id("a.b_c~d-1"));
    }

    #[test]
    fn rejects_empty() {
        assert!(!is_url_safe_id(""));
    }

    #[test]
    fn rejects_the_id_that_created_548() {
        assert!(!is_url_safe_id("test mint 526 repro B"));
    }

    #[test]
    fn rejects_everything_that_would_not_round_trip() {
        // A slash would split into two segments; the rest either need
        // encoding or change meaning once encoded.
        for bad in [
            "a/b", "a?b", "a#b", "a%20b", "a b", "a+b", "a&b", "a:b", "a@b", "..", "a\nb", "café",
        ] {
            assert!(!is_url_safe_id(bad), "{bad} must be rejected");
        }
    }

    #[test]
    fn the_dot_segments_are_rejected_though_their_characters_are_unreserved() {
        // The charset alone would admit both: `.` is unreserved. They are
        // out because a normalizing intermediary rewrites them away, so the
        // segment the authority matches is not the segment that was sent.
        // `.` inside a longer id is untouched by that rule and stays legal.
        assert!(!is_url_safe_id("."));
        assert!(!is_url_safe_id(".."));
        assert!(is_url_safe_id("a..b"));
    }
}
