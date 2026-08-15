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
///
/// **"Blank" is stated here rather than inherited from `str::trim`.** The
/// rule this replaced was JavaScript's `String.trim()`, and the two
/// character sets are not the same in either direction: `U+FEFF` (a BOM, the
/// single likeliest invisible to arrive by paste) is whitespace to
/// ECMAScript but not to Unicode's `White_Space`, so `str::trim` alone would
/// *accept* a BOM-only draft — precisely the junk row #110/S12 exists to
/// keep out of the queue — while `U+0085` goes the other way. Neither
/// runtime's trivia belongs in a rule three clients read, so the rule is:
/// **a draft is blank when every character in it is invisible**, meaning
/// Unicode `White_Space` plus `U+FEFF`. That is a superset of both sides, so
/// the only drafts it decides differently from the retired TS copy are ones
/// made entirely of invisibles, which were junk under either reading.
pub fn can_submit_capture(draft: &str) -> bool {
    !draft.chars().all(is_invisible)
}

/// The blank-draft alphabet above. `U+FEFF` is not in Unicode's
/// `White_Space` — it is a zero-width no-break space — so it is named.
fn is_invisible(c: char) -> bool {
    c.is_whitespace() || c == '\u{feff}'
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

    /// The two characters where `str::trim` and JavaScript's `String.trim()`
    /// disagree, pinned in both directions so the stated rule cannot quietly
    /// revert to either runtime's default. `U+FEFF` is the one that matters:
    /// under `str::trim` a BOM-only draft was submittable.
    #[test]
    fn the_invisibles_the_two_runtimes_disagree_about_are_refused() {
        for draft in ["\u{feff}", "\u{85}", "\u{feff}\u{85}  \n"] {
            assert!(!can_submit_capture(draft), "{draft:?} should be refused");
        }
    }

    /// An invisible only decides the answer when it is *all* there is — a
    /// BOM in front of real text is still real text, and the caller submits
    /// the string it holds, BOM included (`parse_capture` is what reads it).
    #[test]
    fn an_invisible_beside_real_text_is_still_real_text() {
        assert!(can_submit_capture("\u{feff}buy milk"));
    }
}
