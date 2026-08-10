//! The DO alarm sweep (#138): the repeat-tick half of ADR-0012's fire-time
//! rule ("rules evaluate at fire time — on each poll batch, each incoming
//! webhook, **and a periodic DO alarm tick for time predicates**"). Reads
//! items already held in the authority, presents each as a synthetic
//! `item_threshold` [`Event`], evaluates every enabled rule against it, and
//! for every match mints/ratchets the alert and calls `deliver` — the exact
//! two-step sequence [`crate::delivery`]'s own module doc describes as the
//! seam this sweep hangs off. **Never writes to `items` or `rules`**: a
//! tick is read-then-mint, never a write to what it read.
//!
//! # Repeat ticks don't re-ring — routed through the frozen dedupe key
//!
//! `item-threshold/v1` is a *state* source (ADR-0014): its `source_key` is
//! `item:<id>`, naming the thing, not the tick. Every tick that still finds
//! an item matching calls [`crate::handlers::upsert_alert`] with
//! `raised_at: None` — "keep the stored stamp" — so an unchanged match is a
//! byte-identical upsert (a no-op, same as an identical webhook replay) and
//! `deliver`'s dedupe key `(alert_id, rule_id, generation, severity)` sees
//! the same `generation` (the alert's `raised_at`) it logged on the tick
//! that first minted the row. That already-logged check is what makes a
//! repeat tick distinguishable from a fresh occurrence: nothing about this
//! sweep bypasses or widens #139's frozen dedupe key, it simply calls it
//! every tick, on every match, exactly as `deliver`'s own doc anticipates.
//! An item whose matching rule's *severity* changes — not possible today,
//! since a rule's `severity` is static, but the same path a future
//! per-item severity would ride — ratchets the alert and rings again
//! through the ordinary escalation path, not a new mechanism.

use std::collections::BTreeMap;

use hummingbird_domain::{
    item_threshold_v1_key, now_as_deadline, AlertIngest, Energy, Event, FieldValue, Item, Size,
};
use hummingbird_rules_engine::{evaluate_rules, RuleOutcome};

use crate::delivery::{deliver, DeliveryOutcome};
use crate::handlers::{item_from_row, rule_from_row, upsert_alert};
use crate::sql::{Row, Sql, SqlError};

/// The alarm's tick interval — a real, readable parameter (ADR-0013: "the
/// DO alarm interval is the precision floor"; #140 warns, never rejects,
/// when a rule's duration is shorter than this). 15 minutes, ADR-0013's own
/// worked example ("`within_next '5m'` on a 15-minute tick fires up to 15
/// minutes late"). A `const`, not a buried literal at the `set_alarm` call
/// site — the shim reads it, and so can any future consumer that needs to
/// compare a duration against it.
pub const ALARM_INTERVAL_MS: i64 = 15 * 60 * 1000;

/// One matching rule against one item, resolved all the way through
/// `deliver` — everything the worker's async `fetch` needs to attempt the
/// FCM sends for a [`DeliveryOutcome::Logged`], without re-deriving
/// anything already decided here.
#[derive(Debug, Clone, PartialEq)]
pub struct TickMatch {
    pub item_id: String,
    pub rule_id: String,
    pub outcome: DeliveryOutcome,
}

/// Runs one alarm tick: every enabled rule against every non-archived item,
/// presented as an `item_threshold` event. Matches mint/ratchet through
/// [`crate::handlers::upsert_alert`] (the same upsert `POST /api/alerts`
/// uses) and are routed through [`deliver`] once per matching rule, per
/// the module doc. Returns one [`TickMatch`] per (item, rule) match,
/// regardless of whether `deliver` decided to log or suppress it — the
/// caller (the worker's `alarm()` handler) only needs to act on `Logged`,
/// but every outcome is reported so a fixture can assert on suppression
/// too.
pub fn tick(sql: &dyn Sql, now_ms: i64) -> Result<Vec<TickMatch>, SqlError> {
    let rules: Vec<_> = load_enabled_rules(sql)?
        .into_iter()
        .map(|row| rule_from_row(&row))
        .collect::<Result<_, _>>()?;
    if rules.is_empty() {
        return Ok(Vec::new());
    }

    let now = now_as_deadline(now_ms);
    let mut matches = Vec::new();
    for row in load_live_items(sql)? {
        let item = item_from_row(&row)?;
        let event = item_threshold_event(&item);
        for (rule_id, outcome) in evaluate_rules(&rules, &event, &now) {
            let RuleOutcome::Matched(verdict) = outcome else {
                continue;
            };
            let ingest = AlertIngest {
                source: "item-threshold/v1".to_string(),
                source_key: item_threshold_v1_key(&item.id),
                title: item.title.clone(),
                body: item.description.clone(),
                url: item.source_url.clone(),
                severity: Some(verdict.severity.clone()),
                // `None`: a repeat tick that still matches keeps the
                // stored `raised_at`, so it lands on the same `deliver`
                // dedupe generation instead of minting a fresh one (see
                // module doc). Absent on a genuine first raise too, in
                // which case `upsert_alert` stamps `now_ms` itself.
                raised_at: None,
                // `item-threshold/v1` never sets `resolved_at`/`expires_at`
                // from this sweep — resolving an alert whose item stopped
                // matching is ADR-0014's separate "resolution pass" over
                // live alerts, out of #138's acceptance criteria.
                resolved_at: None,
                expires_at: None,
            };
            let (_status, alert) = upsert_alert(sql, now_ms, ingest)?;
            let outcome = deliver(sql, now_ms, &alert, &rule_id, verdict.tier)?;
            matches.push(TickMatch { item_id: item.id.clone(), rule_id, outcome });
        }
    }
    Ok(matches)
}

fn load_enabled_rules(sql: &dyn Sql) -> Result<Vec<Row>, SqlError> {
    sql.exec("SELECT * FROM rules WHERE enabled = 1", &[])
}

/// Archived items are excluded from evaluation entirely (ADR-0014): the
/// sweep never mutates or re-classes an item, and an archived item is the
/// operator's own signal that it is no longer live work.
fn load_live_items(sql: &dyn Sql) -> Result<Vec<Row>, SqlError> {
    sql.exec("SELECT * FROM items WHERE archived_at IS NULL", &[])
}

/// Presents one item as the `item_threshold` [`Event`] the rule engine
/// evaluates against (#133/ADR-0013's field table). `source`/`source_key`
/// carry the item's own provenance (its adapter origin), never the alert
/// identity `item-threshold/v1`/`item:<id>` minted above — the two are
/// deliberately different namespaces answering different questions (where
/// did this item come from, vs. which alert row does a matching rule
/// ratchet).
fn item_threshold_event(item: &Item) -> Event {
    let mut extras = BTreeMap::new();
    if let Some(deadline) = &item.deadline {
        extras.insert("deadline".to_string(), FieldValue::Str(deadline.clone()));
    }
    if let Some(scheduled_date) = &item.scheduled_date {
        extras.insert("scheduled_date".to_string(), FieldValue::Str(scheduled_date.clone()));
    }
    extras.insert("title".to_string(), FieldValue::Str(item.title.clone()));
    extras.insert("stage".to_string(), FieldValue::Str(item.stage.as_str().to_string()));
    if let Some(size) = item.size {
        extras.insert("size".to_string(), FieldValue::Str(Size::as_str(size).to_string()));
    }
    if let Some(energy) = item.energy {
        extras.insert("energy".to_string(), FieldValue::Str(Energy::as_str(energy).to_string()));
    }
    if let Some(context) = &item.context {
        extras.insert("context".to_string(), FieldValue::Str(context.clone()));
    }
    extras.insert("priority".to_string(), FieldValue::Num(item.priority as f64));
    if let Some(project) = &item.project_id {
        extras.insert("project".to_string(), FieldValue::Str(project.clone()));
    }

    Event {
        source: item.source.clone().unwrap_or_default(),
        source_key: item.source_key.clone().unwrap_or_default(),
        occurred_at: now_as_deadline(item.updated_at),
        title: item.title.clone(),
        body: item.description.clone(),
        url: item.source_url.clone(),
        severity: None,
        calendar_busy: None,
        event_kind: Some("item_threshold".to_string()),
        extras,
    }
}
