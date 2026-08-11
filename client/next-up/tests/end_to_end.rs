//! The one end-to-end fixture: a real `GET /api/sweep` shape in, ranked
//! JSON out. `tests/fixtures/sweep.json` is also the file
//! `HB_SWEEP_FIXTURE` points at, so exercising the skill with no
//! credential and running this test read the same payload.

use hummingbird_core::rank::ReasonCode;
use hummingbird_next_up::{run, Envelope};

fn envelope(axes: &str, calendar: &str) -> Envelope {
    let sweep = include_str!("fixtures/sweep.json");
    let raw = format!(
        r#"{{"sweep":{sweep},"axes":{axes},
            "now":{{"local":"2026-08-11T09:53","epoch_ms":1786553580000}}{calendar}}}"#
    );
    serde_json::from_str(&raw).expect("the fixture envelope parses")
}

fn ids(output: &hummingbird_next_up::Output) -> Vec<&str> {
    output
        .candidates
        .iter()
        .map(|c| c.item.id.as_str())
        .collect()
}

#[test]
fn the_fixture_sweep_ranks_into_the_frontier_the_selection_rules_leave_standing() {
    let output = run(&envelope("{}", "")).expect("runs");

    // Present: the overdue item, the in-progress one, a plain Ready item
    // whose only blocker is Done, and a Triage item due inside the week.
    // Absent: the undated Triage capture, the `Blocked`-stage item, the
    // archived one, the Done one, and the item held by a live blocker.
    assert_eq!(
        ids(&output),
        ["i-overdue", "i-wip", "i-ready-quick", "i-triage-soon"]
    );
}

#[test]
fn the_winning_candidate_carries_the_reason_codes_the_why_line_cites() {
    let output = run(&envelope("{}", "")).expect("runs");
    let winner = &output.candidates[0];
    assert_eq!(winner.item.id, "i-overdue");
    assert!(winner.reasons.contains(&ReasonCode::Overdue));
    assert!(winner.reasons.contains(&ReasonCode::Priority(2)));
    // Step 6 rides on every candidate — never on its own a claim that age
    // was decisive (see `ReasonCode::OldestFirst`).
    assert!(winner.reasons.contains(&ReasonCode::OldestFirst));
}

#[test]
fn the_health_block_carries_the_footers_facts_and_the_fog_questions_verbatim() {
    let output = run(&envelope("{}", "")).expect("runs");
    assert_eq!(output.health.triage, 2);
    assert_eq!(output.health.grilling, 0);
    assert_eq!(output.health.blocked_dropped, 1);
    assert_eq!(output.health.fog_exhausted.len(), 1);
    let flagged = &output.health.fog_exhausted[0];
    assert_eq!(flagged.project, "Update Acumatica");
    // Only the open row, and it arrives unedited — the reading is the
    // skill's.
    assert_eq!(
        flagged.questions,
        ["Which licence tier are we actually on?"]
    );
}

#[test]
fn a_declared_context_hard_filters_while_untagged_items_survive() {
    let output = run(&envelope(r#"{"context":"calls"}"#, "")).expect("runs");
    // `@computer`'s overdue item is gone; the untagged in-progress item and
    // the untagged Triage capture survive alongside the `@calls` one.
    assert_eq!(ids(&output), ["i-wip", "i-ready-quick", "i-triage-soon"]);
}

/// The 30-minute nudge, end to end through the owned→borrowed calendar
/// adapter: an in-progress event masks the next start, so it is read off
/// `today` — the exact case `/next-up-personal`'s masking example names.
#[test]
fn an_in_progress_event_still_lets_the_quick_nudge_fire_off_today() {
    let calendar = r#","calendar":{
        "current_or_next":{"status":"in_progress","event":{
            "provider_event_id":"standup","calendar_id":"primary","title":"Standup",
            "when":{"kind":"timed","start_ms":1786551000000,"end_ms":1786554600000},
            "recurrence_id":null,"location":null,"organizer":null,
            "status":"confirmed","provider_updated_at_ms":0,"html_link":null}},
        "today":[{
            "provider_event_id":"review","calendar_id":"primary","title":"Review",
            "when":{"kind":"timed","start_ms":1786554600000,"end_ms":1786558200000},
            "recurrence_id":null,"location":null,"organizer":null,
            "status":"confirmed","provider_updated_at_ms":0,"html_link":null}]}"#;
    let output = run(&envelope("{}", calendar)).expect("runs");
    let quick: Vec<&str> = output
        .candidates
        .iter()
        .filter(|c| c.reasons.contains(&ReasonCode::QuickBeforeNextStart))
        .map(|c| c.item.id.as_str())
        .collect();
    assert_eq!(quick, ["i-wip", "i-ready-quick"]);
}

#[test]
fn a_repeat_run_of_the_same_envelope_is_byte_identical() {
    let first =
        serde_json::to_string(&run(&envelope("{}", "")).expect("runs")).expect("serializes");
    let second =
        serde_json::to_string(&run(&envelope("{}", "")).expect("runs")).expect("serializes");
    assert_eq!(first, second);
}

/// The other half of the same wire contract: an all-day event on the
/// calendar block. It rides the `all_day` arm — civil dates, no instants,
/// no zone — and it can never fire the nudge, because there is no moment to
/// be thirty minutes before (ADR-0015's 2026-08-10 amendment).
#[test]
fn an_all_day_event_on_the_wire_parses_and_fires_no_nudge() {
    let calendar = r#","calendar":{
        "current_or_next":{"status":"in_progress","event":{
            "provider_event_id":"conference","calendar_id":"primary","title":"Conference",
            "when":{"kind":"all_day","start_date":"2026-08-11","end_date":"2026-08-13"},
            "recurrence_id":null,"location":null,"organizer":null,
            "status":"confirmed","provider_updated_at_ms":0,"html_link":null}},
        "today":[]}"#;
    let output = run(&envelope("{}", calendar)).expect("runs");
    assert!(!output
        .candidates
        .iter()
        .any(|c| c.reasons.contains(&ReasonCode::QuickBeforeNextStart)));
}
