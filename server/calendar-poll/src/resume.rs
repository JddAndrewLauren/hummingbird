//! The cursor-loss decision (`gmail_poll::resume`'s own pattern, for
//! `syncToken` rather than `historyId`): whether a poll continues from its
//! stored `syncToken` or must fall back to a bounded re-sync, resolved
//! purely from the stored cursor and the *outcome* of trying to resume from
//! it. Lifted out of `main.rs` — which can only ever call `events.list` and
//! read its status — so the required cursor-loss fixture case (first-run,
//! expired, normal-advance, and the no-`nextSyncToken`-in-response cases)
//! is natively testable, the same split `gmail_poll`/`city_waste` draw
//! between what is decidable and what is not.

use crate::sync::SyncPage;

/// The two shapes an `events.list` sync attempt can produce, once
/// `main.rs` has already read the HTTP status (only it can — this module
/// never sees a status code, only what the status meant). `main.rs`'s own
/// job is exactly the `Ok(page) => Page`, `Err(Status(410)) => Expired`
/// mapping; anything else (a transport error, a non-410 status) is not a
/// cursor-loss case at all and is returned as a run error before `resume`
/// is ever called.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncOutcome {
    Page(SyncPage),
    /// Google's 410 Gone on `events.list` — its own signal that the stored
    /// `syncToken` has aged out or was invalidated (a full ACL change, or
    /// simply exceeding the retention window).
    Expired,
}

/// What to do next. `Resync` carries no facts of its own: it is simply
/// "start over," named so this function's return type can express the
/// decision without performing any of the bounded re-sync's own I/O — that
/// stays in `main.rs`, which alone can make the bounded `events.list` call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Plan {
    /// Continue from a normal `events.list` page: the raw event bodies to
    /// evaluate, and the `syncToken` to write back once they are handled.
    Advance { raw_events: Vec<String>, new_sync_token: String },
    /// No cursor was ever stored, or Google reports the stored one expired
    /// — a bounded re-sync is required.
    Resync,
}

/// Resolves the cursor-loss decision. `stored` is the previously written
/// `syncToken`, if any (`None` on a first-ever run — the same bounded start
/// a lost cursor gets, since neither has an earlier point to resume from).
/// A page's own `nextSyncToken` wins when Google includes it; **the stored
/// cursor is carried forward unchanged when it does not** — a page that is
/// not yet the final one of a multi-page sync carries no `nextSyncToken` at
/// all, and losing the stored value there would silently rewind the next
/// poll's window rather than replaying it.
pub fn resume(stored: Option<&str>, outcome: SyncOutcome) -> Plan {
    match (stored, outcome) {
        (None, _) => Plan::Resync,
        (Some(_), SyncOutcome::Expired) => Plan::Resync,
        (Some(prev), SyncOutcome::Page(page)) => Plan::Advance {
            raw_events: page.raw_events,
            new_sync_token: page.next_sync_token.unwrap_or_else(|| prev.to_string()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(raw: &[&str], next_sync_token: Option<&str>) -> SyncPage {
        SyncPage {
            raw_events: raw.iter().map(|s| s.to_string()).collect(),
            next_page_token: None,
            next_sync_token: next_sync_token.map(str::to_string),
        }
    }

    /// AC1: a first-ever run (no stored cursor at all) is a bounded
    /// re-sync, the same as a lost one — neither has an earlier point to
    /// resume from.
    #[test]
    fn first_run_with_no_stored_cursor_is_a_resync() {
        assert_eq!(resume(None, SyncOutcome::Page(page(&["e1"], Some("tok")))), Plan::Resync);
        assert_eq!(resume(None, SyncOutcome::Expired), Plan::Resync);
    }

    /// AC1: an aged-out/invalid `syncToken` recovers via a bounded re-sync
    /// rather than silently dropping the window.
    #[test]
    fn an_expired_cursor_is_a_resync() {
        assert_eq!(resume(Some("tok-old"), SyncOutcome::Expired), Plan::Resync);
    }

    /// AC1: the ordinary case — a live cursor, a normal page — advances to
    /// the page's own new `syncToken`.
    #[test]
    fn a_normal_page_advances_to_its_own_sync_token() {
        let plan = resume(Some("tok-1"), SyncOutcome::Page(page(&["e1", "e2"], Some("tok-2"))));
        assert_eq!(
            plan,
            Plan::Advance {
                raw_events: vec!["e1".to_string(), "e2".to_string()],
                new_sync_token: "tok-2".to_string(),
            }
        );
    }

    /// A non-final page (mid-pagination) that omits `nextSyncToken` must
    /// not rewind the cursor — the stored value is carried forward
    /// unchanged, so a caller that (incorrectly) stopped mid-pagination
    /// replays rather than rewinds.
    #[test]
    fn a_page_missing_next_sync_token_holds_the_stored_cursor() {
        let plan = resume(Some("tok-1"), SyncOutcome::Page(page(&[], None)));
        assert_eq!(plan, Plan::Advance { raw_events: Vec::new(), new_sync_token: "tok-1".to_string() });
    }
}
