//! **The weekend-plans question** (#122), sunk out of
//! `screens/weekend-pane/weekend.ts` by ADR-0025 (#534) — the hardest of
//! the seven remaining panes, because unlike `waste.rs` its zone is never
//! carried on a payload: it is always [`DEVICE_ZONE`], the reader's own.
//!
//! # The two-phase-within-a-phase problem, and how it is resolved
//!
//! [`weekend_window`] needs to know "today" (at the device) before it can
//! even name which civil dates its Friday/Saturday/Sunday are — but
//! [`weekend_zone_queries`] has to name every [`ZoneQuery`] it needs
//! *before* any of them are resolved. A real two-phase call (ask for
//! "today", then ask for its Friday) is not on offer: the bridge is
//! exactly one round trip.
//!
//! The way out is **bounded over-asking**. [`weekend_zone_queries`] takes
//! `now_ms` alone (no snapshot, no prior answer — the module doc for
//! [`super`](super) already promises this is possible for a pane with no
//! payload) and computes a *pivot* civil date from it directly in UTC —
//! `DateTime::<Utc>::date_naive()`, which needs no tzdb at all, because UTC
//! carries no offset table to look up. The device's real "today" can differ
//! from that UTC pivot by at most one calendar day either way (every IANA
//! offset sits inside [-12h, +14h], which can never move a wall-clock date
//! by more than one day from its UTC date). So the query builder computes
//! the *candidate* window — this week's Friday/Saturday/Sunday/day-after,
//! **and** the rolled-forward next week's four — for all three possible
//! "todays" (`pivot - 1`, `pivot`, `pivot + 1`), and asks
//! [`ZoneQuery::Midnight`] for the union of every date that touches. Twenty
//! or so `Midnight` queries plus one `CivilDate` query is the whole cost of
//! never being wrong about which "today" the host resolves.
//!
//! [`weekend_window`] itself then reads the *real* resolved "today" out of
//! [`ZoneFacts`] and walks forward using ordinary Rust weekday arithmetic
//! (`zone.rs`'s [`weekday_index`]/[`add_civil_days`]) — it never touches the
//! pivot again. If the host omitted [`DEVICE_ZONE`] entirely (it could not
//! resolve the reader's own zone, which should not happen in practice but
//! is not assumed here), every lookup inside [`weekend_window`] answers
//! `None` and the whole function answers `None` — never a fabricated
//! window built off the pivot, which would silently be a UTC weekend
//! instead of the reader's own.
//!
//! # What did not sink: `entryUrgency`, and most of `WindowEntry`
//!
//! `weekend.ts`'s `entryUrgency` is `computeUrgency(entry.item?.deadline,
//! nowMs)` and nothing more — and `computeUrgency` **is** already this
//! crate's own [`super::super::urgency::compute_urgency`] (`urgency.ts` is
//! now a re-export of the sunk seam, per that module's header). So the
//! *computation* `entryUrgency` calls is core already. But `entryUrgency`
//! itself is never read by `weekendBand`/`weekendWithinBand`/`countKinds`:
//! its only caller is the expanded view, where it colours one entry's dot.
//! That is downstream rendering of an already-merged entry, not a fact this
//! pane's own answer needs, so it stays a one-line web call
//! (`computeUrgency(entry.item?.deadline, nowMs)` against the seam) rather
//! than a second copy here.
//!
//! The same reasoning trims [`WindowEntry`]'s TS shape down to nothing at
//! all in the port: `weekendBand`/`weekendWithinBand` read only the
//! window's own `startMs`/`endMs`/`underWay`, and `countKinds` reads only
//! each entry's *kind*, never its `id`/`title`/`atMs`/`anchor`/
//! `alsoScheduledOn`/`deadlineOutsideWindow`. So [`merge_counts`] below
//! folds the merge straight down to a [`WindowCounts`] tally instead of
//! building a per-entry list the answer never reads — the full merged
//! window with titles (`weekendView`'s job) is the client's own to keep,
//! exactly as `waste.rs`'s parser/answer split leaves rendering-only data
//! out. `alsoScheduledOn` and `deadlineOutsideWindow` are exactly this: the
//! dedupe rule they annotate *is* ported (an item both due and scheduled
//! inside the window counts once, as due), but the residue strings
//! themselves are never read by anything this module exposes.

use serde::{Deserialize, Serialize};

use super::inputs::{
    CalendarEventFacts, CalendarEventStatusFact, CalendarEventWhenFacts, CalendarReadFacts,
    PaneInputs, PaneItemFacts,
};
use super::contract::{AnswerState, Band, CalendarInterval, PaneAnswerCore};
use super::zone::{add_civil_days, weekday_index, CivilDate, ZoneFacts, ZoneQuery, DEVICE_ZONE};

use hummingbird_domain::deadline_sort_key;

/// The one subject this question ever has — always present, even unbound,
/// so the setup prompt is discoverable (`waste.ts`'s own reasoning).
pub const SUBJECT_KEY: &str = "coming-weekend";

/// The registry's `requiredCalendarRequests()` key, and the
/// `PaneInputs::calendar_reads` lookup key.
pub const CALENDAR_REQUEST_KEY: &str = "weekend";

const HOUR_MS: i64 = 60 * 60 * 1000;
const MINUTE_MS: i64 = 60 * 1000;

/// The window opens this many hours before it starts to read as `imminent`
/// rather than `near` — beside the band function, ADR-0015's own discipline.
pub const IMMINENT_WITHIN_MS: i64 = 48 * HOUR_MS;

/// Beyond [`IMMINENT_WITHIN_MS`] but inside this reads as `near`; beyond it,
/// `dormant`.
pub const NEAR_WITHIN_MS: i64 = 96 * HOUR_MS;

/// One of the window's three days — trimmed to what a decision reads: its
/// own civil date and the local instants it spans. No `label`/`dateLabel`
/// (rendering) and no `entries` (the module header's own call).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeekendDay {
    pub date: CivilDate,
    pub start_ms: i64,
    pub end_ms: i64,
}

/// Friday 17:00 through Sunday 23:59:59.999, at the device — #122's pinned
/// window, resolved through [`DEVICE_ZONE`] rather than raw `Date` math.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeekendWindow {
    pub start_ms: i64,
    pub end_ms: i64,
    /// Always exactly three days — Friday, Saturday, Sunday — even once
    /// some are in the past: "what are my plans", not "what is left".
    pub days: [WeekendDay; 3],
    /// Whether `now_ms` falls inside `[start_ms, end_ms]` — the weekend
    /// answered is the one under way, not the next one.
    pub under_way: bool,
}

/// The merge's decision-relevant residue: how many of each kind landed in
/// the window. Never a per-entry list — see the module header.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowCounts {
    pub events: i64,
    pub due: i64,
    pub scheduled: i64,
}

/// Why this pane has no answer — a kind, not a sentence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WeekendGap {
    /// This device has never connected a calendar at all
    /// (`inputs.calendar_connected`) — the only path to `unbound` (#122
    /// review fix; see [`weekend_answer`]'s doc).
    NotConnected,
    /// Connected, but the calendar arm has not landed a real answer yet:
    /// no entry at all, or [`CalendarReadFacts::NotRead`] — covers "never
    /// polled", "offline" and "needs reconnect" alike, on `waste.ts`'s own
    /// "unread" reasoning.
    Unacquired,
    /// [`DEVICE_ZONE`] could not be resolved by this host — the absent
    /// fact of `zone.rs`'s bridge, decided here exactly as
    /// `WasteGap::UnresolvableZone` is.
    UnresolvableZone,
}

/// Everything an answered pane needs to decide its band, plus the counts
/// its (unsunk) headline reads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeekendFacts {
    pub window: WeekendWindow,
    pub counts: WindowCounts,
}

/// The whole answered fact set, or the reason there is none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum WeekendResolved {
    Facts(WeekendFacts),
    Gap { gap: WeekendGap },
}

/// The UTC civil date `at_ms` falls on — pure calendar math, no zone table,
/// used only as [`weekend_zone_queries`]'s over-asking pivot. Never used as
/// an answer to "what day is it at the device" — that is
/// [`ZoneFacts::civil_date`]'s job, resolved through the bridge.
fn utc_pivot_date(at_ms: i64) -> CivilDate {
    use chrono::{DateTime, Utc};
    DateTime::<Utc>::from_timestamp_millis(at_ms)
        .unwrap_or_else(|| DateTime::<Utc>::from_timestamp_millis(0).unwrap())
        .date_naive()
        .format("%Y-%m-%d")
        .to_string()
}

/// Every [`ZoneQuery`] this pane could possibly need, given only `now_ms` —
/// see the module header for why this is an over-ask rather than a real
/// two-phase call.
pub fn weekend_zone_queries(now_ms: i64) -> Vec<ZoneQuery> {
    let pivot = utc_pivot_date(now_ms);
    let mut dates: Vec<CivilDate> = Vec::new();

    for delta in [-1i64, 0, 1] {
        let Some(candidate_today) = add_civil_days(&pivot, delta) else { continue };
        let Some(dow) = weekday_index(&candidate_today) else { continue };
        let days_since_last_friday = (i64::from(dow) - 5 + 7) % 7;
        let Some(friday) = add_civil_days(&candidate_today, -days_since_last_friday) else {
            continue;
        };
        // This candidate's own window (Fri/Sat/Sun/day-after, offsets
        // 0..3) and the rolled-forward next one (offsets 7..10) — one of
        // the two is what `weekend_window` will actually pick, and this
        // builder cannot know which without the round trip it is avoiding.
        for offset in [0i64, 1, 2, 3, 7, 8, 9, 10] {
            if let Some(date) = add_civil_days(&friday, offset) {
                if !dates.contains(&date) {
                    dates.push(date);
                }
            }
        }
    }

    let mut queries = vec![ZoneQuery::CivilDate { zone: DEVICE_ZONE.to_string(), at_ms: now_ms }];
    for date in dates {
        queries.push(ZoneQuery::Midnight { zone: DEVICE_ZONE.to_string(), date });
    }
    queries
}

/// Friday 17:00 through Sunday 23:59:59.999, resolved at the device.
/// `None` when [`DEVICE_ZONE`] could not be resolved by the host — never a
/// fallback built off the UTC pivot (see the module header).
///
/// Rolls forward to the *next* weekend from Sunday 20:00 at the device: the
/// threshold is computed once, as the current candidate window's own Sunday
/// 20:00, and if `now_ms` has already passed it the whole window shifts
/// forward by exactly one week — `weekend.ts`'s own rule, ported verbatim.
pub fn weekend_window(now_ms: i64, facts: &ZoneFacts) -> Option<WeekendWindow> {
    let today = facts.civil_date(DEVICE_ZONE, now_ms)?;
    let dow = weekday_index(&today)?; // 0 Sun … 6 Sat
    let days_since_last_friday = (i64::from(dow) - 5 + 7) % 7;
    let mut friday = add_civil_days(&today, -days_since_last_friday)?;

    let candidate_sunday = add_civil_days(&friday, 2)?;
    let roll_at_ms = facts.midnight_ms(DEVICE_ZONE, &candidate_sunday)? + 20 * HOUR_MS;
    if now_ms >= roll_at_ms {
        friday = add_civil_days(&friday, 7)?;
    }

    // Friday, Saturday, Sunday, and the day after Sunday — four midnights
    // resolve the window's start, its three days, and its end in one pass.
    let day_dates: [CivilDate; 4] = [
        friday.clone(),
        add_civil_days(&friday, 1)?,
        add_civil_days(&friday, 2)?,
        add_civil_days(&friday, 3)?,
    ];
    let midnights: Vec<i64> = day_dates
        .iter()
        .map(|date| facts.midnight_ms(DEVICE_ZONE, date))
        .collect::<Option<Vec<i64>>>()?;

    let start_ms = midnights[0] + 17 * HOUR_MS;
    let end_ms = midnights[3] - 1;
    let days = [
        WeekendDay { date: day_dates[0].clone(), start_ms: midnights[0], end_ms: midnights[1] - 1 },
        WeekendDay { date: day_dates[1].clone(), start_ms: midnights[1], end_ms: midnights[2] - 1 },
        WeekendDay { date: day_dates[2].clone(), start_ms: midnights[2], end_ms: midnights[3] - 1 },
    ];
    let under_way = now_ms >= start_ms && now_ms <= end_ms;

    Some(WeekendWindow { start_ms, end_ms, days, under_way })
}

/// The calendar-arm interval this question needs (#267), off its own
/// resolved window — `weekend-pane/question.ts`'s `calendarRequests`,
/// sunk by #564 because Android needs the identical window and a second
/// copy of these bounds is exactly the drift ADR-0025 forbids.
///
/// The lower civil bound is **Friday's own day key**, not `start_ms`'s
/// (Friday 17:00): an all-day event covering Friday is a fact about the
/// whole day, and asking from 17:00 would ask about Saturday onwards —
/// the same reason [`in_window`] uses `days[0].start_ms` below. The upper
/// civil bound is exclusive, so it is the day after Sunday; `end_ms` is
/// Sunday 23:59:59.999 and one millisecond later is Monday local midnight.
///
/// `None` when [`DEVICE_ZONE`] could not be resolved — never a window
/// built off a UTC pivot (the module header).
pub fn weekend_calendar_interval(now_ms: i64, facts: &ZoneFacts) -> Option<CalendarInterval> {
    let window = weekend_window(now_ms, facts)?;
    Some(CalendarInterval {
        start_ms: window.start_ms,
        end_ms: window.end_ms,
        start_date: window.days[0].date.clone(),
        end_date: add_civil_days(&window.days[2].date, 1)?,
    })
}

/// Day-membership test for an item's due/do-date — deliberately NOT
/// `[window.start_ms, window.end_ms]`. `window.start_ms` is Friday
/// **17:00** (the band's own "has the weekend started" instant), but a
/// scheduled or due date anchors to the *start* of its day, so the lower
/// bound here is `days[0].start_ms` (Friday local midnight) — #122's own
/// review fix, ported verbatim from `weekend.ts`'s `inWindow`.
fn in_window(ms: i64, window: &WeekendWindow) -> bool {
    let lower_ms = window.days[0].start_ms;
    ms >= lower_ms && ms <= window.end_ms
}

/// Resolves a deadline to an instant at the device, through
/// [`deadline_sort_key`] exactly as `weekend.ts`'s `deadlineToMs` resolves
/// through the TS `deadlineSortKey` — a day-only deadline means end of that
/// day. The date part is answered by [`ZoneFacts::midnight_ms`] (already
/// resolved for every window day by [`weekend_window`]'s own queries); the
/// time-of-day is plain ms arithmetic on top, the same "midnight, then add
/// a fixed offset" discipline `weekend_window` itself uses for 17:00/20:00.
fn deadline_to_ms(deadline: &str, facts: &ZoneFacts) -> Option<i64> {
    let key = deadline_sort_key(deadline);
    if key.len() != 16 || key.as_bytes().get(10) != Some(&b'T') {
        return None;
    }
    let date = &key[0..10];
    let hour: i64 = key[11..13].parse().ok()?;
    let minute: i64 = key[14..16].parse().ok()?;
    if key.as_bytes().get(13) != Some(&b':') || hour > 23 || minute > 59 {
        return None;
    }
    let midnight = facts.midnight_ms(DEVICE_ZONE, date)?;
    Some(midnight + hour * HOUR_MS + minute * MINUTE_MS)
}

/// A scheduled date is day-only by construction (ADR-0009/0013), so it
/// anchors to the start of its day.
fn scheduled_to_ms(scheduled: &str, facts: &ZoneFacts) -> Option<i64> {
    facts.midnight_ms(DEVICE_ZONE, scheduled)
}

/// The merge, folded straight to counts — events overlapping the window,
/// plus due/scheduled items inside it, with #122's own dedupe rule: an item
/// both due and scheduled in the window counts once, as due (a deadline is
/// a consequence, a do-date is a preference, and the one with consequences
/// is what the day owes).
///
/// A multi-day event counts once per day it touches — `weekend.ts`'s
/// `countKinds` walks `window.days` and sums each day's own entries, so an
/// all-day trip spanning Saturday and Sunday reads as two, not one.
fn merge_counts(
    window: &WeekendWindow,
    events: &[CalendarEventFacts],
    items: &[PaneItemFacts],
    facts: &ZoneFacts,
) -> WindowCounts {
    let mut counts = WindowCounts::default();

    for event in events {
        if event.status == CalendarEventStatusFact::Cancelled {
            continue; // defence in depth — the sync layer already filters these.
        }
        match &event.when {
            CalendarEventWhenFacts::AllDay { start_date, end_date } => {
                for day in &window.days {
                    if start_date.as_str() <= day.date.as_str()
                        && day.date.as_str() < end_date.as_str()
                    {
                        counts.events += 1;
                    }
                }
            }
            CalendarEventWhenFacts::Timed { start_ms, end_ms } => {
                let overlap_start = (*start_ms).max(window.start_ms);
                let overlap_end = (*end_ms).min(window.end_ms);
                if overlap_start > overlap_end {
                    continue;
                }
                for day in &window.days {
                    let day_overlap_start = overlap_start.max(day.start_ms);
                    let day_overlap_end = overlap_end.min(day.end_ms);
                    if day_overlap_start <= day_overlap_end {
                        counts.events += 1;
                    }
                }
            }
        }
    }

    for item in items {
        let due_ms = item.deadline.as_deref().and_then(|d| deadline_to_ms(d, facts));
        let scheduled_ms = item.scheduled_date.as_deref().and_then(|s| scheduled_to_ms(s, facts));
        let due_here = due_ms.is_some_and(|ms| in_window(ms, window));
        let scheduled_here = scheduled_ms.is_some_and(|ms| in_window(ms, window));

        if due_here {
            counts.due += 1;
            continue; // the dedupe: never also counted as scheduled.
        }
        if scheduled_here {
            counts.scheduled += 1;
        }
    }

    counts
}

/// The whole answered fact set, or the reason there is none.
///
/// Reads `calendar_connected` first — #122 review fix, ported verbatim:
/// `!calendar_connected` is the only path to [`WeekendGap::NotConnected`],
/// so a connected-but-not-yet-polled device never reads as "go set this up".
pub fn weekend_facts(inputs: &PaneInputs, facts: &ZoneFacts) -> WeekendResolved {
    if !inputs.calendar_connected {
        return WeekendResolved::Gap { gap: WeekendGap::NotConnected };
    }
    let events = match inputs.calendar_reads.get(CALENDAR_REQUEST_KEY) {
        None | Some(CalendarReadFacts::NotRead) => {
            return WeekendResolved::Gap { gap: WeekendGap::Unacquired }
        }
        Some(CalendarReadFacts::Read { events, .. }) => events,
    };
    let Some(window) = weekend_window(inputs.now_ms, facts) else {
        return WeekendResolved::Gap { gap: WeekendGap::UnresolvableZone };
    };
    let counts = merge_counts(&window, events, &inputs.items, facts);
    WeekendResolved::Facts(WeekendFacts { window, counts })
}

/// How soon the window matters — never `distant`: the window is at most a
/// handful of days away by construction (`weekend_window`'s own rollover).
pub fn weekend_band(window: &WeekendWindow, now_ms: i64) -> Band {
    if window.under_way {
        return Band::Live;
    }
    let until_start_ms = window.start_ms - now_ms;
    if until_start_ms <= IMMINENT_WITHIN_MS {
        Band::Imminent
    } else if until_start_ms <= NEAR_WITHIN_MS {
        Band::Near
    } else {
        Band::Dormant
    }
}

/// Epoch ms of the window's next relevant moment — its start while that is
/// still ahead, its end once the weekend is under way.
pub fn weekend_within_band(window: &WeekendWindow) -> i64 {
    if window.under_way {
        window.end_ms
    } else {
        window.start_ms
    }
}

/// This question's answer for the shell (#245/#122), minus its rendering
/// half. Every [`WeekendGap`] collapses to [`AnswerState::BoundButUnacquired`]
/// except [`WeekendGap::NotConnected`], which is the only
/// [`AnswerState::Unbound`] — `waste_answer`'s own collapsing discipline.
pub fn weekend_answer(inputs: &PaneInputs, facts: &ZoneFacts) -> PaneAnswerCore {
    let gap = |answer_state| PaneAnswerCore { answer_state, band: Band::Dormant, within_band: None };
    match weekend_facts(inputs, facts) {
        WeekendResolved::Gap { gap: WeekendGap::NotConnected } => gap(AnswerState::Unbound),
        WeekendResolved::Gap { .. } => gap(AnswerState::BoundButUnacquired),
        WeekendResolved::Facts(view) => PaneAnswerCore {
            answer_state: AnswerState::Answered,
            band: weekend_band(&view.window, inputs.now_ms),
            within_band: Some(weekend_within_band(&view.window)),
        },
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::decisions::panes::zone::{civil_days_between, ZoneFact};

    /// A fixed device zone, UTC-7 (like `America/Los_Angeles` in August) —
    /// the same offset convention `waste.rs`'s own fixture uses, applied
    /// here to [`DEVICE_ZONE`] instead of a payload zone.
    const OFFSET_MS: i64 = 7 * HOUR_MS;

    /// Device-local wall-clock time -> UTC instant, mirroring the resolver
    /// below exactly, the same way `weekend.test.ts`'s own `at()` helper
    /// builds instants against the *actual* device zone the test runs in.
    fn at(y: i32, m: u32, d: u32, h: i64, min: i64) -> i64 {
        let epoch = chrono::NaiveDate::from_ymd_opt(1970, 1, 1).unwrap();
        let date = chrono::NaiveDate::from_ymd_opt(y, m as i32 as u32, d).unwrap();
        let days = (date - epoch).num_days();
        days * 24 * HOUR_MS + h * HOUR_MS + min * MINUTE_MS + OFFSET_MS
    }

    fn day_key(y: i32, m: u32, d: u32) -> String {
        format!("{y:04}-{m:02}-{d:02}")
    }

    /// A host resolver stood up from the queries the core itself asked,
    /// exactly the shape a real host takes — `waste.rs`'s own fixture,
    /// keyed to [`DEVICE_ZONE`] instead of a payload zone.
    fn resolve(queries: &[ZoneQuery]) -> ZoneFacts {
        let mut facts = ZoneFacts::default();
        for query in queries {
            match query {
                ZoneQuery::CivilDate { zone, at_ms } if zone == DEVICE_ZONE => {
                    let local = at_ms - OFFSET_MS;
                    let days = local.div_euclid(24 * HOUR_MS);
                    let date = add_civil_days("1970-01-01", days).unwrap();
                    facts.insert(query, ZoneFact::Date(date));
                }
                ZoneQuery::Midnight { zone, date } if zone == DEVICE_ZONE => {
                    let days = civil_days_between("1970-01-01", date).unwrap();
                    facts.insert(query, ZoneFact::Instant(days * 24 * HOUR_MS + OFFSET_MS));
                }
                _ => {}
            }
        }
        facts
    }

    fn window_at(now_ms: i64) -> WeekendWindow {
        weekend_window(now_ms, &resolve(&weekend_zone_queries(now_ms))).expect("window resolves")
    }

    fn event(start_ms: i64, end_ms: i64) -> CalendarEventFacts {
        CalendarEventFacts {
            provider_event_id: "ev".to_string(),
            calendar_id: "cal-primary".to_string(),
            title: "ev".to_string(),
            when: CalendarEventWhenFacts::Timed { start_ms, end_ms },
            location: None,
            status: CalendarEventStatusFact::Confirmed,
        }
    }

    fn all_day_event(start_date: &str, end_date: &str) -> CalendarEventFacts {
        CalendarEventFacts {
            provider_event_id: "ev".to_string(),
            calendar_id: "cal-primary".to_string(),
            title: "ev".to_string(),
            when: CalendarEventWhenFacts::AllDay {
                start_date: start_date.to_string(),
                end_date: end_date.to_string(),
            },
            location: None,
            status: CalendarEventStatusFact::Confirmed,
        }
    }

    fn item(id: &str, deadline: Option<&str>, scheduled_date: Option<&str>) -> PaneItemFacts {
        PaneItemFacts {
            id: id.to_string(),
            title: id.to_string(),
            deadline: deadline.map(str::to_string),
            scheduled_date: scheduled_date.map(str::to_string),
        }
    }

    fn inputs(now_ms: i64, events: Vec<CalendarEventFacts>, items: Vec<PaneItemFacts>) -> PaneInputs {
        PaneInputs {
            now_ms,
            bindings: None,
            pane_reads: HashMap::new(),
            calendar_reads: HashMap::from([(
                CALENDAR_REQUEST_KEY.to_string(),
                CalendarReadFacts::Read {
                    events,
                    freshness: super::super::inputs::FreshnessFact::Unknown,
                },
            )]),
            calendar_connected: true,
            items,
            sync: Default::default(),
        }
    }

    fn resolved(inputs: &PaneInputs) -> WeekendResolved {
        weekend_facts(inputs, &resolve(&weekend_zone_queries(inputs.now_ms)))
    }

    fn facts_of(inputs: &PaneInputs) -> WeekendFacts {
        match resolved(inputs) {
            WeekendResolved::Facts(facts) => facts,
            WeekendResolved::Gap { gap } => panic!("expected facts, got {gap:?}"),
        }
    }

    // ------------------------------------------------------ weekend_window

    #[test]
    fn is_friday_17_00_through_sunday_23_59_59_999_asked_on_a_friday_afternoon() {
        // 2026-08-14 is a Friday.
        let window = window_at(at(2026, 8, 14, 16, 0));
        assert_eq!(window.start_ms, at(2026, 8, 14, 17, 0));
        assert_eq!(window.end_ms, at(2026, 8, 17, 0, 0) - 1);
    }

    #[test]
    fn keeps_fridays_own_evening_inside_the_answer_asked_at_17_00_sharp() {
        let window = window_at(at(2026, 8, 14, 17, 0));
        let dinner = at(2026, 8, 14, 19, 0);
        assert!(dinner >= window.start_ms && dinner <= window.end_ms);
    }

    #[test]
    fn excludes_a_friday_afternoon_event_before_the_window_opens() {
        let window = window_at(at(2026, 8, 14, 16, 0));
        let lunch = at(2026, 8, 14, 12, 0);
        assert!(lunch < window.start_ms);
    }

    #[test]
    fn asked_on_a_weekday_answers_the_upcoming_weekend() {
        // 2026-08-10 is a Monday; the coming weekend is Aug 14-16.
        let window = window_at(at(2026, 8, 10, 9, 0));
        assert_eq!(window.start_ms, at(2026, 8, 14, 17, 0));
    }

    #[test]
    fn stays_on_the_weekend_under_way_at_sunday_19_59() {
        let window = window_at(at(2026, 8, 16, 19, 59));
        assert_eq!(window.start_ms, at(2026, 8, 14, 17, 0));
        assert!(window.under_way);
    }

    #[test]
    fn rolls_at_exactly_sunday_20_00() {
        let window = window_at(at(2026, 8, 16, 20, 0));
        assert_eq!(window.start_ms, at(2026, 8, 21, 17, 0));
        assert!(!window.under_way);
    }

    #[test]
    fn rolls_forward_to_the_following_weekend_at_sunday_20_01() {
        let window = window_at(at(2026, 8, 16, 20, 1));
        assert_eq!(window.start_ms, at(2026, 8, 21, 17, 0));
        assert!(!window.under_way);
    }

    #[test]
    fn always_carries_exactly_three_days_friday_saturday_sunday() {
        let window = window_at(at(2026, 8, 10, 9, 0));
        assert_eq!(
            window.days.iter().map(|day| day.date.clone()).collect::<Vec<_>>(),
            vec![day_key(2026, 8, 14), day_key(2026, 8, 15), day_key(2026, 8, 16)],
        );
    }

    #[test]
    fn an_unresolvable_device_zone_is_a_gap_never_a_utc_fallback() {
        assert_eq!(weekend_window(at(2026, 8, 10, 9, 0), &ZoneFacts::default()), None);
    }

    // ---------------------------------------------- weekend_calendar_interval

    #[test]
    fn the_calendar_interval_covers_the_whole_of_friday_and_ends_the_day_after_sunday() {
        // Friday 2026-08-14 at 09:00 device time.
        let now_ms = at(2026, 8, 14, 9, 0);
        let facts = resolve(&weekend_zone_queries(now_ms));
        let window = weekend_window(now_ms, &facts).expect("a resolvable window");

        let interval = weekend_calendar_interval(now_ms, &facts).expect("an interval");

        // The instants are the window's own, unmodified.
        assert_eq!(interval.start_ms, window.start_ms);
        assert_eq!(interval.end_ms, window.end_ms);
        // The civil arm starts at Friday's whole day, NOT at 17:00's day —
        // an all-day event covering Friday is a fact about the whole day,
        // and asking from 17:00 would ask about Saturday onwards.
        assert_eq!(interval.start_date, day_key(2026, 8, 14));
        // ...and ends exclusively on the day after Sunday.
        assert_eq!(interval.end_date, day_key(2026, 8, 17));
    }

    #[test]
    fn an_unresolvable_device_zone_has_no_calendar_interval() {
        assert_eq!(
            weekend_calendar_interval(at(2026, 8, 14, 9, 0), &ZoneFacts::default()),
            None
        );
    }

    // --------------------------------------------------------- merge_counts

    #[test]
    fn places_an_event_overlapping_the_window_and_drops_one_entirely_outside_it() {
        let window = window_at(at(2026, 8, 10, 9, 0)); // Fri 14 - Sun 16 Aug 2026
        let facts = resolve(&weekend_zone_queries(at(2026, 8, 10, 9, 0)));

        let inside = merge_counts(
            &window,
            &[event(at(2026, 8, 15, 9, 0), at(2026, 8, 15, 10, 0))],
            &[],
            &facts,
        );
        assert_eq!(inside.events, 1);

        let outside = merge_counts(
            &window,
            &[event(at(2026, 8, 10, 9, 0), at(2026, 8, 10, 10, 0))],
            &[],
            &facts,
        );
        assert_eq!(outside.events, 0);
    }

    #[test]
    fn an_all_day_event_spanning_two_days_counts_once_on_each() {
        let window = window_at(at(2026, 8, 10, 9, 0));
        let facts = resolve(&weekend_zone_queries(at(2026, 8, 10, 9, 0)));
        let counts = merge_counts(
            &window,
            &[all_day_event(&day_key(2026, 8, 15), &day_key(2026, 8, 17))],
            &[],
            &facts,
        );
        assert_eq!(counts.events, 2); // Saturday and Sunday, clipped to the window.
    }

    #[test]
    fn an_all_day_events_exclusive_end_date_abutting_the_window_excludes_it() {
        // Thursday only: `endDate` is Friday, so nothing in Fri-Sun matches.
        let window = window_at(at(2026, 8, 10, 9, 0));
        let facts = resolve(&weekend_zone_queries(at(2026, 8, 10, 9, 0)));
        let counts = merge_counts(
            &window,
            &[all_day_event(&day_key(2026, 8, 13), &day_key(2026, 8, 14))],
            &[],
            &facts,
        );
        assert_eq!(counts.events, 0);
    }

    #[test]
    fn a_cancelled_event_is_never_counted() {
        let window = window_at(at(2026, 8, 10, 9, 0));
        let facts = resolve(&weekend_zone_queries(at(2026, 8, 10, 9, 0)));
        let mut cancelled = event(at(2026, 8, 15, 9, 0), at(2026, 8, 15, 10, 0));
        cancelled.status = CalendarEventStatusFact::Cancelled;
        let counts = merge_counts(&window, &[cancelled], &[], &facts);
        assert_eq!(counts.events, 0);
    }

    #[test]
    fn shows_a_due_item_distinct_in_kind_from_a_scheduled_item() {
        let now_ms = at(2026, 8, 10, 9, 0);
        let window = window_at(now_ms);
        let facts = resolve(&weekend_zone_queries(now_ms));

        let due = merge_counts(&window, &[], &[item("due-1", Some(&day_key(2026, 8, 15)), None)], &facts);
        assert_eq!((due.due, due.scheduled), (1, 0));

        let scheduled =
            merge_counts(&window, &[], &[item("sched-1", None, Some(&day_key(2026, 8, 16)))], &facts);
        assert_eq!((scheduled.due, scheduled.scheduled), (0, 1));
    }

    #[test]
    fn an_item_both_due_and_scheduled_in_the_window_counts_once_as_due() {
        let now_ms = at(2026, 8, 10, 9, 0);
        let window = window_at(now_ms);
        let facts = resolve(&weekend_zone_queries(now_ms));
        let due_day = day_key(2026, 8, 15);
        let counts =
            merge_counts(&window, &[], &[item("both", Some(&due_day), Some(&due_day))], &facts);
        assert_eq!((counts.due, counts.scheduled), (1, 0));
    }

    #[test]
    fn an_item_scheduled_in_the_window_but_due_outside_it_still_counts_as_scheduled() {
        let now_ms = at(2026, 8, 10, 9, 0);
        let window = window_at(now_ms);
        let facts = resolve(&weekend_zone_queries(now_ms));
        let counts = merge_counts(
            &window,
            &[],
            &[item("out", Some("2026-08-25"), Some(&day_key(2026, 8, 15)))],
            &facts,
        );
        assert_eq!((counts.due, counts.scheduled), (0, 1));
    }

    #[test]
    fn drops_an_item_with_neither_a_due_date_nor_a_do_date_in_the_window() {
        let now_ms = at(2026, 8, 10, 9, 0);
        let window = window_at(now_ms);
        let facts = resolve(&weekend_zone_queries(now_ms));
        let counts = merge_counts(&window, &[], &[item("irrelevant", None, None)], &facts);
        assert_eq!((counts.due, counts.scheduled), (0, 0));
    }

    // #122 review fix: `window.start_ms` is Friday 17:00, but a scheduled or
    // due date anchors to the START of its day — the trap `in_window`
    // guards against.
    #[test]
    fn keeps_an_item_scheduled_or_due_on_friday_in_the_window() {
        let now_ms = at(2026, 8, 10, 9, 0);
        let window = window_at(now_ms);
        let facts = resolve(&weekend_zone_queries(now_ms));
        let friday = day_key(2026, 8, 14);

        let scheduled = merge_counts(&window, &[], &[item("fri-plan", None, Some(&friday))], &facts);
        assert_eq!(scheduled.scheduled, 1);

        let due = merge_counts(&window, &[], &[item("fri-due", Some(&friday), None)], &facts);
        assert_eq!(due.due, 1);
    }

    #[test]
    fn a_day_only_deadline_resolves_to_end_of_day_via_deadline_sort_key() {
        // A deadline of exactly Sunday, day-only, resolves to Sunday 23:59 —
        // still inside the window, whose own end is Sunday 23:59:59.999.
        let now_ms = at(2026, 8, 10, 9, 0);
        let window = window_at(now_ms);
        let facts = resolve(&weekend_zone_queries(now_ms));
        let counts =
            merge_counts(&window, &[], &[item("sun-due", Some(&day_key(2026, 8, 16)), None)], &facts);
        assert_eq!(counts.due, 1);
    }

    // ----------------------------------------------------------- band, live

    #[test]
    fn is_live_while_the_weekend_is_under_way() {
        let window = window_at(at(2026, 8, 15, 12, 0));
        assert_eq!(weekend_band(&window, at(2026, 8, 15, 12, 0)), Band::Live);
    }

    fn window_starting_at(start_ms: i64) -> WeekendWindow {
        WeekendWindow {
            start_ms,
            end_ms: start_ms + 3 * 24 * HOUR_MS - 1,
            days: [
                WeekendDay { date: "2026-08-14".into(), start_ms, end_ms: start_ms },
                WeekendDay { date: "2026-08-15".into(), start_ms, end_ms: start_ms },
                WeekendDay { date: "2026-08-16".into(), start_ms, end_ms: start_ms },
            ],
            under_way: false,
        }
    }

    #[test]
    fn is_imminent_within_48_hours_of_the_start() {
        let window = window_starting_at(at(2026, 8, 14, 17, 0));
        assert_eq!(weekend_band(&window, at(2026, 8, 13, 0, 0)), Band::Imminent); // ~41h before.
    }

    #[test]
    fn is_imminent_at_exactly_the_48_hour_boundary() {
        let window = window_starting_at(at(2026, 8, 14, 17, 0));
        assert_eq!(weekend_band(&window, window.start_ms - IMMINENT_WITHIN_MS), Band::Imminent);
    }

    #[test]
    fn is_near_just_beyond_the_48_hour_boundary() {
        let window = window_starting_at(at(2026, 8, 14, 17, 0));
        assert_eq!(weekend_band(&window, window.start_ms - IMMINENT_WITHIN_MS - 1), Band::Near);
    }

    #[test]
    fn is_near_at_exactly_the_96_hour_boundary() {
        let window = window_starting_at(at(2026, 8, 14, 17, 0));
        assert_eq!(weekend_band(&window, window.start_ms - NEAR_WITHIN_MS), Band::Near);
    }

    #[test]
    fn is_dormant_just_beyond_the_96_hour_boundary() {
        let window = window_starting_at(at(2026, 8, 14, 17, 0));
        assert_eq!(weekend_band(&window, window.start_ms - NEAR_WITHIN_MS - 1), Band::Dormant);
    }

    #[test]
    fn is_never_distant_at_any_distance() {
        let window = window_starting_at(at(2026, 8, 14, 17, 0));
        assert_ne!(weekend_band(&window, at(2020, 1, 1, 0, 0)), Band::Distant);
    }

    #[test]
    fn within_band_is_the_start_while_ahead_and_the_end_once_under_way() {
        let ahead = window_at(at(2026, 8, 10, 9, 0));
        assert_eq!(weekend_within_band(&ahead), ahead.start_ms);

        let live = window_at(at(2026, 8, 15, 12, 0));
        assert_eq!(weekend_within_band(&live), live.end_ms);
    }

    // ------------------------------------------------------- weekend_answer

    #[test]
    fn is_unbound_when_never_connected_regardless_of_the_calendar_read() {
        let mut never_connected = inputs(at(2026, 8, 10, 9, 0), vec![], vec![]);
        never_connected.calendar_connected = false;
        never_connected.calendar_reads =
            HashMap::from([(CALENDAR_REQUEST_KEY.to_string(), CalendarReadFacts::NotRead)]);
        let facts = resolve(&weekend_zone_queries(never_connected.now_ms));
        assert_eq!(weekend_answer(&never_connected, &facts).answer_state, AnswerState::Unbound);

        // Even a fully-answered read is still `unbound` while disconnected —
        // `calendar_connected` gates before the arm is ever consulted.
        let mut answered_but_disconnected = inputs(at(2026, 8, 10, 9, 0), vec![], vec![]);
        answered_but_disconnected.calendar_connected = false;
        let facts = resolve(&weekend_zone_queries(answered_but_disconnected.now_ms));
        assert_eq!(
            weekend_answer(&answered_but_disconnected, &facts).answer_state,
            AnswerState::Unbound,
        );
    }

    #[test]
    fn is_bound_but_unacquired_when_connected_but_the_read_has_not_landed() {
        let mut connected_no_entry = inputs(at(2026, 8, 10, 9, 0), vec![], vec![]);
        connected_no_entry.calendar_reads = HashMap::new();
        let facts = resolve(&weekend_zone_queries(connected_no_entry.now_ms));
        assert_eq!(
            weekend_answer(&connected_no_entry, &facts).answer_state,
            AnswerState::BoundButUnacquired,
        );

        let mut connected_not_read = inputs(at(2026, 8, 10, 9, 0), vec![], vec![]);
        connected_not_read.calendar_reads =
            HashMap::from([(CALENDAR_REQUEST_KEY.to_string(), CalendarReadFacts::NotRead)]);
        let facts = resolve(&weekend_zone_queries(connected_not_read.now_ms));
        assert_eq!(
            weekend_answer(&connected_not_read, &facts).answer_state,
            AnswerState::BoundButUnacquired,
        );
    }

    #[test]
    fn is_answered_for_a_genuinely_empty_weekend() {
        let inputs = inputs(at(2026, 8, 10, 9, 0), vec![], vec![]);
        assert_eq!(facts_of(&inputs).counts, WindowCounts::default());
        let answer = weekend_answer(&inputs, &resolve(&weekend_zone_queries(inputs.now_ms)));
        assert_eq!(answer.answer_state, AnswerState::Answered);
    }

    #[test]
    fn is_answered_with_real_content_when_items_and_events_land() {
        let now_ms = at(2026, 8, 10, 9, 0);
        let inputs = inputs(
            now_ms,
            vec![event(at(2026, 8, 15, 9, 0), at(2026, 8, 15, 10, 0))],
            vec![item("due-1", Some(&day_key(2026, 8, 15)), None)],
        );
        let facts_view = facts_of(&inputs);
        assert_eq!(facts_view.counts, WindowCounts { events: 1, due: 1, scheduled: 0 });
        let answer = weekend_answer(&inputs, &resolve(&weekend_zone_queries(now_ms)));
        assert_eq!(answer.answer_state, AnswerState::Answered);
    }

    #[test]
    fn registers_exactly_one_subject_always() {
        assert_eq!(SUBJECT_KEY, "coming-weekend");
    }
}
