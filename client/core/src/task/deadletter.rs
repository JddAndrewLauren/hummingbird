//! [`DeadLetterEntry`]: a mutation that will never be sent, kept so its
//! content is not lost.
//!
//! ADR-0007: "Losing != vanishing." A mutation dropped by field-level
//! Linear-wins, or rejected permanently by Linear, lands here rather than
//! disappearing — surfaced as a low-key "1 edit didn't apply" affordance for
//! manual re-apply. This generalizes the additive-parse invariant from map
//! #35: reconciliation may discard an *effect*, never *content* the user
//! produced.

use serde::{Deserialize, Serialize};

use super::item::ItemId;

/// Why a mutation was abandoned.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeadLetterReason {
    /// A newer server value existed for the same field, so the local edit
    /// lost under ADR-0001's Linear-wins rule and was pulled from the queue
    /// before it was ever sent.
    ConflictLost {
        /// What the sweep brought back for this field. `None` when the server
        /// value is not representable as text (an unmodelled field type) —
        /// the local value is still preserved, which is the point.
        server_value: Option<String>,
    },
    /// Linear rejected it in a way no retry can fix — the client-side twin of
    /// the sweeper's terminal-rejection quarantine (`sweep.py::_is_terminal`,
    /// issue #24).
    PermanentRejection { message: String },
}

/// One abandoned mutation, with enough context to re-apply it by hand.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeadLetterEntry {
    pub item_id: ItemId,
    /// The item's human-facing key at the time, so the affordance can name
    /// the item even if it has since gone [`super::Presence::Absent`].
    pub identifier: String,
    /// The domain field name — this model's vocabulary, never Linear's.
    pub field: String,
    /// What the user meant. The one thing here that must never be dropped.
    pub local_value: String,
    pub reason: DeadLetterReason,
    pub at_ms: i64,
}

/// The journal, oldest first.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeadLetterJournal {
    entries: Vec<DeadLetterEntry>,
}

impl DeadLetterJournal {
    pub fn record(&mut self, entry: DeadLetterEntry) {
        self.entries.push(entry);
    }

    pub fn entries(&self) -> &[DeadLetterEntry] {
        &self.entries
    }

    /// What the "1 edit didn't apply" affordance counts.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Drops one entry by index — the user acknowledging it, after
    /// re-applying by hand or deciding not to.
    ///
    /// Returns the entry so a caller can re-queue it. Acknowledging is the
    /// *only* way an entry leaves: nothing in the sync cycle prunes this
    /// journal, because an entry silently vanishing is exactly the failure
    /// this type exists to prevent.
    pub fn acknowledge(&mut self, index: usize) -> Option<DeadLetterEntry> {
        if index < self.entries.len() {
            Some(self.entries.remove(index))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(field: &str, local: &str) -> DeadLetterEntry {
        DeadLetterEntry {
            item_id: "id-1".to_string(),
            identifier: "ION-16".to_string(),
            field: field.to_string(),
            local_value: local.to_string(),
            reason: DeadLetterReason::ConflictLost {
                server_value: Some("theirs".to_string()),
            },
            at_ms: 1_000,
        }
    }

    #[test]
    fn a_journaled_loser_keeps_the_content_the_user_produced() {
        // The effect is discarded; the content is not. That is the whole
        // contract (ADR-0007).
        let mut journal = DeadLetterJournal::default();
        journal.record(entry("title", "the words I typed"));

        assert_eq!(journal.entries()[0].local_value, "the words I typed");
        assert_eq!(journal.len(), 1);
    }

    #[test]
    fn entries_stay_in_the_order_they_were_recorded() {
        let mut journal = DeadLetterJournal::default();
        journal.record(entry("title", "first"));
        journal.record(entry("title", "second"));

        let values: Vec<&str> = journal
            .entries()
            .iter()
            .map(|e| e.local_value.as_str())
            .collect();
        assert_eq!(values, vec!["first", "second"]);
    }

    #[test]
    fn acknowledging_returns_the_entry_so_it_can_be_requeued() {
        let mut journal = DeadLetterJournal::default();
        journal.record(entry("title", "mine"));

        let taken = journal.acknowledge(0).unwrap();
        assert_eq!(taken.local_value, "mine");
        assert!(journal.is_empty());
    }

    #[test]
    fn acknowledging_out_of_range_is_none_rather_than_a_panic() {
        let mut journal = DeadLetterJournal::default();
        assert_eq!(journal.acknowledge(0), None);
    }
}
