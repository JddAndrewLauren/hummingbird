//! **The which-cans question** (#120), sunk out of
//! `screens/waste-pane/waste.ts` by ADR-0025 (#533/M4) — and M4's proof
//! that a civil-date pane can be decided by a core with no tzdb.
//!
//! What is here is the *deciding* half: the payload parser that pins the
//! `city-waste/v2` body shape, the setup arm, the days-away/holiday/stale
//! derivations, the band and its threshold (ADR-0015 keeps those two
//! together, and ADR-0025 moves the pair), and the gap **kinds**.
//!
//! What is not here is every word. [`WasteGap`] is a structured enum, not a
//! sentence: "Trash Tonight", "Trash today", "The collection payload isn't
//! JSON" and the bins' own colours are renderings, and a second client
//! phrasing them differently is a design choice rather than a bug. The one
//! sentence fragment that does cross is [`WasteGap::Malformed`]'s `reason`,
//! which is the *domain's* own wording (`EnvelopeProblem`) passed through
//! as data rather than composed here.
//!
//! **The zone check moved, it did not vanish.** `waste.ts` refused an
//! unusable zone inside `parseWasteBody` by calling `zonedMidnightMs`;
//! nothing here can. So [`parse_waste_body`] answers only about the
//! payload's *shape*, and the refusal becomes
//! [`WasteGap::UnresolvableZone`] — reached in [`waste_facts`] exactly when
//! the host omitted the fact (`zone.rs`'s module header). The decision is
//! still a core decision; only the lookup is the host's.

use serde::{Deserialize, Serialize};

use super::contract::{AnswerState, Band, PaneAnswerCore};
use super::inputs::{BindingValueFact, FreshnessFact, PaneEnvelopeFacts, PaneInputs, PaneSnapshotFacts};
use super::zone::{civil_days_between, is_civil_date, weekday_index, CivilDate, ZoneFacts, ZoneQuery};

/// Both the snapshot's source and every alert's — ADR-0009's join
/// constraint is that these are one string. `/v2` because `city-waste/v1`
/// is retired. Nothing here checks it against the frozen source registry
/// (`server/domain/src/sources.rs`); ADR-0015 forbids checking a snapshot's
/// `schema` against one, so this constant is this module's own and the two
/// agree by review.
pub const SOURCE: &str = "city-waste/v2";

/// The one `context_snapshots.key` this question reads, and its subject key.
pub const SNAPSHOT_KEY: &str = "collection";

/// The binding that has to be set before this question can be asked at all.
pub const BINDING_KEY: &str = "city-waste-page";

/// How old an answer may be before it is worth saying so — beside the band
/// function, where ADR-0015 puts every threshold.
///
/// **26 hours, not `2 ×` the daily cadence.** The driver is the cost of a
/// wrong answer, not the poll interval: a 47-hour-old waste answer can be a
/// whole collection cycle out of date and would render "Trash Tonight" on
/// the wrong night, which is worse than saying nothing. A slightly-late
/// daily poll (25h) is routine and must stay quiet, so the line sits just
/// past a day.
pub const STALE_AFTER_MS: i64 = 26 * 60 * 60 * 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Stream {
    Trash,
    Recycling,
    Yard,
}

impl Stream {
    pub fn as_str(self) -> &'static str {
        match self {
            Stream::Trash => "trash",
            Stream::Recycling => "recycling",
            Stream::Yard => "yard",
        }
    }

    pub fn parse(s: &str) -> Option<Stream> {
        STREAM_ORDER.into_iter().find(|stream| stream.as_str() == s)
    }
}

/// Kerb order — the order the bins stand in outside, never the order the
/// payload happened to list.
pub const STREAM_ORDER: [Stream; 3] = [Stream::Trash, Stream::Recycling, Stream::Yard];

/// The `city-waste/v2` payload body — **this module's parser is what pins
/// the shape**, since the body inside ADR-0015's envelope is deliberately
/// unfrozen and opaque to everything else.
///
/// `scheduled` is where the cadence puts this week's collection and
/// `collected_on` is where it actually happens; they differ exactly on a
/// holiday week, which is how a holiday is read — off the snapshot, never
/// off an alert.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WasteBody {
    /// IANA zone the two dates are civil in. Carried verbatim and never
    /// validated here — see the module header.
    pub zone: String,
    pub scheduled: CivilDate,
    pub collected_on: CivilDate,
    /// In the payload's own order. Kerb order is a rendering concern and
    /// is applied by the client that draws the bins.
    pub streams: Vec<Stream>,
}

/// Why this pane has no answer — a **kind**, not a sentence.
///
/// Every arm names what was wrong so a client can say it in its own words.
/// [`WasteGap::UnknownSchema`] is separate from [`WasteGap::Malformed`] on
/// purpose: a newer build wrote a shape this one has never heard of, which
/// is a fact about *this device* and is fixed by updating it, not by
/// looking at the council's website.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "gap", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum WasteGap {
    /// No snapshot row at all: nothing has ever been fetched.
    NotFetched,
    /// The envelope itself could not be read. `reason` is
    /// `hummingbird_domain::EnvelopeProblem`'s own wording, passed through
    /// as data.
    Malformed { reason: String },
    UnknownSchema { schema: String },
    NotJson,
    NotAnObject,
    NoZone,
    BadDates,
    UnknownStream,
    /// The payload's zone is one the host could not resolve — the absent
    /// fact of `zone.rs`'s bridge, decided here.
    UnresolvableZone { zone: String },
    /// **A collection in the past is not an answer.** The poll is daily, so
    /// between the address's midnight and the day's fetch the snapshot
    /// still names the collection that has already happened — comfortably
    /// inside the 26h stale line, so freshness says nothing about it.
    /// Rendering it would put "Trash today" on screen about yesterday.
    PastCollection { collected_on: CivilDate, weekday_index: u8 },
}

/// Whether the question has been asked at all — **four answers, not a
/// boolean**, for the same reason `BindingValueDTO` is not `string | null`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum WasteSetup {
    Bound { page: String },
    /// The bindings table has not been read on this device yet. *Not*
    /// "nobody has set the page".
    Unread,
    /// A row exists but holds something that is not text. A gap the reader
    /// can act on, never an absence.
    Unusable,
    /// No row, or one holding nothing — the only arm that is genuinely
    /// `unbound`.
    Unset,
}

/// Everything an answered pane needs, decided once and read by both the
/// answer and the client's expanded rendering. **No rendered sentence
/// crosses** — a client composes its headline from these facts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WasteFacts {
    pub zone: String,
    pub scheduled: CivilDate,
    pub collected_on: CivilDate,
    pub streams: Vec<Stream>,
    /// The civil date it is *right now at the address* — the day "today"
    /// means to the person whose bins these are, not to the device.
    pub today: CivilDate,
    /// Whole days from today at the address to the collection. **Never
    /// negative**: a past collection is [`WasteGap::PastCollection`].
    pub days_away: i64,
    /// The city moved this week's collection off its cadence day. Read
    /// straight off the snapshot (`collected_on != scheduled`), never from
    /// the alert lane: with no card to acknowledge there is nothing to
    /// quiet, and the changed day IS the answer.
    pub holiday: bool,
    /// The collection day's weekday, `0` = Sunday — the index, because the
    /// word is per-client (`zone.rs`'s [`weekday_index`]).
    pub weekday_index: u8,
    pub stale: bool,
    /// **Epoch ms of the collection day's start at the address** — the
    /// `within_band` instant, resolved by the host through the bridge.
    pub starts_at_ms: i64,
    pub freshness: FreshnessFact,
}

/// The whole answered fact set, or the reason there is none. One type, so
/// the decision to be a gap and the gap's kind cannot disagree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum WasteResolved {
    Facts(WasteFacts),
    Gap { gap: WasteGap },
}

/// Reads one snapshot row into a body, or says why it could not.
///
/// Shape only. A zone this build cannot resolve is not a shape problem and
/// is refused later, in [`waste_facts`] — see the module header.
pub fn parse_waste_body(snapshot: Option<&PaneSnapshotFacts>) -> Result<WasteBody, WasteGap> {
    let Some(snapshot) = snapshot else {
        return Err(WasteGap::NotFetched);
    };
    let (schema, body) = match &snapshot.envelope {
        PaneEnvelopeFacts::Malformed { reason } => {
            return Err(WasteGap::Malformed { reason: reason.clone() })
        }
        PaneEnvelopeFacts::Ok { schema, body } => (schema, body),
    };
    if schema != SOURCE {
        return Err(WasteGap::UnknownSchema { schema: schema.clone() });
    }

    let parsed: serde_json::Value =
        serde_json::from_str(body).map_err(|_| WasteGap::NotJson)?;
    let object = parsed.as_object().ok_or(WasteGap::NotAnObject)?;

    let zone = match object.get("zone").and_then(|v| v.as_str()) {
        Some(zone) if !zone.is_empty() => zone.to_string(),
        _ => return Err(WasteGap::NoZone),
    };
    let day = |field: &str| -> Option<CivilDate> {
        object
            .get(field)
            .and_then(|v| v.as_str())
            .filter(|value| is_civil_date(value))
            .map(str::to_string)
    };
    let (Some(scheduled), Some(collected_on)) = (day("scheduled"), day("collected_on")) else {
        return Err(WasteGap::BadDates);
    };
    let raw_streams = object.get("streams").and_then(|v| v.as_array()).ok_or(WasteGap::UnknownStream)?;
    let streams = raw_streams
        .iter()
        .map(|value| value.as_str().and_then(Stream::parse).ok_or(WasteGap::UnknownStream))
        .collect::<Result<Vec<Stream>, WasteGap>>()?;

    Ok(WasteBody { zone, scheduled, collected_on, streams })
}

/// Whether the collection page has been set, and if not, which kind of
/// not-set it is.
pub fn waste_setup(inputs: &PaneInputs) -> WasteSetup {
    if inputs.bindings.is_none() {
        return WasteSetup::Unread;
    }
    match inputs.binding(BINDING_KEY).map(|binding| &binding.value) {
        None | Some(BindingValueFact::Unset) => WasteSetup::Unset,
        Some(BindingValueFact::Other) => WasteSetup::Unusable,
        Some(BindingValueFact::Text { text }) => {
            let page = text.trim();
            // A row blanked to whitespace is the nearest thing `settings`
            // has to a DELETE, and reads as never having been set.
            if page.is_empty() {
                WasteSetup::Unset
            } else {
                WasteSetup::Bound { page: page.to_string() }
            }
        }
    }
}

/// Every `(zone, civil-date)` fact this question needs, given these inputs
/// — the core half of `zone.rs`'s bridge.
///
/// Empty whenever the payload does not parse: there is no zone to ask
/// about, and asking anyway would make the host resolve facts for a body
/// that is already a gap.
pub fn waste_zone_queries(inputs: &PaneInputs) -> Vec<ZoneQuery> {
    let Ok(body) = parse_waste_body(inputs.snapshot(SOURCE, SNAPSHOT_KEY)) else {
        return Vec::new();
    };
    vec![
        ZoneQuery::CivilDate { zone: body.zone.clone(), at_ms: inputs.now_ms },
        ZoneQuery::Midnight { zone: body.zone, date: body.collected_on },
    ]
}

/// The whole answered fact set, or the reason there is none.
pub fn waste_facts(inputs: &PaneInputs, facts: &ZoneFacts) -> WasteResolved {
    let snapshot = inputs.snapshot(SOURCE, SNAPSHOT_KEY);
    let body = match parse_waste_body(snapshot) {
        Ok(body) => body,
        Err(gap) => return WasteResolved::Gap { gap },
    };
    // `parse_waste_body` returning `Ok` guarantees the row exists.
    let freshness = snapshot.expect("a parsed body implies a snapshot row").freshness;

    let (Some(today), Some(starts_at_ms)) = (
        facts.civil_date(&body.zone, inputs.now_ms),
        facts.midnight_ms(&body.zone, &body.collected_on),
    ) else {
        return WasteResolved::Gap { gap: WasteGap::UnresolvableZone { zone: body.zone } };
    };
    // Both dates were civil-checked on the way in, so this cannot be `None`
    // — but it is read as an answer rather than unwrapped, since a host
    // could resolve `today` to any string it liked.
    let Some(days_away) = civil_days_between(&today, &body.collected_on) else {
        return WasteResolved::Gap { gap: WasteGap::UnresolvableZone { zone: body.zone } };
    };
    let weekday = weekday_index(&body.collected_on).unwrap_or(0);
    if days_away < 0 {
        return WasteResolved::Gap {
            gap: WasteGap::PastCollection {
                collected_on: body.collected_on,
                weekday_index: weekday,
            },
        };
    }

    WasteResolved::Facts(WasteFacts {
        holiday: body.collected_on != body.scheduled,
        zone: body.zone,
        scheduled: body.scheduled,
        collected_on: body.collected_on,
        streams: body.streams,
        today,
        days_away,
        weekday_index: weekday,
        stale: freshness.is_stale_beyond(STALE_AFTER_MS),
        starts_at_ms,
        freshness,
    })
}

/// This question's answer for the shell (#245), minus its rendering half.
///
/// The band is deliberately **binary** — `dormant` or `imminent` — because
/// the real question only has two settings: it is either the night the cans
/// go out (or the day itself, or any day of a week whose day has moved), or
/// it is furniture. A `within_band` is produced whenever there is an
/// answer, including while dormant, so two dormant panes still order
/// meaningfully against each other rather than alphabetically.
///
/// The alert lane is deliberately never read. A holiday *is* the answer on
/// this pane, so joining it would render the same fact twice and give the
/// reader something to dismiss that would not change what they have to do.
pub fn waste_answer(inputs: &PaneInputs, facts: &ZoneFacts) -> PaneAnswerCore {
    let gap = |answer_state| PaneAnswerCore { answer_state, band: Band::Dormant, within_band: None };
    match waste_setup(inputs) {
        WasteSetup::Unset => return gap(AnswerState::Unbound),
        // Neither answered nor unbound: the table has not been read yet, or
        // it holds something this pane cannot use. Both are gaps — claiming
        // "not set up" here would state a fact about the reader's
        // configuration that this device has not established.
        WasteSetup::Unread | WasteSetup::Unusable => return gap(AnswerState::BoundButUnacquired),
        WasteSetup::Bound { .. } => {}
    }

    match waste_facts(inputs, facts) {
        WasteResolved::Gap { .. } => gap(AnswerState::BoundButUnacquired),
        WasteResolved::Facts(view) => PaneAnswerCore {
            answer_state: AnswerState::Answered,
            band: if view.holiday || view.days_away <= 1 { Band::Imminent } else { Band::Dormant },
            // Already in the past on the day itself, which is what sorts
            // today ahead of tomorrow inside the band.
            within_band: Some(view.starts_at_ms),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decisions::panes::zone::ZoneFact;

    const ZONE: &str = "America/Los_Angeles";
    /// Monday 2026-08-10, 09:00 at the address.
    const NOW: i64 = 1_786_377_600_000;

    fn body_json(zone: &str, scheduled: &str, collected_on: &str, streams: &str) -> String {
        format!(
            r#"{{"zone":"{zone}","scheduled":"{scheduled}","collected_on":"{collected_on}","streams":{streams}}}"#
        )
    }

    fn inputs_with(envelope: serde_json::Value, bindings: serde_json::Value) -> PaneInputs {
        serde_json::from_value(serde_json::json!({
            "nowMs": NOW,
            "bindings": bindings,
            "paneReads": {
                SOURCE: {
                    "snapshots": [{
                        "key": SNAPSHOT_KEY,
                        "envelope": envelope,
                        "freshness": {"kind":"age","ageMs":60000,"declaredCadenceMs":86400000},
                    }],
                },
            },
        }))
        .unwrap()
    }

    fn bound() -> serde_json::Value {
        serde_json::json!([
            {"key": BINDING_KEY, "value": {"state":"text","text":"https://example.gov"}}
        ])
    }

    fn ok_envelope(body: &str) -> serde_json::Value {
        serde_json::json!({"kind":"ok","schema":SOURCE,"body":body})
    }

    fn inputs(scheduled: &str, collected_on: &str) -> PaneInputs {
        inputs_with(
            ok_envelope(&body_json(ZONE, scheduled, collected_on, r#"["trash","yard"]"#)),
            bound(),
        )
    }

    /// A host resolver, stood up from the queries the core itself asked —
    /// which is the shape every real host takes. `resolve` answers `None`
    /// for a zone it does not know, and the key is then simply absent.
    fn resolve(queries: &[ZoneQuery]) -> ZoneFacts {
        let mut facts = ZoneFacts::default();
        for query in queries {
            match query {
                ZoneQuery::CivilDate { zone, at_ms } => {
                    if zone == ZONE {
                        // -7h in August; NOW is 16:00Z, i.e. 09:00 local.
                        let local = at_ms - 7 * 3_600_000;
                        let days = local.div_euclid(86_400_000);
                        let date = super::super::zone::add_civil_days("1970-01-01", days).unwrap();
                        facts.insert(query, ZoneFact::Date(date));
                    }
                }
                ZoneQuery::Midnight { zone, date } => {
                    if zone == ZONE {
                        let days =
                            super::super::zone::civil_days_between("1970-01-01", date).unwrap();
                        facts.insert(
                            query,
                            ZoneFact::Instant(days * 86_400_000 + 7 * 3_600_000),
                        );
                    }
                }
            }
        }
        facts
    }

    fn resolved(inputs: &PaneInputs) -> WasteResolved {
        waste_facts(inputs, &resolve(&waste_zone_queries(inputs)))
    }

    fn gap_of(inputs: &PaneInputs) -> WasteGap {
        match resolved(inputs) {
            WasteResolved::Gap { gap } => gap,
            WasteResolved::Facts(facts) => panic!("expected a gap, got {facts:?}"),
        }
    }

    fn facts_of(inputs: &PaneInputs) -> WasteFacts {
        match resolved(inputs) {
            WasteResolved::Facts(facts) => facts,
            WasteResolved::Gap { gap } => panic!("expected facts, got {gap:?}"),
        }
    }

    // -------------------------------------------------------- parse_waste_body

    #[test]
    fn reads_a_well_formed_body() {
        let inputs = inputs("2026-08-17", "2026-08-17");
        assert_eq!(
            parse_waste_body(inputs.snapshot(SOURCE, SNAPSHOT_KEY)),
            Ok(WasteBody {
                zone: ZONE.to_string(),
                scheduled: "2026-08-17".to_string(),
                collected_on: "2026-08-17".to_string(),
                streams: vec![Stream::Trash, Stream::Yard],
            }),
        );
    }

    #[test]
    fn names_which_kind_of_gap_it_is_rather_than_answering_emptily() {
        let cases: Vec<(serde_json::Value, WasteGap)> = vec![
            (
                serde_json::json!({"kind":"malformed","reason":"`body` is missing"}),
                WasteGap::Malformed { reason: "`body` is missing".into() },
            ),
            (
                serde_json::json!({"kind":"ok","schema":"city-waste/v9","body":"{}"}),
                WasteGap::UnknownSchema { schema: "city-waste/v9".into() },
            ),
            (ok_envelope("{"), WasteGap::NotJson),
            (ok_envelope("[]"), WasteGap::NotAnObject),
            (
                ok_envelope(&body_json("", "2026-08-17", "2026-08-17", r#"["trash"]"#)),
                WasteGap::NoZone,
            ),
            (
                ok_envelope(&body_json(ZONE, "2026-08-17", "next tuesday", r#"["trash"]"#)),
                WasteGap::BadDates,
            ),
            (
                ok_envelope(&body_json(ZONE, "2026-08-17", "2026-08-17", r#"["trash","nuclear"]"#)),
                WasteGap::UnknownStream,
            ),
            (
                ok_envelope(&body_json(ZONE, "2026-08-17", "2026-08-17", "\"trash\"")),
                WasteGap::UnknownStream,
            ),
        ];
        for (envelope, expected) in cases {
            let inputs = inputs_with(envelope, bound());
            assert_eq!(
                parse_waste_body(inputs.snapshot(SOURCE, SNAPSHOT_KEY)),
                Err(expected.clone()),
                "{expected:?}",
            );
        }
    }

    #[test]
    fn an_absent_snapshot_is_not_fetched_rather_than_malformed() {
        let inputs: PaneInputs =
            serde_json::from_value(serde_json::json!({"nowMs": NOW, "bindings": bound()})).unwrap();
        assert_eq!(parse_waste_body(inputs.snapshot(SOURCE, SNAPSHOT_KEY)), Err(WasteGap::NotFetched));
    }

    #[test]
    fn a_calendar_impossible_day_is_bad_dates_and_never_a_zone_problem() {
        // `zoned-day.ts`'s regex accepts 2026-02-31 and every downstream
        // answer is then `null`; the core refuses it once, at the door.
        let inputs = inputs_with(
            ok_envelope(&body_json(ZONE, "2026-02-31", "2026-02-31", r#"["trash"]"#)),
            bound(),
        );
        assert_eq!(gap_of(&inputs), WasteGap::BadDates);
    }

    // -------------------------------------------------------------- waste_setup

    #[test]
    fn the_setup_arm_distinguishes_unread_unset_unusable_and_bound() {
        let with = |bindings: serde_json::Value| {
            waste_setup(&inputs_with(
                ok_envelope(&body_json(ZONE, "2026-08-17", "2026-08-17", r#"["trash"]"#)),
                bindings,
            ))
        };
        assert_eq!(with(serde_json::Value::Null), WasteSetup::Unread);
        assert_eq!(with(serde_json::json!([])), WasteSetup::Unset);
        assert_eq!(
            with(serde_json::json!([{"key": BINDING_KEY, "value": {"state":"unset"}}])),
            WasteSetup::Unset,
        );
        // A row blanked to whitespace: `settings` has no DELETE.
        assert_eq!(
            with(serde_json::json!([{"key": BINDING_KEY, "value": {"state":"text","text":"   "}}])),
            WasteSetup::Unset,
        );
        assert_eq!(
            with(serde_json::json!([{"key": BINDING_KEY, "value": {"state":"other","raw":"7"}}])),
            WasteSetup::Unusable,
        );
        assert_eq!(
            with(bound()),
            WasteSetup::Bound { page: "https://example.gov".to_string() },
        );
    }

    // -------------------------------------------------------- the zone bridge

    #[test]
    fn asks_for_exactly_the_two_facts_it_cannot_answer_itself() {
        let queries = waste_zone_queries(&inputs("2026-08-17", "2026-08-17"));
        assert_eq!(
            queries,
            vec![
                ZoneQuery::CivilDate { zone: ZONE.to_string(), at_ms: NOW },
                ZoneQuery::Midnight { zone: ZONE.to_string(), date: "2026-08-17".to_string() },
            ],
        );
    }

    #[test]
    fn asks_for_nothing_when_the_payload_is_already_a_gap() {
        assert!(waste_zone_queries(&inputs_with(ok_envelope("{"), bound())).is_empty());
    }

    #[test]
    fn an_unresolvable_zone_is_a_gap_decided_here_not_a_fallback_invented_by_the_host() {
        let inputs = inputs_with(
            ok_envelope(&body_json(
                "Mars/Olympus_Mons",
                "2026-08-17",
                "2026-08-17",
                r#"["trash"]"#,
            )),
            bound(),
        );
        // The resolver above answers nothing for a zone it does not know,
        // so both keys are simply absent.
        assert_eq!(
            gap_of(&inputs),
            WasteGap::UnresolvableZone { zone: "Mars/Olympus_Mons".to_string() },
        );
        assert_eq!(waste_answer(&inputs, &resolve(&waste_zone_queries(&inputs))).answer_state,
            AnswerState::BoundButUnacquired);
    }

    // --------------------------------------------------------------- the facts

    #[test]
    fn resolves_the_day_at_the_address_never_on_the_device() {
        // 2026-08-11T03:00Z is still Monday the 10th in Los Angeles, so a
        // Tuesday collection is one day away.
        let mut late = inputs("2026-08-11", "2026-08-11");
        late.now_ms = 1_786_417_200_000;
        let facts = facts_of(&late);
        assert_eq!(facts.today, "2026-08-10");
        assert_eq!(facts.days_away, 1);
        assert_eq!(facts.weekday_index, 2); // Tuesday
    }

    #[test]
    fn reads_a_holiday_off_the_snapshot_as_a_moved_day() {
        let plain = facts_of(&inputs("2026-08-17", "2026-08-17"));
        assert!(!plain.holiday);
        let moved = facts_of(&inputs("2026-08-13", "2026-08-14"));
        assert!(moved.holiday);
        assert_eq!(moved.days_away, 4);
    }

    #[test]
    fn never_presents_a_collection_that_has_already_happened() {
        let inputs = inputs("2026-08-09", "2026-08-09");
        assert_eq!(
            gap_of(&inputs),
            WasteGap::PastCollection {
                collected_on: "2026-08-09".to_string(),
                weekday_index: 0, // Sunday
            },
        );
    }

    #[test]
    fn marks_an_answer_stale_past_the_threshold_without_losing_it() {
        let mut inputs = inputs("2026-08-17", "2026-08-17");
        let snapshot = inputs
            .pane_reads
            .get_mut(SOURCE)
            .and_then(|read| read.snapshots.first_mut())
            .unwrap();
        snapshot.freshness = FreshnessFact::Unknown;
        let facts = facts_of(&inputs);
        assert!(facts.stale);
        assert_eq!(facts.days_away, 7);
    }

    // -------------------------------------------------------------- the answer

    #[test]
    fn is_unbound_when_nobody_has_set_the_page_and_still_ranks_a_pane() {
        let inputs = inputs_with(
            ok_envelope(&body_json(ZONE, "2026-08-17", "2026-08-17", r#"["trash"]"#)),
            serde_json::json!([]),
        );
        let answer = waste_answer(&inputs, &ZoneFacts::default());
        assert_eq!(answer.answer_state, AnswerState::Unbound);
        assert_eq!(answer.band, Band::Dormant);
        assert_eq!(answer.within_band, None);
    }

    #[test]
    fn is_a_gap_rather_than_unbound_before_the_bindings_table_has_been_read() {
        let inputs = inputs_with(
            ok_envelope(&body_json(ZONE, "2026-08-17", "2026-08-17", r#"["trash"]"#)),
            serde_json::Value::Null,
        );
        assert_eq!(
            waste_answer(&inputs, &ZoneFacts::default()).answer_state,
            AnswerState::BoundButUnacquired,
        );
    }

    #[test]
    fn is_answered_dormant_and_still_ordered_seven_days_out() {
        let inputs = inputs("2026-08-17", "2026-08-17");
        let answer = waste_answer(&inputs, &resolve(&waste_zone_queries(&inputs)));
        assert_eq!(answer.answer_state, AnswerState::Answered);
        assert_eq!(answer.band, Band::Dormant);
        // An absolute instant — midnight at the address — never a duration.
        assert_eq!(answer.within_band, Some(1_786_950_000_000));
    }

    #[test]
    fn is_imminent_on_the_eve_on_the_day_itself_and_all_holiday_week() {
        for (scheduled, collected_on) in
            [("2026-08-11", "2026-08-11"), ("2026-08-10", "2026-08-10"), ("2026-08-13", "2026-08-14")]
        {
            let inputs = inputs(scheduled, collected_on);
            let answer = waste_answer(&inputs, &resolve(&waste_zone_queries(&inputs)));
            assert_eq!(answer.band, Band::Imminent, "{scheduled} -> {collected_on}");
        }
        // Already past on the day itself, which sorts today ahead of
        // tomorrow inside the band.
        let today = inputs("2026-08-10", "2026-08-10");
        let answer = waste_answer(&today, &resolve(&waste_zone_queries(&today)));
        assert!(answer.within_band.unwrap() < NOW);
    }
}
