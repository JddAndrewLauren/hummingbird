//! **How long to the next vacation** (#121, ADR-0015), sunk out of
//! `screens/vacation-pane/vacation.ts` by ADR-0025 (#534) — the pane whose
//! zone is always [`zone::DEVICE_ZONE`], never a payload-carried IANA name
//! (`waste.rs`'s contrast, spelled out in `zone.rs`'s own header): a trip is
//! booked on the calendar the reader is holding, in the zone the reader is
//! standing in.
//!
//! # What crossed and what did not
//!
//! Every decision half of `vacation.ts` sank: [`classify`]/[`trip_from_event`],
//! [`trip_queue`], [`vacation_band`]/[`vacation_within_band`],
//! [`vacation_setup`] and the decision half of `vacationView`/`vacationAnswer`.
//! Three things stayed TS on purpose:
//!
//! - `tripDateRange`, `tripDayLabel`, `vacationHeadline`, `MONTH_NAMES` and
//!   `civilParts` are rendering — composing a sentence or a month name out of
//!   already-decided facts, exactly `waste.rs`'s "no rendered sentence
//!   crosses" line.
//! - **`Trip.name` (and `tripName`) stayed with them, and does not sink.**
//!   `tripName` looks decision-shaped — it trims a title — but nothing that
//!   crosses here reads it: `vacationAnswer` never touches `next.name`, only
//!   `vacationHeadline` does, and that stays TS. A field only [`Trip`] itself
//!   would carry `name` for, with no core reader, is exactly the "re-crossing
//!   a whole DTO nobody asked for" [`super::inputs`] warns against — so
//!   [`Trip`] simply has no `name` field, and a client renders its own
//!   headline off `event.title` the way `vacationHeadline`/`tripName`
//!   already do together.
//! - `HORIZON_LABEL` is a display string ("2 years") and stays TS.
//!
//! # The unresolvable `DEVICE_ZONE` gap
//!
//! `vacationAnswer` never named a gap for "the host could not resolve
//! today" — the web's `Intl.DateTimeFormat().resolvedOptions().timeZone`
//! cannot fail the way a payload-carried IANA name can. But the bridge
//! (`zone.rs`) makes every civil-date lookup fallible by construction, and a
//! core function must answer *something* rather than panic on a host that
//! omitted the key. [`VacationGap::UnresolvableZone`] is that answer: a
//! single unit variant (never a `{ zone }` field — there is only ever one
//! zone in play here, `DEVICE_ZONE` itself, so naming it back would say
//! nothing a reader does not already know) that folds into
//! [`AnswerState::BoundButUnacquired`] in [`vacation_answer`], the same
//! place every other "bound, but nothing usable landed" case lands
//! (`waste.rs`'s own `WasteGap::UnresolvableZone` handling).

use super::contract::{AnswerState, Band, CalendarInterval, PaneAnswerCore};
use super::inputs::{
    BindingValueFact, CalendarEventFacts, CalendarEventStatusFact, CalendarEventWhenFacts,
    CalendarReadFacts, FreshnessFact, PaneInputs,
};
use super::zone::{add_civil_days, civil_days_between, is_civil_date, CivilDate, ZoneFacts, ZoneQuery, DEVICE_ZONE};
use serde::{Deserialize, Serialize};

/// The one subject this question ever has — present even while unbound, so
/// the setup prompt is discoverable (`waste.rs`'s own reasoning).
pub const SUBJECT_KEY: &str = "next-trip";

/// The `calendar_reads` key this pane requests under (#267).
pub const CALENDAR_REQUEST_KEY: &str = "vacation";

/// The interval this question needs from the calendar mirror, in days —
/// `CalendarHorizon::Long`'s own window.
pub const HORIZON_BEFORE_DAYS: i64 = 7;
pub const HORIZON_AHEAD_DAYS: i64 = 730;

/// Beyond this the calendar read is stale. Declared beside the band
/// function, on `waste.rs`'s own precedent — the driver is the cost of a
/// wrong answer here, and here alone.
pub const STALE_AFTER_MS: i64 = 24 * 60 * 60 * 1000;

/// Bands: within a week, within a month, and everything further out.
pub const IMMINENT_WITHIN_DAYS: i64 = 7;
pub const NEAR_WITHIN_DAYS: i64 = 30;

/// ADR-0015's binding key for the Trips calendar
/// (`client/web/src/calendar/selection.ts`'s `TRIPS_CALENDAR_BINDING_KEY`).
pub const TRIPS_CALENDAR_BINDING_KEY: &str = "trips-calendar";

/// Where a trip sits relative to today. Five, not three — see `vacation.ts`'s
/// own doc comment for why `returns_today` is its own phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TripPhase {
    Upcoming,
    DepartsToday,
    UnderWay,
    ReturnsToday,
    Past,
}

/// One trip, decided — `vacation.ts`'s `Trip` minus `name` (see the module
/// header for why that field does not sink).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Trip {
    pub id: String,
    pub location: Option<String>,
    /// First day: provider civil date for all-day, `DEVICE_ZONE` civil date
    /// for timed.
    pub start_date: CivilDate,
    /// Last day — for an all-day event the exclusive end's civil date minus
    /// one civil day; for a timed one the end instant's own civil date,
    /// clamped to never precede `start_date`.
    pub last_date: CivilDate,
    /// `DEVICE_ZONE` midnight (all-day) or the real instant (timed) — for
    /// `within_band` alone, never for a day count.
    pub start_ms: i64,
    pub end_ms: i64,
    pub phase: TripPhase,
    /// Whole civil days from today to the first day; 0 on the departure day,
    /// negative once it has started.
    pub days_until: i64,
    /// Total civil days the trip covers, both ends included.
    pub length_days: i64,
    /// Which day of the trip today is, 1-based; 0 while it is still upcoming
    /// or past.
    pub day_of_trip: i64,
}

/// The classification core shared by both `tripFromEvent` arms — civil-date
/// arithmetic only, no zone lookup.
fn classify(
    event: &CalendarEventFacts,
    today: &CivilDate,
    start_date: CivilDate,
    last_date: CivilDate,
    start_ms: i64,
    end_ms: i64,
) -> Option<Trip> {
    let days_until = civil_days_between(today, &start_date)?;
    let days_to_last = civil_days_between(today, &last_date)?;
    let length_days = civil_days_between(&start_date, &last_date)?;

    let phase = if days_to_last < 0 {
        TripPhase::Past
    } else if days_until > 0 {
        TripPhase::Upcoming
    } else if days_until == 0 {
        TripPhase::DepartsToday
    } else if days_to_last == 0 {
        TripPhase::ReturnsToday
    } else {
        TripPhase::UnderWay
    };

    Some(Trip {
        id: event.provider_event_id.clone(),
        location: event.location.clone(),
        start_date,
        last_date,
        start_ms,
        end_ms,
        phase,
        days_until,
        length_days: length_days + 1,
        day_of_trip: match phase {
            TripPhase::Upcoming | TripPhase::Past => 0,
            _ => -days_until + 1,
        },
    })
}

/// One calendar event read as a trip, or `None` if its dates are malformed
/// or the host could not resolve a `DEVICE_ZONE` fact it needed.
///
/// **The exclusive-end rule is the ALL-DAY rule, and only that** — see
/// `vacation.ts`'s `tripFromEvent` doc for the two defects this guards
/// against. `today` and `facts` are supplied rather than looked up, on
/// `waste.rs`'s `waste_facts(inputs, facts)` shape: this crate cannot
/// resolve `DEVICE_ZONE` itself.
pub fn trip_from_event(event: &CalendarEventFacts, today: &CivilDate, facts: &ZoneFacts) -> Option<Trip> {
    match &event.when {
        CalendarEventWhenFacts::AllDay { start_date, end_date } => {
            if !is_civil_date(start_date) {
                return None;
            }
            let last_date = add_civil_days(end_date, -1)?;
            let start_ms = facts.midnight_ms(DEVICE_ZONE, start_date)?;
            let end_ms = facts.midnight_ms(DEVICE_ZONE, end_date)?;
            classify(event, today, start_date.clone(), last_date, start_ms, end_ms)
        }
        CalendarEventWhenFacts::Timed { start_ms, end_ms } => {
            let start_date = facts.civil_date(DEVICE_ZONE, *start_ms)?;
            let end_date = facts.civil_date(DEVICE_ZONE, *end_ms)?;
            let last_date = if end_date < start_date { start_date.clone() } else { end_date };
            classify(event, today, start_date, last_date, *start_ms, *end_ms)
        }
    }
}

/// Every `(DEVICE_ZONE, civil-date/instant)` fact this question needs, given
/// these inputs — the core half of `zone.rs`'s bridge. Always asks for
/// "today" itself, plus one `Midnight`/`CivilDate` pair per trip-candidate
/// event (matching the bound calendar, not cancelled) on the bound read —
/// exactly what [`trip_from_event`] will need for each. Deduplicated within
/// this list; [`super::zone_queries`] dedupes again across questions.
pub fn vacation_zone_queries(inputs: &PaneInputs) -> Vec<ZoneQuery> {
    let mut queries = vec![ZoneQuery::CivilDate { zone: DEVICE_ZONE.to_string(), at_ms: inputs.now_ms }];
    // The horizon's own two ends, so `vacation_calendar_interval` is
    // answerable from the same single round trip (#564): a host that has to
    // ask twice would be resolving the interval it needs *before* it can
    // request the read that interval is for.
    queries.extend(calendar_interval_queries(inputs.now_ms));

    if let VacationSetup::Bound { calendar_id, events, .. } = vacation_setup(inputs) {
        for event in events {
            if event.calendar_id != calendar_id || event.status == CalendarEventStatusFact::Cancelled {
                continue;
            }
            match &event.when {
                CalendarEventWhenFacts::AllDay { start_date, end_date } => {
                    if is_civil_date(start_date) {
                        queries.push(ZoneQuery::Midnight {
                            zone: DEVICE_ZONE.to_string(),
                            date: start_date.clone(),
                        });
                    }
                    if is_civil_date(end_date) {
                        queries.push(ZoneQuery::Midnight {
                            zone: DEVICE_ZONE.to_string(),
                            date: end_date.clone(),
                        });
                    }
                }
                CalendarEventWhenFacts::Timed { start_ms, end_ms } => {
                    queries.push(ZoneQuery::CivilDate { zone: DEVICE_ZONE.to_string(), at_ms: *start_ms });
                    queries.push(ZoneQuery::CivilDate { zone: DEVICE_ZONE.to_string(), at_ms: *end_ms });
                }
            }
        }
    }

    let mut deduped: Vec<ZoneQuery> = Vec::new();
    for query in queries {
        if !deduped.iter().any(|existing| existing.key() == query.key()) {
            deduped.push(query);
        }
    }
    deduped
}

/// The two `CivilDate` facts [`vacation_calendar_interval`] needs — the
/// device's own day at each end of the horizon. Folded into
/// [`vacation_zone_queries`] so one round trip answers both the interval
/// and the trips.
fn calendar_interval_queries(now_ms: i64) -> [ZoneQuery; 2] {
    [
        ZoneQuery::CivilDate { zone: DEVICE_ZONE.to_string(), at_ms: horizon_start_ms(now_ms) },
        ZoneQuery::CivilDate { zone: DEVICE_ZONE.to_string(), at_ms: horizon_end_ms(now_ms) },
    ]
}

const DAY_MS: i64 = 24 * 60 * 60 * 1000;

fn horizon_start_ms(now_ms: i64) -> i64 {
    now_ms - HORIZON_BEFORE_DAYS * DAY_MS
}

fn horizon_end_ms(now_ms: i64) -> i64 {
    now_ms + HORIZON_AHEAD_DAYS * DAY_MS
}

/// The calendar-arm interval this question needs (#267) — the long horizon,
/// [`HORIZON_BEFORE_DAYS`] back and [`HORIZON_AHEAD_DAYS`] ahead.
/// `vacation.ts`'s `vacationCalendarInterval`, sunk by #564 for the same
/// reason [`super::weekend::weekend_calendar_interval`] was.
///
/// **`None` rather than a throw** where the TS copy throws: the web's
/// version runs inside an effect where a throw takes the whole hook down,
/// which its own comment calls out as the reason it guards. A pane that
/// cannot resolve the device's zone has no interval to request, and that is
/// an ordinary answer here — the same `None` [`vacation_zone_queries`]'
/// consumers already handle.
pub fn vacation_calendar_interval(now_ms: i64, facts: &ZoneFacts) -> Option<CalendarInterval> {
    let start_ms = horizon_start_ms(now_ms);
    let end_ms = horizon_end_ms(now_ms);
    let end_day = facts.civil_date(DEVICE_ZONE, end_ms)?;
    Some(CalendarInterval {
        start_ms,
        end_ms,
        start_date: facts.civil_date(DEVICE_ZONE, start_ms)?,
        // Exclusive, like every other civil upper bound here.
        end_date: add_civil_days(&end_day, 1)?,
    })
}

/// Every trip still ahead of (or under) today, soonest first.
///
/// **Every non-cancelled event on the bound calendar is a trip** — no filter
/// and no merging (`vacation.ts`'s `tripQueue` doc, #121 §4).
pub fn trip_queue(
    events: &[CalendarEventFacts],
    calendar_id: &str,
    today: &CivilDate,
    facts: &ZoneFacts,
) -> Vec<Trip> {
    let mut trips: Vec<Trip> = events
        .iter()
        .filter(|event| event.calendar_id == calendar_id && event.status != CalendarEventStatusFact::Cancelled)
        .filter_map(|event| trip_from_event(event, today, facts))
        .filter(|trip| trip.phase != TripPhase::Past)
        .collect();
    trips.sort_by(|left, right| left.start_date.cmp(&right.start_date).then_with(|| left.id.cmp(&right.id)));
    trips
}

/// How soon the answer matters. `dormant` is reserved for "nothing to count
/// to" — never for "far away" (`distant`'s own job).
pub fn vacation_band(next: Option<&Trip>) -> Band {
    let Some(next) = next else {
        return Band::Dormant;
    };
    if next.phase != TripPhase::Upcoming {
        return Band::Live;
    }
    if next.days_until <= IMMINENT_WITHIN_DAYS {
        return Band::Imminent;
    }
    if next.days_until <= NEAR_WITHIN_DAYS {
        return Band::Near;
    }
    Band::Distant
}

/// Epoch ms of this pane's next relevant moment — the next trip's start
/// while it is still ahead, the current trip's end once it is under way, and
/// `None` when nothing is booked.
pub fn vacation_within_band(next: Option<&Trip>) -> Option<i64> {
    let next = next?;
    Some(if next.phase == TripPhase::Upcoming { next.start_ms } else { next.end_ms })
}

/// The designated Trips calendar id, or `None` when there isn't one —
/// `client/web/src/calendar/selection.ts`'s `tripsCalendarId`, read through
/// [`PaneInputs::binding`]. An unread bindings table already reads as `None`
/// here (`PaneInputs::binding` returns `None` whenever `bindings` is `None`),
/// which is exactly `tripsCalendarId`'s own `bindings === null` arm.
pub fn trips_calendar_id(inputs: &PaneInputs) -> Option<String> {
    match inputs.binding(TRIPS_CALENDAR_BINDING_KEY).map(|binding| &binding.value) {
        Some(BindingValueFact::Text { text }) => {
            let id = text.trim();
            if id.is_empty() {
                None
            } else {
                Some(id.to_string())
            }
        }
        _ => None,
    }
}

/// Why this pane has no answer, or that it has one — `vacation.ts`'s
/// `VacationSetup`. Borrows its bound read's events rather than cloning them
/// ([`CalendarEventFacts`] carries no `Clone`), the same reasoning
/// `waste.rs`'s `PaneSnapshotFacts` borrows follow.
#[derive(Debug, Clone, PartialEq)]
pub enum VacationSetup<'a> {
    NoCalendar,
    Unbound,
    Unread,
    Bound { calendar_id: String, events: &'a [CalendarEventFacts], freshness: FreshnessFact },
}

/// **`calendar_connected` is checked first** (#122's rule): "no calendar at
/// all" and "no Trips calendar designated" are two different missing steps,
/// and the earlier one wins.
pub fn vacation_setup(inputs: &PaneInputs) -> VacationSetup<'_> {
    if !inputs.calendar_connected {
        return VacationSetup::NoCalendar;
    }
    let Some(calendar_id) = trips_calendar_id(inputs) else {
        // An unread bindings table lands here too, and deliberately: the
        // *other* unbound reason has already been ruled out, so the device
        // is connected and simply has no trips calendar to read yet.
        return if inputs.bindings.is_none() { VacationSetup::Unread } else { VacationSetup::Unbound };
    };
    match inputs.calendar_reads.get(CALENDAR_REQUEST_KEY) {
        None | Some(CalendarReadFacts::NotRead) => VacationSetup::Unread,
        Some(CalendarReadFacts::Read { events, freshness }) => {
            VacationSetup::Bound { calendar_id, events, freshness: *freshness }
        }
    }
}

/// [`VacationSetup`] minus its borrowed `Bound` payload — the wire form.
/// `VacationSetup<'a>` cannot itself cross the seam (its `Bound` arm
/// borrows the inputs' own event slice, so it has no `Serialize`); this is
/// the kind-only projection a host pins its own precedence copy against,
/// on `RaceSetup`'s/`WasteSetup`'s own shape. `Bound` keeps `calendar_id`
/// (a decided fact — which calendar the binding named) but not the events
/// or freshness, which the host already has on its own `QuestionInputs`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum VacationSetupKind {
    NoCalendar,
    Unbound,
    Unread,
    Bound { calendar_id: String },
}

/// [`vacation_setup`], projected to [`VacationSetupKind`] — see that type's
/// own doc for why this is the wire form rather than [`VacationSetup`]
/// itself.
pub fn vacation_setup_kind(inputs: &PaneInputs) -> VacationSetupKind {
    match vacation_setup(inputs) {
        VacationSetup::NoCalendar => VacationSetupKind::NoCalendar,
        VacationSetup::Unbound => VacationSetupKind::Unbound,
        VacationSetup::Unread => VacationSetupKind::Unread,
        VacationSetup::Bound { calendar_id, .. } => VacationSetupKind::Bound { calendar_id },
    }
}

/// Why this pane has no *facts*, once it is known to be bound — the zone
/// equivalent of `WasteGap::UnresolvableZone`. See the module header for why
/// this carries no `zone` field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "gap", rename_all = "camelCase")]
pub enum VacationGap {
    UnresolvableZone,
}

/// The answered view an expanded pane draws — `vacation.ts`'s `VacationView`,
/// plus the gap this crate's bridge can produce that TS never named.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VacationFacts {
    /// The soonest unfinished trip — the one the headline is about. `None`
    /// when nothing is booked inside the horizon, which is still an answer.
    pub next: Option<Trip>,
    /// Every trip after `next`, in order, never truncated.
    pub later: Vec<Trip>,
    pub freshness: FreshnessFact,
    pub stale: bool,
}

/// The whole answered fact set for a bound pane, or the reason there is
/// none.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum VacationResolved {
    Facts(VacationFacts),
    Gap { gap: VacationGap },
}

/// The fully-resolved queue for an already-[`VacationSetup::Bound`] setup —
/// the tail of `vacationView` once `setup.kind === "bound"`. Exposed
/// separately from [`vacation_view`] so a caller already holding the bound
/// arm's fields does not have to re-derive [`vacation_setup`].
pub fn vacation_facts(
    calendar_id: &str,
    events: &[CalendarEventFacts],
    freshness: FreshnessFact,
    inputs: &PaneInputs,
    facts: &ZoneFacts,
) -> VacationResolved {
    let Some(today) = facts.civil_date(DEVICE_ZONE, inputs.now_ms) else {
        return VacationResolved::Gap { gap: VacationGap::UnresolvableZone };
    };
    let trips = trip_queue(events, calendar_id, &today, facts);
    let mut iter = trips.into_iter();
    let next = iter.next();
    let later = iter.collect();
    VacationResolved::Facts(VacationFacts {
        next,
        later,
        freshness,
        stale: freshness.is_stale_beyond(STALE_AFTER_MS),
    })
}

/// The answered view an expanded pane draws — `None` for every setup that is
/// not `bound`, mirroring `vacationView`.
pub fn vacation_view(inputs: &PaneInputs, facts: &ZoneFacts) -> Option<VacationResolved> {
    match vacation_setup(inputs) {
        VacationSetup::Bound { calendar_id, events, freshness } => {
            Some(vacation_facts(&calendar_id, events, freshness, inputs, facts))
        }
        _ => None,
    }
}

/// This question's answer for the shell.
///
/// **No glyphs, no headline** — this question has one subject and its
/// answer is already a sentence, composed entirely client-side.
pub fn vacation_answer(inputs: &PaneInputs, facts: &ZoneFacts) -> PaneAnswerCore {
    let gap = |answer_state| PaneAnswerCore { answer_state, band: Band::Dormant, within_band: None };
    match vacation_setup(inputs) {
        VacationSetup::NoCalendar | VacationSetup::Unbound => gap(AnswerState::Unbound),
        VacationSetup::Unread => gap(AnswerState::BoundButUnacquired),
        VacationSetup::Bound { calendar_id, events, freshness } => {
            match vacation_facts(&calendar_id, events, freshness, inputs, facts) {
                VacationResolved::Gap { .. } => gap(AnswerState::BoundButUnacquired),
                VacationResolved::Facts(view) => {
                    let next = view.next.as_ref();
                    PaneAnswerCore {
                        answer_state: AnswerState::Answered,
                        band: vacation_band(next),
                        within_band: vacation_within_band(next),
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decisions::panes::inputs::{BindingFact, CalendarEventStatusFact as Status};
    use crate::decisions::panes::zone::ZoneFact;

    /// `DEVICE_ZONE` stands in for a fixed real zone in every test — the
    /// resolver does not care that the query's zone string is
    /// `"device-local"` rather than an IANA name (`zone.rs`'s own doc). This
    /// zone is 7h behind UTC, unmodeled DST included, on `waste.rs`'s own
    /// test precedent — consistency, not real-world accuracy, is what the
    /// arithmetic below relies on.
    const OFFSET_MS: i64 = 7 * 3_600_000;

    fn epoch_day_ms(date: &str) -> i64 {
        civil_days_between("1970-01-01", date).unwrap() * 86_400_000
    }

    /// `hour:00` local time on `date`, as an epoch instant.
    fn now_at(date: &str, hour: i64) -> i64 {
        epoch_day_ms(date) + hour * 3_600_000 + OFFSET_MS
    }

    /// A host resolver, stood up from the queries the core itself asked —
    /// `waste.rs`'s own `resolve` helper, generalised to `DEVICE_ZONE`.
    fn resolve(queries: &[ZoneQuery]) -> ZoneFacts {
        let mut facts = ZoneFacts::default();
        for query in queries {
            match query {
                ZoneQuery::CivilDate { zone, at_ms } => {
                    if zone == DEVICE_ZONE {
                        let local = at_ms - OFFSET_MS;
                        let days = local.div_euclid(86_400_000);
                        let date = add_civil_days("1970-01-01", days).unwrap();
                        facts.insert(query, ZoneFact::Date(date));
                    }
                }
                ZoneQuery::Midnight { zone, date } => {
                    if zone == DEVICE_ZONE {
                        let days = civil_days_between("1970-01-01", date).unwrap();
                        facts.insert(query, ZoneFact::Instant(days * 86_400_000 + OFFSET_MS));
                    }
                }
            }
        }
        facts
    }

    fn all_day(id: &str, calendar_id: &str, start: &str, end_exclusive: &str) -> CalendarEventFacts {
        CalendarEventFacts {
            provider_event_id: id.to_string(),
            calendar_id: calendar_id.to_string(),
            title: format!("Trip: {id}"),
            when: CalendarEventWhenFacts::AllDay {
                start_date: start.to_string(),
                end_date: end_exclusive.to_string(),
            },
            location: None,
            status: Status::Confirmed,
        }
    }

    fn timed(id: &str, calendar_id: &str, start_ms: i64, end_ms: i64) -> CalendarEventFacts {
        CalendarEventFacts {
            provider_event_id: id.to_string(),
            calendar_id: calendar_id.to_string(),
            title: format!("Trip: {id}"),
            when: CalendarEventWhenFacts::Timed { start_ms, end_ms },
            location: None,
            status: Status::Confirmed,
        }
    }

    fn bound_inputs(now_ms: i64) -> PaneInputs {
        PaneInputs {
            now_ms,
            calendar_connected: true,
            bindings: Some(vec![BindingFact {
                key: TRIPS_CALENDAR_BINDING_KEY.to_string(),
                value: BindingValueFact::Text { text: "trips@g".to_string() },
            }]),
            ..Default::default()
        }
    }

    fn with_read(mut inputs: PaneInputs, events: Vec<CalendarEventFacts>, age_ms: i64) -> PaneInputs {
        inputs.calendar_reads.insert(
            CALENDAR_REQUEST_KEY.to_string(),
            CalendarReadFacts::Read {
                events,
                freshness: FreshnessFact::Age { age_ms, declared_cadence_ms: None },
            },
        );
        inputs
    }

    /// `trip_queue`'s answer for `inputs`, resolved end-to-end through the
    /// zone bridge exactly as a real host would drive it.
    fn queue_of(inputs: &PaneInputs) -> Vec<Trip> {
        let facts = resolve(&vacation_zone_queries(inputs));
        match vacation_setup(inputs) {
            VacationSetup::Bound { calendar_id, events, .. } => {
                let today = facts.civil_date(DEVICE_ZONE, inputs.now_ms).unwrap();
                trip_queue(events, &calendar_id, &today, &facts)
            }
            _ => Vec::new(),
        }
    }

    // -------------------------------------------------------- trip_queue

    #[test]
    fn counts_to_the_first_day_in_whole_civil_days() {
        let trip = all_day("t1", "trips@g", "2026-03-10", "2026-03-16"); // 10th-15th
        let inputs = with_read(bound_inputs(now_at("2026-03-01", 9)), vec![trip], 0);
        let queue = queue_of(&inputs);
        assert_eq!(queue[0].phase, TripPhase::Upcoming);
        assert_eq!(queue[0].days_until, 9);
        assert_eq!(queue[0].length_days, 6);
    }

    #[test]
    fn reads_the_exclusive_end_as_the_day_after_the_last_day() {
        // `endMs - DAY` arithmetic would put the last day on the 16th and
        // make the trip seven days long — the ADR-0015 defect.
        let trip = all_day("t1", "trips@g", "2026-03-10", "2026-03-16");
        let inputs = with_read(bound_inputs(now_at("2026-03-15", 20)), vec![trip], 0);
        let queue = queue_of(&inputs);
        assert_eq!(queue[0].phase, TripPhase::ReturnsToday);
        assert_eq!(queue[0].length_days, 6);
        assert_eq!(queue[0].day_of_trip, 6);
    }

    #[test]
    fn is_still_the_trip_on_its_return_day_and_gone_the_day_after() {
        let trip = || all_day("t1", "trips@g", "2026-03-10", "2026-03-16");
        let last_day = with_read(bound_inputs(now_at("2026-03-15", 9)), vec![trip()], 0);
        assert_eq!(queue_of(&last_day).len(), 1);
        let day_after = with_read(bound_inputs(now_at("2026-03-16", 9)), vec![trip()], 0);
        assert_eq!(queue_of(&day_after).len(), 0);
    }

    #[test]
    fn departs_today_on_the_first_day_and_is_under_way_in_between() {
        let trip = || all_day("t1", "trips@g", "2026-03-10", "2026-03-16");
        let departs = with_read(bound_inputs(now_at("2026-03-10", 9)), vec![trip()], 0);
        assert_eq!(queue_of(&departs)[0].phase, TripPhase::DepartsToday);
        let mid = with_read(bound_inputs(now_at("2026-03-12", 9)), vec![trip()], 0);
        let mid_trip = &queue_of(&mid)[0];
        assert_eq!(mid_trip.phase, TripPhase::UnderWay);
        assert_eq!(mid_trip.day_of_trip, 3);
    }

    #[test]
    fn keeps_the_providers_all_day_dates_without_resolving_them_through_a_zone() {
        // The provider already said which civil days the trip occupies. This
        // is the "India in 394 days" defect's regression: `2027-01-05` to
        // `2027-01-20` (exclusive) with `today` one day before the exclusive
        // end must read as the return day, not something the zone shifted.
        let india = all_day("t3", "trips@g", "2027-01-05", "2027-01-20");
        let inputs = with_read(bound_inputs(now_at("2027-01-19", 9)), vec![india], 0);
        let queue = queue_of(&inputs);
        assert_eq!(queue[0].phase, TripPhase::ReturnsToday);
        assert_eq!(queue[0].length_days, 15);
    }

    #[test]
    fn drops_an_event_whose_provider_civil_dates_are_malformed() {
        let broken = all_day("t4", "trips@g", "not-a-date", "2026-03-12");
        let inputs = with_read(bound_inputs(now_at("2026-03-01", 9)), vec![broken], 0);
        assert_eq!(queue_of(&inputs), Vec::new());
    }

    #[test]
    fn reads_only_the_bound_calendar_and_never_a_cancelled_instance() {
        let other = all_day("t5", "work@g", "2026-03-02", "2026-03-03");
        let mut cancelled = all_day("t6", "trips@g", "2026-03-04", "2026-03-06");
        cancelled.status = Status::Cancelled;
        let kept = all_day("t1", "trips@g", "2026-03-10", "2026-03-16");
        let inputs = with_read(bound_inputs(now_at("2026-03-01", 9)), vec![other, cancelled, kept], 0);
        let ids: Vec<String> = queue_of(&inputs).into_iter().map(|trip| trip.id).collect();
        assert_eq!(ids, vec!["t1".to_string()]);
    }

    #[test]
    fn orders_the_whole_queue_by_first_day_soonest_first() {
        let later = all_day("t7", "trips@g", "2026-06-01", "2026-06-05");
        let earlier = all_day("t1", "trips@g", "2026-03-10", "2026-03-16");
        let inputs = with_read(bound_inputs(now_at("2026-03-01", 9)), vec![later, earlier], 0);
        let ids: Vec<String> = queue_of(&inputs).into_iter().map(|trip| trip.id).collect();
        assert_eq!(ids, vec!["t1".to_string(), "t7".to_string()]);
    }

    #[test]
    fn reads_a_timed_events_end_as_its_real_end_not_an_exclusive_one() {
        // The all-day "minus one civil day" rule applied here would end this
        // trip a day early — a short `lengthDays` and `returns_today` early.
        let start = now_at("2026-04-03", 18) + 30 * 60_000;
        let end = now_at("2026-04-06", 21);
        let trip = timed("t8", "trips@g", start, end);
        let inputs = with_read(bound_inputs(now_at("2026-03-01", 9)), vec![trip], 0);
        let queue = queue_of(&inputs);
        assert_eq!(queue[0].last_date, "2026-04-06");
        assert_eq!(queue[0].length_days, 4);

        let trip2 = timed("t8", "trips@g", start, end);
        let on_return = with_read(bound_inputs(now_at("2026-04-06", 9)), vec![trip2], 0);
        assert_eq!(queue_of(&on_return)[0].phase, TripPhase::ReturnsToday);

        let trip3 = timed("t8", "trips@g", start, end);
        let after = with_read(bound_inputs(now_at("2026-04-07", 9)), vec![trip3], 0);
        assert_eq!(queue_of(&after).len(), 0);
    }

    #[test]
    fn keeps_a_same_day_timed_event_in_the_queue() {
        // The misapplied all-day rule puts `lastDate` BEFORE `startDate`
        // here, which reads as `past` and silently drops the event.
        let start = now_at("2026-04-03", 7) + 15 * 60_000;
        let end = now_at("2026-04-03", 22) + 40 * 60_000;
        let day = timed("t9", "trips@g", start, end);
        let inputs = with_read(bound_inputs(now_at("2026-04-03", 9)), vec![day], 0);
        let queue = queue_of(&inputs);
        assert_eq!(queue[0].phase, TripPhase::DepartsToday);
        assert_eq!(queue[0].length_days, 1);

        let day2 = timed("t9", "trips@g", start, end);
        let earlier = with_read(bound_inputs(now_at("2026-04-01", 9)), vec![day2], 0);
        assert_eq!(queue_of(&earlier)[0].days_until, 2);
    }

    // ------------------------------------------- vacation_calendar_interval

    #[test]
    fn the_calendar_interval_is_the_long_horizon_in_both_arms() {
        let now_ms = now_at("2026-08-14", 9);
        // Answerable from the pane's own single round trip — the interval's
        // two civil ends ride `vacation_zone_queries`.
        let facts = resolve(&vacation_zone_queries(&PaneInputs {
            now_ms,
            ..PaneInputs::default()
        }));

        let interval = vacation_calendar_interval(now_ms, &facts).expect("an interval");

        assert_eq!(interval.start_ms, now_ms - HORIZON_BEFORE_DAYS * 86_400_000);
        assert_eq!(interval.end_ms, now_ms + HORIZON_AHEAD_DAYS * 86_400_000);
        assert_eq!(interval.start_date, "2026-08-07");
        // 730 days after 2026-08-14 is 2028-08-13; the civil end is
        // exclusive, so it is the day after.
        assert_eq!(interval.end_date, "2028-08-14");
    }

    #[test]
    fn an_unresolvable_device_zone_has_no_calendar_interval() {
        // `None`, not a throw — the web copy throws because it runs inside
        // an effect; here a pane with no resolvable zone simply has no
        // interval to request.
        assert_eq!(
            vacation_calendar_interval(now_at("2026-08-14", 9), &ZoneFacts::default()),
            None
        );
    }

    // -------------------------------------------------------- vacation_band

    #[test]
    fn is_live_for_every_day_of_the_trip_itself() {
        for day in ["2026-03-10", "2026-03-12", "2026-03-15"] {
            let trip = all_day("b", "trips@g", "2026-03-10", "2026-03-16");
            let inputs = with_read(bound_inputs(now_at(day, 9)), vec![trip], 0);
            let queue = queue_of(&inputs);
            assert_eq!(vacation_band(queue.first()), Band::Live, "{day}");
        }
    }

    #[test]
    fn climbs_imminent_near_distant_as_the_trip_gets_further_away() {
        let ladder = [
            ("2026-03-05", Band::Imminent), // 5 days out
            ("2026-02-20", Band::Near),      // 18 days out
            ("2026-01-01", Band::Distant),   // 68 days out
        ];
        for (today, expected) in ladder {
            let trip = all_day("b", "trips@g", "2026-03-10", "2026-03-16");
            let inputs = with_read(bound_inputs(now_at(today, 9)), vec![trip], 0);
            let queue = queue_of(&inputs);
            assert_eq!(vacation_band(queue.first()), expected, "{today}");
        }
    }

    #[test]
    fn sits_exactly_on_the_seven_and_thirty_day_boundaries() {
        let trip = all_day("b", "trips@g", "2026-03-10", "2026-03-16");
        let seven_out = with_read(bound_inputs(now_at("2026-03-03", 9)), vec![trip], 0);
        assert_eq!(vacation_band(queue_of(&seven_out).first()), Band::Imminent);

        let trip2 = all_day("b", "trips@g", "2026-03-10", "2026-03-16");
        let eight_out = with_read(bound_inputs(now_at("2026-03-02", 9)), vec![trip2], 0);
        assert_eq!(vacation_band(queue_of(&eight_out).first()), Band::Near);

        let trip3 = all_day("b", "trips@g", "2026-04-09", "2026-04-15");
        let thirty_out = with_read(bound_inputs(now_at("2026-03-10", 9)), vec![trip3], 0);
        assert_eq!(vacation_band(queue_of(&thirty_out).first()), Band::Near);

        let trip4 = all_day("b", "trips@g", "2026-04-10", "2026-04-16");
        let thirty_one_out = with_read(bound_inputs(now_at("2026-03-10", 9)), vec![trip4], 0);
        assert_eq!(vacation_band(queue_of(&thirty_one_out).first()), Band::Distant);
    }

    #[test]
    fn keeps_a_trip_seven_hundred_days_out_out_of_dormant() {
        // ADR-0015 names this pane as the reason "dormant is not a synonym
        // for far away": dormant means there is nothing to count to.
        let trip = all_day("b", "trips@g", "2028-01-20", "2028-02-01");
        let inputs = with_read(bound_inputs(now_at("2026-03-01", 9)), vec![trip], 0);
        assert_eq!(vacation_band(queue_of(&inputs).first()), Band::Distant);
    }

    #[test]
    fn is_dormant_only_when_nothing_is_booked() {
        assert_eq!(vacation_band(None), Band::Dormant);
        assert_eq!(vacation_within_band(None), None);
    }

    // -------------------------------------------------------------- setup

    #[test]
    fn setup_precedence_is_no_calendar_then_unbound_then_unread_then_bound() {
        // No calendar connected wins over everything else, even a bound and
        // read trips calendar.
        let mut no_calendar = bound_inputs(now_at("2026-03-01", 9));
        no_calendar.calendar_connected = false;
        assert_eq!(vacation_setup(&no_calendar), VacationSetup::NoCalendar);

        // Connected, but no trips calendar designated (an unset row).
        let mut unbound = bound_inputs(now_at("2026-03-01", 9));
        unbound.bindings = Some(vec![BindingFact {
            key: TRIPS_CALENDAR_BINDING_KEY.to_string(),
            value: BindingValueFact::Unset,
        }]);
        assert_eq!(vacation_setup(&unbound), VacationSetup::Unbound);

        // Connected, bindings table itself unread — distinct from unbound.
        let mut table_unread = bound_inputs(now_at("2026-03-01", 9));
        table_unread.bindings = None;
        assert_eq!(vacation_setup(&table_unread), VacationSetup::Unread);

        // Connected and bound, but the calendar read has not landed.
        let read_unread = bound_inputs(now_at("2026-03-01", 9));
        assert_eq!(vacation_setup(&read_unread), VacationSetup::Unread);

        // Connected, bound, and read.
        let bound = with_read(bound_inputs(now_at("2026-03-01", 9)), Vec::new(), 0);
        assert!(matches!(vacation_setup(&bound), VacationSetup::Bound { .. }));
    }

    #[test]
    fn the_setup_kind_projection_matches_vacation_setup_arm_for_arm() {
        let mut no_calendar = bound_inputs(now_at("2026-03-01", 9));
        no_calendar.calendar_connected = false;
        assert_eq!(vacation_setup_kind(&no_calendar), VacationSetupKind::NoCalendar);

        let mut unbound = bound_inputs(now_at("2026-03-01", 9));
        unbound.bindings = Some(vec![BindingFact {
            key: TRIPS_CALENDAR_BINDING_KEY.to_string(),
            value: BindingValueFact::Unset,
        }]);
        assert_eq!(vacation_setup_kind(&unbound), VacationSetupKind::Unbound);

        let mut table_unread = bound_inputs(now_at("2026-03-01", 9));
        table_unread.bindings = None;
        assert_eq!(vacation_setup_kind(&table_unread), VacationSetupKind::Unread);

        let bound = with_read(bound_inputs(now_at("2026-03-01", 9)), Vec::new(), 0);
        assert_eq!(
            vacation_setup_kind(&bound),
            VacationSetupKind::Bound { calendar_id: "trips@g".to_string() },
        );
    }

    #[test]
    fn a_blanked_or_unusable_trips_calendar_row_is_also_unbound() {
        for value in [
            BindingValueFact::Other,
            BindingValueFact::Text { text: "   ".to_string() },
        ] {
            let mut inputs = bound_inputs(now_at("2026-03-01", 9));
            inputs.bindings =
                Some(vec![BindingFact { key: TRIPS_CALENDAR_BINDING_KEY.to_string(), value }]);
            assert_eq!(vacation_setup(&inputs), VacationSetup::Unbound);
        }
    }

    // ------------------------------------------------------------- answer

    #[test]
    fn is_unbound_with_no_calendar_connected_at_all_before_anything_else() {
        let trip = all_day("a1", "trips@g", "2026-03-10", "2026-03-16");
        let mut inputs = with_read(bound_inputs(now_at("2026-03-01", 9)), vec![trip], 0);
        inputs.calendar_connected = false;
        let answer = vacation_answer(&inputs, &ZoneFacts::default());
        assert_eq!(answer.answer_state, AnswerState::Unbound);
        assert_eq!(answer.band, Band::Dormant);
        assert_eq!(answer.within_band, None);
    }

    #[test]
    fn waits_rather_than_claiming_an_answer_while_the_read_has_not_landed() {
        let inputs = bound_inputs(now_at("2026-03-01", 9));
        let answer = vacation_answer(&inputs, &ZoneFacts::default());
        assert_eq!(answer.answer_state, AnswerState::BoundButUnacquired);
    }

    #[test]
    fn answers_not_a_gap_when_the_window_holds_no_trip_at_all() {
        let inputs = with_read(bound_inputs(now_at("2026-03-01", 9)), Vec::new(), 0);
        let facts = resolve(&vacation_zone_queries(&inputs));
        let answer = vacation_answer(&inputs, &facts);
        assert_eq!(answer.answer_state, AnswerState::Answered);
        assert_eq!(answer.band, Band::Dormant);
        assert_eq!(answer.within_band, None);
    }

    #[test]
    fn sorts_by_the_next_relevant_moment_start_while_upcoming_end_while_live() {
        let trip = all_day("a1", "trips@g", "2026-03-10", "2026-03-16");
        let upcoming = with_read(bound_inputs(now_at("2026-03-01", 9)), vec![trip], 0);
        let facts = resolve(&vacation_zone_queries(&upcoming));
        let answer = vacation_answer(&upcoming, &facts);
        assert_eq!(answer.within_band, Some(now_at("2026-03-10", 0)));

        let trip2 = all_day("a1", "trips@g", "2026-03-10", "2026-03-16");
        let live = with_read(bound_inputs(now_at("2026-03-12", 9)), vec![trip2], 0);
        let facts2 = resolve(&vacation_zone_queries(&live));
        let answer2 = vacation_answer(&live, &facts2);
        assert_eq!(answer2.within_band, Some(now_at("2026-03-16", 0)));
    }

    #[test]
    fn still_answers_when_the_read_is_stale() {
        let trip = all_day("a1", "trips@g", "2026-03-10", "2026-03-16");
        let inputs = with_read(bound_inputs(now_at("2026-03-01", 9)), vec![trip], STALE_AFTER_MS + 1);
        let facts = resolve(&vacation_zone_queries(&inputs));
        let view = vacation_view(&inputs, &facts).unwrap();
        match view {
            VacationResolved::Facts(facts) => {
                assert!(facts.stale);
                assert_eq!(facts.next.map(|trip| trip.id), Some("a1".to_string()));
            }
            VacationResolved::Gap { gap } => panic!("expected facts, got {gap:?}"),
        }
    }

    #[test]
    fn lists_the_whole_queue_never_truncated() {
        let events = vec![
            all_day("a1", "trips@g", "2026-03-10", "2026-03-16"),
            all_day("a2", "trips@g", "2026-06-01", "2026-06-05"),
            all_day("a3", "trips@g", "2027-02-01", "2027-02-14"),
            all_day("a4", "trips@g", "2027-04-01", "2027-04-20"),
        ];
        let inputs = with_read(bound_inputs(now_at("2026-03-01", 9)), events, 0);
        let facts = resolve(&vacation_zone_queries(&inputs));
        let view = vacation_view(&inputs, &facts).unwrap();
        match view {
            VacationResolved::Facts(facts) => {
                let ids: Vec<String> = facts.later.into_iter().map(|trip| trip.id).collect();
                assert_eq!(ids, vec!["a2".to_string(), "a3".to_string(), "a4".to_string()]);
            }
            VacationResolved::Gap { gap } => panic!("expected facts, got {gap:?}"),
        }
    }

    // ------------------------------------------------- the unresolvable zone

    #[test]
    fn an_unresolvable_device_zone_is_a_gap_never_a_fabricated_today() {
        let trip = all_day("a1", "trips@g", "2026-03-10", "2026-03-16");
        let inputs = with_read(bound_inputs(now_at("2026-03-01", 9)), vec![trip], 0);
        // No `resolve()` call: the host simply never answered the "today"
        // query, exactly `zone.rs`'s "an omitted key reads as absent" case.
        let facts = ZoneFacts::default();
        let view = vacation_view(&inputs, &facts);
        assert_eq!(
            view,
            Some(VacationResolved::Gap { gap: VacationGap::UnresolvableZone }),
        );
        let answer = vacation_answer(&inputs, &facts);
        assert_eq!(answer.answer_state, AnswerState::BoundButUnacquired);
        assert_eq!(answer.band, Band::Dormant);
        assert_eq!(answer.within_band, None);
    }
}
