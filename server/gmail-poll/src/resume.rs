//! The cursor-loss decision (#264 review item 5): whether a poll continues
//! from its stored `historyId` or must fall back to a bounded re-sync,
//! resolved purely from the stored cursor and the *outcome* of trying to
//! resume from it. Lifted out of `main.rs` — which can only ever call
//! `gmail.history.list` and read its status — so AC6's required
//! cursor-loss fixture case (first-run, expired, normal-advance, and the
//! no-`historyId`-in-response cases) is natively testable, the same split
//! `server/city-waste` draws between what is decidable and what is not.

use crate::HistoryPage;

/// The two shapes a `history.list` attempt can produce, once `main.rs` has
/// already read the HTTP status (only it can — this module never sees a
/// status code, only what the status meant). `main.rs`'s own job is
/// exactly the `Ok(page) => Page`, `Err(Status(404)) => Expired` mapping;
/// anything else (a transport error, a non-404 status) is not a
/// cursor-loss case at all and is returned as a run error before `resume`
/// is ever called.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoryOutcome {
    Page(HistoryPage),
    /// Gmail's 404 on `history.list` — its own signal that the stored
    /// `historyId` has aged out (its retention window is roughly a week).
    Expired,
}

/// What to do next. `Resync` carries no facts of its own: it is simply
/// "start over," named so this function's return type can express the
/// decision without performing any of the bounded re-sync's own I/O —
/// that stays in `main.rs`, which alone can make the `messages.list` /
/// `getProfile` calls.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Plan {
    /// Continue from a normal `history.list` page: the message ids to
    /// fetch, and the `historyId` to write back once they are handled.
    Advance { message_ids: Vec<String>, new_history_id: String },
    /// No cursor was ever stored, or Gmail reports the stored one expired
    /// — a bounded re-sync is required.
    Resync,
}

/// Resolves the cursor-loss decision. `stored` is the previously written
/// `historyId`, if any (`None` on a first-ever run — the same bounded
/// start a lost cursor gets, since neither has an earlier point to resume
/// from). A page's own `historyId` wins when Gmail includes one; **the
/// stored cursor is carried forward unchanged when it does not** — Gmail
/// omits `historyId` on some "nothing changed" pages, and losing it there
/// would silently rewind the next poll's window rather than replaying it,
/// which is the opposite of AC2's "recovers... instead of dropping the
/// window silently."
pub fn resume(stored: Option<&str>, outcome: HistoryOutcome) -> Plan {
    match (stored, outcome) {
        (None, _) => Plan::Resync,
        (Some(_), HistoryOutcome::Expired) => Plan::Resync,
        (Some(prev), HistoryOutcome::Page(page)) => Plan::Advance {
            message_ids: page.message_ids,
            new_history_id: page.history_id.unwrap_or_else(|| prev.to_string()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(ids: &[&str], history_id: Option<&str>) -> HistoryPage {
        HistoryPage {
            message_ids: ids.iter().map(|s| s.to_string()).collect(),
            next_page_token: None,
            history_id: history_id.map(str::to_string),
        }
    }

    /// AC1/AC2: a first-ever run (no stored cursor at all) is a bounded
    /// re-sync, the same as a lost one — neither has an earlier point to
    /// resume from.
    #[test]
    fn first_run_with_no_stored_cursor_is_a_resync() {
        assert_eq!(resume(None, HistoryOutcome::Page(page(&["m-1"], Some("50")))), Plan::Resync);
        assert_eq!(resume(None, HistoryOutcome::Expired), Plan::Resync);
    }

    /// AC2: Gmail's 404 on an aged-out `historyId` recovers via a bounded
    /// re-sync rather than silently dropping the window.
    #[test]
    fn an_expired_cursor_is_a_resync() {
        assert_eq!(resume(Some("100"), HistoryOutcome::Expired), Plan::Resync);
    }

    /// AC1: the ordinary case — a live cursor, a normal page — advances to
    /// the page's own new `historyId`.
    #[test]
    fn a_normal_page_advances_to_its_own_history_id() {
        let plan = resume(Some("100"), HistoryOutcome::Page(page(&["m-1", "m-2"], Some("103"))));
        assert_eq!(
            plan,
            Plan::Advance {
                message_ids: vec!["m-1".to_string(), "m-2".to_string()],
                new_history_id: "103".to_string(),
            }
        );
    }

    /// A "nothing changed" page that omits `historyId` entirely must not
    /// rewind the cursor — the stored value is carried forward unchanged,
    /// so the next poll's window starts exactly where this one did rather
    /// than replaying it.
    #[test]
    fn a_page_missing_history_id_holds_the_stored_cursor_the_batch_replays() {
        let plan = resume(Some("100"), HistoryOutcome::Page(page(&[], None)));
        assert_eq!(
            plan,
            Plan::Advance { message_ids: Vec::new(), new_history_id: "100".to_string() }
        );
    }
}
