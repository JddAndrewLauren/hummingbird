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
//! # The window shrinks as the weekend is spent
//!
//! [`WeekendWindow::days`] used to be `[WeekendDay; 3]` — Friday, Saturday
//! and Sunday for the whole life of the window, on the reading that the
//! question is "what are my plans", not "what is left". It is the other
//! one now: a day leaves the window at its own end (local midnight), so on
//! Saturday the pane holds Saturday and Sunday, and on Sunday, Sunday
//! alone. An unfinished item that was due or scheduled on a dropped day
//! goes with it — there is no carry-forward group, because a day that has
//! ended is not a plan.
//!
//! The cut is here, in [`weekend_window`], rather than in either renderer:
//! [`in_window`], [`merge_window`], [`count_kinds`], the collapsed
//! headline, the glyphs, the plan chips and both expanded renderings all
//! read the window's days, so one `retain` moves all of them and neither
//! client writes the rule a second time (ADR-0025). What deliberately does
//! **not** shrink is `start_ms`/`end_ms`/`under_way`, and therefore
//! [`weekend_band`]/[`weekend_within_band`]: the pane's *band* is a fact
//! about the whole weekend.
//!
//! # What did not sink: `entryUrgency`
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
//! # What DID sink at #564: the per-day entries
//!
//! At #534 this module folded the merge straight down to a
//! [`WindowCounts`] tally and left `weekend.ts`'s per-entry
//! [`WindowEntry`] list on the web, on the grounds that no *decision* read
//! an entry's `id`/`title`/`at_ms`/`anchor` — only its kind. That was the
//! right call with one client. #564 gave the expanded weekend rendering a
//! second one (Android), and ADR-0025's own tie-breaker is *sink what has
//! a demonstrated second caller*: two hand-written merges would each have
//! to get the dedupe rule right, and the one that got it wrong would look
//! completely plausible.
//!
//! So [`merge_window`] builds the entries and [`count_kinds`] tallies
//! **from them** — the counts can no longer disagree with the list they
//! describe, which they could when each was computed separately. The dedupe
//! rule (an item both due and scheduled inside the window appears once, as
//! due — a deadline is a consequence, a do-date is a preference, and the one
//! with consequences is what the day owes) now exists exactly once, and
//! `also_scheduled_on`/`deadline_outside_window` — its two residues — sink
//! with it rather than being re-derived per client.
//!
//! **What still does not cross is the DTOs themselves.** An entry carries
//! [`WindowEntry::source_id`], the handle its host uses to reach back into
//! its own event or item, not a copy of the event or the item —
//! `inputs.rs`'s "do not re-cross whole DTOs" discipline, applied on the way
//! out as well as the way in.

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

/// One of the window's days — trimmed to what a decision reads: its own
/// civil date and the local instants it spans. No `label`/`dateLabel`
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
    /// The window's days that have **not yet ended** at the device, in
    /// window order — Friday, Saturday, Sunday while the weekend is still
    /// ahead, then shrinking by one as each day's own local midnight
    /// passes: on Saturday, Saturday and Sunday; on Sunday, Sunday alone.
    /// A day that has ended is noise, and an unfinished item that was due
    /// or scheduled on it leaves with it — no carry-forward.
    ///
    /// **Never empty**: the rollover to next weekend fires at Sunday
    /// 20:00, before Sunday's own day end, so Sunday is always still
    /// ahead of any `now_ms` this window is built for.
    pub days: Vec<WeekendDay>,
    /// Whether `now_ms` falls inside `[start_ms, end_ms]` — the weekend
    /// answered is the one under way, not the next one.
    pub under_way: bool,
}

/// The merge's decision-relevant residue: how many of each kind landed in
/// the window — tallied from the entries themselves ([`count_kinds`]), so
/// the two can never disagree.
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

/// What one of the window's days holds — its own civil date and the
/// entries that landed on it, already in display order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeekendDayEntries {
    pub date: CivilDate,
    pub entries: Vec<WindowEntry>,
}

/// Which of the three things a merged entry is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EntryKind {
    Event,
    Due,
    Scheduled,
}

impl EntryKind {
    /// The wire spelling, and the tie-break order the sort uses — see
    /// [`merge_window`] for why it is alphabetical rather than semantic.
    pub fn as_str(self) -> &'static str {
        match self {
            EntryKind::Due => "due",
            EntryKind::Event => "event",
            EntryKind::Scheduled => "scheduled",
        }
    }
}

/// Whether an entry sits at an instant within its day or covers the day as
/// a whole — what a renderer needs to decide between "9:30 – 10:00" and
/// "all day".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EntryAnchor {
    Time,
    Day,
}

/// One thing on one day of the weekend, merged from the calendar arm and
/// the item list. See the module header for what deliberately is **not**
/// here: the event or the item itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowEntry {
    /// Stable within a merge, and unique across days: an event spanning two
    /// days produces two entries, and a shared id would make a keyed list
    /// collapse them into one.
    pub id: String,
    pub kind: EntryKind,
    pub title: String,
    pub at_ms: i64,
    pub anchor: EntryAnchor,
    pub day_key: CivilDate,
    /// The host's own handle on whatever this entry came from — the
    /// provider event id for an [`EntryKind::Event`], the item id for a
    /// due or scheduled one. Never the record itself (the module header).
    pub source_id: String,
    /// Set only on a `due` entry that is ALSO scheduled inside the window:
    /// the dedupe suppressed a second entry, and this is what it suppressed.
    pub also_scheduled_on: Option<String>,
    /// Set only on a `scheduled` entry whose item is due *outside* the
    /// window — the inverse residue, and the reason a weekend plan can
    /// still show a deadline it does not contain.
    pub deadline_outside_window: Option<String>,
}

/// Everything an answered pane needs to decide its band, the counts its
/// headline reads, and (since #564) the merged entries both clients'
/// expanded renderings draw.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeekendFacts {
    pub window: WeekendWindow,
    pub counts: WindowCounts,
    /// One per day still ahead, in window order — [`WeekendWindow::days`]
    /// verbatim, so this shrinks with it as the weekend is spent — even
    /// when some hold nothing. Never empty.
    pub days: Vec<WeekendDayEntries>,
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
///
/// `start_ms`/`end_ms`/`under_way` are facts about the **whole** weekend
/// and never shrink; [`WeekendWindow::days`] does, dropping each day as it
/// ends at the device (that field's own doc).
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
    // resolve the window's start, its three candidate days, and its end in
    // one pass.
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
    let mut days = vec![
        WeekendDay { date: day_dates[0].clone(), start_ms: midnights[0], end_ms: midnights[1] - 1 },
        WeekendDay { date: day_dates[1].clone(), start_ms: midnights[1], end_ms: midnights[2] - 1 },
        WeekendDay { date: day_dates[2].clone(), start_ms: midnights[2], end_ms: midnights[3] - 1 },
    ];
    // The shrink: a day leaves the window at its own end (local midnight),
    // because "what is still ahead" is what the operator reads this pane
    // for. Sunday can never be dropped — the rollover above has already
    // moved the whole window forward a week by Sunday 20:00 — so this
    // never empties `days` (the field's own doc).
    days.retain(|day| day.end_ms >= now_ms);
    let under_way = now_ms >= start_ms && now_ms <= end_ms;

    Some(WeekendWindow { start_ms, end_ms, days, under_way })
}

/// The calendar-arm interval this question needs (#267), off its own
/// resolved window — `weekend-pane/question.ts`'s `calendarRequests`,
/// sunk by #564 because Android needs the identical window and a second
/// copy of these bounds is exactly the drift ADR-0025 forbids.
///
/// The lower civil bound is the **first still-ahead day's own day key**,
/// not `start_ms`'s: an all-day event covering that day is a fact about the
/// whole day, and asking from 17:00 would ask about the next day onwards —
/// the same reason [`in_window`] uses the first day's `start_ms` below.
/// The upper civil bound is exclusive, so it is the day after Sunday;
/// `end_ms` is Sunday 23:59:59.999 and one millisecond later is Monday
/// local midnight.
///
/// The timed lower bound is raised to that same first day's start once the
/// window has begun to shrink, so the two arms cannot disagree about which
/// day the interval opens on: before the weekend the first day is Friday
/// and its midnight is below Friday 17:00, so this is `start_ms` exactly as
/// it always was; on Sunday it is Sunday's own midnight rather than a
/// Friday 17:00 the day columns no longer cover.
///
/// `None` when [`DEVICE_ZONE`] could not be resolved — never a window
/// built off a UTC pivot (the module header). The `?`s on `first`/`last`
/// are unreachable by [`WeekendWindow::days`]' never-empty invariant, and
/// are guards rather than a claim it can happen.
pub fn weekend_calendar_interval(now_ms: i64, facts: &ZoneFacts) -> Option<CalendarInterval> {
    let window = weekend_window(now_ms, facts)?;
    let first = window.days.first()?;
    let last = window.days.last()?;
    Some(CalendarInterval {
        start_ms: window.start_ms.max(first.start_ms),
        end_ms: window.end_ms,
        start_date: first.date.clone(),
        end_date: add_civil_days(&last.date, 1)?,
    })
}

/// Day-membership test for an item's due/do-date — deliberately NOT
/// `[window.start_ms, window.end_ms]`. `window.start_ms` is Friday
/// **17:00** (the band's own "has the weekend started" instant), but a
/// scheduled or due date anchors to the *start* of its day, so the lower
/// bound here is the first still-ahead day's own local midnight — #122's
/// own review fix, ported verbatim from `weekend.ts`'s `inWindow`.
///
/// That lower bound is also **what drops an item with its day**: once
/// Friday has ended, Friday midnight is no longer the bound, so an
/// unfinished Friday-due item is outside the window and leaves the pane
/// rather than carrying forward ([`WeekendWindow::days`]' own ruling).
/// `false` on an empty `days`, which the never-empty invariant rules out.
fn in_window(ms: i64, window: &WeekendWindow) -> bool {
    let Some(lower_ms) = window.days.first().map(|day| day.start_ms) else {
        return false;
    };
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

/// The stages the merge counts an item from — the *frontier* stages, plus
/// `blocked`.
///
/// Explicit since #675, and behaviour-preserving on the day it was written:
/// `QuestionInputs.items` was `frontier ∪ blocked` on both clients, so
/// every row already had one of these stages. #675 widens that union to
/// every live item (the homework pane's own subject is the operator's
/// items, including the ones still in Triage), and without this filter a
/// captured-but-untriaged item with a deadline would have started
/// appearing on a weekend day — a silent change to a shipped pane, made by
/// a different pane's inputs. The filter is where the weekend pane's own
/// reading of "a plan" is stated rather than inherited from whoever
/// assembles the list.
const MERGED_STAGES: [&str; 3] = ["ready", "in_progress", "blocked"];

/// The merge: events overlapping the window plus due/scheduled items
/// inside it, grouped by day and in display order within a day.
///
/// **The dedupe rule lives here and nowhere else** — an item both due and
/// scheduled inside the window appears once, as due (a deadline is a
/// consequence, a do-date is a preference, and the one with consequences is
/// what the day owes), and the suppressed do-date rides the surviving entry
/// as [`WindowEntry::also_scheduled_on`]. The inverse — scheduled inside,
/// due outside — keeps its deadline as
/// [`WindowEntry::deadline_outside_window`].
///
/// A multi-day event belongs to **each** day it touches: it is a fact about
/// each of them, and showing it once would leave the other day reading as
/// free. That is why [`WindowEntry::id`] carries the day.
///
/// The within-day order is `at_ms`, then the kind's own wire spelling, then
/// the id — alphabetical on the kind rather than semantic, because it is a
/// tie-break for entries at the identical instant and any total order does,
/// as long as it is the *same* total order on every client. `weekend.ts`'s
/// `localeCompare` is what it was, so that is what it stays.
fn merge_window(
    window: &WeekendWindow,
    events: &[CalendarEventFacts],
    items: &[PaneItemFacts],
    facts: &ZoneFacts,
) -> Vec<WeekendDayEntries> {
    let mut days: Vec<WeekendDayEntries> = window
        .days
        .iter()
        .map(|day| WeekendDayEntries { date: day.date.clone(), entries: Vec::new() })
        .collect();

    for event in events {
        if event.status == CalendarEventStatusFact::Cancelled {
            continue; // defence in depth — the sync layer already filters these.
        }
        match &event.when {
            // The two arms are asked in their own terms and never converted
            // into one another (ADR-0015's 2026-08-10 amendment). An
            // all-day event's day membership is a pure string compare
            // against the day's own civil date — no zone, no instant, and
            // so no way for a device east or west of the calendar to land
            // it on the wrong day.
            CalendarEventWhenFacts::AllDay { start_date, end_date } => {
                for (index, day) in window.days.iter().enumerate() {
                    if start_date.as_str() <= day.date.as_str()
                        && day.date.as_str() < end_date.as_str()
                    {
                        days[index].entries.push(WindowEntry {
                            id: format!("{}@{}", event.provider_event_id, day.date),
                            kind: EntryKind::Event,
                            title: event.title.clone(),
                            at_ms: day.start_ms,
                            anchor: EntryAnchor::Day,
                            day_key: day.date.clone(),
                            source_id: event.provider_event_id.clone(),
                            also_scheduled_on: None,
                            deadline_outside_window: None,
                        });
                    }
                }
            }
            CalendarEventWhenFacts::Timed { start_ms, end_ms } => {
                let overlap_start = (*start_ms).max(window.start_ms);
                let overlap_end = (*end_ms).min(window.end_ms);
                if overlap_start > overlap_end {
                    continue;
                }
                for (index, day) in window.days.iter().enumerate() {
                    let day_overlap_start = overlap_start.max(day.start_ms);
                    let day_overlap_end = overlap_end.min(day.end_ms);
                    if day_overlap_start <= day_overlap_end {
                        days[index].entries.push(WindowEntry {
                            id: format!("{}@{}", event.provider_event_id, day.date),
                            kind: EntryKind::Event,
                            title: event.title.clone(),
                            at_ms: day_overlap_start,
                            anchor: EntryAnchor::Time,
                            day_key: day.date.clone(),
                            source_id: event.provider_event_id.clone(),
                            also_scheduled_on: None,
                            deadline_outside_window: None,
                        });
                    }
                }
            }
        }
    }

    for item in items {
        if !MERGED_STAGES.contains(&item.stage.as_str()) {
            continue;
        }
        let due_ms = item.deadline.as_deref().and_then(|d| deadline_to_ms(d, facts));
        let scheduled_ms = item.scheduled_date.as_deref().and_then(|s| scheduled_to_ms(s, facts));
        let due_here = due_ms.is_some_and(|ms| in_window(ms, window));
        let scheduled_here = scheduled_ms.is_some_and(|ms| in_window(ms, window));

        if let (true, Some(due_ms)) = (due_here, due_ms) {
            if let Some(index) = day_index_of(window, due_ms) {
                let day = &window.days[index];
                days[index].entries.push(WindowEntry {
                    id: item.id.clone(),
                    kind: EntryKind::Due,
                    title: item.title.clone(),
                    at_ms: due_ms,
                    // A day-only deadline (`YYYY-MM-DD`, ten characters)
                    // covers the day; anything longer names a time.
                    anchor: match item.deadline.as_deref() {
                        Some(deadline) if deadline.len() == 10 => EntryAnchor::Day,
                        _ => EntryAnchor::Time,
                    },
                    day_key: day.date.clone(),
                    source_id: item.id.clone(),
                    also_scheduled_on: scheduled_here
                        .then(|| item.scheduled_date.clone())
                        .flatten(),
                    deadline_outside_window: None,
                });
            }
            continue; // the dedupe: never also emitted as scheduled.
        }

        if let (true, Some(scheduled_ms)) = (scheduled_here, scheduled_ms) {
            if let Some(index) = day_index_of(window, scheduled_ms) {
                let day = &window.days[index];
                days[index].entries.push(WindowEntry {
                    id: item.id.clone(),
                    kind: EntryKind::Scheduled,
                    title: item.title.clone(),
                    at_ms: scheduled_ms,
                    anchor: EntryAnchor::Day,
                    day_key: day.date.clone(),
                    source_id: item.id.clone(),
                    also_scheduled_on: None,
                    deadline_outside_window: item.deadline.clone(),
                });
            }
        }
    }

    for day in &mut days {
        day.entries.sort_by(|a, b| {
            a.at_ms
                .cmp(&b.at_ms)
                .then_with(|| a.kind.as_str().cmp(b.kind.as_str()))
                .then_with(|| a.id.cmp(&b.id))
        });
    }
    days
}

/// Which of the window's days `ms` falls on, or `None` if it falls on
/// none of them. `weekend.ts` reaches its day through a `dayKeyOf(ms)` map
/// lookup; here the day bounds are already resolved on the window, so the
/// instant is compared against them directly rather than being re-derived
/// into a civil date — one fewer place a zone could be assumed.
fn day_index_of(window: &WeekendWindow, ms: i64) -> Option<usize> {
    window
        .days
        .iter()
        .position(|day| ms >= day.start_ms && ms <= day.end_ms)
}

/// The tally, taken **from the merged entries** rather than computed
/// alongside them — `weekend.ts`'s `countKinds`, and the reason the counts
/// and the list can no longer disagree.
fn count_kinds(days: &[WeekendDayEntries]) -> WindowCounts {
    let mut counts = WindowCounts::default();
    for day in days {
        for entry in &day.entries {
            match entry.kind {
                EntryKind::Event => counts.events += 1,
                EntryKind::Due => counts.due += 1,
                EntryKind::Scheduled => counts.scheduled += 1,
            }
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
    let days = merge_window(&window, events, &inputs.items, facts);
    let counts = count_kinds(&days);
    WeekendResolved::Facts(WeekendFacts { window, counts, days })
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
            description: None,
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
            description: None,
        }
    }

    fn item(id: &str, deadline: Option<&str>, scheduled_date: Option<&str>) -> PaneItemFacts {
        staged_item(id, deadline, scheduled_date, "ready")
    }

    fn staged_item(
        id: &str,
        deadline: Option<&str>,
        scheduled_date: Option<&str>,
        stage: &str,
    ) -> PaneItemFacts {
        PaneItemFacts {
            id: id.to_string(),
            title: id.to_string(),
            deadline: deadline.map(str::to_string),
            scheduled_date: scheduled_date.map(str::to_string),
            stage: stage.to_string(),
            context: None,
            description: None,
            created_at: 0,
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

    fn day_dates(window: &WeekendWindow) -> Vec<String> {
        window.days.iter().map(|day| day.date.clone()).collect()
    }

    #[test]
    fn carries_all_three_days_while_the_whole_weekend_is_still_ahead() {
        let window = window_at(at(2026, 8, 10, 9, 0));
        assert_eq!(
            day_dates(&window),
            vec![day_key(2026, 8, 14), day_key(2026, 8, 15), day_key(2026, 8, 16)],
        );
    }

    #[test]
    fn still_carries_all_three_days_on_friday_evening_friday_has_not_ended() {
        // The window is under way and Friday is being spent — but "spent"
        // is not "ended", and the shrink is keyed to the day's own
        // midnight, not to the window's 17:00 start.
        let window = window_at(at(2026, 8, 14, 20, 0));
        assert!(window.under_way);
        assert_eq!(
            day_dates(&window),
            vec![day_key(2026, 8, 14), day_key(2026, 8, 15), day_key(2026, 8, 16)],
        );
    }

    #[test]
    fn drops_friday_once_friday_has_ended() {
        let window = window_at(at(2026, 8, 15, 9, 0));
        assert_eq!(day_dates(&window), vec![day_key(2026, 8, 15), day_key(2026, 8, 16)]);
    }

    #[test]
    fn keeps_friday_through_its_own_last_millisecond() {
        // The boundary is `end_ms >= now_ms`: Friday 23:59:59.999 still
        // holds Friday, and one millisecond later (Saturday midnight) does
        // not.
        let last_ms = at(2026, 8, 15, 0, 0) - 1;
        assert_eq!(day_dates(&window_at(last_ms)).len(), 3);
        assert_eq!(day_dates(&window_at(last_ms + 1)).len(), 2);
    }

    #[test]
    fn holds_sunday_alone_on_sunday() {
        let window = window_at(at(2026, 8, 16, 9, 0));
        assert_eq!(day_dates(&window), vec![day_key(2026, 8, 16)]);
    }

    #[test]
    fn is_never_empty_at_sunday_19_59_the_last_instant_before_the_rollover() {
        // The invariant that replaced "always exactly three": the rollover
        // fires at Sunday 20:00, before Sunday's own end, so the last
        // instant this weekend is answered for still has a day in it.
        let window = window_at(at(2026, 8, 16, 19, 59));
        assert_eq!(day_dates(&window), vec![day_key(2026, 8, 16)]);
    }

    #[test]
    fn is_three_days_again_once_the_rollover_moves_to_next_weekend() {
        let window = window_at(at(2026, 8, 16, 20, 1));
        assert_eq!(
            day_dates(&window),
            vec![day_key(2026, 8, 21), day_key(2026, 8, 22), day_key(2026, 8, 23)],
        );
    }

    #[test]
    fn the_window_start_and_end_never_shrink_with_the_days() {
        // Deliberately unchanged: the band is a fact about the whole
        // weekend, so `start_ms`/`end_ms`/`under_way` stay put while the
        // day columns go.
        let sunday = window_at(at(2026, 8, 16, 9, 0));
        assert_eq!(sunday.start_ms, at(2026, 8, 14, 17, 0));
        assert_eq!(sunday.end_ms, at(2026, 8, 17, 0, 0) - 1);
        assert!(sunday.under_way);
        assert_eq!(weekend_band(&sunday, at(2026, 8, 16, 9, 0)), Band::Live);
        assert_eq!(weekend_within_band(&sunday), sunday.end_ms);
    }

    /// Midnight at a device zone that springs forward one hour at
    /// Saturday 2026-08-15 02:00 local — UTC-8 before, UTC-7 from then.
    /// Not a real August transition, but the shape of one; August is what
    /// every other fixture here uses.
    fn midnight_dst(date: &str) -> i64 {
        let days = civil_days_between("1970-01-01", date).unwrap();
        let offset = if date < "2026-08-16" { 8 * HOUR_MS } else { 7 * HOUR_MS };
        days * 24 * HOUR_MS + offset
    }

    fn resolve_dst(queries: &[ZoneQuery]) -> ZoneFacts {
        let mut facts = ZoneFacts::default();
        for query in queries {
            match query {
                ZoneQuery::CivilDate { zone, at_ms } if zone == DEVICE_ZONE => {
                    // The offset depends on the date the offset decides,
                    // so guess with the pre-transition one and re-ask once
                    // if the guess lands on the far side.
                    let mut date =
                        add_civil_days("1970-01-01", (at_ms - 8 * HOUR_MS).div_euclid(24 * HOUR_MS))
                            .unwrap();
                    if date.as_str() >= "2026-08-16" {
                        date = add_civil_days(
                            "1970-01-01",
                            (at_ms - 7 * HOUR_MS).div_euclid(24 * HOUR_MS),
                        )
                        .unwrap();
                    }
                    facts.insert(query, ZoneFact::Date(date));
                }
                ZoneQuery::Midnight { zone, date } if zone == DEVICE_ZONE => {
                    facts.insert(query, ZoneFact::Instant(midnight_dst(date)));
                }
                _ => {}
            }
        }
        facts
    }

    #[test]
    fn a_day_ends_at_its_own_midnight_across_a_dst_transition() {
        // Saturday is 23 hours long here. The day end is
        // `next_midnight - 1` off the host's own `midnight_ms`, never
        // Saturday's midnight plus 24 hours — so the shrink fires an hour
        // earlier (in elapsed terms) than 24-hour arithmetic would.
        let true_saturday_end = midnight_dst("2026-08-16") - 1;
        let naive_saturday_end = midnight_dst("2026-08-15") + 24 * HOUR_MS - 1;
        assert_eq!(naive_saturday_end - true_saturday_end, HOUR_MS);

        let dates = |now_ms: i64| -> Vec<String> {
            weekend_window(now_ms, &resolve_dst(&weekend_zone_queries(now_ms)))
                .expect("window resolves")
                .days
                .iter()
                .map(|day| day.date.clone())
                .collect()
        };

        assert_eq!(dates(true_saturday_end), vec!["2026-08-15", "2026-08-16"]);
        assert_eq!(dates(true_saturday_end + 1), vec!["2026-08-16"]);
        // The instant 24-hour arithmetic would have called "still
        // Saturday" is already Sunday.
        assert_eq!(dates(naive_saturday_end), vec!["2026-08-16"]);
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
    fn the_calendar_interval_shrinks_with_the_window_and_its_two_arms_agree() {
        // Asked on Sunday morning: the civil arm opens on Sunday, and the
        // timed arm is raised off Friday 17:00 to Sunday's own midnight so
        // the two do not name different days. The upper bounds are the
        // whole weekend's, unchanged.
        let now_ms = at(2026, 8, 16, 9, 0);
        let facts = resolve(&weekend_zone_queries(now_ms));

        let interval = weekend_calendar_interval(now_ms, &facts).expect("an interval");

        assert_eq!(interval.start_date, day_key(2026, 8, 16));
        assert_eq!(interval.end_date, day_key(2026, 8, 17));
        assert_eq!(interval.start_ms, at(2026, 8, 16, 0, 0));
        assert_eq!(interval.end_ms, at(2026, 8, 17, 0, 0) - 1);
    }

    #[test]
    fn an_unresolvable_device_zone_has_no_calendar_interval() {
        assert_eq!(
            weekend_calendar_interval(at(2026, 8, 14, 9, 0), &ZoneFacts::default()),
            None
        );
    }

    /// The old `merge_counts` — now `count_kinds(merge_window(..))`, kept
    /// as a test-local shim so #534's whole counting suite below runs
    /// **unchanged** against the sunk entries. That is the point: the
    /// counts are derived from the list now, and every case those tests
    /// pinned must still tally the same.
    fn merge_counts(
        window: &WeekendWindow,
        events: &[CalendarEventFacts],
        items: &[PaneItemFacts],
        facts: &ZoneFacts,
    ) -> WindowCounts {
        count_kinds(&merge_window(window, events, items, facts))
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
    fn an_item_still_in_triage_never_reaches_a_weekend_day() {
        // The #675 pin. `QuestionInputs.items` widened from
        // `frontier ∪ blocked` to every live item so the homework pane
        // could see its own subject; this pane's answer must not move
        // because of it. A triage-stage item due squarely inside the
        // window counts zero, and the same item at `ready` counts one —
        // so the assertion is about the *filter*, not about the fixture
        // happening to fall outside the window.
        let now_ms = at(2026, 8, 10, 9, 0);
        let window = window_at(now_ms);
        let facts = resolve(&weekend_zone_queries(now_ms));
        let saturday = day_key(2026, 8, 15);

        for stage in ["triage", "grilling", "done"] {
            let counts = merge_counts(
                &window,
                &[],
                &[staged_item("captured", Some(&saturday), Some(&saturday), stage)],
                &facts,
            );
            assert_eq!((counts.due, counts.scheduled), (0, 0), "{stage}");
        }

        for stage in MERGED_STAGES {
            let counts = merge_counts(
                &window,
                &[],
                &[staged_item("planned", Some(&saturday), None, stage)],
                &facts,
            );
            assert_eq!((counts.due, counts.scheduled), (1, 0), "{stage}");
        }
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

    // --------------------------------------------------------- merge_window

    // The entries themselves, which #534 left on the web and #564 sank.

    /// [`all_day_event`] with an id of its own — the fixture above fixes
    /// one, and these tests are about ids.
    fn named_all_day(id: &str, start_date: &str, end_date: &str) -> CalendarEventFacts {
        CalendarEventFacts {
            provider_event_id: id.to_string(),
            title: id.to_string(),
            ..all_day_event(start_date, end_date)
        }
    }

    fn entries_on(days: &[WeekendDayEntries], date: &str) -> Vec<WindowEntry> {
        days.iter()
            .find(|day| day.date == date)
            .map(|day| day.entries.clone())
            .unwrap_or_default()
    }

    #[test]
    fn a_multi_day_all_day_event_lands_on_each_day_with_its_own_id() {
        // A fact about each day: showing it once would leave the other
        // reading as free. Distinct ids, so a keyed list cannot collapse
        // them.
        let now_ms = at(2026, 8, 14, 9, 0);
        let facts = resolve(&weekend_zone_queries(now_ms));
        let window = weekend_window(now_ms, &facts).unwrap();
        let trip = named_all_day("trip", &day_key(2026, 8, 15), &day_key(2026, 8, 17));

        let days = merge_window(&window, &[trip], &[], &facts);

        assert_eq!(entries_on(&days, &day_key(2026, 8, 14)), Vec::new());
        let saturday = entries_on(&days, &day_key(2026, 8, 15));
        let sunday = entries_on(&days, &day_key(2026, 8, 16));
        assert_eq!(saturday.len(), 1);
        assert_eq!(sunday.len(), 1);
        assert_eq!(saturday[0].id, format!("trip@{}", day_key(2026, 8, 15)));
        assert_eq!(sunday[0].id, format!("trip@{}", day_key(2026, 8, 16)));
        // Both reach the same event through the host's own handle.
        assert_eq!(saturday[0].source_id, "trip");
        assert_eq!(sunday[0].source_id, "trip");
        assert_eq!(saturday[0].anchor, EntryAnchor::Day);
    }

    #[test]
    fn an_item_both_due_and_scheduled_in_the_window_appears_once_as_due() {
        // #122's own acceptance criterion, now with the residue the merge
        // suppressed riding the entry that survived.
        let now_ms = at(2026, 8, 14, 9, 0);
        let facts = resolve(&weekend_zone_queries(now_ms));
        let window = weekend_window(now_ms, &facts).unwrap();
        let both = item(
            "both",
            Some(&day_key(2026, 8, 15)),
            Some(&day_key(2026, 8, 16)),
        );

        let days = merge_window(&window, &[], &[both], &facts);

        let all: Vec<&WindowEntry> = days.iter().flat_map(|day| day.entries.iter()).collect();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].kind, EntryKind::Due);
        assert_eq!(all[0].also_scheduled_on.as_deref(), Some(day_key(2026, 8, 16).as_str()));
        assert_eq!(all[0].deadline_outside_window, None);
    }

    #[test]
    fn an_item_scheduled_inside_but_due_outside_keeps_its_deadline() {
        // The inverse residue: the weekend does not contain the deadline,
        // and saying nothing about it would make a do-date look free.
        let now_ms = at(2026, 8, 14, 9, 0);
        let facts = resolve(&weekend_zone_queries(now_ms));
        let window = weekend_window(now_ms, &facts).unwrap();
        let later = item(
            "later",
            Some(&day_key(2026, 8, 25)),
            Some(&day_key(2026, 8, 15)),
        );

        let days = merge_window(&window, &[], &[later], &facts);

        let saturday = entries_on(&days, &day_key(2026, 8, 15));
        assert_eq!(saturday.len(), 1);
        assert_eq!(saturday[0].kind, EntryKind::Scheduled);
        assert_eq!(saturday[0].anchor, EntryAnchor::Day);
        assert_eq!(
            saturday[0].deadline_outside_window.as_deref(),
            Some(day_key(2026, 8, 25).as_str())
        );
    }

    #[test]
    fn entries_within_a_day_are_ordered_by_instant_then_kind_then_id() {
        let now_ms = at(2026, 8, 14, 9, 0);
        let facts = resolve(&weekend_zone_queries(now_ms));
        let window = weekend_window(now_ms, &facts).unwrap();
        // Looked up by date, not by index: the window's days are only the
        // ones still ahead, so an index is a claim about how much of the
        // weekend is spent.
        let saturday_midnight = window
            .days
            .iter()
            .find(|day| day.date == day_key(2026, 8, 15))
            .expect("Saturday is still ahead on Friday morning")
            .start_ms;
        // An all-day event and a scheduled item both anchor to Saturday's
        // own start, so only the kind tie-break separates them; a day-only
        // deadline resolves to the END of its day (`deadline_sort_key`),
        // so it sorts last on instant alone.
        let events = [named_all_day("zeta", &day_key(2026, 8, 15), &day_key(2026, 8, 16))];
        let items = [
            item("alpha", None, Some(&day_key(2026, 8, 15))),
            item("beta", Some(&day_key(2026, 8, 15)), None),
        ];

        let days = merge_window(&window, &events, &items, &facts);

        let saturday = entries_on(&days, &day_key(2026, 8, 15));
        assert_eq!(saturday[0].at_ms, saturday_midnight);
        assert_eq!(saturday[1].at_ms, saturday_midnight);
        // "event" < "scheduled" alphabetically — a tie-break, not a
        // semantic ranking, and the point is only that every client uses
        // the same one.
        let kinds: Vec<&str> = saturday.iter().map(|entry| entry.kind.as_str()).collect();
        assert_eq!(kinds, vec!["event", "scheduled", "due"]);
    }

    #[test]
    fn the_counts_are_the_entries_own_tally() {
        // The reason the merge sank at all: two separately-computed answers
        // about the same merge could disagree, and this one cannot.
        let now_ms = at(2026, 8, 14, 9, 0);
        let facts = resolve(&weekend_zone_queries(now_ms));
        let window = weekend_window(now_ms, &facts).unwrap();
        let events = [named_all_day("trip", &day_key(2026, 8, 15), &day_key(2026, 8, 17))];
        let items = [
            item("due-1", Some(&day_key(2026, 8, 15)), None),
            item("sched-1", None, Some(&day_key(2026, 8, 16))),
        ];

        let days = merge_window(&window, &events, &items, &facts);
        let counts = count_kinds(&days);

        let total: usize = days.iter().map(|day| day.entries.len()).sum();
        assert_eq!(
            total as i64,
            counts.events + counts.due + counts.scheduled
        );
        assert_eq!(counts, WindowCounts { events: 2, due: 1, scheduled: 1 });
    }

    #[test]
    fn an_answered_pane_carries_the_days_still_ahead_even_when_they_are_empty() {
        let now_ms = at(2026, 8, 14, 9, 0);
        let facts = resolve(&weekend_zone_queries(now_ms));
        let window = weekend_window(now_ms, &facts).unwrap();

        let days = merge_window(&window, &[], &[], &facts);

        assert_eq!(days.len(), 3);
        assert!(days.iter().all(|day| day.entries.is_empty()));
        let dates: Vec<&str> = days.iter().map(|day| day.date.as_str()).collect();
        assert_eq!(
            dates,
            vec![
                day_key(2026, 8, 14).as_str(),
                day_key(2026, 8, 15).as_str(),
                day_key(2026, 8, 16).as_str()
            ]
        );
    }

    #[test]
    fn an_answered_pane_on_sunday_carries_sunday_alone() {
        let now_ms = at(2026, 8, 16, 9, 0);
        let facts = resolve(&weekend_zone_queries(now_ms));
        let window = weekend_window(now_ms, &facts).unwrap();

        let days = merge_window(&window, &[], &[], &facts);

        let dates: Vec<&str> = days.iter().map(|day| day.date.as_str()).collect();
        assert_eq!(dates, vec![day_key(2026, 8, 16).as_str()]);
    }

    #[test]
    fn an_unfinished_item_due_on_a_dropped_day_leaves_with_its_day() {
        // Ruling 2 of the shrink: no carry-forward, no "left over" group.
        // The same item, same stage, asked twice — present on Friday,
        // absent on Saturday.
        let friday_due = item("friday-due", Some(&day_key(2026, 8, 14)), None);

        let on_friday = facts_of(&inputs(at(2026, 8, 14, 9, 0), vec![], vec![friday_due.clone()]));
        assert_eq!(on_friday.counts.due, 1);

        let on_saturday = facts_of(&inputs(at(2026, 8, 15, 9, 0), vec![], vec![friday_due]));
        assert_eq!(on_saturday.counts.due, 0);
        assert!(on_saturday.days.iter().all(|day| day.entries.is_empty()));
    }

    #[test]
    fn an_item_scheduled_on_a_dropped_day_leaves_with_it_too() {
        let friday_plan = item("friday-plan", None, Some(&day_key(2026, 8, 14)));

        let on_friday = facts_of(&inputs(at(2026, 8, 14, 9, 0), vec![], vec![friday_plan.clone()]));
        assert_eq!(on_friday.counts.scheduled, 1);

        let on_sunday = facts_of(&inputs(at(2026, 8, 16, 9, 0), vec![], vec![friday_plan]));
        assert_eq!(on_sunday.counts.scheduled, 0);
    }

    #[test]
    fn a_saturday_event_survives_saturday_and_goes_on_sunday() {
        let saturday_lunch = event(at(2026, 8, 15, 12, 0), at(2026, 8, 15, 13, 0));

        let on_saturday_evening =
            facts_of(&inputs(at(2026, 8, 15, 20, 0), vec![saturday_lunch.clone()], vec![]));
        assert_eq!(on_saturday_evening.counts.events, 1);

        let on_sunday = facts_of(&inputs(at(2026, 8, 16, 9, 0), vec![saturday_lunch], vec![]));
        assert_eq!(on_sunday.counts.events, 0);
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
            days: vec![
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
