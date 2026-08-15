//! **Recall** (#478, ADR-0025's core-first rule, applied — no ADR of its
//! own): a pure, deterministic re-find over the whole retained roster,
//! never a ranking or attention surface like [`crate::rank`]. Lives beside
//! `rank.rs` for the same reason `rank` does — this is decision logic, not
//! presentation, so it belongs in the core the way `frontier-facets.ts`
//! deliberately does not.
//!
//! # What Recall is not
//!
//! No fuzzy matching, no relevance score, no ranking: every match is a
//! literal, case-insensitive substring test, and every match is worth
//! exactly as much as every other. The one thing this module orders by is
//! *where the item stands* (live, `Done`, archived) and *when it was last
//! touched* — never how well the query "fits".
//!
//! # Corpus and grouping
//!
//! [`Core::search`](crate::Core::search) hands this module the same
//! retained roster [`Core::ledger`](crate::Core::ledger) reads — live,
//! `Done` and archived items alike — each already labelled with a
//! [`Group`] the same way the Ledger derives one: a row the mirror retains
//! as absent (an explicit `archived_at`, or a sweep's retention stamp) is
//! [`Group::Archived`] regardless of its own `stage`; otherwise a live
//! `Stage::Done` row is [`Group::Done`]; everything else is
//! [`Group::Live`]. An item completed and *later* archived is grouped
//! `Archived`, not `Done` — the same "current facts, not a chronicle" rule
//! the Ledger's own doc states.
//!
//! # Match semantics (decision 7)
//!
//! The query is split on whitespace into tokens. A **multi**-token query is
//! an AND: every token must appear, case-insensitively, as a substring of
//! the item's title, description, or resolved project name (decision 11 —
//! project name is searchable *through* its items, never a result of its
//! own). A **single** token shaped like a handle (`hb-42`, `HB42`, or bare
//! `42`) instead matches that one item's `seq` **exactly**, and does not
//! also fall back to a substring search — a handle query means "that one
//! item", not "everything mentioning that number". An empty or
//! whitespace-only query matches nothing (decision: recall is never
//! "browse everything").
//!
//! # Ordering and the cap (decision 8)
//!
//! Live before `Done` before archived; within a group, `updated_at`
//! descending, `seq` ascending as the final, total tie-break (mirroring
//! [`crate::rank`]'s own "always a total order" discipline). At most
//! [`CAP`] rows are returned; [`Outcome::total`] is the *un-capped* match
//! count, so a caller can render "N more" without inventing a second query.

use hummingbird_domain::Item;
use serde::{Deserialize, Serialize};

/// The display cap (decision 8): at most this many rows come back, however
/// many matched. [`Outcome::total`] carries the real count.
pub const CAP: usize = 50;

/// Where one candidate stands in the retained roster, decided from its
/// *current* facts alone (no chronicle exists to read instead). Declaration
/// order is the sort order — [`Group`] derives `Ord`, so live sorts before
/// `Done` sorts before archived, the same declaration-order trick
/// [`crate::rank`]'s `DeadlineBucket` uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Group {
    Live,
    Done,
    Archived,
}

/// One item as [`search`] sees it: the item itself, which [`Group`] it
/// falls in (the caller's call — this module has no opinion on retention
/// stamps or presence), and the project name resolved for it, if any
/// (decision 11's searchable-through-its-items text; `None` when the item
/// carries no `project_id` or the project itself could not be resolved).
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub item: Item,
    pub group: Group,
    pub project_name: Option<String>,
}

/// One matched row: the item plus the [`Group`] it was matched in. Never
/// carries the project name — a caller already has [`crate::Core::projects`]
/// for display, and Recall's output is read-only in this slice (#478) with
/// no reason to duplicate that lookup.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Row {
    pub item: Item,
    pub group: Group,
}

/// [`search`]'s whole answer: the capped, ordered rows, and the un-capped
/// total match count the "N more" line reads (decision 8 — the cap and the
/// count are core-decided, never a UI invention).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Outcome {
    pub rows: Vec<Row>,
    pub total: usize,
}

/// Lowercased, whitespace-split tokens — [`search`]'s only tokenization
/// rule (decision 7: no punctuation stripping, no stemming).
fn tokenize(query: &str) -> Vec<String> {
    query
        .split_whitespace()
        .map(|token| token.to_ascii_lowercase())
        .collect()
}

/// The handle a single query token names, if it is handle-shaped at all:
/// `hb-42`, `HB42`, or bare `42` (decision 3), matched case-insensitively.
/// Anything with a non-digit remainder after stripping an optional
/// `hb`/`hb-` prefix is not handle-shaped — including an all-punctuation or
/// empty remainder, so `"hb-"` alone names no handle.
fn handle_seq(token: &str) -> Option<i64> {
    let lower = token.to_ascii_lowercase();
    let digits = lower
        .strip_prefix("hb-")
        .or_else(|| lower.strip_prefix("hb"))
        .unwrap_or(lower.as_str());
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse::<i64>().ok()
}

/// Whether every token in `tokens` is a case-insensitive substring of
/// `candidate`'s title, description or resolved project name (decision 3
/// and 11) — the multi-word AND (decision 7).
fn matches_every_token(candidate: &Candidate, tokens: &[String]) -> bool {
    let haystack = format!(
        "{} {} {}",
        candidate.item.title,
        candidate.item.description.as_deref().unwrap_or(""),
        candidate.project_name.as_deref().unwrap_or(""),
    )
    .to_ascii_lowercase();
    tokens.iter().all(|token| haystack.contains(token.as_str()))
}

/// Recall's whole read: `candidates` is the retained roster, already
/// labelled with each row's [`Group`] and resolved project name; `query` is
/// the raw, unnormalized user text. Pure and deterministic — the same
/// inputs, in any order, produce byte-identical output (the final `id`
/// tie-break leaves nothing to chance, exactly [`crate::rank::rank`]'s own
/// discipline).
pub fn search(candidates: &[Candidate], query: &str) -> Outcome {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Outcome {
            rows: Vec::new(),
            total: 0,
        };
    }

    let tokens = tokenize(trimmed);
    let single_handle = if tokens.len() == 1 {
        handle_seq(&tokens[0])
    } else {
        None
    };

    let mut matched: Vec<&Candidate> = match single_handle {
        Some(seq) => candidates
            .iter()
            .filter(|candidate| candidate.item.seq == Some(seq))
            .collect(),
        None => candidates
            .iter()
            .filter(|candidate| matches_every_token(candidate, &tokens))
            .collect(),
    };

    matched.sort_by(|a, b| {
        a.group
            .cmp(&b.group)
            .then_with(|| b.item.updated_at.cmp(&a.item.updated_at))
            .then_with(|| a.item.seq.cmp(&b.item.seq))
            .then_with(|| a.item.id.cmp(&b.item.id))
    });

    let total = matched.len();
    let rows = matched
        .into_iter()
        .take(CAP)
        .map(|candidate| Row {
            item: candidate.item.clone(),
            group: candidate.group,
        })
        .collect();

    Outcome { rows, total }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hummingbird_domain::Stage;

    fn item(id: &str, seq: Option<i64>, title: &str) -> Item {
        Item {
            id: id.to_string(),
            seq,
            title: title.to_string(),
            description: None,
            stage: Stage::Ready,
            size: None,
            energy: None,
            context: None,
            priority: 0,
            project_id: None,
            project_pos: None,
            deadline: None,
            scheduled_date: None,
            source: None,
            source_key: None,
            source_url: None,
            archived_at: None,
            agent: false,
            created_at: 1_000,
            updated_at: 1_000,
            version: 1,
        }
    }

    fn candidate(item: Item, group: Group) -> Candidate {
        Candidate {
            item,
            group,
            project_name: None,
        }
    }

    fn ids(outcome: &Outcome) -> Vec<String> {
        outcome.rows.iter().map(|row| row.item.id.clone()).collect()
    }

    // -- tokenization / multi-word AND -----------------------------------

    #[test]
    fn a_multi_word_query_matches_only_items_containing_every_token() {
        let dentist = item("dentist", None, "Call the dentist about the crown");
        let vet = item("vet", None, "Call the vet about the crown");
        let candidates = vec![
            candidate(dentist, Group::Live),
            candidate(vet, Group::Live),
        ];

        let outcome = search(&candidates, "dentist crown");
        assert_eq!(ids(&outcome), vec!["dentist".to_string()]);
    }

    #[test]
    fn matching_is_case_insensitive_on_every_matched_field() {
        let mut described = item("a", None, "Dentist");
        described.description = Some("Ask about the CROWN".to_string());
        let candidates = vec![candidate(described, Group::Live)];

        let outcome = search(&candidates, "dentist crown");
        assert_eq!(ids(&outcome), vec!["a".to_string()]);
    }

    #[test]
    fn a_query_token_matches_the_resolved_project_name_too() {
        let mut in_project = candidate(item("a", None, "Buy stamps"), Group::Live);
        in_project.project_name = Some("House move".to_string());
        let not_in_project = candidate(item("b", None, "Buy stamps"), Group::Live);

        let outcome = search(&[in_project, not_in_project], "move");
        assert_eq!(ids(&outcome), vec!["a".to_string()]);
    }

    #[test]
    fn an_empty_or_whitespace_only_query_matches_nothing() {
        let candidates = vec![candidate(item("a", None, "anything"), Group::Live)];

        assert_eq!(search(&candidates, "").rows.len(), 0);
        assert_eq!(search(&candidates, "   ").rows.len(), 0);
        assert_eq!(search(&candidates, "").total, 0);
    }

    // -- handle lookup ----------------------------------------------------

    #[test]
    fn a_single_handle_shaped_token_matches_seq_exactly_in_every_spelling() {
        let target = item("target", Some(42), "Something unrelated to the query");
        let decoy = item("decoy", Some(4), "Something unrelated to the query");
        let candidates = vec![
            candidate(target, Group::Live),
            candidate(decoy, Group::Live),
        ];

        for spelling in ["hb-42", "HB42", "42", "Hb-42"] {
            let outcome = search(&candidates, spelling);
            assert_eq!(
                ids(&outcome),
                vec!["target".to_string()],
                "spelling {spelling:?} should match seq 42 exactly"
            );
        }
    }

    #[test]
    fn a_handle_query_does_not_fall_back_to_a_substring_search() {
        // "42" appears nowhere in either item's title/description, and
        // neither item's seq is 42 — a handle query names one item or none,
        // never a substring scan of the same token.
        let candidates = vec![candidate(
            item("a", Some(7), "Item number 42 in a list somewhere"),
            Group::Live,
        )];

        let outcome = search(&candidates, "42");
        assert_eq!(outcome.rows.len(), 0);
    }

    #[test]
    fn a_multi_token_query_is_never_treated_as_a_handle_lookup() {
        let mut with_seq = item("has-seq", Some(42), "unrelated");
        with_seq.description = None;
        let mentions_42 = item("mentions", None, "hb-42 was mentioned in this title");
        let candidates = vec![
            candidate(with_seq, Group::Live),
            candidate(mentions_42, Group::Live),
        ];

        // Two tokens: falls through to the ordinary substring AND, so only
        // the item whose *text* contains both tokens matches.
        let outcome = search(&candidates, "hb-42 mentioned");
        assert_eq!(ids(&outcome), vec!["mentions".to_string()]);
    }

    // -- archived inclusion -------------------------------------------------

    #[test]
    fn done_and_archived_items_are_in_the_corpus_and_appear_in_results() {
        let live = item("live", None, "shared word");
        let done = item("done", None, "shared word");
        let archived = item("archived", None, "shared word");
        let candidates = vec![
            candidate(live, Group::Live),
            candidate(done, Group::Done),
            candidate(archived, Group::Archived),
        ];

        let outcome = search(&candidates, "shared");
        let mut got = ids(&outcome);
        got.sort();
        assert_eq!(
            got,
            vec![
                "archived".to_string(),
                "done".to_string(),
                "live".to_string()
            ]
        );
    }

    // -- ordering -----------------------------------------------------------

    #[test]
    fn results_order_live_before_done_before_archived() {
        let mut done = item("done", None, "shared");
        done.updated_at = 9_999; // newest, but must still sort after live
        let mut archived = item("archived", None, "shared");
        archived.updated_at = 9_999;
        let live = item("live", None, "shared");

        let candidates = vec![
            candidate(done, Group::Done),
            candidate(archived, Group::Archived),
            candidate(live, Group::Live),
        ];

        let outcome = search(&candidates, "shared");
        assert_eq!(
            ids(&outcome),
            vec![
                "live".to_string(),
                "done".to_string(),
                "archived".to_string()
            ]
        );
    }

    #[test]
    fn within_a_group_updated_at_orders_most_recently_touched_first() {
        let mut older = item("older", None, "shared");
        older.updated_at = 1_000;
        let mut newer = item("newer", None, "shared");
        newer.updated_at = 2_000;

        let candidates = vec![
            candidate(older, Group::Live),
            candidate(newer, Group::Live),
        ];

        let outcome = search(&candidates, "shared");
        assert_eq!(ids(&outcome), vec!["newer".to_string(), "older".to_string()]);
    }

    #[test]
    fn a_seq_tiebreak_decides_an_exact_updated_at_tie() {
        let mut a = item("z-id", Some(9), "shared");
        a.updated_at = 1_000;
        let mut b = item("a-id", Some(3), "shared");
        b.updated_at = 1_000;

        let candidates = vec![candidate(a, Group::Live), candidate(b, Group::Live)];

        let outcome = search(&candidates, "shared");
        assert_eq!(ids(&outcome), vec!["a-id".to_string(), "z-id".to_string()]);
    }

    // -- cap ------------------------------------------------------------------

    #[test]
    fn at_most_cap_rows_render_and_total_carries_the_real_count() {
        let candidates: Vec<Candidate> = (0..(CAP + 7))
            .map(|n| {
                let mut i = item(&format!("id-{n}"), Some(n as i64), "shared");
                i.updated_at = n as i64;
                candidate(i, Group::Live)
            })
            .collect();

        let outcome = search(&candidates, "shared");
        assert_eq!(outcome.rows.len(), CAP);
        assert_eq!(outcome.total, CAP + 7);
    }

    #[test]
    fn a_query_matching_at_or_under_the_cap_reports_no_more() {
        let candidates: Vec<Candidate> = (0..CAP)
            .map(|n| candidate(item(&format!("id-{n}"), Some(n as i64), "shared"), Group::Live))
            .collect();

        let outcome = search(&candidates, "shared");
        assert_eq!(outcome.rows.len(), CAP);
        assert_eq!(outcome.total, CAP);
    }
}
