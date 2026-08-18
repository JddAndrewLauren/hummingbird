//! Backtesting a draft rule against the items a client already mirrors
//! (ADR-0011: "re-fetch recent history from the source and show which
//! events a draft rule *would have* promoted... needs no persistence... the
//! match count is shown before save"). Sunk from
//! `client/web/src/screens/rules/backtest.ts` — **the other of ADR-0025's
//! two named M4 drifts** (`rules/backtest.ts:52`), where the day-only →
//! `T23:59` resolution and the duration arithmetic were re-derived in TS.
//!
//! **Which delegation arm this took, and why.** The stronger retirement
//! would have been to call [`hummingbird_rules_engine::evaluate_rule`]
//! wholesale. It cannot be: that function takes a *single* `now`, and this
//! evaluation needs two frames (below). The web's existing pins decided it
//! rather than taste — `backtest.test.ts:125-138` reads a `deadline` while
//! `:66-78` reads `occurred_at`, against one clock, and no single `now`
//! satisfies both off a non-UTC device. So this is core-side evaluation
//! built from the engine's own [`Operator`] plus
//! [`hummingbird_domain::deadline`]'s `parse_duration`/`shift`/
//! `deadline_sort_key`: every primitive still has exactly one owner, which
//! is what the drift criterion asks for, but the two-frame assembly lives
//! here.
//!
//! **The two frames, stated once.** `deadline` and `scheduled_date` are
//! **device-local** civil strings; `occurred_at` is **UTC** (the authority
//! derives it with `now_as_deadline`, whose own doc says "the Worker runs
//! UTC. Not per-rule, not per-device"). This crate holds no tzdb
//! (`client/core/Cargo.toml` says why), so the frames are *named at the
//! boundary* rather than inferred: [`BacktestClock`] carries both readings
//! of the same instant, and each field picks the one it belongs to. The
//! host does the one epoch → civil conversion it is allowed to, at its own
//! edge, exactly as `computeUrgency` already does.
//!
//! **One deliberate fidelity change from the retired TS.** String
//! comparison is case-insensitive here, ADR-0013's own rule ("all string
//! comparison is case-insensitive") and what
//! `hummingbird_rules_engine::eval` actually does at fire time; the TS used
//! `===`/`String.includes`. A backtest that disagrees with the sweep about
//! `stage eq "Ready"` is reporting a count the sweep would not produce,
//! which is the one thing this panel must never do.
//!
//! **Two honest gaps against `authority::sweep::item_threshold_event`,
//! neither hidden by the count this reports** — carried over verbatim from
//! the retired module, because that half of its header was accurate:
//!
//! 1. **The corpus.** `sweep_tick` evaluates every non-archived item
//!    (`load_live_items`); this backtest only ever sees the items the
//!    caller passes, which on the web is `task.frontier` —
//!    `Ready`/`InProgress`, unarchived, *and* unblocked (`Core::frontier`).
//!    Triage-stage and blocked items are outside the count, and the UI copy
//!    names that rather than presenting a bare "N matches" a reader could
//!    mistake for the sweep's own answer.
//! 2. **[`BacktestItem`] cannot synthesize every core field with full
//!    fidelity.** `occurred_at` is derived from the item's `updated_at` the
//!    same way the server derives it (`now_as_deadline`), and
//!    `calendar_busy` is always `false` here because the server's own
//!    synthesis always writes `None` for it (`item_threshold_event` never
//!    sets it — `Event::core_value` resolves that to `false`), so both are
//!    exact rather than approximate. Every other field offered here is a
//!    direct 1:1 copy of what `item_threshold_event` sets.

use hummingbird_domain::{
    deadline_sort_key, is_valid_deadline, shift, Condition, DurationUnit, FieldValue,
};
use hummingbird_rules_engine::Operator;

/// The one instant, in both frames a backtest reads (see the header).
/// Both are deadline-shaped (`YYYY-MM-DDTHH:MM`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BacktestClock {
    /// This device's own wall clock — the frame `deadline` and
    /// `scheduled_date` are stored in.
    pub now_local: String,
    /// The same instant in UTC — the frame `occurred_at` is stamped in.
    pub now_utc: String,
}

/// One mirrored item, as the `item_threshold` event the sweep would have
/// synthesized from it. `occurred_at_utc` is already resolved by the host
/// (`now_as_deadline(item.updated_at)`) for the tzdb reason in the header.
#[derive(Debug, Clone, PartialEq)]
pub struct BacktestItem {
    pub id: String,
    pub occurred_at_utc: String,
    pub title: String,
    pub body: Option<String>,
    pub url: Option<String>,
    pub deadline: Option<String>,
    pub scheduled_date: Option<String>,
    pub stage: String,
    pub size: Option<String>,
    pub energy: Option<String>,
    pub context: Option<String>,
    pub priority: i64,
    pub project_id: Option<String>,
    pub source: Option<String>,
    pub source_key: Option<String>,
}

/// Why a draft rule cannot be backtested here at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BacktestUnavailable {
    /// The rule targets a kind this client holds no raw material for.
    NoLocalHistory,
}

impl BacktestUnavailable {
    pub fn as_str(self) -> &'static str {
        match self {
            BacktestUnavailable::NoLocalHistory => "no_local_history",
        }
    }
}

/// What a backtest can say: either it could not run, or these are the ids
/// that matched, in the order given.
#[derive(Debug, Clone, PartialEq)]
pub enum BacktestOutcome {
    Unavailable { reason: BacktestUnavailable },
    Ok { matched_ids: Vec<String> },
}

/// The one kind a client holds raw material for.
const BACKTESTABLE_KIND: &str = "item_threshold";

/// Runs a draft rule's conditions (ANDed) against every mirrored item, as
/// an `item_threshold` event.
///
/// A rule targeting any other kind (`email`, `calendar_event`,
/// `snapshot_change`, `alert_raised`) has no client-side event history and
/// reports [`BacktestOutcome::Unavailable`] rather than silently answering
/// zero matches for a different reason. `event_kind: None` ("any kind") is
/// still evaluated — `item_threshold_event` synthesizes it just like every
/// other source, and a rule matching "any kind" matches this one too — so
/// it is `Ok`, the same reading `sweep_tick` itself gives a null
/// `event_kind`.
///
/// Deliberately writes nothing and calls nothing: ADR-0011's "needs no
/// persistence" in full.
pub fn backtest(
    event_kind: Option<&str>,
    conditions: &[Condition],
    items: &[BacktestItem],
    clock: &BacktestClock,
) -> BacktestOutcome {
    if event_kind.is_some_and(|kind| kind != BACKTESTABLE_KIND) {
        return BacktestOutcome::Unavailable {
            reason: BacktestUnavailable::NoLocalHistory,
        };
    }
    let matched_ids = items
        .iter()
        .filter(|item| {
            conditions
                .iter()
                .all(|condition| matches_condition(item, condition, clock))
        })
        .map(|item| item.id.clone())
        .collect();
    BacktestOutcome::Ok { matched_ids }
}

/// One condition against one item's synthesized fields. A **missing field
/// makes the condition false, even negated** — ADR-0013's evaluation rule
/// 2, and the one place a negation must not invert.
fn matches_condition(item: &BacktestItem, condition: &Condition, clock: &BacktestClock) -> bool {
    let Some(value) = field_value(item, &condition.field) else {
        return false;
    };
    let Some(op) = Operator::parse(&condition.op) else {
        return false;
    };
    let matched = matches_operator(&condition.field, op, &value, &condition.value, clock);
    matched != condition.negate
}

/// The `item_threshold` fields this build can synthesize from one mirrored
/// item — the exact field set `authority::sweep::item_threshold_event`
/// populates, so a match here agrees with what the sweep would evaluate.
/// `project` is the raw project id, never a resolved name: the same value
/// the server-side synthesis carries.
fn field_value(item: &BacktestItem, field: &str) -> Option<FieldValue> {
    let str_value = |s: &str| Some(FieldValue::Str(s.to_string()));
    match field {
        "source" => str_value(item.source.as_deref().unwrap_or_default()),
        "source_key" => str_value(item.source_key.as_deref().unwrap_or_default()),
        "occurred_at" => str_value(&item.occurred_at_utc),
        "title" => str_value(&item.title),
        "body" => item.body.as_deref().and_then(str_value),
        "url" => item.url.as_deref().and_then(str_value),
        // The server's synthesis never sets `calendar_busy` at all;
        // `Event::core_value` resolves that to `false`, ADR-0013's one
        // "missing is false" field, so matching it exactly means always
        // answering `false` here too.
        "calendar_busy" => Some(FieldValue::Bool(false)),
        "deadline" => item.deadline.as_deref().and_then(str_value),
        "scheduled_date" => item.scheduled_date.as_deref().and_then(str_value),
        "stage" => str_value(&item.stage),
        "size" => item.size.as_deref().and_then(str_value),
        "energy" => item.energy.as_deref().and_then(str_value),
        "context" => item.context.as_deref().and_then(str_value),
        "priority" => Some(FieldValue::Num(item.priority as f64)),
        "project" => item.project_id.as_deref().and_then(str_value),
        _ => None,
    }
}

fn matches_operator(
    field: &str,
    op: Operator,
    value: &FieldValue,
    condition_value: &serde_json::Value,
    clock: &BacktestClock,
) -> bool {
    match op {
        Operator::Eq | Operator::Contains => string_match(op, value, condition_value),
        Operator::Gt | Operator::Lt => numeric_match(op, value, condition_value),
        Operator::Is => bool_match(value, condition_value),
        Operator::WithinNext | Operator::WithinLast => {
            time_match(op, field, value, condition_value, clock)
        }
    }
}

/// A condition's `value` is a scalar or a list — a list means any-of
/// (ADR-0013). Both shapes normalize to a list of JSON values, exactly as
/// `hummingbird_rules_engine::eval` does.
fn literals(condition_value: &serde_json::Value) -> Vec<&serde_json::Value> {
    match condition_value.as_array() {
        Some(array) => array.iter().collect(),
        None => vec![condition_value],
    }
}

fn string_match(op: Operator, value: &FieldValue, condition_value: &serde_json::Value) -> bool {
    let field_strs: Vec<&str> = match value {
        FieldValue::Str(s) => vec![s.as_str()],
        FieldValue::StrList(list) => list.iter().map(String::as_str).collect(),
        _ => return false,
    };
    let cond_strs: Vec<&str> = literals(condition_value)
        .into_iter()
        .filter_map(|v| v.as_str())
        .collect();
    field_strs.iter().any(|field_str| {
        cond_strs.iter().any(|cond_str| match op {
            Operator::Eq => field_str.to_lowercase() == cond_str.to_lowercase(),
            Operator::Contains => field_str.to_lowercase().contains(&cond_str.to_lowercase()),
            _ => false,
        })
    })
}

fn numeric_match(op: Operator, value: &FieldValue, condition_value: &serde_json::Value) -> bool {
    let FieldValue::Num(field_num) = value else {
        return false;
    };
    literals(condition_value)
        .into_iter()
        .filter_map(|v| v.as_f64())
        .any(|cond_num| match op {
            Operator::Gt => *field_num > cond_num,
            Operator::Lt => *field_num < cond_num,
            _ => false,
        })
}

fn bool_match(value: &FieldValue, condition_value: &serde_json::Value) -> bool {
    let FieldValue::Bool(field_bool) = value else {
        return false;
    };
    literals(condition_value)
        .into_iter()
        .filter_map(|v| v.as_bool())
        .any(|cond_bool| cond_bool == *field_bool)
}

/// `within_next D` means `t <= now + D` — unbounded on the past side, so an
/// overdue item still matches (ADR-0013, deliberately); `within_last D` is
/// symmetric on the other side. The comparison stays string-based
/// throughout, reusing [`deadline_sort_key`] on both sides so a day-only
/// value means "the end of that day" here exactly as it does in the sort
/// and in the fire-time evaluator.
///
/// A field value that is not [`is_valid_deadline`]-shaped never matches:
/// resolving the frame of a value carrying its own zone designator is the
/// host's job at its own edge (see the header), and a lexicographic
/// comparison against a shape this crate cannot read would be an answer,
/// not a refusal.
///
/// `scheduled_date` is the exception, and the reason this dispatches on the
/// field name a second time: `EVENT_KINDS` types it `date`, not `timestamp`,
/// and `eval`'s `FieldType::Date` arm compares it day to day and accepts `d`
/// units only. Resolving it to `T23:59` like a deadline and comparing against
/// a cutoff that carries `now`'s time of day drops the boundary day
/// wholesale — `within_next 3d` at noon excludes the date exactly three days
/// out, which is the day the sweep would have fired on.
fn time_match(
    op: Operator,
    field: &str,
    value: &FieldValue,
    condition_value: &serde_json::Value,
    clock: &BacktestClock,
) -> bool {
    let FieldValue::Str(field_str) = value else {
        return false;
    };
    if !is_valid_deadline(field_str) {
        return false;
    }
    let now = if field == "occurred_at" {
        &clock.now_utc
    } else {
        &clock.now_local
    };
    let resolved = deadline_sort_key(field_str);
    literals(condition_value)
        .into_iter()
        .filter_map(|v| v.as_str())
        .any(|duration| {
            let Some((amount, unit)) = hummingbird_domain::parse_duration(duration.trim()) else {
                return false;
            };
            if amount <= 0 {
                return false;
            }
            let signed = match op {
                Operator::WithinNext => amount,
                Operator::WithinLast => -amount,
                _ => return false,
            };
            if field == "scheduled_date" {
                if unit != DurationUnit::Days || field_str.len() != 10 {
                    return false;
                }
                let Some(now_date) = now.get(..10) else {
                    return false;
                };
                let Some(cutoff) = shift(now_date, signed, unit) else {
                    return false;
                };
                return match op {
                    Operator::WithinNext => field_str.as_str() <= cutoff.as_str(),
                    Operator::WithinLast => field_str.as_str() >= cutoff.as_str(),
                    _ => false,
                };
            }
            let Some(raw_cutoff) = shift(now, signed, unit) else {
                return false;
            };
            let cutoff = deadline_sort_key(&raw_cutoff);
            match op {
                Operator::WithinNext => resolved.as_ref() <= cutoff.as_ref(),
                Operator::WithinLast => resolved.as_ref() >= cutoff.as_ref(),
                _ => false,
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The web's own fixture instant, in both frames: `2026-08-15T12:00Z`
    /// read by a device that happens to be on UTC.
    fn clock() -> BacktestClock {
        BacktestClock {
            now_local: "2026-08-15T12:00".to_string(),
            now_utc: "2026-08-15T12:00".to_string(),
        }
    }

    fn item(id: &str) -> BacktestItem {
        BacktestItem {
            id: id.to_string(),
            occurred_at_utc: "2026-08-15T11:00".to_string(),
            title: "buy milk".to_string(),
            body: None,
            url: None,
            deadline: None,
            scheduled_date: None,
            stage: "ready".to_string(),
            size: None,
            energy: None,
            context: None,
            priority: 0,
            project_id: None,
            source: None,
            source_key: None,
        }
    }

    fn condition(field: &str, op: &str, value: serde_json::Value, negate: bool) -> Condition {
        Condition {
            field: field.to_string(),
            op: op.to_string(),
            value,
            negate,
        }
    }

    fn matched(outcome: &BacktestOutcome) -> Vec<String> {
        match outcome {
            BacktestOutcome::Ok { matched_ids } => matched_ids.clone(),
            BacktestOutcome::Unavailable { .. } => panic!("expected an available backtest"),
        }
    }

    #[test]
    fn reports_unavailable_for_a_kind_other_than_item_threshold() {
        assert_eq!(
            backtest(Some("email"), &[], &[item("i-1")], &clock()),
            BacktestOutcome::Unavailable {
                reason: BacktestUnavailable::NoLocalHistory,
            },
        );
    }

    #[test]
    fn any_kind_is_available_because_item_threshold_still_satisfies_it() {
        let mut matching = item("i-1");
        matching.title = "renew passport".to_string();
        let outcome = backtest(
            None,
            &[condition("title", "contains", serde_json::json!("passport"), false)],
            &[matching],
            &clock(),
        );
        assert_eq!(matched(&outcome), ["i-1"]);
    }

    #[test]
    fn calendar_busy_is_always_false_matching_the_servers_own_synthesis() {
        let items = [item("i-1")];
        let busy = backtest(
            Some("item_threshold"),
            &[condition("calendar_busy", "is", serde_json::json!(true), false)],
            &items,
            &clock(),
        );
        let not_busy = backtest(
            Some("item_threshold"),
            &[condition("calendar_busy", "is", serde_json::json!(false), false)],
            &items,
            &clock(),
        );
        assert!(matched(&busy).is_empty());
        assert_eq!(matched(&not_busy), ["i-1"]);
    }

    #[test]
    fn occurred_at_is_read_against_the_utc_frame_not_the_local_one() {
        // A device eight hours behind UTC: the same instant reads
        // 04:00 locally. `occurred_at` must still resolve against 12:00.
        let clock = BacktestClock {
            now_local: "2026-08-15T04:00".to_string(),
            now_utc: "2026-08-15T12:00".to_string(),
        };
        let outcome = backtest(
            Some("item_threshold"),
            &[condition("occurred_at", "within_last", serde_json::json!("2h"), false)],
            &[item("i-1")],
            &clock,
        );
        assert_eq!(matched(&outcome), ["i-1"]);
    }

    #[test]
    fn deadline_is_read_against_the_local_frame_not_the_utc_one() {
        let clock = BacktestClock {
            now_local: "2026-08-15T04:00".to_string(),
            now_utc: "2026-08-15T12:00".to_string(),
        };
        let mut soon = item("i-1");
        soon.deadline = Some("2026-08-15T05:00".to_string());
        let outcome = backtest(
            Some("item_threshold"),
            &[condition("deadline", "within_next", serde_json::json!("2h"), false)],
            &[soon],
            &clock,
        );
        assert_eq!(matched(&outcome), ["i-1"]);
    }

    #[test]
    fn ands_multiple_conditions() {
        let mut matching = item("i-1");
        matching.title = "renew passport".to_string();
        let mut wrong_stage = item("i-2");
        wrong_stage.title = "renew passport".to_string();
        wrong_stage.stage = "blocked".to_string();
        let outcome = backtest(
            Some("item_threshold"),
            &[
                condition("title", "contains", serde_json::json!("passport"), false),
                condition("stage", "eq", serde_json::json!("ready"), false),
            ],
            &[matching, wrong_stage],
            &clock(),
        );
        assert_eq!(matched(&outcome), ["i-1"]);
    }

    #[test]
    fn a_missing_field_is_false_even_negated() {
        let mut with_project = item("i-1");
        with_project.project_id = Some("p-1".to_string());
        let without_project = item("i-2");
        let outcome = backtest(
            Some("item_threshold"),
            &[condition("project", "eq", serde_json::json!("p-1"), true)],
            &[with_project, without_project],
            &clock(),
        );
        assert!(matched(&outcome).is_empty());
    }

    #[test]
    fn within_next_is_unbounded_on_the_past_side() {
        let mut overdue = item("i-1");
        overdue.deadline = Some("2026-08-10T09:00".to_string());
        let mut soon = item("i-2");
        soon.deadline = Some("2026-08-15T13:00".to_string());
        let mut later = item("i-3");
        later.deadline = Some("2026-08-20T09:00".to_string());
        let outcome = backtest(
            Some("item_threshold"),
            &[condition("deadline", "within_next", serde_json::json!("2h"), false)],
            &[overdue, soon, later],
            &clock(),
        );
        assert_eq!(matched(&outcome), ["i-1", "i-2"]);
    }

    /// `scheduled_date` is `date`-typed, so the boundary day is inside the
    /// window whatever time of day it is asked at — the clock here is noon,
    /// where resolving the value to `T23:59` against a `T12:00` cutoff would
    /// drop `i-2`, the very day the sweep fires on.
    #[test]
    fn scheduled_date_compares_whole_days_including_the_boundary() {
        let mut inside = item("i-1");
        inside.scheduled_date = Some("2026-08-17".to_string());
        let mut boundary = item("i-2");
        boundary.scheduled_date = Some("2026-08-18".to_string());
        let mut beyond = item("i-3");
        beyond.scheduled_date = Some("2026-08-19".to_string());
        let outcome = backtest(
            Some("item_threshold"),
            &[condition("scheduled_date", "within_next", serde_json::json!("3d"), false)],
            &[inside, boundary, beyond],
            &clock(),
        );
        assert_eq!(matched(&outcome), ["i-1", "i-2"]);
    }

    /// A `date` field accepts `d` units only, and the authority 400s a rule
    /// that says otherwise (`validate_rule`). Carrying `18h` over from a
    /// retyped `deadline` row must count nothing here rather than report a
    /// number for a rule that cannot be saved — `18h` and not `6h` because
    /// only a duration that reaches past tonight tells the two readings
    /// apart.
    #[test]
    fn a_sub_day_unit_on_scheduled_date_matches_nothing() {
        let mut today = item("i-1");
        today.scheduled_date = Some("2026-08-15".to_string());
        let outcome = backtest(
            Some("item_threshold"),
            &[condition("scheduled_date", "within_next", serde_json::json!("18h"), false)],
            &[today],
            &clock(),
        );
        assert!(matched(&outcome).is_empty());
    }

    #[test]
    fn a_day_only_deadline_resolves_to_the_end_of_that_day() {
        let mut end_of_today = item("i-1");
        end_of_today.deadline = Some("2026-08-15".to_string());
        // 23:59 today is more than 2h out, but inside 12h.
        let two_hours = backtest(
            Some("item_threshold"),
            &[condition("deadline", "within_next", serde_json::json!("2h"), false)],
            std::slice::from_ref(&end_of_today),
            &clock(),
        );
        let twelve_hours = backtest(
            Some("item_threshold"),
            &[condition("deadline", "within_next", serde_json::json!("12h"), false)],
            &[end_of_today],
            &clock(),
        );
        assert!(matched(&two_hours).is_empty());
        assert_eq!(matched(&twelve_hours), ["i-1"]);
    }

    #[test]
    fn a_field_value_that_is_not_deadline_shaped_never_matches() {
        let mut zoned = item("i-1");
        zoned.deadline = Some("2026-08-15T13:00Z".to_string());
        let outcome = backtest(
            Some("item_threshold"),
            &[condition("deadline", "within_next", serde_json::json!("2h"), false)],
            &[zoned],
            &clock(),
        );
        assert!(matched(&outcome).is_empty());
    }

    #[test]
    fn string_comparison_is_case_insensitive_as_adr_0013_requires() {
        let mut shouty = item("i-1");
        shouty.stage = "Ready".to_string();
        let outcome = backtest(
            Some("item_threshold"),
            &[condition("stage", "eq", serde_json::json!("ready"), false)],
            &[shouty],
            &clock(),
        );
        assert_eq!(matched(&outcome), ["i-1"]);
    }

    #[test]
    fn a_list_condition_value_means_any_of() {
        let outcome = backtest(
            Some("item_threshold"),
            &[condition(
                "stage",
                "eq",
                serde_json::json!(["blocked", "ready"]),
                false,
            )],
            &[item("i-1")],
            &clock(),
        );
        assert_eq!(matched(&outcome), ["i-1"]);
    }

    #[test]
    fn numeric_operators_read_priority() {
        let mut high = item("i-1");
        high.priority = 3;
        let outcome = backtest(
            Some("item_threshold"),
            &[condition("priority", "gt", serde_json::json!(2), false)],
            &[high],
            &clock(),
        );
        assert_eq!(matched(&outcome), ["i-1"]);
    }

    #[test]
    fn an_unrecognised_operator_never_matches() {
        let outcome = backtest(
            Some("item_threshold"),
            &[condition("stage", "not_an_op", serde_json::json!("ready"), false)],
            &[item("i-1")],
            &clock(),
        );
        assert!(matched(&outcome).is_empty());
    }

    #[test]
    fn no_conditions_matches_every_item_in_the_given_order() {
        let outcome = backtest(
            Some("item_threshold"),
            &[],
            &[item("i-2"), item("i-1")],
            &clock(),
        );
        assert_eq!(matched(&outcome), ["i-2", "i-1"]);
    }
}
