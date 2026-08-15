//! The capture-submission decision (#110/S12's client-side refusal),
//! sunk here from `client/web`'s `capture-validation.ts` by ADR-0025.
//!
//! [`crate::Core::capture`] still has no opinion of its own — it enqueues
//! whatever `title` it is handed, and [`crate::capture::parse_seam`]
//! deliberately adds nothing — so the refusal is still a *pre-submit*
//! decision made by the caller. What changed is where the caller reads it
//! from: one function, shared by every client, instead of a TS copy on the
//! web and a Kotlin copy on Android that could drift apart silently.
//!
//! Grew at M1-2 (#500) with the rest of the capture box's field-level
//! decisions: [`capture_meta_problems`] (the web's retired
//! `capture-meta.ts`/`triage-form.ts` — both hand-copied the same two
//! problem strings) and [`priority_from_select`] (the `"0"` -> "not sent"
//! rule, the one piece of `resolveCaptureFields` that was a decision rather
//! than a slider-index lookup).

use super::urgency::{is_valid_deadline_field, is_valid_scheduled_date};

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

/// Whatever is wrong with a capture/triage draft's two free-text date
/// fields, keyed by field — shared verbatim between the retired
/// `capture-meta.ts`'s `captureMetaProblems` and `triage-form.ts`'s
/// `triageDraftProblems`, which is the whole reason this lives here rather
/// than behind either name: those two hand-copied the same two message
/// strings (`capture-meta.ts:106,109` vs `triage-form.ts:104,107` at the
/// point the web sank it), and a single Rust function makes that copy
/// impossible to have.
///
/// Only the free-text dates can be wrong: every other capture/triage field
/// is a closed-vocabulary `Select` (see [`super::vocabulary`]) or the
/// title, which `can_submit_capture` already answers for.
pub struct CaptureMetaProblems {
    pub deadline: Option<String>,
    pub scheduled_date: Option<String>,
}

pub fn capture_meta_problems(deadline: &str, scheduled_date: &str) -> CaptureMetaProblems {
    CaptureMetaProblems {
        deadline: (!deadline.is_empty() && !is_valid_deadline_field(deadline))
            .then(|| "Use YYYY-MM-DD or YYYY-MM-DDTHH:MM".to_string()),
        scheduled_date: (!scheduled_date.is_empty() && !is_valid_scheduled_date(scheduled_date))
            .then(|| "Use YYYY-MM-DD".to_string()),
    }
}

/// The capture box's priority `Select` resolves to the wire's
/// `CaptureFields.priority`: `"0"` — the priority vocabulary's own "none",
/// and the control's own resting value — means "not sent", never a sent
/// zero. The server's own default for `priority` is already `0`, so
/// sending it would be this client asserting a value the reader never
/// chose, and priority would be the one capture field that could not be
/// left undecided.
///
/// A `Select`'s value should never reach here as anything but `"0"`..`"4"`,
/// but this stays a total function rather than trusting that: anything
/// that fails to parse as an integer also resolves to "not sent" — the
/// same answer the retired TS call (`Number(meta.priority)`, then
/// `JSON.stringify`'s own `NaN` -> `null` coercion) already produced for a
/// malformed value, not a new rule.
pub fn priority_from_select(raw: &str) -> Option<i64> {
    if raw == "0" {
        return None;
    }
    raw.parse::<i64>().ok()
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

    // ------------------------------------------------- capture_meta_problems
    // Ported from `capture-meta.test.ts`'s `captureMetaProblems` describe
    // block (the same rules `triage-form.test.ts` pins for its own draft
    // shape, since both now call this one function).

    #[test]
    fn finds_nothing_wrong_with_two_empty_fields() {
        let problems = capture_meta_problems("", "");
        assert!(problems.deadline.is_none());
        assert!(problems.scheduled_date.is_none());
    }

    #[test]
    fn accepts_both_deadline_shapes_and_a_whole_day_scheduled_date() {
        assert!(capture_meta_problems("2026-09-01", "").deadline.is_none());
        assert!(capture_meta_problems("2026-09-01T09:30", "").deadline.is_none());
        assert!(capture_meta_problems("", "2026-08-30").scheduled_date.is_none());
    }

    #[test]
    fn names_a_malformed_deadline_an_impossible_date_and_a_timed_do_date() {
        assert_eq!(
            capture_meta_problems("next tuesday", "").deadline,
            Some("Use YYYY-MM-DD or YYYY-MM-DDTHH:MM".to_string()),
        );
        assert!(capture_meta_problems("2026-02-30", "").deadline.is_some());
        // A do-date is a whole civil day; the date-time form is refused here
        // even though the deadline field accepts it.
        assert_eq!(
            capture_meta_problems("", "2026-08-30T09:30").scheduled_date,
            Some("Use YYYY-MM-DD".to_string()),
        );
    }

    // ------------------------------------------------- priority_from_select

    #[test]
    fn priority_zero_is_not_sent() {
        assert_eq!(priority_from_select("0"), None);
    }

    #[test]
    fn every_other_priority_is_sent_as_its_number() {
        assert_eq!(priority_from_select("1"), Some(1));
        assert_eq!(priority_from_select("4"), Some(4));
    }

    #[test]
    fn a_malformed_value_resolves_to_not_sent_rather_than_erroring() {
        assert_eq!(priority_from_select("abc"), None);
        assert_eq!(priority_from_select(""), None);
    }
}
