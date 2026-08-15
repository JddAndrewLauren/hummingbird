//! The capture-submission decision (#110/S12's client-side refusal),
//! sunk here from `client/web`'s `capture-validation.ts` by ADR-0025.
//!
//! [`crate::Core::capture`] still has no opinion of its own — it enqueues
//! whatever `title` it is handed, and [`crate::capture::parse_seam`]
//! deliberately adds nothing — so the refusal is still a *pre-submit*
//! decision made by the caller. What changed is where the caller reads it
//! from: one function, shared by every client, instead of a TS copy on the
//! web and a Kotlin copy on Android that could drift apart silently.

/// Whether `draft` is a real capture worth submitting — #110/S12's "an
/// empty capture is refused client-side; a junk row must never be able to
/// wedge the queue", with a whitespace-only draft counting as empty.
///
/// Pure, and deliberately answers only *whether* to submit, never *what* to
/// submit: #110's "raw string reaches the mutation unmodified" criterion
/// means the caller sends its own original, untrimmed string on submit.
/// Nothing here trims anything on the caller's behalf.
pub fn can_submit_capture(draft: &str) -> bool {
    !draft.trim().is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The three cases ported verbatim from `capture-validation.test.ts`,
    /// which stays green against the seam wrapper — the two suites pinning
    /// the same rule from either side of the boundary is what makes the
    /// sink a rewire rather than a rewrite.
    #[test]
    fn an_empty_string_is_refused() {
        assert!(!can_submit_capture(""));
    }

    #[test]
    fn a_whitespace_only_draft_is_refused() {
        for draft in ["   ", "\t\n  ", "\u{a0}"] {
            assert!(!can_submit_capture(draft), "{draft:?} should be refused");
        }
    }

    #[test]
    fn real_text_is_accepted_padding_included() {
        assert!(can_submit_capture("buy milk"));
        assert!(can_submit_capture("  buy milk  "));
    }
}
