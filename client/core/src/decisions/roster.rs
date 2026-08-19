//! Done and the Ledger's ordering, plus the Ledger's row-state read — sunk
//! from the web's `done-order.ts` and `ledger-order.ts` (ADR-0025, #141/M3,
//! #532). Same convention [`super::queue`] already keeps: a minimal,
//! clockless struct in, an ordered `Vec<String>` of ids out — a caller maps
//! the returned ids back onto its own full record, and the mobile seam
//! ([`MobileTaskHost::done_items`]/`ledger_rows`) does the same mapping
//! Rust-side rather than crossing it to Kotlin at all, since "Android does
//! no ordering" is this sink's own acceptance criterion.

/// One item's roster-relevant fields for [`order_done`]: an id and when it
/// was last touched. Pure and clockless, [`super::queue::QueueItem`]'s own
/// convention.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RosterItem {
    pub id: String,
    pub updated_at: i64,
}

/// Done's display order: most recently touched first. The schema has no
/// `done_at` (declined in the grilling that shaped the Done screen — a
/// later additive column if the caveat grates), so `updated_at` is the only
/// stamp available, and a later edit to a done item honestly re-sorts it.
/// Ties break on id so the order is total. Pure: never mutates `items`.
pub fn order_done(items: &[RosterItem]) -> Vec<String> {
    let mut ordered: Vec<&RosterItem> = items.iter().collect();
    ordered.sort_by(|a, b| b.updated_at.cmp(&a.updated_at).then_with(|| a.id.cmp(&b.id)));
    ordered.into_iter().map(|item| item.id.clone()).collect()
}

/// One Ledger row's roster-relevant fields for [`ledger_row_state`],
/// [`last_touched_ms`] and [`order_ledger`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedgerRosterItem {
    pub id: String,
    pub updated_at: i64,
    pub archived_at: Option<i64>,
    pub absent_since_ms: Option<i64>,
}

/// What one Ledger row's state label says beside its stage. `Archived`
/// covers both an explicit `archived_at` flag and a row the mirror demoted
/// because a complete sweep stopped carrying it — either way the row is no
/// longer live anywhere else in the app, and the Ledger is the one read
/// that still shows it, labelled rather than hidden.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum LedgerRowState {
    Live,
    Archived {
        #[serde(rename = "sinceMs")]
        since_ms: i64,
    },
}

/// The row's own flag wins over the mirror's demotion stamp when both
/// exist: `archived_at` is when the archive actually happened, while
/// `absent_since_ms` for a flagged row is derived from it anyway (the
/// mirror stamps an absence from the flag's own timestamp when it has one).
pub fn ledger_row_state(row: &LedgerRosterItem) -> LedgerRowState {
    if let Some(archived_at) = row.archived_at {
        return LedgerRowState::Archived { since_ms: archived_at };
    }
    if let Some(absent_since_ms) = row.absent_since_ms {
        return LedgerRowState::Archived { since_ms: absent_since_ms };
    }
    LedgerRowState::Live
}

/// The roster's sort key: when this row was last touched, as far as the
/// derivable facts can say. `updated_at` covers every server-stamped edit;
/// `archived_at`/`absent_since_ms` can postdate it (archiving is a flag
/// write, and a sweep demotion happens with no row write at all), so the
/// latest of the three is the honest "something last happened here" instant.
pub fn last_touched_ms(row: &LedgerRosterItem) -> i64 {
    row.updated_at.max(row.archived_at.unwrap_or(0)).max(row.absent_since_ms.unwrap_or(0))
}

/// Last touched first — the log-like spine the grilling that shaped the
/// Ledger picked, with the known caveat that any edit re-sorts a row
/// (`updated_at` is all-purpose). Ties break on id, so the order is total.
/// Pure: never mutates `rows`.
pub fn order_ledger(rows: &[LedgerRosterItem]) -> Vec<String> {
    let mut ordered: Vec<&LedgerRosterItem> = rows.iter().collect();
    ordered.sort_by(|a, b| {
        last_touched_ms(b).cmp(&last_touched_ms(a)).then_with(|| a.id.cmp(&b.id))
    });
    ordered.into_iter().map(|item| item.id.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roster_item(id: &str, updated_at: i64) -> RosterItem {
        RosterItem { id: id.to_string(), updated_at }
    }

    fn ledger_item(
        id: &str,
        updated_at: i64,
        archived_at: Option<i64>,
        absent_since_ms: Option<i64>,
    ) -> LedgerRosterItem {
        LedgerRosterItem { id: id.to_string(), updated_at, archived_at, absent_since_ms }
    }

    // ------------------------------------------------------------ order_done
    // Ported from `done-order.test.ts`.

    #[test]
    fn orders_most_recently_touched_first_id_ascending_on_ties_without_mutating() {
        let items = vec![roster_item("b", 1_000), roster_item("a", 1_000), roster_item("c", 4_000)];
        let before = items.clone();

        assert_eq!(order_done(&items), vec!["c", "a", "b"]);
        assert_eq!(items, before);
    }

    // -------------------------------------------------------- ledger_row_state
    // Ported from `ledger-order.test.ts`.

    #[test]
    fn a_live_row_is_live() {
        assert_eq!(ledger_row_state(&ledger_item("item-1", 1_000, None, None)), LedgerRowState::Live);
    }

    #[test]
    fn an_explicitly_archived_row_is_archived_as_of_its_own_flag() {
        assert_eq!(
            ledger_row_state(&ledger_item("item-1", 1_000, Some(5_000), Some(6_000))),
            LedgerRowState::Archived { since_ms: 5_000 },
        );
    }

    #[test]
    fn a_row_demoted_by_a_sweep_with_no_flag_is_archived_as_of_the_demotion_stamp() {
        assert_eq!(
            ledger_row_state(&ledger_item("item-1", 1_000, None, Some(7_000))),
            LedgerRowState::Archived { since_ms: 7_000 },
        );
    }

    // -------------------------------------------------------- last_touched_ms

    #[test]
    fn is_updated_at_for_a_live_row() {
        assert_eq!(last_touched_ms(&ledger_item("item-1", 3_000, None, None)), 3_000);
    }

    #[test]
    fn an_archive_or_demotion_stamp_later_than_updated_at_wins() {
        assert_eq!(last_touched_ms(&ledger_item("item-1", 3_000, Some(9_000), None)), 9_000);
        assert_eq!(last_touched_ms(&ledger_item("item-1", 3_000, None, Some(8_000))), 8_000);
    }

    // ------------------------------------------------------------ order_ledger

    #[test]
    fn orders_last_touched_first_id_ascending_on_ties_without_mutating_its_input() {
        let rows = vec![
            ledger_item("b", 1_000, None, None),
            ledger_item("a", 1_000, None, None),
            ledger_item("c", 5_000, None, None),
            ledger_item("d", 2_000, Some(9_000), None),
        ];
        let before = rows.clone();

        assert_eq!(order_ledger(&rows), vec!["d", "c", "a", "b"]);
        assert_eq!(rows, before);
    }
}
