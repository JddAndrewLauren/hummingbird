//! The `context_snapshots.payload` this poller writes: ADR-0015's envelope
//! around a `city-waste/v2` body.
//!
//! **This is one half of a cross-language contract.** The other half is
//! `client/web/src/screens/waste-pane/waste.ts::parseWasteBody`, and nothing
//! mechanically connects them — the body inside the envelope is deliberately
//! unfrozen and opaque to the server, so no type, no schema and no test on
//! either side can see the other. `the_body_this_poller_writes_is_the_body_the_pane_parses`
//! in `tests/contract.rs` is the only guard against the two drifting, which
//! is why it asserts the literal snake_case key names rather than going
//! through this module's own serde.
//!
//! `cadence` is the one field the prototype's shape gained. It is additive
//! and the pane ignores it (`parseWasteBody` does no unknown-field
//! rejection), and it is what makes materiality decidable without a previous
//! snapshot — see [`crate::judge`].
//!
//! There is deliberately **no `deviation` field**. The judgement is derivable
//! from `scheduled` vs `collected_on`, and a judgement duplicated into a
//! payload is a fact that can disagree with itself.

use serde::Serialize;

use crate::cadence::Cadence;
use crate::date::Date;

/// How often this poller says it runs, for `Freshness`'s declared cadence.
/// **Must match `.github/workflows/city-waste.yml`'s cron**; the workflow's
/// header says so from the other side.
pub const POLLED_EVERY_MS: i64 = 24 * 60 * 60 * 1000;

/// The one `context_snapshots.key` this source owns — the whole answer is
/// one row, replaced wholesale by each poll — and the `subject_key` every
/// alert carries, which is the `(source, subject_key)` ↔ `(source, key)`
/// join's own half.
pub const SNAPSHOT_KEY: &str = "collection";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CadenceBody {
    pub anchor: String,
    pub every_n_weeks: i64,
}

/// The `city-waste/v2` body. Field names are the wire contract; see the
/// module note above before renaming one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WasteBody {
    /// The IANA zone both dates are civil in. Every day-shaped question on
    /// the pane is resolved in it, so "tonight" flips at the address's
    /// midnight and not the device's.
    pub zone: String,
    /// Where the cadence puts this week's collection. **Derived**, not read
    /// off the page.
    pub scheduled: String,
    /// Where it actually happens. **Observed.** `collected_on != scheduled`
    /// is exactly how a holiday is read — off the snapshot, never off an
    /// alert.
    pub collected_on: String,
    pub streams: Vec<String>,
    pub cadence: CadenceBody,
}

impl WasteBody {
    pub fn new(
        zone: &str,
        cadence: Cadence,
        collected_on: Date,
        streams: Vec<String>,
    ) -> WasteBody {
        WasteBody {
            zone: zone.to_string(),
            // Derived here and nowhere else, from the same cadence `judge`
            // reads, so the pane's "is this a holiday?" and the poller's
            // "should this ring?" can never answer differently.
            scheduled: cadence.latest_on_or_before(collected_on).iso(),
            collected_on: collected_on.iso(),
            streams,
            cadence: CadenceBody {
                anchor: cadence.anchor.iso(),
                every_n_weeks: cadence.every_n_weeks,
            },
        }
    }

    /// Wraps the body in ADR-0015's envelope — the `payload` value of a
    /// `POST /api/snapshots`.
    pub fn envelope(&self) -> serde_json::Value {
        serde_json::json!({
            "schema": hummingbird_domain::CITY_WASTE_V2,
            "polled_every_ms": POLLED_EVERY_MS,
            "body": self,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn weekly() -> Cadence {
        Cadence { anchor: Date::parse("2026-08-03").unwrap(), every_n_weeks: 1 }
    }

    #[test]
    fn scheduled_is_derived_from_the_cadence_never_copied_from_the_observation() {
        let body = WasteBody::new(
            "America/Los_Angeles",
            weekly(),
            Date::parse("2026-08-18").unwrap(),
            vec!["trash".into()],
        );
        assert_eq!(body.collected_on, "2026-08-18", "observed");
        assert_eq!(body.scheduled, "2026-08-17", "derived — the Monday it moved from");
    }

    #[test]
    fn an_ordinary_week_has_the_two_dates_equal() {
        let body = WasteBody::new(
            "America/Los_Angeles",
            weekly(),
            Date::parse("2026-08-17").unwrap(),
            vec!["trash".into()],
        );
        assert_eq!(body.scheduled, body.collected_on, "no holiday to read");
    }

    /// The envelope half of the contract: `SnapshotEnvelope::parse` is the
    /// exact check `POST /api/snapshots` runs, so anything this poller can
    /// build must survive it.
    #[test]
    fn the_envelope_this_poller_builds_passes_the_authoritys_own_parse() {
        let body = WasteBody::new(
            "America/Los_Angeles",
            weekly(),
            Date::parse("2026-08-18").unwrap(),
            vec!["trash".into(), "recycling".into()],
        );
        let payload = body.envelope().to_string();
        let parsed = hummingbird_domain::SnapshotEnvelope::parse(&payload)
            .expect("the authority accepts what this poller writes");
        assert_eq!(parsed.schema, "city-waste/v2");
        assert_eq!(parsed.polled_every_ms, Some(POLLED_EVERY_MS));
    }
}
