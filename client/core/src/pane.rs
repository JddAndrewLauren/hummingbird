//! The **generic pane read** (#245, ADR-0015): for one source, its snapshot
//! rows and its live alerts, in the shape a standing question's pane needs
//! before it can answer anything.
//!
//! A top-level view module beside [`crate::bindings`], on the same
//! discipline: `Serialize`-only bespoke types, built fresh per read, never
//! stored. Nothing here is a schema shape — [`PaneSnapshot`] is a
//! `context_snapshots` row *as a reader sees it*, with the envelope already
//! parsed and the age already measured.
//!
//! **What is generic and what is not.** This module knows that a snapshot
//! carries a [`SnapshotEnvelope`] and that an alert has a lifecycle; it
//! knows nothing about what any particular pane means by its answer. The
//! answer state, the band, the headline and the threshold all live beside
//! each pane's own band function, not here — ADR-0015's carve-out. (Where
//! that is has moved: ADR-0015 put those modules in TS, ADR-0025 sinks the
//! deciding half of them — band and threshold — into this crate as each
//! screen is built for Android, leaving the headline per-client. Either
//! way, none of it is *this* module's: it stays generic.) The two things
//! that must not drift — the age clamp
//! ([`crate::freshness::Freshness::measure`]) and alert liveness
//! ([`hummingbird_domain::Alert::is_live`]) — are applied here, once.
//!
//! **The `schema` is never registry-checked.** An unrecognised one rides
//! through untouched into [`PaneEnvelope::Parsed`] and the pane renders its
//! own "this device is behind" state; a check against
//! `hummingbird_domain::REGISTRY` here would turn a fact about this build
//! into a broken row (ADR-0015, and [`SnapshotEnvelope`]'s own doc).

use hummingbird_domain::{Alert, ContextSnapshot, SnapshotEnvelope};
use serde::Serialize;

use crate::freshness::Freshness;

/// One source's whole pane-facing read: its snapshot rows (key order) and
/// the alerts it has raised that are live right now (id order).
///
/// Deliberately one call rather than a row-at-a-time API: freshness is
/// embedded per row, so a pane with several snapshots does not make one
/// `snapshot_freshness` seam crossing per row, and the clock is injected
/// exactly once for the whole read.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PaneRead {
    pub snapshots: Vec<PaneSnapshot>,
    /// The source's **live** alerts (ADR-0014's predicate, against the
    /// caller's clock). Full rows, `subject_key` included and untouched:
    /// the `(source, subject_key)` ↔ `(source, key)` join is the pane's own,
    /// in TS, and it is *additive* — an alert matching no pane is not
    /// dropped here, it simply finds no pane to join.
    pub alerts: Vec<Alert>,
}

/// One `context_snapshots` row as a pane reads it: its identity, its stamps,
/// how old it is, and its envelope already parsed.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PaneSnapshot {
    pub source: String,
    pub key: String,
    pub fetched_at: i64,
    pub version: i64,
    /// How old this row is, measured once, core-side — the finished value,
    /// never `fetched_at` handed over for TS to subtract from (ADR-0015's
    /// clock rule lives in [`Freshness::measure`] and nowhere else).
    ///
    /// A **broken envelope costs the cadence, not the age**: a
    /// [`PaneEnvelope::Malformed`] row still carries
    /// `Age { declared_cadence_ms: None }`, never [`Freshness::Unknown`],
    /// exactly as [`Freshness::of_snapshot`] does — pinned by test, since
    /// this read parses the payload once and derives both from that one
    /// parse rather than calling it.
    pub freshness: Freshness,
    pub envelope: PaneEnvelope,
}

/// The envelope, read shallowly — or the reason it could not be.
///
/// "Visibly broken, never quietly empty" (ADR-0015): a payload this build
/// cannot read arrives as [`PaneEnvelope::Malformed`] carrying
/// [`hummingbird_domain::EnvelopeProblem`]'s own wording, so the pane can
/// put *what was wrong* on screen rather than rendering an empty answer that
/// looks like "nothing is happening".
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum PaneEnvelope {
    Parsed {
        /// The payload shape's versioned name, passed through untouched —
        /// never checked against the source registry.
        schema: String,
        polled_every_ms: Option<i64>,
        /// The source-shaped payload as opaque JSON *text*. This crate does
        /// not parse it and must not: the body shape belongs to the pane
        /// that renders it.
        body: String,
    },
    Malformed {
        /// [`hummingbird_domain::EnvelopeProblem`]'s `Display` wording —
        /// a sentence a pane can render as-is.
        reason: String,
    },
}

impl PaneSnapshot {
    /// Reads one mirror row against the caller's clock. The payload is
    /// parsed exactly once: the envelope view and the declared cadence
    /// behind [`Freshness`] both come from that single parse, which is what
    /// keeps this in step with [`Freshness::of_snapshot`] rather than
    /// merely near it.
    pub(crate) fn of_row(now_ms: i64, snapshot: &ContextSnapshot) -> PaneSnapshot {
        let (freshness, envelope) = match SnapshotEnvelope::parse(&snapshot.payload) {
            Ok(envelope) => (
                Freshness::measure(now_ms, Some(snapshot.fetched_at), envelope.polled_every_ms),
                PaneEnvelope::Parsed {
                    schema: envelope.schema,
                    polled_every_ms: envelope.polled_every_ms,
                    body: envelope.body,
                },
            ),
            Err(problem) => (
                Freshness::measure(now_ms, Some(snapshot.fetched_at), None),
                PaneEnvelope::Malformed {
                    reason: problem.to_string(),
                },
            ),
        };
        PaneSnapshot {
            source: snapshot.source.clone(),
            key: snapshot.key.clone(),
            fetched_at: snapshot.fetched_at,
            version: snapshot.version,
            freshness,
            envelope,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(payload: &str) -> ContextSnapshot {
        ContextSnapshot {
            source: "city-waste/v2".to_string(),
            key: "collection".to_string(),
            payload: payload.to_string(),
            fetched_at: 1_000,
            version: 3,
        }
    }

    #[test]
    fn a_parsed_row_carries_the_body_through_opaque() {
        let snapshot = PaneSnapshot::of_row(
            61_000,
            &row(r#"{"schema":"city-waste/v2","polled_every_ms":86400000,"body":{"zone":"Europe/London"}}"#),
        );
        assert_eq!(
            snapshot.freshness,
            Freshness::Age { age_ms: 60_000, declared_cadence_ms: Some(86_400_000) }
        );
        match snapshot.envelope {
            PaneEnvelope::Parsed { schema, polled_every_ms, body } => {
                assert_eq!(schema, "city-waste/v2");
                assert_eq!(polled_every_ms, Some(86_400_000));
                // Still JSON, still unparsed by this crate.
                let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
                assert_eq!(parsed["zone"], "Europe/London");
            }
            other => panic!("expected a parsed envelope, got {other:?}"),
        }
    }

    #[test]
    fn a_malformed_row_names_the_reason_and_keeps_its_age() {
        // The row was really fetched; only the cadence is lost.
        let snapshot = PaneSnapshot::of_row(61_000, &row(r#"{"schema":"s/v1"}"#));
        assert_eq!(
            snapshot.freshness,
            Freshness::Age { age_ms: 60_000, declared_cadence_ms: None }
        );
        assert_eq!(
            snapshot.envelope,
            PaneEnvelope::Malformed { reason: "`body` is missing".to_string() }
        );
    }

    #[test]
    fn an_unrecognised_schema_is_passed_through_untouched() {
        let snapshot =
            PaneSnapshot::of_row(1_000, &row(r#"{"schema":"not-a-real-source/v9","body":{}}"#));
        assert!(matches!(
            snapshot.envelope,
            PaneEnvelope::Parsed { ref schema, .. } if schema == "not-a-real-source/v9"
        ));
    }

    #[test]
    fn the_envelope_serializes_as_a_tagged_union_never_a_nullable_field() {
        assert_eq!(
            serde_json::to_string(&PaneEnvelope::Malformed { reason: "boom".to_string() }).unwrap(),
            r#"{"state":"malformed","reason":"boom"}"#
        );
        assert_eq!(
            serde_json::to_string(&PaneEnvelope::Parsed {
                schema: "s/v1".to_string(),
                polled_every_ms: None,
                body: "{}".to_string(),
            })
            .unwrap(),
            r#"{"state":"parsed","schema":"s/v1","polled_every_ms":null,"body":"{}"}"#
        );
    }
}
