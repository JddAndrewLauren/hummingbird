//! [`Freshness`]: how old a standing question's answer is, and what
//! "normal" looks like for the source that answered it (ADR-0015's
//! Rust/TS carve-out).
//!
//! A top-level pure module beside [`Core`](crate::Core), on exactly
//! [`crate::rank`]'s discipline: **no I/O, no credentials, no clock read**.
//! "Now" arrives caller-injected on every call, because bare
//! `wasm32-unknown-unknown` has no clock that does not panic.
//!
//! # Why this is in Rust at all
//!
//! Two things must not drift between clients; alert liveness is one
//! ([`hummingbird_domain::is_live`]), and this is the other. It exists to
//! hold two lines:
//!
//! ADR-0015 originally carved out only those two and left bespoke answer
//! and band in TS `screens/*.ts` to protect the visual iteration loop.
//! ADR-0025 redrew that line for the multi-client world — the band
//! functions and their thresholds are decision logic and sink here too, as
//! each screen is built for Android (#141). This module is unaffected by
//! that redraw: it was always on the Rust side of it. What changed is that
//! it is no longer the exception.
//!
//! **`Unknown` may never render as fresh.** Two different unknowns exist,
//! and collapsing them into a boolean hides a real difference:
//! [`Freshness::Unknown`] is *we do not know the age*, while
//! [`Freshness::Age`] with `declared_cadence_ms: None` is *we know the age
//! but not what normal looks like*. The vacation prototype quietly broke
//! this — its `staleness(null, …)` returns `{ label: null, stale: false }`,
//! harmless there only because a separate `AnswerState` said `no_snapshot`.
//! Here it cannot be broken: there is no age accessor that fabricates a
//! zero, and [`Freshness::is_stale_beyond`] — the one stale decision this
//! module makes — answers `true` for `Unknown` against **every** threshold,
//! the same rule `shell/sync-status.ts` already carries for sync outcomes
//! ("an outcome that did not run must never read as success").
//!
//! **The clock rule, stated once.** See [`Freshness::measure`]. Two
//! prototypes independently hand-rolled `Math.max(0, now - fetchedAt)`;
//! that drift is the whole reason for the carve-out.
//!
//! # What is deliberately *not* here
//!
//! **The threshold.** ADR-0015 is emphatic that it is not carved out: it
//! belongs beside each band function, because the driver is not the
//! cadence but the **cost of a wrong answer**, which differs per question.
//! (ADR-0025 keeps that reasoning exactly and moves the pair together —
//! band *and* threshold sink into the core per screen. "Beside its band
//! function" is the invariant; "in TS" was only where the bands were.)
//! Race polls every six hours and calls `2 ×` stale; waste polls daily and
//! calls **26h** stale rather than 48h, because a 47-hour-old waste answer
//! can be a whole collection cycle wrong and would render "Trash Tonight"
//! on the wrong night. So [`Freshness::is_stale_beyond`] takes the
//! threshold as an argument and this module never derives one. A
//! `stale_after_ms` field on this type would be the rejected design.
//!
//! # One type, both lanes
//!
//! The two snapshot-lane panes (race, waste) take `declared_cadence_ms`
//! from the [`SnapshotEnvelope`] their payload carries —
//! [`Freshness::of_snapshot`]. The two calendar-lane panes (vacation,
//! weekend) have no envelope at all and take it from the mirror's own poll
//! interval (`client/web/src/shell/useCalendarWiring.ts`'s
//! `TIMER_INTERVAL_MS`, ADR-0005's 15-minute foreground poll), passed
//! straight into [`Freshness::measure`]. Making one type serve all four is
//! the point: ADR-0015 rejected putting `stale_after_ms` in the envelope as
//! data precisely because it could not serve the calendar lane.

use hummingbird_domain::{ContextSnapshot, SnapshotEnvelope};
use serde::{Deserialize, Serialize};

/// How old an answer is. Three arms, not a boolean — see the module doc.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Freshness {
    /// No fetch stamp at all: nothing has ever been polled for this
    /// question on this device, so its age is not merely large, it is
    /// unmeasured. Never renders as fresh.
    Unknown,
    /// The age is known. `declared_cadence_ms` is what the source says
    /// normal looks like, and `None` there is a *second, different*
    /// unknown — legitimate, not a failure.
    Age {
        /// Milliseconds since the fetch, clamped at zero by
        /// [`Freshness::measure`]'s clock rule. Never negative.
        age_ms: i64,
        /// Always strictly positive when `Some`: a non-positive cadence
        /// cannot say what normal looks like, so [`Freshness::measure`]
        /// reads it as "not declared" rather than carrying a number every
        /// downstream threshold would turn into nonsense. (The server-side
        /// [`SnapshotEnvelope::parse`] already rejects one on the wire;
        /// this covers the calendar lane, whose cadence comes from a host
        /// constant no parser ever sees.)
        declared_cadence_ms: Option<i64>,
    },
}

impl Freshness {
    /// Measure an answer's age against a caller-supplied `now_ms`.
    ///
    /// **The clock rule, stated rather than implied by a `max`.** Both
    /// stamps are read from the *device's* clock — `fetched_at_ms` came
    /// off the wire from a server whose clock the device cannot check, and
    /// `now_ms` is the device's own. A fetch stamp in the future therefore
    /// means the two clocks disagree, not that the answer is from the
    /// future; the honest reading of a disagreement is **"as fresh as it
    /// could possibly be"**, so age clamps to `0` and never goes negative.
    /// The alternative — a negative age propagating into a band function —
    /// would read as fresher-than-fresh and could not be distinguished
    /// from a good answer. This is the **only** place that clamp happens;
    /// no caller, in Rust or TS, may repeat it.
    ///
    /// `fetched_at_ms: None` is [`Freshness::Unknown`]. That is the whole
    /// reason it is an `Option` here rather than a sentinel `0`.
    pub fn measure(
        now_ms: i64,
        fetched_at_ms: Option<i64>,
        declared_cadence_ms: Option<i64>,
    ) -> Freshness {
        let Some(fetched_at_ms) = fetched_at_ms else {
            return Freshness::Unknown;
        };
        Freshness::Age {
            age_ms: now_ms.saturating_sub(fetched_at_ms).max(0),
            declared_cadence_ms: declared_cadence_ms.filter(|ms| *ms > 0),
        }
    }

    /// The snapshot lane: a `context_snapshots` row's own `fetched_at`,
    /// with the cadence its [`SnapshotEnvelope`] declares.
    ///
    /// A **broken envelope costs the cadence, not the age**. The row still
    /// carries a real `fetched_at`, so the answer is `Age { .. ,
    /// declared_cadence_ms: None }` — the pane can say "as of four hours
    /// ago" while it renders its own malformed state. The reason for the
    /// break is not this type's to report: the pane parses the payload
    /// itself and reads
    /// [`EnvelopeProblem`](hummingbird_domain::EnvelopeProblem) from there,
    /// which is what keeps "visibly broken, never quietly empty" a
    /// property of the pane rather than something freshness has to encode.
    pub fn of_snapshot(now_ms: i64, snapshot: &ContextSnapshot) -> Freshness {
        Freshness::measure(
            now_ms,
            Some(snapshot.fetched_at),
            SnapshotEnvelope::parse(&snapshot.payload)
                .ok()
                .and_then(|envelope| envelope.polled_every_ms),
        )
    }

    /// The measured age, or `None` when there is no stamp to measure.
    /// Deliberately not an `age_ms() -> i64` returning `0` for
    /// [`Freshness::Unknown`]: that shape is exactly how "unknown renders
    /// as fresh" gets reintroduced one call site at a time.
    pub fn age_ms(&self) -> Option<i64> {
        match self {
            Freshness::Unknown => None,
            Freshness::Age { age_ms, .. } => Some(*age_ms),
        }
    }

    /// What the source says normal looks like, or `None` when it did not
    /// say (or there is no answer to say it about).
    pub fn declared_cadence_ms(&self) -> Option<i64> {
        match self {
            Freshness::Unknown => None,
            Freshness::Age { declared_cadence_ms, .. } => *declared_cadence_ms,
        }
    }

    /// The one stale decision, against a **caller-supplied** threshold —
    /// each pane's own, in TS, because the threshold is not carved out.
    ///
    /// [`Freshness::Unknown`] is stale against every threshold, including
    /// an absurdly large one. That is the invariant this module exists for,
    /// and it lives here so that no pane can express the question in a way
    /// that answers it differently.
    pub fn is_stale_beyond(&self, threshold_ms: i64) -> bool {
        match self {
            Freshness::Unknown => true,
            Freshness::Age { age_ms, .. } => *age_ms > threshold_ms,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(payload: &str, fetched_at: i64) -> ContextSnapshot {
        ContextSnapshot {
            source: "city-waste/v2".to_string(),
            key: "next_collection".to_string(),
            payload: payload.to_string(),
            fetched_at,
            version: 1,
        }
    }

    #[test]
    fn unknown_is_stale_against_every_threshold() {
        // The invariant the carve-out exists for. Not "stale past some
        // large number" — stale past all of them, including one no real
        // age could ever exceed.
        let unknown = Freshness::measure(1_000, None, Some(60_000));
        assert_eq!(unknown, Freshness::Unknown);
        for threshold in [0, 1, 60_000, i64::MAX] {
            assert!(unknown.is_stale_beyond(threshold), "threshold {threshold}");
        }
        assert_eq!(unknown.age_ms(), None);
        assert_eq!(unknown.declared_cadence_ms(), None);
    }

    #[test]
    fn a_declared_cadence_does_not_rescue_an_unknown_age() {
        // The two unknowns are not interchangeable: knowing the cadence
        // says nothing about whether anything was ever fetched.
        assert_eq!(Freshness::measure(1_000, None, Some(21_600_000)), Freshness::Unknown);
    }

    #[test]
    fn a_known_age_with_no_declared_cadence_is_the_other_unknown() {
        // We know the age; we do not know what normal looks like. A
        // legitimate state, and distinct from `Unknown` at the type level.
        let freshness = Freshness::measure(5_000, Some(1_000), None);
        assert_eq!(freshness, Freshness::Age { age_ms: 4_000, declared_cadence_ms: None });
        assert_eq!(freshness.age_ms(), Some(4_000));
        assert!(!freshness.is_stale_beyond(4_000), "an age equal to the threshold is not past it");
        assert!(freshness.is_stale_beyond(3_999));
    }

    #[test]
    fn a_fetch_stamp_in_the_future_clamps_to_zero_rather_than_reading_as_negative() {
        // The clock rule. The device and the source disagree; the honest
        // reading is "as fresh as it could possibly be", and a negative
        // age must never reach a band function.
        let skewed = Freshness::measure(1_000, Some(9_999), Some(60_000));
        assert_eq!(skewed.age_ms(), Some(0));
        assert!(!skewed.is_stale_beyond(0));

        // Even at the extremes the clamp holds rather than wrapping.
        assert_eq!(Freshness::measure(i64::MIN, Some(i64::MAX), None).age_ms(), Some(0));
        assert_eq!(
            Freshness::measure(i64::MAX, Some(i64::MIN), None).age_ms(),
            Some(i64::MAX),
        );
    }

    #[test]
    fn a_non_positive_declared_cadence_is_read_as_not_declared() {
        // A cadence of zero or less cannot say what normal looks like.
        // Carrying it would hand every downstream threshold a number that
        // makes nonsense of itself; `None` says the true thing.
        for cadence in [0, -1, i64::MIN] {
            assert_eq!(
                Freshness::measure(5_000, Some(1_000), Some(cadence)).declared_cadence_ms(),
                None,
                "cadence {cadence}",
            );
        }
    }

    #[test]
    fn the_snapshot_lane_takes_its_cadence_from_the_envelope() {
        let row = snapshot(
            r#"{"schema": "city-waste/v2", "polled_every_ms": 86400000, "body": {}}"#,
            1_000,
        );
        assert_eq!(
            Freshness::of_snapshot(4_600_000, &row),
            Freshness::Age { age_ms: 4_599_000, declared_cadence_ms: Some(86_400_000) },
        );
    }

    #[test]
    fn a_broken_envelope_costs_the_cadence_but_not_the_age() {
        // The row still carries a real `fetched_at`, so the pane can say
        // "as of an hour ago" while rendering its own malformed state. It
        // reads the *reason* from its own parse, not from here.
        for payload in ["not json at all", "[]", r#"{"body": {}}"#, r#"{"schema": "s/v1"}"#] {
            assert_eq!(
                Freshness::of_snapshot(3_601_000, &snapshot(payload, 1_000)),
                Freshness::Age { age_ms: 3_600_000, declared_cadence_ms: None },
                "{payload}",
            );
        }
    }

    #[test]
    fn a_snapshot_row_is_never_unknown_because_fetched_at_is_not_nullable() {
        // `context_snapshots.fetched_at` is a non-null column — a row that
        // exists was fetched. `Unknown` on this lane means "no row", which
        // is the caller's `None`, not a shape this constructor can produce.
        let just_written = Freshness::of_snapshot(1_000, &snapshot(r#"{"schema":"s/v1"}"#, 1_000));
        assert_eq!(just_written.age_ms(), Some(0));
    }

    #[test]
    fn the_calendar_lane_is_the_same_type_with_a_host_supplied_cadence() {
        // No envelope exists on this lane; the cadence is ADR-0005's
        // 15-minute foreground poll, injected by the host exactly as the
        // clock is.
        const TIMER_INTERVAL_MS: i64 = 15 * 60 * 1000;
        let freshness = Freshness::measure(3_000_000, Some(1_200_000), Some(TIMER_INTERVAL_MS));
        assert_eq!(
            freshness,
            Freshness::Age { age_ms: 1_800_000, declared_cadence_ms: Some(900_000) },
        );
        // And the same absence answers the same way on either lane.
        assert!(Freshness::measure(3_000_000, None, Some(TIMER_INTERVAL_MS))
            .is_stale_beyond(i64::MAX));
    }

    #[test]
    fn the_wire_shape_names_the_state_and_round_trips() {
        // Whichever way the seam decision goes, the two arms must stay
        // distinguishable on the wire — a shape that serialises `Unknown`
        // as a missing/zero age is the same collapse in another costume.
        let unknown = serde_json::to_string(&Freshness::Unknown).unwrap();
        assert_eq!(unknown, r#"{"state":"unknown"}"#);
        let aged = Freshness::Age { age_ms: 4_000, declared_cadence_ms: Some(900_000) };
        assert_eq!(
            serde_json::to_string(&aged).unwrap(),
            r#"{"state":"age","age_ms":4000,"declared_cadence_ms":900000}"#,
        );
        for value in [Freshness::Unknown, aged] {
            let json = serde_json::to_string(&value).unwrap();
            assert_eq!(serde_json::from_str::<Freshness>(&json).unwrap(), value);
        }
    }
}
