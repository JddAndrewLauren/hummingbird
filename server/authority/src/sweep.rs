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
//! # One mint per item, order-independent (ADR-0014)
//!
//! When more than one rule matches the same item on one tick, every
//! matching verdict is collected **before** any write: [`upsert_alert`] is
//! called exactly once, at the highest severity among the matches, and only
//! then does [`deliver`] run once per matching rule against that one
//! already-ratcheted alert. Interleaving mint-then-deliver per rule instead
//! would let the *first* rule deliver at a pre-ratchet severity and then
//! fire again on a later tick once the ratchet moved the dedupe
//! generation — a spurious re-ring with nothing changed in the world, and
//! rule order would silently decide how many times a device's phone buzzes.
//! ADR-0014 is explicit that this must not depend on order: "two rules
//! matching one event mint one alert whose severity ratchets up... never
//! down."
//!
//! # Repeat ticks don't re-ring — routed through the frozen dedupe key
//!
//! `item-threshold/v1` is a *state* source (ADR-0014): its `source_key` is
//! `item:<id>`, naming the thing, not the tick. This sweep is the "source"
//! for that state, which carries ADR-0014's wiring-time obligation
//! verbatim: *"`raised_at` is the moment the condition began, never the
//! moment the webhook fired."* Concretely: before minting, the sweep reads
//! whatever alert already exists under `item:<id>` and asks whether it is
//! still live *right now*.
//!
//! - **Still live:** the match is a continuation of the same occurrence.
//!   `raised_at` is passed as `None` — "keep the stored stamp" — so an
//!   unchanged match is a byte-identical upsert (a no-op) and lands on the
//!   same `generation` `deliver`'s dedupe key already logged. This is what
//!   makes a repeat tick distinguishable from a fresh occurrence, entirely
//!   inside #139's existing, unwidened dedupe key.
//! - **Not live (no row yet, or the row is dismissed/resolved/expired):**
//!   the match is a *new* occurrence. `raised_at` is stamped `now_ms`,
//!   overtaking whatever `dismissed_at`/`resolved_at` made it settled — the
//!   exact mechanism ADR-0014's "Live: how a settled alert rings again"
//!   describes for a re-committed deadline re-raising the same row. Without
//!   this, an item that is ever hand-dismissed once could never ring again:
//!   `raised_at` would stay frozen at its original mint forever, so the
//!   live predicate (`raised_at > dismissed_at`) could never flip back true
//!   no matter how many times the condition later re-triggers.
//!
//! **What this does not fix:** ADR-0014 also assigns the DO alarm a
//! *separate* "resolution pass" — iterating live `item-threshold/v1`
//! alerts (not items) and stamping `resolved_at` once an item is done,
//! archived, deleted, or no longer matches. That pass is not implemented
//! here (see `tick`'s own doc and the tracking issue named in the PR). Two
//! concrete consequences follow, both accepted deliberately for this issue
//! and not silently absorbed:
//!
//! 1. An item whose alert was never dismissed, and whose item later stops
//!    matching (done, archived, or edited past the threshold) without a
//!    human ever hand-acking it, has **no path back to live-and-quiet**: no
//!    resolution pass ever runs to close it, so it sits live and
//!    top-of-stack forever until a human dismisses it by hand. This sweep
//!    cannot detect "the condition ended" — only "the condition still
//!    holds" or "nothing to report" — which is exactly the gap ADR-0014
//!    names as the reason the resolution pass has to exist.
//! 2. Even after that pass exists, it will not compose for free with this
//!    mint path as written: every call here passes `resolved_at: None`,
//!    and `upsert_alert` (`handlers/alerts.rs`) sets a source-owned field
//!    *absolutely* from what it is given — an absent optional clears. If
//!    the item is still matching on the tick right after the resolution
//!    pass stamps `resolved_at`, this sweep's next `upsert_alert` call
//!    erases that stamp on the very next tick. Wiring the two together
//!    correctly is part of that future issue's job, not a side effect of
//!    reading this module.

use std::collections::BTreeMap;

use hummingbird_domain::{
    higher_severity, item_threshold_v1_key, now_as_deadline, AlertIngest, Energy, Event,
    FieldValue, Item, Size, ITEM_THRESHOLD_V1,
};
use hummingbird_rules_engine::{evaluate_rules, RuleOutcome, Verdict};

use crate::delivery::{deliver, DeliveryOutcome};
use crate::handlers::{find_alert_by_identity, item_from_row, rule_from_row, upsert_alert};
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
/// presented as an `item_threshold` event. All of one item's matching
/// verdicts are resolved to a single mint/ratchet (see module doc's "one
/// mint per item" section) before [`deliver`] runs once per matching rule
/// against that one alert. Returns one [`TickMatch`] per (item, rule)
/// match, regardless of whether `deliver` decided to log or suppress it —
/// the caller (the worker's `alarm()` handler) only needs to act on
/// `Logged`, but every outcome is reported so a fixture can assert on
/// suppression too.
///
/// A single malformed row (`item_from_row`/`rule_from_row` failing to parse
/// a cell) is skipped rather than aborting the whole tick — a poisoned row
/// must not become a poison pill that blocks every other item, forever,
/// once the alarm reschedules and tries again. This should never happen in
/// practice (every row was written by this crate's own handlers against
/// its own DDL); it is defensive, not an expected path.
pub fn tick(sql: &dyn Sql, now_ms: i64) -> Result<Vec<TickMatch>, SqlError> {
    let rules: Vec<_> =
        load_enabled_rules(sql)?.iter().filter_map(|row| rule_from_row(row).ok()).collect();
    if rules.is_empty() {
        return Ok(Vec::new());
    }

    let now = now_as_deadline(now_ms);
    let mut matches = Vec::new();
    for row in load_live_items(sql)? {
        let Ok(item) = item_from_row(&row) else { continue };
        let event = item_threshold_event(&item);

        let verdicts: Vec<(String, Verdict)> = evaluate_rules(&rules, &event, &now)
            .into_iter()
            .filter_map(|(rule_id, outcome)| match outcome {
                RuleOutcome::Matched(verdict) => Some((rule_id, verdict)),
                _ => None,
            })
            .collect();
        if verdicts.is_empty() {
            continue;
        }

        let source_key = item_threshold_v1_key(&item.id);
        let existing = find_alert_by_identity(sql, ITEM_THRESHOLD_V1, &source_key)?;
        // Still live: this tick's match is a continuation of the same
        // occurrence, so keep the stored `raised_at` (module doc). Not
        // live, or no row yet: a fresh occurrence starts now — the
        // mechanism that lets a settled alert ring again.
        let raised_at = match &existing {
            Some(alert) if alert.is_live(now_ms) => None,
            _ => Some(now_ms),
        };
        // ADR-0014: N matching rules mint ONE alert at the highest
        // severity among them, never N mints — order-independent, since
        // `higher_severity` folded over any order converges on the same
        // maximum.
        let severity = verdicts
            .iter()
            .fold(None, |acc: Option<&str>, (_, v)| higher_severity(acc, Some(v.severity.as_str())))
            .map(str::to_string);

        let ingest = AlertIngest {
            source: ITEM_THRESHOLD_V1.to_string(),
            source_key,
            title: item.title.clone(),
            body: item.description.clone(),
            url: item.source_url.clone(),
            severity,
            raised_at,
            // `item-threshold/v1` never sets `resolved_at`/`expires_at`
            // from this sweep — resolving an alert whose item stopped
            // matching is ADR-0014's separate "resolution pass" over live
            // alerts, out of #138's acceptance criteria (see module doc's
            // "what this does not fix").
            resolved_at: None,
            expires_at: None,
        };
        let (_status, alert) = upsert_alert(sql, now_ms, ingest)?;

        for (rule_id, verdict) in verdicts {
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
/// operator's own signal that it is no longer live work. A `done`-but-not-
/// archived item is deliberately *not* excluded here in addition: ADR-0014
/// names `archived_at` as the one evaluation boundary, `stage` is a
/// separate lifecycle axis it never gates on, and inventing a second
/// exclusion criterion with no ADR or acceptance-criterion anchor would be
/// scope creep past what this issue was asked to decide.
fn load_live_items(sql: &dyn Sql) -> Result<Vec<Row>, SqlError> {
    sql.exec("SELECT * FROM items WHERE archived_at IS NULL", &[])
}

/// Presents one item as the `item_threshold` [`Event`] the rule engine
/// evaluates against (#133/ADR-0013's field table). `source`/`source_key`
/// carry the item's own provenance (its adapter origin), never the alert
/// identity `item-threshold/v1`/`item:<id>` minted above — the two are
/// deliberately different namespaces answering different questions (where
/// did this item come from, vs. which alert row does a matching rule
/// ratchet). A rule naming no `event_kind` at all (`NULL` = "any kind",
/// ADR-0013) evaluates against this event's core fields exactly like every
/// other kind — nothing here special-cases that away.
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
