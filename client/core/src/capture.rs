//! Issue #110 (S12)'s **named no-op parse seam**.
//!
//! #42's on-device capture-parse bake-off is unresolved (see that issue for
//! the harness and the four postures it might resolve to). [`parse_seam`]
//! exists so a resolved bake-off has somewhere to land, and does nothing so
//! it never pre-empts a decision that has not been made — the failure mode
//! this module is named against is an agent "helpfully" implementing
//! parsing here. Every [`crate::Core::capture`] call runs the raw string
//! through it, and discards the result: the title that actually reaches the
//! mutation is the caller's own, verbatim, never this seam's output. That is
//! #42's "parse is additive, never destructive" invariant, restated for a
//! seam that today adds nothing at all.

/// What [`parse_seam`] would eventually split a raw capture into. Carries
/// only the raw string today — there is no vocabulary to add fields for
/// until #42 resolves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedCapture {
    pub raw: String,
}

/// The seam itself: echoes `raw` back, unexamined. Never called for its
/// return value — [`crate::Core::capture`] discards it — only for the shape
/// to exist and be named.
pub fn parse_seam(raw: &str) -> ParsedCapture {
    ParsedCapture { raw: raw.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #110 acceptance: "The parse seam is present, named, and does
    /// nothing." — pins the "does nothing" half: whatever it is handed
    /// comes back byte-for-byte, including the pathological inputs a real
    /// parser would be tempted to normalise (padding, empty, multiline).
    #[test]
    fn the_seam_returns_whatever_it_is_handed_unexamined() {
        for raw in ["buy milk", "  padded  ", "", "multi\nline\ttabs", "emoji 🎉"] {
            assert_eq!(parse_seam(raw), ParsedCapture { raw: raw.to_string() });
        }
    }
}
