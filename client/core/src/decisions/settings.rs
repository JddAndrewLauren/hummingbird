//! The Settings screen's decision half (#535/M4, ADR-0025): the sync-status
//! readout and the dead-letter heading, sunk out of the web's
//! `shell/sync-status.ts` so a second client (Android) does not carry its
//! own classification of what "stale", "held" and "synced" mean.
//!
//! This is the same carve-out `decisions::urgency` already draws: whether a
//! sync outcome counts as informative, and what a queue depth/staleness
//! reading is worded and coloured as, are facts two clients must agree on —
//! a phone and a browser disagreeing about whether the mirror is "Stale" or
//! "Synced" is exactly the drift ADR-0025 exists to prevent. Everything
//! here is pure and clock-free: `now_ms` arrives as an argument, never
//! sampled.

use serde::{Deserialize, Serialize};

/// One `RunOutcome::kind` wire string, classified into the four disjoint
/// staleness classes `shell/sync-status.ts`'s `OUTCOME_CLASS` table used to
/// hold as a hand-maintained `Record`. Kept as an `&str` match rather than a
/// mirrored enum: the wire kind already has exactly one owner
/// (`Core::run`'s `RunOutcome`), and a second enum here would be a second
/// vocabulary of the same eight strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SyncOutcomeClass {
    /// The cycle did not run because the credential needs attention; never
    /// staled by the passage of time the way a transient network failure
    /// is.
    Held,
    /// The cycle ran but did not land: "stale" rather than "synced" even
    /// though a timestamp exists.
    Failed,
    /// Nothing was attempted at all (`Skipped`'s backoff not being ready,
    /// or a host wedged mid-cycle). Carries NO information about how stale
    /// the mirror is — see [`is_informative_sync_outcome`].
    NotRun,
    /// The cycle completed.
    Landed,
}

/// Classifies one `RunOutcome::kind` wire string. An unrecognised kind
/// (a future build's new outcome, read by an older one) classifies as
/// [`SyncOutcomeClass::Failed`] — the same "never silently green" instinct
/// [`sync_status_tone`] applies below: an outcome this build cannot name is
/// never worth calling a success.
pub fn sync_outcome_class(kind: &str) -> SyncOutcomeClass {
    match kind {
        "held" | "credential_needed" | "no_credential" => SyncOutcomeClass::Held,
        "pull_failed" | "persist_failed" | "blocked" => SyncOutcomeClass::Failed,
        "skipped" | "busy" => SyncOutcomeClass::NotRun,
        "completed" => SyncOutcomeClass::Landed,
        _ => SyncOutcomeClass::Failed,
    }
}

/// Whether an outcome says anything about how stale the mirror is. A
/// `NotRun` outcome does not: no request was made, so the last real
/// outcome — and the timestamp that goes with it — is still the truth. A
/// caller filters on this before overwriting its own last-outcome/
/// last-synced-at state, which is what keeps a "Stale" badge stale across
/// an outage's worth of backed-off ticks.
pub fn is_informative_sync_outcome(kind: &str) -> bool {
    sync_outcome_class(kind) != SyncOutcomeClass::NotRun
}

/// A short relative age in the product's own register — `just now`,
/// `12m ago`, `3h ago`, `2d ago` (the design system's "Numbers and time"
/// rule). `age_ms` is caller-computed (`now_ms - last_sync_at_ms`), never
/// negative by construction — a negative input floors to `0`.
pub fn relative_age(age_ms: i64) -> String {
    let age_ms = age_ms.max(0);
    let minutes = age_ms / 60_000;
    if minutes < 1 {
        return "just now".to_string();
    }
    if minutes < 60 {
        return format!("{minutes}m ago");
    }
    let hours = minutes / 60;
    if hours < 24 {
        return format!("{hours}h ago");
    }
    let days = hours / 24;
    format!("{days}d ago")
}

/// The queued-count suffix, present only when there is something queued —
/// a "0 queued" pill is decoration.
fn queued_suffix(queue_depth: Option<u32>) -> String {
    match queue_depth {
        Some(depth) if depth > 0 => format!(" · {depth} queued"),
        _ => String::new(),
    }
}

/// Everything [`sync_status_tone`]/[`sync_status_label`]/
/// [`sync_status_tone_word`] need — the sync card's whole input, gathered
/// so the three functions can never be called against three different
/// snapshots of it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatusInput {
    pub online: bool,
    /// The last completed cycle's outcome kind, any trigger — `None`
    /// before the first one this session. The wire string
    /// [`sync_outcome_class`] classifies, never re-parsed here.
    pub last_sync_outcome_kind: Option<String>,
    /// When the last cycle happened — `None` before the first one this
    /// session.
    pub last_sync_at_ms: Option<i64>,
    pub queue_depth: Option<u32>,
    pub now_ms: i64,
}

fn class_of(input: &SyncStatusInput) -> Option<SyncOutcomeClass> {
    input
        .last_sync_outcome_kind
        .as_deref()
        .map(sync_outcome_class)
}

/// The badge tone the sync card renders in — matches the design system's
/// status colours, kept as a pure decision alongside the label rather than
/// re-derived by parsing the string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SyncStatusTone {
    Neutral,
    Warn,
    Danger,
    Success,
}

pub fn sync_status_tone(input: &SyncStatusInput) -> SyncStatusTone {
    if !input.online {
        return SyncStatusTone::Neutral;
    }
    let outcome = class_of(input);
    if outcome == Some(SyncOutcomeClass::Held) {
        return SyncStatusTone::Warn;
    }
    if input.last_sync_at_ms.is_none() {
        return SyncStatusTone::Neutral;
    }
    match outcome {
        Some(SyncOutcomeClass::Failed) | Some(SyncOutcomeClass::NotRun) => SyncStatusTone::Danger,
        _ => SyncStatusTone::Success,
    }
}

pub fn sync_status_label(input: &SyncStatusInput) -> String {
    if !input.online {
        return format!("Offline{}", queued_suffix(input.queue_depth));
    }
    let outcome = class_of(input);
    if outcome == Some(SyncOutcomeClass::Held) {
        return format!(
            "Held — device token needed{}",
            queued_suffix(input.queue_depth)
        );
    }
    let Some(last_sync_at_ms) = input.last_sync_at_ms else {
        return format!("Not yet synced{}", queued_suffix(input.queue_depth));
    };
    let age = relative_age((input.now_ms - last_sync_at_ms).max(0));
    let state = match outcome {
        Some(SyncOutcomeClass::Failed) | Some(SyncOutcomeClass::NotRun) => "Stale",
        _ => "Synced",
    };
    format!(
        "{state} — as of {age}{}",
        queued_suffix(input.queue_depth)
    )
}

/// The short word the sync card badges its tone with — computed from the
/// same branches as [`sync_status_tone`]/[`sync_status_label`] so a badge
/// and its label can never disagree about which state they describe.
pub fn sync_status_tone_word(input: &SyncStatusInput) -> String {
    if !input.online {
        return "offline".to_string();
    }
    let outcome = class_of(input);
    if outcome == Some(SyncOutcomeClass::Held) {
        return "held".to_string();
    }
    if input.last_sync_at_ms.is_none() {
        return "not synced".to_string();
    }
    match outcome {
        Some(SyncOutcomeClass::Failed) | Some(SyncOutcomeClass::NotRun) => "stale".to_string(),
        _ => "synced".to_string(),
    }
}

/// The dead-letter affordance's heading — pluralised off the real count.
pub fn dead_letter_heading(count: u32) -> String {
    format!("{count} edit{} didn't apply", if count == 1 { "" } else { "s" })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(
        online: bool,
        kind: Option<&str>,
        last_sync_at_ms: Option<i64>,
        queue_depth: Option<u32>,
        now_ms: i64,
    ) -> SyncStatusInput {
        SyncStatusInput {
            online,
            last_sync_outcome_kind: kind.map(str::to_string),
            last_sync_at_ms,
            queue_depth,
            now_ms,
        }
    }

    #[test]
    fn every_wire_kind_classifies_into_exactly_one_of_the_four_classes() {
        assert_eq!(sync_outcome_class("held"), SyncOutcomeClass::Held);
        assert_eq!(
            sync_outcome_class("credential_needed"),
            SyncOutcomeClass::Held
        );
        assert_eq!(sync_outcome_class("no_credential"), SyncOutcomeClass::Held);
        assert_eq!(sync_outcome_class("pull_failed"), SyncOutcomeClass::Failed);
        assert_eq!(
            sync_outcome_class("persist_failed"),
            SyncOutcomeClass::Failed
        );
        assert_eq!(sync_outcome_class("blocked"), SyncOutcomeClass::Failed);
        assert_eq!(sync_outcome_class("skipped"), SyncOutcomeClass::NotRun);
        assert_eq!(sync_outcome_class("busy"), SyncOutcomeClass::NotRun);
        assert_eq!(sync_outcome_class("completed"), SyncOutcomeClass::Landed);
    }

    #[test]
    fn a_not_run_outcome_is_the_only_uninformative_one() {
        assert!(!is_informative_sync_outcome("skipped"));
        assert!(!is_informative_sync_outcome("busy"));
        for kind in ["held", "credential_needed", "no_credential", "pull_failed",
            "persist_failed", "blocked", "completed"]
        {
            assert!(is_informative_sync_outcome(kind), "{kind}");
        }
    }

    #[test]
    fn relative_age_reads_in_the_products_own_register() {
        assert_eq!(relative_age(0), "just now");
        assert_eq!(relative_age(59_999), "just now");
        assert_eq!(relative_age(60_000), "1m ago");
        assert_eq!(relative_age(59 * 60_000), "59m ago");
        assert_eq!(relative_age(60 * 60_000), "1h ago");
        assert_eq!(relative_age(23 * 60 * 60_000), "23h ago");
        assert_eq!(relative_age(24 * 60 * 60_000), "1d ago");
        assert_eq!(relative_age(-1), "just now");
    }

    #[test]
    fn offline_wins_over_every_other_reading() {
        let offline = input(false, Some("completed"), Some(0), Some(3), 1_000);
        assert_eq!(sync_status_tone(&offline), SyncStatusTone::Neutral);
        assert_eq!(sync_status_tone_word(&offline), "offline");
        assert_eq!(sync_status_label(&offline), "Offline · 3 queued");
    }

    #[test]
    fn held_outcomes_read_as_held_whichever_of_the_three_kinds() {
        for kind in ["held", "credential_needed", "no_credential"] {
            let held = input(true, Some(kind), Some(0), None, 1_000);
            assert_eq!(sync_status_tone(&held), SyncStatusTone::Warn, "{kind}");
            assert_eq!(sync_status_tone_word(&held), "held", "{kind}");
            assert_eq!(
                sync_status_label(&held),
                "Held — device token needed",
                "{kind}"
            );
        }
    }

    #[test]
    fn never_synced_reads_as_not_yet_synced_not_stale() {
        let never = input(true, None, None, None, 1_000);
        assert_eq!(sync_status_tone(&never), SyncStatusTone::Neutral);
        assert_eq!(sync_status_tone_word(&never), "not synced");
        assert_eq!(sync_status_label(&never), "Not yet synced");
    }

    #[test]
    fn a_landed_outcome_reads_as_synced_with_its_age() {
        let landed = input(true, Some("completed"), Some(0), Some(0), 60_000);
        assert_eq!(sync_status_tone(&landed), SyncStatusTone::Success);
        assert_eq!(sync_status_tone_word(&landed), "synced");
        assert_eq!(sync_status_label(&landed), "Synced — as of 1m ago");
    }

    #[test]
    fn a_failed_or_not_run_outcome_reads_as_stale_never_a_silent_success() {
        for kind in ["pull_failed", "persist_failed", "blocked", "skipped", "busy"] {
            let stale = input(true, Some(kind), Some(0), None, 60_000);
            assert_eq!(sync_status_tone(&stale), SyncStatusTone::Danger, "{kind}");
            assert_eq!(sync_status_tone_word(&stale), "stale", "{kind}");
            assert_eq!(sync_status_label(&stale), "Stale — as of 1m ago", "{kind}");
        }
    }

    #[test]
    fn a_queued_count_appends_only_when_positive() {
        let none_queued = input(true, Some("completed"), Some(0), Some(0), 0);
        assert!(!sync_status_label(&none_queued).contains("queued"));
        let some_queued = input(true, Some("completed"), Some(0), Some(2), 0);
        assert!(sync_status_label(&some_queued).ends_with("2 queued"));
    }

    #[test]
    fn the_dead_letter_heading_pluralises_off_the_real_count() {
        assert_eq!(dead_letter_heading(0), "0 edits didn't apply");
        assert_eq!(dead_letter_heading(1), "1 edit didn't apply");
        assert_eq!(dead_letter_heading(2), "2 edits didn't apply");
    }
}
