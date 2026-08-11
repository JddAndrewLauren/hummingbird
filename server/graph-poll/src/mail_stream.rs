//! Folding one poll's raw mail-message bodies into evaluation candidates —
//! `calendar_poll::stream`'s own role, applied to
//! [`crate::mail_message::parse_mail_message`]. Graph's delta query already
//! returns each item's full body (unlike Gmail's `history.list`, ids only),
//! so there is no per-item network call to fail transiently here, and
//! therefore no analogue of `gmail_poll::batch::fold_messages`'s "abort the
//! whole batch on a fetch failure" — the only network call in this lane is
//! the one delta-page fetch itself, and `main.rs` already aborts the run
//! with `?` before ever writing the cursor if that fails.
//!
//! - **Removed** (Graph's own `@removed` delta marker) is expected and
//!   permanent — nothing to evaluate, nothing to retry.
//! - **Unparseable** (a malformed body Graph actually returned with a 200)
//!   is also permanent — retrying will not fix it. Skipping it lets the
//!   rest of the batch (and every future poll) proceed.

use crate::evaluate::Candidate;
use crate::mail_event::mail_message_to_candidate;
use crate::mail_message::{parse_mail_message, ParsedMailMessage};

/// One poll's fold result: the candidates ready for
/// [`crate::evaluate::evaluate_events`], plus every item skipped and why.
#[derive(Debug, Clone, PartialEq)]
pub struct MailBatch {
    pub candidates: Vec<Candidate>,
    pub skipped: Vec<(String, String)>,
}

/// Folds `raw_items` (each item's own compact JSON text,
/// [`crate::delta::DeltaPage::raw_items`]) into a [`MailBatch`]. Pure — no
/// I/O — because the delta page already handed every item's full body to
/// `main.rs`.
pub fn fold_mail_messages(raw_items: &[String]) -> MailBatch {
    let mut candidates = Vec::new();
    let mut skipped = Vec::new();
    for (i, raw) in raw_items.iter().enumerate() {
        match parse_mail_message(raw) {
            Ok(ParsedMailMessage::Live(msg)) => candidates.push(mail_message_to_candidate(&msg)),
            Ok(ParsedMailMessage::Removed) => skipped.push((i.to_string(), "removed".to_string())),
            Err(e) => skipped.push((i.to_string(), e.to_string())),
        }
    }
    MailBatch { candidates, skipped }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message_json(internet_message_id: &str) -> String {
        format!(
            r#"{{"id": "x", "internetMessageId": "{internet_message_id}", "subject": "s",
                "receivedDateTime": "2026-08-15T09:00:00Z"}}"#
        )
    }

    #[test]
    fn every_item_parsing_cleanly_produces_one_candidate_each() {
        let raw = vec![message_json("<a@b.com>"), message_json("<c@d.com>")];
        let batch = fold_mail_messages(&raw);
        assert_eq!(batch.candidates.len(), 2);
        assert!(batch.skipped.is_empty());
    }

    #[test]
    fn a_removed_item_is_skipped_loudly_and_does_not_abort_the_batch() {
        let raw = vec![r#"{"id": "x1", "@removed": {"reason": "deleted"}}"#.to_string(), message_json("<c@d.com>")];
        let batch = fold_mail_messages(&raw);
        assert_eq!(batch.candidates.len(), 1);
        assert_eq!(batch.skipped, vec![("0".to_string(), "removed".to_string())]);
    }

    #[test]
    fn an_unparseable_item_is_skipped_loudly_and_does_not_abort_the_batch() {
        let raw = vec!["not json".to_string(), message_json("<c@d.com>")];
        let batch = fold_mail_messages(&raw);
        assert_eq!(batch.candidates.len(), 1);
        assert_eq!(batch.skipped.len(), 1);
    }

    #[test]
    fn mail_candidates_never_carry_an_expiry() {
        let raw = vec![message_json("<a@b.com>")];
        let batch = fold_mail_messages(&raw);
        assert_eq!(batch.candidates[0].ends_at_ms, None);
    }
}
