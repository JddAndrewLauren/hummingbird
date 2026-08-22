//! **The SCPS pane** (#693, ADR-0032) — the tenth standing question,
//! "when is the next SCPS event, and what is this month's Photo Quest",
//! answered entirely over the calendar arm plus one `settings` binding.
//!
//! # Why this pane reads no source of its own
//!
//! ADR-0032 rejects the poller shape every other externally-fed question
//! uses (`server/city-waste`, `server/race-poll`, ADR-0011): SCPS emails
//! need a model to read them, so the OpenClaw agent (ADR-0029) extracts
//! them and writes the answer through two ordinary places every pane
//! already reads — the operator's primary Google Calendar (events, under a
//! title convention) and a `settings` binding (`scps-quest`, a phrase with
//! its month folded into the value). This module is then ordinary
//! ADR-0015 work: a calendar-arm pane in the weekend/vacation family
//! (`sources: []`), reading events the way [`super::vacation::trip_from_event`]
//! does, through the zone bridge (`super::zone`).
//!
//! # Unlike `vacation.rs`, there is no per-question calendar binding
//!
//! Vacation and weekend each read one *designated* calendar (a binding, or
//! the device's whole selection). SCPS reads every non-cancelled event on
//! every calendar the device already polls, filtered only by the `SCPS `
//! title prefix ([`parse_scps_title`]) — ADR-0032 part 2's "the operator
//! wants these on the calendar they look at" decision, declined a
//! dedicated calendar and its binding plumbing. So **`AnswerState::Unbound`
//! is never produced here** (ADR-0032 part 5's own table): there is no
//! setup step to be unbound from. `!calendar_connected` and an unread
//! calendar arm both read as [`AnswerState::BoundButUnacquired`], the same
//! "waiting for the first sync" state every other pane's *unread* arm
//! already uses — this pane simply has no *unbound* arm at all. An empty
//! window is `Answered`, banded [`Band::Dormant`] — "nothing to count to"
//! never collapses into "not set up".
//!
//! # The quest never moves the band
//!
//! `scps-quest` is read independently of the event queue
//! ([`scps_quest`]/[`scps_quest_fact`]) and shown only when its stored
//! month equals the device's own current civil month at [`DEVICE_ZONE`] —
//! otherwise the pane says whose month it *was* posted for. Never a
//! reason to change [`Band`]: the band answers "how soon is the next
//! event", and the quest answers a different question entirely (ADR-0032
//! part 5).

use serde::{Deserialize, Serialize};

use super::contract::{AnswerState, Band, CalendarInterval, PaneAnswerCore};
use super::inputs::{
    BindingValueFact, CalendarEventFacts, CalendarEventStatusFact, CalendarEventWhenFacts,
    CalendarReadFacts, FreshnessFact, PaneInputs,
};
use super::zone::{add_civil_days, civil_days_between, is_civil_date, CivilDate, ZoneFacts, ZoneQuery, DEVICE_ZONE};

/// The one subject this question ever has — present even while unread, so
/// the pane is discoverable before the first calendar sync
/// (`vacation.rs`'s own reasoning for [`super::vacation::SUBJECT_KEY`]).
pub const SUBJECT_KEY: &str = "next-scps-event";

/// The `calendar_reads` key this pane requests under (#267).
pub const CALENDAR_REQUEST_KEY: &str = "scps";

/// ADR-0015's binding key for the Photo Quest phrase
/// (`bindings.rs`'s `BindingKey::ScpsQuest`, `"scps-quest"`).
pub const SCPS_QUEST_BINDING_KEY: &str = "scps-quest";

/// The calendar window this pane requests: 6 hours behind now, 90 days
/// ahead — the standard horizon `CalendarHorizon::Standard` polls
/// (`client/core/src/calendar/google/adapter.rs`'s `WINDOW_AFTER_DAYS`),
/// not the long horizon vacation asks for.
pub const HORIZON_BEFORE_MS: i64 = 6 * 60 * 60 * 1000;
pub const HORIZON_AFTER_DAYS: i64 = 90;

/// Beyond this the calendar read is stale — `vacation.rs`'s own precedent.
pub const STALE_AFTER_MS: i64 = 24 * 60 * 60 * 1000;

const DAY_MS: i64 = 24 * 60 * 60 * 1000;

/// The three named SCPS event kinds, plus the catch-all every other
/// `SCPS `-prefixed title reads as (ADR-0032 part 2 / the contract table).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScpsKind {
    Meeting,
    Activity,
    HappyHour,
    /// Anything else starting with `SCPS ` — shown to the reader as the
    /// literal word "event", never the title's own next word.
    Event,
}

/// One `SCPS `-prefixed title, parsed — kind plus an optional topic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScpsTitle {
    pub kind: ScpsKind,
    /// `None` when the title carried no colon at all (the bare `SCPS Happy
    /// Hour` form, or the unrecognised-kind fallback); `Some` — possibly
    /// empty once trimmed — when a colon was present.
    pub topic: Option<String>,
}

/// The prefix/kind parser (the contract's own rules, restated in the module
/// header): a title must start with `SCPS ` to be read at all; `Meeting:`
/// and `Activity:` require their colon; `Happy Hour` is recognised bare or
/// with a colon; anything else starting with `SCPS ` reads as
/// [`ScpsKind::Event`] with no topic.
pub fn parse_scps_title(title: &str) -> Option<ScpsTitle> {
    let rest = title.strip_prefix("SCPS ")?;
    if let Some(topic) = rest.strip_prefix("Meeting:") {
        return Some(ScpsTitle { kind: ScpsKind::Meeting, topic: Some(topic.trim().to_string()) });
    }
    if let Some(topic) = rest.strip_prefix("Activity:") {
        return Some(ScpsTitle { kind: ScpsKind::Activity, topic: Some(topic.trim().to_string()) });
    }
    if rest == "Happy Hour" {
        return Some(ScpsTitle { kind: ScpsKind::HappyHour, topic: None });
    }
    if let Some(topic) = rest.strip_prefix("Happy Hour:") {
        return Some(ScpsTitle { kind: ScpsKind::HappyHour, topic: Some(topic.trim().to_string()) });
    }
    Some(ScpsTitle { kind: ScpsKind::Event, topic: None })
}

/// One SCPS event, resolved through the zone bridge — `vacation.rs`'s
/// `Trip`, trimmed to what this pane's headline and expanded body need.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScpsEvent {
    pub id: String,
    pub kind: ScpsKind,
    pub topic: Option<String>,
    pub start_ms: i64,
    pub end_ms: i64,
    /// The event's first civil day at [`DEVICE_ZONE`] — the day the band
    /// and the headline's "today"/"tomorrow" reasoning key on.
    pub start_date: CivilDate,
    pub location: Option<String>,
    /// The provider's free-text body — the writer's get-together/set-up
    /// notes (ADR-0032 part 6).
    pub notes: Option<String>,
    /// Whole civil days from today to [`ScpsEvent::start_date`]; 0 on the
    /// day it starts, negative once it has started on an earlier day.
    pub days_until: i64,
    /// Whether `now_ms` falls inside `[start_ms, end_ms)` — the "in
    /// progress" half of the band rule.
    pub in_progress: bool,
}

/// One calendar event read as an [`ScpsEvent`], or `None` when its title
/// does not parse as `SCPS `-prefixed, its dates are malformed, or the host
/// could not resolve a [`DEVICE_ZONE`] fact it needed.
fn scps_event_from_calendar_event(
    event: &CalendarEventFacts,
    today: &CivilDate,
    now_ms: i64,
    facts: &ZoneFacts,
) -> Option<ScpsEvent> {
    let title = parse_scps_title(&event.title)?;
    let (start_date, start_ms, end_ms) = match &event.when {
        CalendarEventWhenFacts::AllDay { start_date, end_date } => {
            if !is_civil_date(start_date) {
                return None;
            }
            let start_ms = facts.midnight_ms(DEVICE_ZONE, start_date)?;
            let end_ms = facts.midnight_ms(DEVICE_ZONE, end_date)?;
            (start_date.clone(), start_ms, end_ms)
        }
        CalendarEventWhenFacts::Timed { start_ms, end_ms } => {
            let start_date = facts.civil_date(DEVICE_ZONE, *start_ms)?;
            (start_date, *start_ms, *end_ms)
        }
    };
    let days_until = civil_days_between(today, &start_date)?;
    Some(ScpsEvent {
        id: event.provider_event_id.clone(),
        kind: title.kind,
        topic: title.topic,
        start_ms,
        end_ms,
        start_date,
        location: event.location.clone(),
        notes: event.description.clone(),
        days_until,
        in_progress: start_ms <= now_ms && now_ms < end_ms,
    })
}

/// Every `SCPS `-prefixed, non-cancelled event still ahead of (or under)
/// `now_ms`, soonest start first — `vacation.rs`'s `trip_queue`, without a
/// calendar-id filter (see the module header for why).
pub fn scps_queue(
    events: &[CalendarEventFacts],
    today: &CivilDate,
    now_ms: i64,
    facts: &ZoneFacts,
) -> Vec<ScpsEvent> {
    let mut queue: Vec<ScpsEvent> = events
        .iter()
        .filter(|event| event.status != CalendarEventStatusFact::Cancelled)
        .filter_map(|event| scps_event_from_calendar_event(event, today, now_ms, facts))
        .filter(|event| event.end_ms > now_ms)
        .collect();
    queue.sort_by(|left, right| left.start_ms.cmp(&right.start_ms).then_with(|| left.id.cmp(&right.id)));
    queue
}

/// How soon the next SCPS event matters (ADR-0032 part 5's table): in
/// progress or starting today is [`Band::Live`], tomorrow
/// [`Band::Imminent`], within 7 days [`Band::Near`], beyond
/// [`Band::Distant`], nothing in the window [`Band::Dormant`].
pub fn scps_band(next: Option<&ScpsEvent>) -> Band {
    let Some(next) = next else {
        return Band::Dormant;
    };
    if next.in_progress || next.days_until <= 0 {
        return Band::Live;
    }
    if next.days_until == 1 {
        return Band::Imminent;
    }
    if next.days_until <= 7 {
        return Band::Near;
    }
    Band::Distant
}

/// Epoch ms of this pane's next relevant moment — the next event's end
/// while it is in progress, its start otherwise; `None` when nothing is in
/// the window.
pub fn scps_within_band(next: Option<&ScpsEvent>) -> Option<i64> {
    let next = next?;
    Some(if next.in_progress { next.end_ms } else { next.start_ms })
}

/// The month-token parser (ADR-0032 part 4): `scps-quest`'s stored text is
/// `YYYY-MM <phrase>` — the first whitespace-delimited token, then
/// everything after it, trimmed. Returns `None` for anything that does not
/// parse: no binding, a non-text binding, a malformed or missing month
/// token, or a phrase that trims to nothing.
pub fn scps_quest(inputs: &PaneInputs) -> Option<(String, String)> {
    let text = match inputs.binding(SCPS_QUEST_BINDING_KEY).map(|binding| &binding.value) {
        Some(BindingValueFact::Text { text }) => text,
        _ => return None,
    };
    let mut parts = text.splitn(2, char::is_whitespace);
    let month = parts.next()?;
    let phrase = parts.next().unwrap_or("").trim();
    if !is_month_token(month) || phrase.is_empty() {
        return None;
    }
    Some((month.to_string(), phrase.to_string()))
}

/// Whether `token` is a well-formed `YYYY-MM` — four digits, a dash, two
/// digits, nothing more. Deliberately not [`is_civil_date`]: a month token
/// has no day component and [`is_civil_date`] would reject every valid one.
fn is_month_token(token: &str) -> bool {
    let bytes = token.as_bytes();
    bytes.len() == 7
        && bytes[0].is_ascii_digit()
        && bytes[1].is_ascii_digit()
        && bytes[2].is_ascii_digit()
        && bytes[3].is_ascii_digit()
        && bytes[4] == b'-'
        && bytes[5].is_ascii_digit()
        && bytes[6].is_ascii_digit()
}

/// The Photo Quest, as the expanded pane and the collapsed meta line each
/// read it — never a rendered sentence; the month name and the "no quest
/// posted" phrasing are the client's own words over these facts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ScpsQuestFact {
    /// No binding, an unparseable one, or `scps-quest` unset — indistinguishable
    /// to this pane (ADR-0032 part 4's own "treated like `Other`" rule).
    None,
    /// The stored month equals the device's current civil month.
    Current { phrase: String },
    /// The stored month is not the device's current civil month — shown as
    /// "posted for `<month>`", never as this month's answer.
    Other { month: String, phrase: String },
}

/// [`scps_quest`], classified against `today`'s own civil month.
fn scps_quest_fact(inputs: &PaneInputs, today: &CivilDate) -> ScpsQuestFact {
    match scps_quest(inputs) {
        None => ScpsQuestFact::None,
        Some((month, phrase)) => {
            if month == today[..7] {
                ScpsQuestFact::Current { phrase }
            } else {
                ScpsQuestFact::Other { month, phrase }
            }
        }
    }
}

/// Why this pane has no answer yet, or that it has landed a read — never a
/// calendar-id filter (the module header's own point). Borrows the bound
/// read's events rather than cloning them, `vacation.rs`'s
/// [`super::vacation::VacationSetup`] precedent.
#[derive(Debug, Clone, PartialEq)]
pub enum ScpsSetup<'a> {
    /// `!calendar_connected`, or the calendar arm has not answered yet —
    /// collapsed into one arm because both read as
    /// [`AnswerState::BoundButUnacquired`] here (see the module header).
    Unacquired,
    Read { events: &'a [CalendarEventFacts], freshness: FreshnessFact },
}

pub fn scps_setup(inputs: &PaneInputs) -> ScpsSetup<'_> {
    if !inputs.calendar_connected {
        return ScpsSetup::Unacquired;
    }
    match inputs.calendar_reads.get(CALENDAR_REQUEST_KEY) {
        None | Some(CalendarReadFacts::NotRead) => ScpsSetup::Unacquired,
        Some(CalendarReadFacts::Read { events, freshness }) => {
            ScpsSetup::Read { events, freshness: *freshness }
        }
    }
}

/// Why this pane has no *facts*, once a calendar read has landed — the
/// zone bridge's own gap, `vacation.rs`'s `VacationGap` precedent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "gap", rename_all = "camelCase")]
pub enum ScpsGap {
    UnresolvableZone,
}

/// The answered view an expanded pane draws.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScpsFacts {
    /// The soonest SCPS event still ahead of (or under) now. `None` when
    /// the window holds nothing — still an answer, banded `dormant`.
    pub next: Option<ScpsEvent>,
    /// Every further SCPS event after `next`, in order, never truncated.
    pub later: Vec<ScpsEvent>,
    pub quest: ScpsQuestFact,
    pub freshness: FreshnessFact,
    pub stale: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ScpsResolved {
    /// Boxed: `ScpsFacts` (two `Vec<ScpsEvent>`-adjacent `String` fields per
    /// event) makes this variant far larger than `Gap`'s, and
    /// `clippy::large_enum_variant` is a workspace-wide gate (CLAUDE.md).
    Facts(Box<ScpsFacts>),
    Gap { gap: ScpsGap },
}

/// The fully-resolved queue and quest for an already-[`ScpsSetup::Read`]
/// setup.
pub fn scps_facts(
    events: &[CalendarEventFacts],
    freshness: FreshnessFact,
    inputs: &PaneInputs,
    facts: &ZoneFacts,
) -> ScpsResolved {
    let Some(today) = facts.civil_date(DEVICE_ZONE, inputs.now_ms) else {
        return ScpsResolved::Gap { gap: ScpsGap::UnresolvableZone };
    };
    let queue = scps_queue(events, &today, inputs.now_ms, facts);
    let mut iter = queue.into_iter();
    let next = iter.next();
    let later = iter.collect();
    ScpsResolved::Facts(Box::new(ScpsFacts {
        next,
        later,
        quest: scps_quest_fact(inputs, &today),
        freshness,
        stale: freshness.is_stale_beyond(STALE_AFTER_MS),
    }))
}

/// The answered view an expanded pane draws — `None` while unacquired.
pub fn scps_view(inputs: &PaneInputs, facts: &ZoneFacts) -> Option<ScpsResolved> {
    match scps_setup(inputs) {
        ScpsSetup::Read { events, freshness } => Some(scps_facts(events, freshness, inputs, facts)),
        ScpsSetup::Unacquired => None,
    }
}

/// This question's answer for the shell. Never [`AnswerState::Unbound`]
/// (the module header's own point) — an empty window is
/// [`AnswerState::Answered`], banded [`Band::Dormant`].
pub fn scps_answer(inputs: &PaneInputs, facts: &ZoneFacts) -> PaneAnswerCore {
    let gap = |answer_state| PaneAnswerCore { answer_state, band: Band::Dormant, within_band: None };
    match scps_setup(inputs) {
        ScpsSetup::Unacquired => gap(AnswerState::BoundButUnacquired),
        ScpsSetup::Read { events, freshness } => match scps_facts(events, freshness, inputs, facts) {
            ScpsResolved::Gap { .. } => gap(AnswerState::BoundButUnacquired),
            ScpsResolved::Facts(view) => {
                let next = view.next.as_ref();
                PaneAnswerCore {
                    answer_state: AnswerState::Answered,
                    band: scps_band(next),
                    within_band: scps_within_band(next),
                }
            }
        },
    }
}

fn horizon_start_ms(now_ms: i64) -> i64 {
    now_ms - HORIZON_BEFORE_MS
}

fn horizon_end_ms(now_ms: i64) -> i64 {
    now_ms + HORIZON_AFTER_DAYS * DAY_MS
}

/// Every `(DEVICE_ZONE, civil-date/instant)` fact this question needs —
/// "today" (for the quest's month comparison and the queue's `days_until`),
/// the horizon's own two civil ends (so [`scps_calendar_interval`] is
/// answerable from the same round trip, #564's rule), and one
/// `CivilDate`/`Midnight` pair per `SCPS `-titled, non-cancelled event on
/// the bound read.
pub fn scps_zone_queries(inputs: &PaneInputs) -> Vec<ZoneQuery> {
    let mut queries = vec![ZoneQuery::CivilDate { zone: DEVICE_ZONE.to_string(), at_ms: inputs.now_ms }];
    queries.push(ZoneQuery::CivilDate { zone: DEVICE_ZONE.to_string(), at_ms: horizon_start_ms(inputs.now_ms) });
    queries.push(ZoneQuery::CivilDate { zone: DEVICE_ZONE.to_string(), at_ms: horizon_end_ms(inputs.now_ms) });

    if let ScpsSetup::Read { events, .. } = scps_setup(inputs) {
        for event in events {
            if event.status == CalendarEventStatusFact::Cancelled || parse_scps_title(&event.title).is_none() {
                continue;
            }
            match &event.when {
                CalendarEventWhenFacts::AllDay { start_date, end_date } => {
                    if is_civil_date(start_date) {
                        queries.push(ZoneQuery::Midnight { zone: DEVICE_ZONE.to_string(), date: start_date.clone() });
                    }
                    if is_civil_date(end_date) {
                        queries.push(ZoneQuery::Midnight { zone: DEVICE_ZONE.to_string(), date: end_date.clone() });
                    }
                }
                CalendarEventWhenFacts::Timed { start_ms, .. } => {
                    queries.push(ZoneQuery::CivilDate { zone: DEVICE_ZONE.to_string(), at_ms: *start_ms });
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

/// The calendar-arm interval this question needs (#267): the standard
/// horizon, [`HORIZON_BEFORE_MS`] behind now and [`HORIZON_AFTER_DAYS`]
/// ahead.
pub fn scps_calendar_interval(now_ms: i64, facts: &ZoneFacts) -> Option<CalendarInterval> {
    let start_ms = horizon_start_ms(now_ms);
    let end_ms = horizon_end_ms(now_ms);
    let end_day = facts.civil_date(DEVICE_ZONE, end_ms)?;
    Some(CalendarInterval {
        start_ms,
        end_ms,
        start_date: facts.civil_date(DEVICE_ZONE, start_ms)?,
        end_date: add_civil_days(&end_day, 1)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decisions::panes::inputs::{BindingFact, CalendarEventStatusFact as Status};
    use crate::decisions::panes::zone::ZoneFact;

    const OFFSET_MS: i64 = 7 * 3_600_000;

    fn epoch_day_ms(date: &str) -> i64 {
        civil_days_between("1970-01-01", date).unwrap() * 86_400_000
    }

    fn now_at(date: &str, hour: i64, minute: i64) -> i64 {
        epoch_day_ms(date) + hour * 3_600_000 + minute * 60_000 + OFFSET_MS
    }

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

    fn timed(id: &str, title: &str, start_ms: i64, end_ms: i64) -> CalendarEventFacts {
        CalendarEventFacts {
            provider_event_id: id.to_string(),
            calendar_id: "cal-primary".to_string(),
            title: title.to_string(),
            when: CalendarEventWhenFacts::Timed { start_ms, end_ms },
            location: None,
            status: Status::Confirmed,
            description: None,
        }
    }

    fn bound_inputs(now_ms: i64) -> PaneInputs {
        PaneInputs { now_ms, calendar_connected: true, ..Default::default() }
    }

    fn with_read(mut inputs: PaneInputs, events: Vec<CalendarEventFacts>) -> PaneInputs {
        inputs.calendar_reads.insert(
            CALENDAR_REQUEST_KEY.to_string(),
            CalendarReadFacts::Read {
                events,
                freshness: FreshnessFact::Age { age_ms: 0, declared_cadence_ms: None },
            },
        );
        inputs
    }

    fn queue_of(inputs: &PaneInputs) -> Vec<ScpsEvent> {
        let facts = resolve(&scps_zone_queries(inputs));
        match scps_setup(inputs) {
            ScpsSetup::Read { events, .. } => {
                let today = facts.civil_date(DEVICE_ZONE, inputs.now_ms).unwrap();
                scps_queue(events, &today, inputs.now_ms, &facts)
            }
            ScpsSetup::Unacquired => Vec::new(),
        }
    }

    // ------------------------------------------------------- title parser

    #[test]
    fn parses_the_three_named_kinds_and_their_topics() {
        assert_eq!(
            parse_scps_title("SCPS Meeting: Impressions of Venice"),
            Some(ScpsTitle { kind: ScpsKind::Meeting, topic: Some("Impressions of Venice".to_string()) })
        );
        assert_eq!(
            parse_scps_title("SCPS Activity: Super Girl Surf Festival"),
            Some(ScpsTitle { kind: ScpsKind::Activity, topic: Some("Super Girl Surf Festival".to_string()) })
        );
        assert_eq!(
            parse_scps_title("SCPS Happy Hour: Friday drinks"),
            Some(ScpsTitle { kind: ScpsKind::HappyHour, topic: Some("Friday drinks".to_string()) })
        );
    }

    #[test]
    fn a_bare_happy_hour_carries_no_topic() {
        assert_eq!(
            parse_scps_title("SCPS Happy Hour"),
            Some(ScpsTitle { kind: ScpsKind::HappyHour, topic: None })
        );
    }

    #[test]
    fn an_unrecognised_scps_title_reads_as_kind_event() {
        assert_eq!(parse_scps_title("SCPS Picnic"), Some(ScpsTitle { kind: ScpsKind::Event, topic: None }));
    }

    #[test]
    fn a_non_scps_title_is_ignored() {
        assert_eq!(parse_scps_title("Team Sync"), None);
        assert_eq!(parse_scps_title("Weekly SCPS catch-up"), None);
    }

    // -------------------------------------------------------------- band

    #[test]
    fn an_event_at_2330_local_is_today_at_the_device_even_though_it_is_tomorrow_in_utc() {
        // 23:30 local (OFFSET_MS ahead of UTC) lands on the next UTC day —
        // the band must still read `live` off the device's own civil date.
        let event = timed("e1", "SCPS Meeting: Late", now_at("2026-03-10", 23, 30), now_at("2026-03-11", 1, 30));
        let inputs = with_read(bound_inputs(now_at("2026-03-10", 9, 0)), vec![event]);
        let queue = queue_of(&inputs);
        assert_eq!(scps_band(queue.first()), Band::Live);
    }

    #[test]
    fn an_in_progress_event_is_live() {
        let event = timed("e1", "SCPS Activity: Tide Pools", now_at("2026-03-10", 8, 0), now_at("2026-03-10", 11, 0));
        let inputs = with_read(bound_inputs(now_at("2026-03-10", 9, 0)), vec![event]);
        let queue = queue_of(&inputs);
        assert_eq!(scps_band(queue.first()), Band::Live);
        assert!(queue[0].in_progress);
    }

    #[test]
    fn an_empty_window_is_dormant() {
        let inputs = with_read(bound_inputs(now_at("2026-03-10", 9, 0)), Vec::new());
        assert_eq!(scps_band(queue_of(&inputs).first()), Band::Dormant);
    }

    #[test]
    fn climbs_imminent_near_distant_as_the_event_gets_further_away() {
        let ladder = [
            ("2026-03-11", Band::Imminent),
            ("2026-03-15", Band::Near),
            ("2026-03-25", Band::Distant),
        ];
        for (start_day, expected) in ladder {
            let event = timed("e1", "SCPS Meeting: X", now_at(start_day, 18, 0), now_at(start_day, 20, 0));
            let inputs = with_read(bound_inputs(now_at("2026-03-10", 9, 0)), vec![event]);
            let queue = queue_of(&inputs);
            assert_eq!(scps_band(queue.first()), expected, "{start_day}");
        }
    }

    // ------------------------------------------------------- month token

    #[test]
    fn parses_a_well_formed_quest_value() {
        let inputs_with = |value: &str| {
            let mut inputs = bound_inputs(now_at("2026-09-01", 9, 0));
            inputs.bindings = Some(vec![BindingFact {
                key: SCPS_QUEST_BINDING_KEY.to_string(),
                value: BindingValueFact::Text { text: value.to_string() },
            }]);
            inputs
        };
        assert_eq!(
            scps_quest(&inputs_with("2026-09 the beauty of reflections")),
            Some(("2026-09".to_string(), "the beauty of reflections".to_string()))
        );
    }

    #[test]
    fn refuses_a_malformed_month_token_or_an_empty_phrase() {
        let inputs_with = |value: &str| {
            let mut inputs = bound_inputs(now_at("2026-09-01", 9, 0));
            inputs.bindings = Some(vec![BindingFact {
                key: SCPS_QUEST_BINDING_KEY.to_string(),
                value: BindingValueFact::Text { text: value.to_string() },
            }]);
            inputs
        };
        assert_eq!(scps_quest(&inputs_with("2026-9 x")), None);
        assert_eq!(scps_quest(&inputs_with("reflections")), None);
        assert_eq!(scps_quest(&inputs_with("")), None);
    }

    #[test]
    fn a_current_month_quest_shows_in_the_collapsed_meta_and_a_last_months_does_not() {
        let mut current = bound_inputs(now_at("2026-09-15", 9, 0));
        current.bindings = Some(vec![BindingFact {
            key: SCPS_QUEST_BINDING_KEY.to_string(),
            value: BindingValueFact::Text { text: "2026-09 Reflected Light".to_string() },
        }]);
        let current = with_read(current, Vec::new());
        let facts = resolve(&scps_zone_queries(&current));
        match scps_view(&current, &facts).unwrap() {
            ScpsResolved::Facts(view) => {
                assert_eq!(view.quest, ScpsQuestFact::Current { phrase: "Reflected Light".to_string() })
            }
            ScpsResolved::Gap { gap } => panic!("expected facts, got {gap:?}"),
        }

        let mut stale = bound_inputs(now_at("2026-10-01", 9, 0));
        stale.bindings = Some(vec![BindingFact {
            key: SCPS_QUEST_BINDING_KEY.to_string(),
            value: BindingValueFact::Text { text: "2026-09 Reflected Light".to_string() },
        }]);
        let stale = with_read(stale, Vec::new());
        let facts = resolve(&scps_zone_queries(&stale));
        match scps_view(&stale, &facts).unwrap() {
            ScpsResolved::Facts(view) => assert_eq!(
                view.quest,
                ScpsQuestFact::Other { month: "2026-09".to_string(), phrase: "Reflected Light".to_string() }
            ),
            ScpsResolved::Gap { gap } => panic!("expected facts, got {gap:?}"),
        }
    }

    // ------------------------------------------------------------- answer

    #[test]
    fn is_bound_but_unacquired_while_disconnected_never_unbound() {
        let mut inputs = bound_inputs(now_at("2026-03-01", 9, 0));
        inputs.calendar_connected = false;
        let answer = scps_answer(&inputs, &ZoneFacts::default());
        assert_eq!(answer.answer_state, AnswerState::BoundButUnacquired);
    }

    #[test]
    fn is_bound_but_unacquired_while_the_read_has_not_landed() {
        let inputs = bound_inputs(now_at("2026-03-01", 9, 0));
        let answer = scps_answer(&inputs, &ZoneFacts::default());
        assert_eq!(answer.answer_state, AnswerState::BoundButUnacquired);
    }

    #[test]
    fn answers_dormant_never_unbound_when_the_window_holds_nothing() {
        let inputs = with_read(bound_inputs(now_at("2026-03-01", 9, 0)), Vec::new());
        let facts = resolve(&scps_zone_queries(&inputs));
        let answer = scps_answer(&inputs, &facts);
        assert_eq!(answer.answer_state, AnswerState::Answered);
        assert_eq!(answer.band, Band::Dormant);
    }

    #[test]
    fn a_non_scps_event_is_never_counted() {
        let event = timed("e1", "Team Sync", now_at("2026-03-10", 9, 0), now_at("2026-03-10", 10, 0));
        let inputs = with_read(bound_inputs(now_at("2026-03-01", 9, 0)), vec![event]);
        assert_eq!(queue_of(&inputs), Vec::new());
    }

    #[test]
    fn a_cancelled_scps_event_is_never_counted() {
        let mut event =
            timed("e1", "SCPS Meeting: X", now_at("2026-03-10", 9, 0), now_at("2026-03-10", 10, 0));
        event.status = Status::Cancelled;
        let inputs = with_read(bound_inputs(now_at("2026-03-01", 9, 0)), vec![event]);
        assert_eq!(queue_of(&inputs), Vec::new());
    }

    #[test]
    fn lists_further_scps_events_in_the_window_never_truncated() {
        let events = vec![
            timed("e1", "SCPS Meeting: A", now_at("2026-03-10", 9, 0), now_at("2026-03-10", 11, 0)),
            timed("e2", "SCPS Activity: B", now_at("2026-03-20", 9, 0), now_at("2026-03-20", 12, 0)),
            timed("e3", "SCPS Happy Hour", now_at("2026-04-01", 17, 0), now_at("2026-04-01", 18, 0)),
        ];
        let inputs = with_read(bound_inputs(now_at("2026-03-01", 9, 0)), events);
        let facts = resolve(&scps_zone_queries(&inputs));
        match scps_view(&inputs, &facts).unwrap() {
            ScpsResolved::Facts(view) => {
                assert_eq!(view.next.map(|e| e.id), Some("e1".to_string()));
                let ids: Vec<String> = view.later.into_iter().map(|e| e.id).collect();
                assert_eq!(ids, vec!["e2".to_string(), "e3".to_string()]);
            }
            ScpsResolved::Gap { gap } => panic!("expected facts, got {gap:?}"),
        }
    }

    // ------------------------------------------------------------- window

    #[test]
    fn the_calendar_interval_is_the_standard_horizon() {
        let now_ms = now_at("2026-08-14", 9, 0);
        let facts = resolve(&scps_zone_queries(&PaneInputs { now_ms, ..PaneInputs::default() }));
        let interval = scps_calendar_interval(now_ms, &facts).expect("an interval");
        assert_eq!(interval.start_ms, now_ms - HORIZON_BEFORE_MS);
        assert_eq!(interval.end_ms, now_ms + HORIZON_AFTER_DAYS * DAY_MS);
    }

    #[test]
    fn an_unresolvable_device_zone_is_a_gap_never_a_fabricated_today() {
        let event = timed("e1", "SCPS Meeting: X", now_at("2026-03-10", 9, 0), now_at("2026-03-10", 10, 0));
        let inputs = with_read(bound_inputs(now_at("2026-03-01", 9, 0)), vec![event]);
        let facts = ZoneFacts::default();
        assert_eq!(scps_view(&inputs, &facts), Some(ScpsResolved::Gap { gap: ScpsGap::UnresolvableZone }));
        let answer = scps_answer(&inputs, &facts);
        assert_eq!(answer.answer_state, AnswerState::BoundButUnacquired);
    }
}
