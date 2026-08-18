//! **The reachability question** (#141/S9), sunk out of
//! `screens/reachability-pane/reachability.ts` by ADR-0025 (#534).
//!
//! This pane has no `context_snapshots` lane of its own — it reads
//! [`super::inputs::SyncFacts`], the device's own authority-sync history,
//! instead. That makes it the simplest pane in the family: no payload
//! parser (there is no payload), and its only gap is "this device has never
//! synced", read straight off `SyncFacts.last_successful_at_ms`.
//!
//! **What is here is the deciding half.** [`reachability_facts`]'s
//! `age_ms`/`stale`/`latest_attempt_landed` are decisions two clients must
//! never disagree about; the *headline* words ("Synced 1m ago" vs. "Last
//! synced 1m ago") are a per-client rendering built from
//! `latest_attempt_landed` plus a relative-age formatter
//! (`relativeAge` in `sync-status.ts`), and stay per-client on the same
//! reasoning `waste.rs`'s header gives for its own gap sentences.

use serde::Serialize;

use super::contract::{AnswerState, Band, PaneAnswerCore};
use super::inputs::PaneInputs;

/// The one subject this question ever has — there is exactly one sync
/// history per device, never one per something.
pub const SUBJECT_KEY: &str = "reachability";

/// ADR-0007's own sync cadence — one ordinary cycle's worth of quiet before
/// the pane may say anything about a miss.
pub const SYNC_TIMER_MS: i64 = 60_000;

/// The sync engine's own backoff ceiling: the longest a *retrying* cycle may
/// legitimately wait between attempts during an outage.
pub const MAX_SYNC_BACKOFF_MS: i64 = 5 * 60_000;

/// One ordinary cycle may fail before the core enters its maximum backoff.
/// Keep the pane quiet until both that cadence and the whole backoff ceiling
/// have elapsed, so it cannot announce failure before a retry is even
/// eligible — the two constants above sum because the failure and the
/// worst-case retry wait are sequential, not overlapping.
pub const REACHABILITY_GRACE_MS: i64 = SYNC_TIMER_MS + MAX_SYNC_BACKOFF_MS;

/// `SyncOutcomeClass` (`sync-status.ts`) ported to classify a plain wire
/// string rather than a closed TS union — see [`super::inputs::SyncFacts`]'s
/// doc comment for why `latest_outcome_kind` crosses untyped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncOutcomeClass {
    Held,
    Failed,
    NotRun,
    Landed,
}

/// Classifies a `TaskRunOutcomeKind` wire string exactly as
/// `sync-status.ts`'s `OUTCOME_CLASS` table does, plus one arm that table
/// cannot have: a kind this build does not recognise at all.
///
/// `reachability.ts` never has to answer that question — `TaskRunOutcomeKind`
/// is a closed TS union, so every value the web ever sees is already one of
/// the nine rows below. Crossing the wire as a plain string reopens it: a
/// newer server build could send a tenth kind an older client has never
/// heard of. The only place this classification is read is the
/// `latest_attempt_landed` check in [`reachability_facts`], where "landed"
/// is the one answer that lets a stale sync read as freshly reachable — so
/// an unrecognised kind is classified [`SyncOutcomeClass::Failed`] rather
/// than [`SyncOutcomeClass::Landed`]: this device does not know what
/// happened, and *cannot* know it succeeded, so it must not silently read
/// as success. `Failed` over `NotRun` for the same reason `waste.rs` prefers
/// naming a gap's kind over collapsing it into a plainer one — a kind this
/// build has never heard of is itself informative (this device is behind),
/// which `NotRun` would understate.
pub fn sync_outcome_class(kind: &str) -> SyncOutcomeClass {
    match kind {
        "held" | "credential_needed" | "no_credential" => SyncOutcomeClass::Held,
        "pull_failed" | "persist_failed" | "blocked" => SyncOutcomeClass::Failed,
        "skipped" | "busy" => SyncOutcomeClass::NotRun,
        "completed" => SyncOutcomeClass::Landed,
        _ => SyncOutcomeClass::Failed,
    }
}

/// Everything an answered reachability pane needs, decided once. **No
/// rendered sentence crosses** — see the module header.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReachabilityFacts {
    /// Milliseconds since the last successful sync, clamped to never go
    /// negative — a clock-skewed `now_ms` reads as "just now", not as a
    /// sync from the future.
    pub age_ms: i64,
    /// Past the grace window — the fact [`reachability_answer`]'s band
    /// escalation reads.
    pub stale: bool,
    /// Whether the most recent sync attempt is the same one that last
    /// succeeded: `true` exactly when it both landed and was the
    /// informative attempt that produced `last_successful_at_ms`. A client
    /// uses this to choose "Synced" over "Last synced".
    pub latest_attempt_landed: bool,
}

/// The reachability decision, or `None` when this device has never synced
/// at all — mirrors `reachabilityView`. Its only clock is `inputs.now_ms`.
pub fn reachability_facts(inputs: &PaneInputs) -> Option<ReachabilityFacts> {
    let last_successful_at_ms = inputs.sync.last_successful_at_ms?;

    let age_ms = (inputs.now_ms - last_successful_at_ms).max(0);
    let latest_class = inputs.sync.latest_outcome_kind.as_deref().map(sync_outcome_class);
    let latest_attempt_landed = latest_class == Some(SyncOutcomeClass::Landed)
        && inputs.sync.latest_informative_at_ms == Some(last_successful_at_ms);

    Some(ReachabilityFacts { age_ms, stale: age_ms > REACHABILITY_GRACE_MS, latest_attempt_landed })
}

/// This question's answer for the shell — mirrors `reachabilityAnswer`'s
/// decision half, minus its headline and icon.
pub fn reachability_answer(inputs: &PaneInputs) -> PaneAnswerCore {
    let Some(last_successful_at_ms) = inputs.sync.last_successful_at_ms else {
        return PaneAnswerCore {
            answer_state: AnswerState::BoundButUnacquired,
            band: Band::Dormant,
            within_band: None,
        };
    };
    let facts = reachability_facts(inputs).expect("last_successful_at_ms is Some");

    PaneAnswerCore {
        answer_state: AnswerState::Answered,
        band: if facts.stale { Band::Live } else { Band::Dormant },
        within_band: Some(last_successful_at_ms + REACHABILITY_GRACE_MS),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decisions::panes::inputs::SyncFacts;

    const NOW: i64 = 1_700_000_000_000;

    fn inputs(sync: SyncFacts) -> PaneInputs {
        PaneInputs { now_ms: NOW, sync, ..PaneInputs::default() }
    }

    #[test]
    fn keeps_a_fresh_completed_sync_quiet_and_calls_it_landed() {
        let at_ms = NOW - 60_000;
        let answer = reachability_answer(&inputs(SyncFacts {
            latest_outcome_kind: Some("completed".to_string()),
            latest_informative_at_ms: Some(at_ms),
            last_successful_at_ms: Some(at_ms),
        }));
        assert_eq!(answer.answer_state, AnswerState::Answered);
        assert_eq!(answer.band, Band::Dormant);
        assert_eq!(answer.within_band, Some(at_ms + REACHABILITY_GRACE_MS));

        let facts = reachability_facts(&inputs(SyncFacts {
            latest_outcome_kind: Some("completed".to_string()),
            latest_informative_at_ms: Some(at_ms),
            last_successful_at_ms: Some(at_ms),
        }))
        .unwrap();
        assert!(facts.latest_attempt_landed);
        assert!(!facts.stale);
    }

    #[test]
    fn calls_an_earlier_success_not_landed_after_one_newer_missed_cycle() {
        let success_at_ms = NOW - 2 * 60_000;
        let facts = reachability_facts(&inputs(SyncFacts {
            latest_outcome_kind: Some("pull_failed".to_string()),
            latest_informative_at_ms: Some(NOW - 60_000),
            last_successful_at_ms: Some(success_at_ms),
        }))
        .unwrap();
        assert!(!facts.latest_attempt_landed);
        assert!(!facts.stale);
        assert_eq!(facts.age_ms, 2 * 60_000);
    }

    #[test]
    fn keeps_the_maximum_backoff_recovery_window_quiet_without_calling_it_landed() {
        let success_at_ms = NOW - 5 * 60_000 - 30_000;
        let answer = reachability_answer(&inputs(SyncFacts {
            latest_outcome_kind: Some("pull_failed".to_string()),
            latest_informative_at_ms: Some(success_at_ms + 60_000),
            last_successful_at_ms: Some(success_at_ms),
        }));
        assert_eq!(answer.band, Band::Dormant);
    }

    #[test]
    fn includes_the_cadence_plus_backoff_boundary_in_the_quiet_grace() {
        let answer = reachability_answer(&inputs(SyncFacts {
            latest_outcome_kind: None,
            latest_informative_at_ms: None,
            last_successful_at_ms: Some(NOW - REACHABILITY_GRACE_MS),
        }));
        assert_eq!(answer.band, Band::Dormant);
    }

    #[test]
    fn escalates_sustained_failure_after_the_grace_ceiling() {
        let answer = reachability_answer(&inputs(SyncFacts {
            latest_outcome_kind: Some("pull_failed".to_string()),
            latest_informative_at_ms: Some(NOW - 5 * 60_000),
            last_successful_at_ms: Some(NOW - REACHABILITY_GRACE_MS - 1),
        }));
        assert_eq!(answer.answer_state, AnswerState::Answered);
        assert_eq!(answer.band, Band::Live);
    }

    #[test]
    fn renders_a_never_synced_gap() {
        let answer = reachability_answer(&inputs(SyncFacts::default()));
        assert_eq!(answer.answer_state, AnswerState::BoundButUnacquired);
        assert_eq!(answer.band, Band::Dormant);
        assert_eq!(answer.within_band, None);
        assert!(reachability_facts(&inputs(SyncFacts::default())).is_none());
    }

    #[test]
    fn clamps_clock_skew_instead_of_producing_a_negative_age() {
        let facts = reachability_facts(&inputs(SyncFacts {
            latest_outcome_kind: None,
            latest_informative_at_ms: None,
            last_successful_at_ms: Some(NOW + 60_000),
        }))
        .unwrap();
        assert_eq!(facts.age_ms, 0);
    }

    #[test]
    fn never_calls_a_direct_skipped_or_busy_outcome_landed() {
        for kind in ["skipped", "busy"] {
            let at_ms = NOW - 60_000;
            let facts = reachability_facts(&inputs(SyncFacts {
                latest_outcome_kind: Some(kind.to_string()),
                latest_informative_at_ms: Some(at_ms),
                last_successful_at_ms: Some(at_ms),
            }))
            .unwrap();
            assert!(!facts.latest_attempt_landed, "{kind}");
        }
    }

    // -------------------------------------------------- sync_outcome_class

    #[test]
    fn classifies_every_known_outcome_kind_exactly_as_sync_status_ts_does() {
        let cases: [(&str, SyncOutcomeClass); 9] = [
            ("held", SyncOutcomeClass::Held),
            ("credential_needed", SyncOutcomeClass::Held),
            ("no_credential", SyncOutcomeClass::Held),
            ("pull_failed", SyncOutcomeClass::Failed),
            ("persist_failed", SyncOutcomeClass::Failed),
            ("blocked", SyncOutcomeClass::Failed),
            ("skipped", SyncOutcomeClass::NotRun),
            ("busy", SyncOutcomeClass::NotRun),
            ("completed", SyncOutcomeClass::Landed),
        ];
        for (kind, expected) in cases {
            assert_eq!(sync_outcome_class(kind), expected, "{kind}");
        }
    }

    #[test]
    fn an_unrecognised_outcome_kind_is_never_read_as_landed() {
        assert_eq!(sync_outcome_class("some_future_kind"), SyncOutcomeClass::Failed);
        let at_ms = NOW - 60_000;
        let facts = reachability_facts(&inputs(SyncFacts {
            latest_outcome_kind: Some("some_future_kind".to_string()),
            latest_informative_at_ms: Some(at_ms),
            last_successful_at_ms: Some(at_ms),
        }))
        .unwrap();
        assert!(!facts.latest_attempt_landed);
    }
}
