//! The delivery leg (#139): transitions-only dedupe against `deliveries`
//! (ADR-0012, amended by ADR-0014) plus the FCM send call, isolated behind
//! [`Pusher`] so it is exercised in tests without a live FCM project. No
//! HTTP route calls in here — this is the seam #138's periodic sweep hangs
//! off directly: for every rule a fired event matches, the sweep
//! mints/ratchets the alert (severity rides the alert, per the ADR-0014
//! ratchet) and then calls [`deliver`] once per matching rule with that
//! rule's `rule_id` and its verdict's [`Tier`] (tier rides the delivery,
//! never the alert).
//!
//! **Log before send.** The delivery row is inserted before any push is
//! attempted — the dedupe key doubles as a claim. A crash (or a partial
//! send failure) between the two leaves the row in place, so a retried or
//! restarted attempt at the same transition sees it as already handled and
//! never re-sends. The cost, accepted deliberately: such a crash can leave
//! a *logged but unsent* delivery. That is the safe direction for a lane
//! whose whole value is that its sound is always worth attending
//! (ADR-0012's clean-layer principle) — under-ringing is a bug, but a
//! spurious re-ring is the one that destroys trust in the channel.

use hummingbird_domain::{Alert, PushTarget, Tier};

use crate::handlers::push_targets::push_target_from_row;
use crate::sql::{Sql, SqlError, SqlValue};

/// The FCM send call, injected the same way as [`crate::Entropy`]: the
/// `workers-rs` shim supplies a real HTTP client against a live FCM
/// project, fixtures supply [`Pusher`] fakes that need neither.
pub trait Pusher {
    fn send(&self, target: &PushTarget, notification: &PushNotification) -> Result<(), PushError>;
}

/// Everything one send needs, independent of which target it's aimed at —
/// built once per [`deliver`] call and handed to every live target.
pub struct PushNotification<'a> {
    pub alert_id: &'a str,
    pub title: &'a str,
    pub body: Option<&'a str>,
    pub severity: &'a str,
    pub tier: Tier,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushError {
    pub message: String,
}

/// One target's failed send, folded into [`DeliveryOutcome::Sent`] — a
/// per-target failure never fails the delivery as a whole (one dead token
/// must not silence every other device).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushFailure {
    pub target_id: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeliveryOutcome {
    /// A delivery row was logged and every live (non-revoked) target was
    /// attempted; `failures` names any that did not receive it.
    Sent {
        delivery_id: String,
        sent_to: usize,
        failures: Vec<PushFailure>,
    },
    /// Absorbed silently: not a delivery-warranting transition
    /// (ADR-0012/0014), still "considered" in the sense that this function
    /// ran and made the call — nothing more is logged for it.
    Suppressed(SuppressReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuppressReason {
    /// `(alert_id, rule_id, generation, severity)` already logged — an
    /// identical re-raise of a live alert, absorbed so a flapping source
    /// cannot spam (ADR-0012).
    AlreadyDelivered,
    /// The alert is not currently live under ADR-0014's three-clause
    /// predicate — nothing should ring for a settled or expired alert.
    NotLive,
    /// No severity to stamp on the delivery. A rule always names one at
    /// mint time, so this is a defensive guard, not an expected path.
    NoSeverity,
}

/// Delivers (or suppresses) one rule's ring against one alert. Call once
/// per matching rule, after the caller has minted/ratcheted `alert` — the
/// dedupe key (`alert_id`, `rule_id`, `alert.raised_at`, `alert.severity`)
/// is read straight off the value passed in, never re-fetched.
pub fn deliver(
    sql: &dyn Sql,
    now_ms: i64,
    alert: &Alert,
    rule_id: &str,
    tier: Tier,
    pusher: &dyn Pusher,
) -> Result<DeliveryOutcome, SqlError> {
    if !alert.is_live(now_ms) {
        return Ok(DeliveryOutcome::Suppressed(SuppressReason::NotLive));
    }
    let Some(severity) = alert.severity.as_deref().filter(|s| !s.is_empty()) else {
        return Ok(DeliveryOutcome::Suppressed(SuppressReason::NoSeverity));
    };
    let generation = alert.raised_at;

    if existing_delivery(sql, &alert.id, rule_id, generation, severity)? {
        return Ok(DeliveryOutcome::Suppressed(SuppressReason::AlreadyDelivered));
    }

    // Log before send (see module doc): the INSERT commits the claim, then
    // every live target is attempted against it.
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
        alert_id: &alert.id,
        title: &alert.title,
        body: alert.body.as_deref(),
        severity,
        tier,
    };
    let mut sent_to = 0;
    let mut failures = Vec::new();
    for target in live_push_targets(sql)? {
        match pusher.send(&target, &notification) {
            Ok(()) => sent_to += 1,
            Err(e) => failures.push(PushFailure {
                target_id: target.id,
                message: e.message,
            }),
        }
    }
    Ok(DeliveryOutcome::Sent { delivery_id, sent_to, failures })
}

fn existing_delivery(
    sql: &dyn Sql,
    alert_id: &str,
    rule_id: &str,
    generation: i64,
    severity: &str,
) -> Result<bool, SqlError> {
    Ok(!sql
        .exec(
            "SELECT id FROM deliveries WHERE alert_id = ? AND rule_id = ? AND generation = ? \
             AND severity = ?",
            &[
                SqlValue::Text(alert_id.to_string()),
                SqlValue::Text(rule_id.to_string()),
                SqlValue::Integer(generation),
                SqlValue::Text(severity.to_string()),
            ],
        )?
        .is_empty())
}

fn live_push_targets(sql: &dyn Sql) -> Result<Vec<PushTarget>, SqlError> {
    sql.exec("SELECT * FROM push_targets WHERE revoked_at IS NULL", &[])?
        .iter()
        .map(push_target_from_row)
        .collect()
}

/// `hex(sha256("delivery:" + alert_id + ":" + rule_id + ":" + generation +
/// ":" + severity))[..32]` — stable across replays, mirroring
/// `alerts::deterministic_id`. A retried `deliver` call for the same
/// transition (a retried sweep tick) would land on the same row, but the
/// already-logged check above absorbs it before this id is ever computed.
fn deterministic_delivery_id(alert_id: &str, rule_id: &str, generation: i64, severity: &str) -> String {
    use sha2::{Digest, Sha256};
    use std::fmt::Write;

    let preimage = format!("delivery:{alert_id}:{rule_id}:{generation}:{severity}");
    let digest = Sha256::digest(preimage.as_bytes());
    let mut id = digest.iter().fold(String::with_capacity(digest.len() * 2), |mut s, b| {
        write!(s, "{b:02x}").expect("writing to a String cannot fail");
        s
    });
    id.truncate(32);
    id
}
