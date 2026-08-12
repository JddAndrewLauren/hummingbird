//! The delivery leg (#139): transitions-only dedupe against `deliveries`
//! (ADR-0012, amended by ADR-0014). No HTTP route calls in here — this is
//! the seam #138's periodic sweep hangs off directly: for every rule a
//! fired event matches, the sweep mints the alert at the highest severity
//! among that tick's matching verdicts and then calls [`deliver`] once per
//! matching rule with that rule's `rule_id` and its verdict's [`Tier`]
//! (severity rides the alert, tier rides the delivery, never the reverse).
//!
//! **This layer owns severity monotonicity** (ADR-0014's 2026-08-12
//! amendment, #188). The alert row records what its source or its minting
//! rules currently assess — a reading, free to fall as well as rise. What
//! must never fall is what *rings*: past the first ring of an occurrence,
//! [`warrants_delivery`] suppresses anything not above the highest severity
//! already rung for this alert, rule and generation. The row and the ring
//! were previously conflated, by a ratchet in `alerts::upsert` that kept the
//! row monotonic so this layer's exact string match could not ring on a
//! downgrade.
//!
//! **`deliver` is sync and does not send.** The authority crate stays a
//! pure, runtime-agnostic library (`lib.rs`'s own guard test) that never
//! blocks on a future — and the real send is an async FCM HTTP call, made
//! from `hummingbird-authority-worker`'s async `fetch` on wasm32, where
//! blocking inside a sync trait method is not an option. So `deliver` does
//! the one thing it *can* do synchronously and honestly: decide whether
//! this transition rings, log it if it does, and hand back exactly what to
//! send and to whom. The async caller (#138) sends; `deliver` never learns
//! whether the send succeeded.
//!
//! **Log before the caller sends.** The delivery row is inserted, and this
//! function returns, before any push is even attempted — the dedupe key
//! doubles as a claim, committed synchronously inside `deliver` itself. A
//! crash during the caller's (necessarily separate, necessarily async)
//! send step leaves the row in place, so a retried or restarted attempt at
//! the same transition calls `deliver` again, finds it already logged, and
//! is suppressed rather than re-sent. The cost, accepted deliberately:
//! such a crash can leave a *logged but unsent* delivery. That is the safe
//! direction for a lane whose whole value is that its sound is always
//! worth attending (ADR-0012's clean-layer principle) — under-ringing is a
//! bug, but a spurious re-ring is the one that destroys trust in the
//! channel.
//!
//! **Zero live targets never burns the transition.** If nothing would
//! receive the push, nothing is logged either — there is no double-ring
//! risk in releasing a transition that was never attempted, and burning it
//! anyway would mean every alert raised before the first device ever
//! registers (all of them, until #141 ships) rings for no one, forever.

use hummingbird_domain::{severity_rank, Alert, PushTarget, Tier};

use crate::codec::RowReader;
use crate::handlers::push_targets::push_target_from_row;
use crate::handlers::sha256_hex;
use crate::sql::{Sql, SqlError, SqlValue};

/// Everything the caller's async send step needs, independent of which
/// target it's aimed at — owned, not borrowed, so it survives across the
/// `.await` the caller (necessarily) makes to actually send it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushNotification {
    pub alert_id: String,
    pub title: String,
    pub body: Option<String>,
    pub severity: String,
    pub tier: Tier,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DeliveryOutcome {
    /// A delivery row was logged; the caller must attempt `notification`
    /// against every target in `targets` (never empty — see
    /// [`SuppressReason::NoTargets`]).
    Logged {
        delivery_id: String,
        targets: Vec<PushTarget>,
        notification: PushNotification,
    },
    /// Absorbed silently: not a delivery-warranting transition
    /// (ADR-0012/0014), or nothing could have received it. Either way
    /// nothing is logged for it.
    Suppressed(SuppressReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuppressReason {
    /// Neither of ADR-0012's warranting transitions: something has already
    /// rung for `(alert_id, rule_id, generation)` and this severity does not
    /// outrank it — an identical re-raise of a live alert, a de-escalation, or
    /// a climb back to a level already reported. Absorbed so a flapping source
    /// cannot spam (ADR-0012, rank-based since #188 — see
    /// [`warrants_delivery`]).
    AlreadyDelivered,
    /// The alert is not currently live under ADR-0014's three-clause
    /// predicate — nothing should ring for a settled or expired alert.
    NotLive,
    /// No severity to stamp on the delivery. This was a defensive guard
    /// while `sweep_tick` was the only caller — a `mints: true` rule always
    /// names a severity at mint time. **#255 made it reachable**: an
    /// `alert_raised` match cannot stamp one (`rules.severity` is unused
    /// for that kind, per ADR-0013), so it rides the alert's own, and
    /// `AlertIngest::severity` is optional. A webhook source that raises
    /// without one therefore matches a rule and then rings nothing,
    /// silently. Every shipped poller happens to carry a rule-derived
    /// severity, so nothing hits this today; a hand-rolled source
    /// (`healthchecks/v1`, `github/v1`) would. Left as-is deliberately —
    /// the fix is a decision about the ingest contract (default a
    /// severity, or reject a severity-less raise at the route), not a
    /// change to this suppressor.
    NoSeverity,
    /// No live (non-revoked) `push_targets` row exists. Distinct from a
    /// send failure: nothing was attempted, so nothing needs to be
    /// retried, and the transition must stay open for the day a target
    /// finally registers.
    NoTargets,
}

/// Decides (and logs) whether one rule's ring against one alert is
/// warranted. Call once per matching rule, after the caller has minted or
/// upserted `alert` — the dedupe key (`alert_id`, `rule_id`,
/// `alert.raised_at`, `alert.severity`) is read straight off the value
/// passed in, never re-fetched. Never sends; see the module doc for why.
pub fn deliver(
    sql: &dyn Sql,
    now_ms: i64,
    alert: &Alert,
    rule_id: &str,
    tier: Tier,
) -> Result<DeliveryOutcome, SqlError> {
    if !alert.is_live(now_ms) {
        return Ok(DeliveryOutcome::Suppressed(SuppressReason::NotLive));
    }
    let Some(severity) = alert.severity.as_deref().filter(|s| !s.is_empty()) else {
        return Ok(DeliveryOutcome::Suppressed(SuppressReason::NoSeverity));
    };
    let generation = alert.raised_at;

    if !warrants_delivery(sql, &alert.id, rule_id, generation, severity)? {
        return Ok(DeliveryOutcome::Suppressed(SuppressReason::AlreadyDelivered));
    }

    let targets = live_push_targets(sql)?;
    if targets.is_empty() {
        return Ok(DeliveryOutcome::Suppressed(SuppressReason::NoTargets));
    }

    // Log before the caller can possibly send (see module doc): the INSERT
    // commits the claim before this function ever returns control.
    let delivery_id = deterministic_delivery_id(&alert.id, rule_id, generation, severity);
    sql.exec(
        "INSERT INTO deliveries (id, alert_id, rule_id, generation, severity, tier, sent_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
        &[
            SqlValue::Text(delivery_id.clone()),
            SqlValue::Text(alert.id.clone()),
            SqlValue::Text(rule_id.to_string()),
            SqlValue::Integer(generation),
            SqlValue::Text(severity.to_string()),
            SqlValue::Text(tier.as_str().to_string()),
            SqlValue::Integer(now_ms),
        ],
    )?;

    let notification = PushNotification {
        alert_id: alert.id.clone(),
        title: alert.title.clone(),
        body: alert.body.clone(),
        severity: severity.to_string(),
        tier,
    };
    Ok(DeliveryOutcome::Logged { delivery_id, targets, notification })
}

/// Whether this ring is one of ADR-0012's two delivery-warranting
/// transitions for `(alert, rule, generation)` — made rank-based by
/// ADR-0014's 2026-08-12 amendment (#188).
///
/// **Both transitions, not one.** ADR-0012 warrants a delivery when an alert
/// *enters* live-unacked **or** when its severity *escalates* while live, and
/// the two are asked in that order here:
///
/// 1. **Entry** — nothing has rung yet for this generation, so this is the
///    first ring of this occurrence and it lands whatever its severity says.
/// 2. **Escalation** — something has rung, so this must outrank the highest
///    of it.
///
/// Collapsing (1) into (2) is a trap worth naming, because it type-checks and
/// passes every fixture with a known severity: `severity` is free text with no
/// `CHECK` (see `domain::severity_rank`), an unranked string ranks `0`, and
/// with a bare `> 0` comparison a first raise at `"warning"` or `"critical"`
/// — plausible strings from a hand-rolled `healthchecks/v1` or `github/v1`
/// webhook — would rank `0 > 0` and **never ring at all**. Silent
/// under-ringing is the direction the module doc calls the bug.
///
/// **The escalation half subsumes the exact-string match it replaces:** an
/// equal rank is not an escalation, so an unchanged re-raise is suppressed
/// exactly as before. What the rank buys is the case the old severity ratchet
/// in `alerts::upsert` used to make unreachable — a source lowering severity
/// on its own still-live occurrence. Under exact-match that minted a dedupe
/// key nobody had seen and rang the phone for *good* news, which ADR-0012
/// never warranted. A later climb back to a level already rung this
/// generation is silent for the same reason — the reader was told that much
/// already.
///
/// Ranking is `domain::severity_rank`, the one total order (so an unranked
/// string can never win an *escalation* it did not earn, even though it may
/// open one), folded here rather than in SQL: a SQL `MAX` would order the
/// free text lexicographically — `"urgent" > "normal" > "low" > "high"` — and
/// quietly disagree with every other consumer of the order.
///
/// The comparison is deliberately scoped to one `generation`. A fresh
/// occurrence rings on its own merits, at whatever level it opens at; a
/// cross-generation baseline would reinstate the high-water mark one level
/// up, so a recovered-then-relapsed check could never ring below its old
/// peak. One consequence rides along, accepted in #188: because a downgrade
/// is a changed source-owned field, a source declaring `restamp_on_change`
/// bumps `raised_at` on one, opening a new generation that rings at the lower
/// level. That is coherent — such a source is asserting a new occurrence —
/// but it is a wiring-time obligation on any restamping source whose
/// severity varies. Both shipped ones (`city-waste/v2`, `race-schedule/v1`)
/// send a constant severity, so nothing hits it today.
fn warrants_delivery(
    sql: &dyn Sql,
    alert_id: &str,
    rule_id: &str,
    generation: i64,
    severity: &str,
) -> Result<bool, SqlError> {
    let rows = sql.exec(
        "SELECT severity FROM deliveries WHERE alert_id = ? AND rule_id = ? AND generation = ?",
        &[
            SqlValue::Text(alert_id.to_string()),
            SqlValue::Text(rule_id.to_string()),
            SqlValue::Integer(generation),
        ],
    )?;
    // `None` is "nothing rung yet", which is not the same as "rung at rank
    // zero" — keeping them distinct is what makes the entry transition fire
    // for an unranked severity.
    let mut rung: Option<usize> = None;
    for row in &rows {
        // A delivery row's `severity` is `NOT NULL` and written by the INSERT
        // above, so a missing or non-text cell is a corrupt row rather than a
        // shape this has to interpret — rank it 0 and let the comparison
        // decide, instead of failing a live ring over it. It still counts as
        // *something* rung, so it closes the entry transition.
        let logged = RowReader(row).opt_text("severity").unwrap_or_default();
        let rank = severity_rank(&logged);
        rung = Some(rung.map_or(rank, |highest: usize| highest.max(rank)));
    }
    Ok(match rung {
        None => true,
        Some(rung) => severity_rank(severity) > rung,
    })
}

fn live_push_targets(sql: &dyn Sql) -> Result<Vec<PushTarget>, SqlError> {
    sql.exec("SELECT * FROM push_targets WHERE revoked_at IS NULL", &[])?
        .iter()
        .map(push_target_from_row)
        .collect()
}

/// `hex(sha256("delivery:" + len(alert_id) + ":" + alert_id + ":" +
/// len(rule_id) + ":" + rule_id + ":" + generation + ":" +
/// severity))[..32]` — stable across replays, mirroring
/// `alerts::deterministic_id`. `alert_id` and `rule_id` are free text and
/// each precede another field, so each is length-prefixed to keep the
/// preimage unambiguous: without it, `(rule_id = "a:1", generation = 2,
/// severity = "high")` and `(rule_id = "a", generation = 1,
/// severity = "2:high")` collide on the same string. `generation` needs no
/// prefix of its own — it is an `i64` printed in decimal, which can never
/// contain the `:` delimiter — and `severity`, being the last field, needs
/// none either. A retried `deliver` call for the same transition (a
/// retried sweep tick) would land on the same row, but the already-logged
/// check above absorbs it before this id is ever computed.
fn deterministic_delivery_id(alert_id: &str, rule_id: &str, generation: i64, severity: &str) -> String {
    let preimage = format!(
        "delivery:{}:{alert_id}:{}:{rule_id}:{generation}:{severity}",
        alert_id.len(),
        rule_id.len()
    );
    let mut id = sha256_hex(&preimage);
    id.truncate(32);
    id
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pins the exact collision pair the issue reported: without the
    /// length prefix on `rule_id`, `("a:1", 2, "high")` and `("a", 1,
    /// "2:high")` both format to the same preimage string and therefore
    /// the same id. With it, they must differ.
    #[test]
    fn deterministic_delivery_id_disambiguates_the_known_collision_pair() {
        let alert_id = "shared-alert";
        let a = deterministic_delivery_id(alert_id, "a:1", 2, "high");
        let b = deterministic_delivery_id(alert_id, "a", 1, "2:high");
        assert_ne!(a, b);
    }
}
