//! The cursor-loss decision, shared between the mail and calendar lanes —
//! `gmail_poll::resume`/`calendar_poll::resume`'s own pattern, generalized
//! over Graph's one delta-page shape ([`crate::delta::DeltaPage`]) since
//! both lanes share it (`delta.rs`'s own module doc). Whether a poll
//! continues from its stored `deltaLink` or must fall back to a bounded
//! re-sync, resolved purely from the stored cursor and the *outcome* of
//! trying to resume from it. Lifted out of each binary's `main.rs` — which
//! can only ever call the stored `deltaLink` and read its HTTP status — so
//! the required cursor-loss fixture case (first-run, expired,
//! normal-advance, no-`deltaLink`-in-response) is natively testable for
//! both lanes from one implementation.

use crate::delta::DeltaPage;

/// The two shapes a delta-query attempt can produce, once `main.rs` has
/// already read the HTTP status (only it can — this module never sees a
/// status code, only what the status meant). Each binary's own job is
/// exactly the `Ok(page) => Page`, `Err(Status(410)) => Expired` mapping —
/// Graph signals an aged-out or invalidated `deltaLink` with HTTP 410 Gone
/// on both the mail and calendar delta endpoints; anything else (a
/// transport error, a non-410 status) is not a cursor-loss case at all and
/// is returned as a run error before `resume` is ever called.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeltaOutcome {
    Page(DeltaPage),
    /// Graph's HTTP 410 Gone — its own signal that the stored `deltaLink`
    /// has aged out or was invalidated.
    Expired,
}

/// What to do next. `Resync` carries no facts of its own: it is simply
/// "start over," named so this function's return type can express the
/// decision without performing any of the bounded re-sync's own I/O — that
/// stays in each binary's `main.rs`, which alone can make the bounded
/// request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Plan {
    /// Continue from a normal delta page: the raw item bodies to evaluate,
    /// and the `deltaLink` to write back once they are handled.
    Advance { raw_items: Vec<String>, new_delta_link: String },
    /// No cursor was ever stored, or Graph reports the stored one expired
    /// — a bounded re-sync is required.
    Resync,
}

/// Resolves the cursor-loss decision. `stored` is the previously written
/// `deltaLink`, if any (`None` on a first-ever run — the same bounded start
/// a lost cursor gets, since neither has an earlier point to resume from).
/// A page's own `delta_link` wins when Graph includes it; **the stored
/// cursor is carried forward unchanged when it does not** — a
/// non-final page of a multi-page sync carries no `delta_link` at all, and
/// losing the stored value there would silently rewind the next poll's
/// window rather than replaying it (`calendar_poll::resume`'s own reason,
/// verbatim).
pub fn resume(stored: Option<&str>, outcome: DeltaOutcome) -> Plan {
    match (stored, outcome) {
        (None, _) => Plan::Resync,
        (Some(_), DeltaOutcome::Expired) => Plan::Resync,
        (Some(prev), DeltaOutcome::Page(page)) => Plan::Advance {
            raw_items: page.raw_items,
            new_delta_link: page.delta_link.unwrap_or_else(|| prev.to_string()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(raw: &[&str], delta_link: Option<&str>) -> DeltaPage {
        DeltaPage {
            raw_items: raw.iter().map(|s| s.to_string()).collect(),
            next_link: None,
            delta_link: delta_link.map(str::to_string),
        }
    }

    /// AC: a first-ever run (no stored cursor at all) is a bounded
    /// re-sync, the same as a lost one — neither has an earlier point to
    /// resume from.
    #[test]
    fn first_run_with_no_stored_cursor_is_a_resync() {
        assert_eq!(resume(None, DeltaOutcome::Page(page(&["e1"], Some("dl")))), Plan::Resync);
        assert_eq!(resume(None, DeltaOutcome::Expired), Plan::Resync);
    }

    /// AC: an aged-out/invalid `deltaLink` recovers via a bounded re-sync
    /// rather than silently dropping the window.
    #[test]
    fn an_expired_cursor_is_a_resync() {
        assert_eq!(resume(Some("dl-old"), DeltaOutcome::Expired), Plan::Resync);
    }

    /// AC: the ordinary case — a live cursor, a normal page — advances to
    /// the page's own new `deltaLink`.
    #[test]
    fn a_normal_page_advances_to_its_own_delta_link() {
        let plan = resume(Some("dl-1"), DeltaOutcome::Page(page(&["e1", "e2"], Some("dl-2"))));
        assert_eq!(
            plan,
            Plan::Advance {
                raw_items: vec!["e1".to_string(), "e2".to_string()],
                new_delta_link: "dl-2".to_string(),
            }
        );
    }

    /// A non-final page (mid-pagination) that omits `delta_link` must not
    /// rewind the cursor — the stored value is carried forward unchanged.
    #[test]
    fn a_page_missing_delta_link_holds_the_stored_cursor() {
        let plan = resume(Some("dl-1"), DeltaOutcome::Page(page(&[], None)));
        assert_eq!(plan, Plan::Advance { raw_items: Vec::new(), new_delta_link: "dl-1".to_string() });
    }
}
