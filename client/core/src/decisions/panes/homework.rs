//! **The homework question** (#675) — the first standing question whose
//! subject is the operator's *own items* rather than an outside source.
//!
//! # Why this one is different
//!
//! Every other pane binds to something the operator does not own: a
//! municipal feed, a calendar, a race schedule, a CI run, the device's own
//! reachability. `weekend.rs` reads items, but only to merge them onto days
//! a *calendar* defined — the calendar is still the subject. Here the items
//! **are** the subject: the pane asks "what is the next piece of homework,
//! and what did I write down about it", and its whole answer is derived
//! from `PaneInputs::items`. ADR-0015 is amended in place to record that a
//! standing question may key on the operator's own items, and that this one
//! does so on a hardcoded context literal.
//!
//! # What "homework" is, and the objection on the record
//!
//! It is a [`HOMEWORK_CONTEXT`] — `@homework` — and nothing else. Not a
//! project, not a tag, not a source: a Context, hardcoded here and
//! suggested by both capture forms
//! ([`crate::decisions::vocabulary::CONTEXTS`], whose last entry this
//! constant must equal — that module's own test pins the pair, so the pane
//! and the form can never disagree about the spelling).
//!
//! CONTEXT.md defines a Context as *where or with what* an item can be done
//! — the tool, place or company it requires — and `@waiting` was deleted
//! from the suggested list for failing exactly that test. `@homework` fails
//! it the same way: it is a topic. **That objection was raised during
//! grilling and overruled by the operator**, and CONTEXT.md's Context entry
//! carries the amendment that admits the widening rather than leaving it
//! silent. It is recorded here too because this module is what makes the
//! decision load-bearing.
//!
//! # Which items count, and which one wins
//!
//! **Open** is everything not `done` — triage, grilling, ready,
//! in_progress, blocked alike. A piece of homework that has not been
//! triaged yet is still homework, and the whole point of the pane is that
//! the notes are reachable without going to find the item.
//!
//! *Archived* items are not filtered here and carry no field on
//! [`PaneItemFacts`]: both hosts assemble `items` from queries that already
//! exclude an archived row ([`crate::Core::frontier`] and its siblings), so
//! a ninth field would be one nothing reads. If that ever stops being true
//! the fix is a field plus a filter, not a silent reinterpretation of
//! `stage`.
//!
//! **Soonest deadline wins.** An item with no deadline only wins when no
//! open homework item has one; among deadline-less items, newest
//! `created_at`, which is the closest thing to "the one I am currently
//! thinking about" that this crate can see.
//!
//! # Zone handling
//!
//! A deadline is a civil date and "days until" is civil-date arithmetic, so
//! this pane goes through [`super::zone`]'s bridge exactly as
//! `weekend.rs` does: [`ZoneQuery::CivilDate`] for the device's own today,
//! [`ZoneQuery::Midnight`] for each candidate deadline's day, then
//! [`civil_days_between`]. **Never `Date`/`i64` millisecond subtraction** —
//! that is the drift the bridge exists to stop, and it puts "due today" on
//! the wrong side of midnight for half the year.
//!
//! Unlike `weekend.rs` this needs no bounded over-ask: the dates it cares
//! about are carried on the items themselves, so
//! [`homework_zone_queries`] can name them exactly.
//!
//! # What does not cross
//!
//! The headline. `Homework due in 3 days` is a sentence, and ADR-0025 is
//! flat that gaps cross as kinds and facts as structured data, never as
//! sentences. [`HomeworkFacts::days_away`] is the number both clients set
//! their own words from — one civil-date subtraction, decided once, rather
//! than a second one per client that cannot even be done without this
//! bridge.

use serde::{Deserialize, Serialize};

use super::contract::{AnswerState, Band, PaneAnswerCore};
use super::inputs::{PaneInputs, PaneItemFacts};
use super::zone::{civil_days_between, CivilDate, ZoneFacts, ZoneQuery, DEVICE_ZONE};

use hummingbird_domain::deadline_sort_key;

const HOUR_MS: i64 = 60 * 60 * 1000;
const MINUTE_MS: i64 = 60 * 1000;

/// The one subject this question ever has. A sentinel rather than a
/// per-item subject: the question is "what is my homework", singular, and
/// one pane per open item would be a list wearing a pane's clothes.
pub const SUBJECT_KEY: &str = "homework";

/// The single literal, matched here and suggested by the capture forms.
///
/// Spelled once on purpose:
/// [`crate::decisions::vocabulary::CONTEXTS`] references *this* constant in
/// its own test rather than repeating the string, so a pane that quietly
/// stopped finding the items the form told the operator to file is a test
/// failure rather than a silent one.
pub const HOMEWORK_CONTEXT: &str = "@homework";

/// Beyond `today` but within this many whole civil days reads as `near`;
/// beyond it, `distant`. Beside the band function, ADR-0015's own
/// discipline.
pub const NEAR_WITHIN_DAYS: i64 = 3;

/// Why this pane has no answer — a kind, not a sentence.
///
/// One arm, and it is the bridge's own: there is no binding to be unset
/// (the context is hardcoded) and no payload to be malformed (the items
/// are already in the mirror), so "no open homework" is an *answer*
/// ([`Band::Dormant`]), the way an off-season race weekend is on
/// `race.rs`, and not a gap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "gap", rename_all = "camelCase")]
pub enum HomeworkGap {
    /// [`DEVICE_ZONE`] could not be resolved by this host — the absent fact
    /// of `zone.rs`'s bridge, decided here exactly as
    /// [`super::weekend::WeekendGap::UnresolvableZone`] is.
    UnresolvableZone,
}

/// One open homework item, as a client's expanded body renders it.
///
/// `description` is carried on the winner and left `None` on the others by
/// [`homework_facts`]: the expanded body shows the winning item's notes and
/// lists the rest by title, so crossing every other item's whole
/// description would be data no rendering reads — `inputs.rs`'s "do not
/// re-cross whole DTOs" discipline applied on the way back out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HomeworkItem {
    pub id: String,
    pub title: String,
    pub deadline: Option<String>,
    pub description: Option<String>,
}

/// Everything an answered pane needs: which item won, how far away it is,
/// and what else is open.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HomeworkFacts {
    /// The soonest-deadline open item, or `None` when nothing is open at
    /// all — the [`Band::Dormant`] answer, not a gap.
    pub winner: Option<HomeworkItem>,
    /// Every other open homework item, in the same order
    /// [`open_homework`] decided.
    pub others: Vec<HomeworkItem>,
    /// Whole civil days from the device's today to the winner's deadline —
    /// negative when overdue, `0` today. `None` when there is no winner, or
    /// when the winner carries no deadline.
    pub days_away: Option<i64>,
}

/// The whole answered fact set, or the reason there is none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum HomeworkResolved {
    Facts(HomeworkFacts),
    Gap { gap: HomeworkGap },
}

/// Whether `context` names this pane's subject — exact after a trim,
/// case-insensitive.
///
/// Deliberately not a prefix or substring match: `@homework-ish` is a
/// different context, and a pane that swallowed it would be filtering by
/// something the operator cannot see in the list they typed it into.
fn is_homework(context: Option<&str>) -> bool {
    context.is_some_and(|value| value.trim().eq_ignore_ascii_case(HOMEWORK_CONTEXT))
}

/// The winner-first ordering key: dated items before undated, then
/// soonest deadline, then newest `created_at`, then id.
///
/// Sorted through [`deadline_sort_key`] rather than the raw string, so a
/// day-only deadline sorts as the *end* of its day against a same-day
/// timed one — the reading the rest of the app already gives
/// (`frontier-order.ts`, `weekend.rs`'s own `deadline_to_ms`).
fn order_key(item: &PaneItemFacts) -> (bool, String, i64, &str) {
    match item.deadline.as_deref() {
        Some(deadline) => (false, deadline_sort_key(deadline).into_owned(), 0, &item.id),
        // `-created_at` is "newest first" without a second comparator: the
        // tuple is compared ascending throughout.
        None => (true, String::new(), -item.created_at, &item.id),
    }
}

/// Every open homework item, winner first.
///
/// Open is "not `done`" — see the module header for why triage and
/// grilling count, and for why archived-ness is not read here.
pub fn open_homework(inputs: &PaneInputs) -> Vec<&PaneItemFacts> {
    let mut open: Vec<&PaneItemFacts> = inputs
        .items
        .iter()
        .filter(|item| item.stage != "done" && is_homework(item.context.as_deref()))
        .collect();
    open.sort_by(|a, b| order_key(a).cmp(&order_key(b)));
    open
}

/// The civil date part of a deadline — `YYYY-MM-DD`, whether the deadline
/// carried a time of day or not.
fn deadline_date(deadline: &str) -> Option<CivilDate> {
    let key = deadline_sort_key(deadline);
    (key.len() == 16 && key.as_bytes().get(10) == Some(&b'T')).then(|| key[0..10].to_string())
}

/// A deadline as an instant at the device — `weekend.rs`'s `deadline_to_ms`
/// verbatim in its reading (a day-only deadline means the end of that day),
/// spelled here rather than shared because that one is private to a merge
/// this pane does not run.
fn deadline_to_ms(deadline: &str, facts: &ZoneFacts) -> Option<i64> {
    let key = deadline_sort_key(deadline);
    if key.len() != 16 || key.as_bytes().get(10) != Some(&b'T') {
        return None;
    }
    if key.as_bytes().get(13) != Some(&b':') {
        return None;
    }
    let hour: i64 = key[11..13].parse().ok()?;
    let minute: i64 = key[14..16].parse().ok()?;
    if hour > 23 || minute > 59 {
        return None;
    }
    let midnight = facts.midnight_ms(DEVICE_ZONE, &key[0..10])?;
    Some(midnight + hour * HOUR_MS + minute * MINUTE_MS)
}

/// Every [`ZoneQuery`] this pane needs: the device's own today, plus one
/// midnight per distinct deadline day among the open items.
///
/// Named exactly rather than over-asked (`weekend.rs`'s own bounded
/// over-ask exists because its dates are a *function* of today, which it
/// cannot know before the round trip): here every date is carried on an
/// item this call already holds.
pub fn homework_zone_queries(inputs: &PaneInputs) -> Vec<ZoneQuery> {
    let mut queries = vec![ZoneQuery::CivilDate {
        zone: DEVICE_ZONE.to_string(),
        at_ms: inputs.now_ms,
    }];
    for item in open_homework(inputs) {
        let Some(date) = item.deadline.as_deref().and_then(deadline_date) else {
            continue;
        };
        let query = ZoneQuery::Midnight {
            zone: DEVICE_ZONE.to_string(),
            date,
        };
        if !queries.iter().any(|existing| existing.key() == query.key()) {
            queries.push(query);
        }
    }
    queries
}

fn to_homework_item(item: &PaneItemFacts, with_description: bool) -> HomeworkItem {
    HomeworkItem {
        id: item.id.clone(),
        title: item.title.clone(),
        deadline: item.deadline.clone(),
        description: with_description.then(|| item.description.clone()).flatten(),
    }
}

/// The whole answered fact set, or the reason there is none.
///
/// Reads the bridge first: an unresolvable [`DEVICE_ZONE`] is a gap even
/// when nothing is open, because "no open homework" and "this device
/// cannot tell you what day it is" are different things to say and the
/// second one is not silently the first.
pub fn homework_facts(inputs: &PaneInputs, facts: &ZoneFacts) -> HomeworkResolved {
    let Some(today) = facts.civil_date(DEVICE_ZONE, inputs.now_ms) else {
        return HomeworkResolved::Gap {
            gap: HomeworkGap::UnresolvableZone,
        };
    };
    let open = open_homework(inputs);
    let Some((winner, others)) = open.split_first() else {
        return HomeworkResolved::Facts(HomeworkFacts {
            winner: None,
            others: Vec::new(),
            days_away: None,
        });
    };
    let days_away = winner
        .deadline
        .as_deref()
        .and_then(deadline_date)
        .and_then(|date| civil_days_between(&today, &date));
    HomeworkResolved::Facts(HomeworkFacts {
        winner: Some(to_homework_item(winner, true)),
        others: others
            .iter()
            .map(|item| to_homework_item(item, false))
            .collect(),
        days_away,
    })
}

/// Which band `days_away` reads as — `None` is "the winner carries no
/// deadline", which is [`Band::Distant`] rather than dormant: there *is*
/// homework, it just has no date on it, and a dormant pane says the
/// opposite.
pub fn homework_band(days_away: Option<i64>) -> Band {
    match days_away {
        // Overdue and due-today are one band on purpose: both mean "this is
        // today's problem", and splitting them would put an overdue item
        // *below* one due tonight in the cross-pane sort for no reason the
        // reader could name.
        Some(days) if days <= 0 => Band::Live,
        Some(1) => Band::Imminent,
        Some(days) if days <= NEAR_WITHIN_DAYS => Band::Near,
        _ => Band::Distant,
    }
}

/// This question's answer for the shell (#245/#675), minus its rendering
/// half.
///
/// `within_band` is the winning deadline's own instant — `None` when the
/// winner carries no deadline and when the pane is dormant, the same
/// "nothing to order by" gap shape `race_answer` has off-season.
pub fn homework_answer(inputs: &PaneInputs, facts: &ZoneFacts) -> PaneAnswerCore {
    let resolved = match homework_facts(inputs, facts) {
        // Not `Unbound`: nobody binds this question — the context is
        // hardcoded — so there is no setup prompt to route anyone to, and
        // `unbound` would render one.
        HomeworkResolved::Gap { .. } => {
            return PaneAnswerCore {
                answer_state: AnswerState::BoundButUnacquired,
                band: Band::Dormant,
                within_band: None,
            }
        }
        HomeworkResolved::Facts(facts) => facts,
    };
    let Some(winner) = resolved.winner else {
        return PaneAnswerCore {
            answer_state: AnswerState::Answered,
            band: Band::Dormant,
            within_band: None,
        };
    };
    PaneAnswerCore {
        answer_state: AnswerState::Answered,
        band: homework_band(resolved.days_away),
        within_band: winner
            .deadline
            .as_deref()
            .and_then(|d| deadline_to_ms(d, facts)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decisions::panes::zone::{add_civil_days, ZoneFact};

    /// A fixed device zone, UTC-7 (like `America/Los_Angeles` in August) —
    /// `weekend.rs`'s own fixture convention.
    const OFFSET_MS: i64 = 7 * HOUR_MS;

    fn at(y: i32, m: u32, d: u32, h: i64, min: i64) -> i64 {
        let epoch = chrono::NaiveDate::from_ymd_opt(1970, 1, 1).unwrap();
        let date = chrono::NaiveDate::from_ymd_opt(y, m, d).unwrap();
        let days = (date - epoch).num_days();
        days * 24 * HOUR_MS + h * HOUR_MS + min * MINUTE_MS + OFFSET_MS
    }

    /// A host resolver stood up from the queries the core itself asked —
    /// the shape a real host takes.
    fn resolve(queries: &[ZoneQuery]) -> ZoneFacts {
        let mut facts = ZoneFacts::default();
        for query in queries {
            match query {
                ZoneQuery::CivilDate { zone, at_ms } if zone == DEVICE_ZONE => {
                    let days = (at_ms - OFFSET_MS).div_euclid(24 * HOUR_MS);
                    facts.insert(
                        query,
                        ZoneFact::Date(add_civil_days("1970-01-01", days).unwrap()),
                    );
                }
                ZoneQuery::Midnight { zone, date } if zone == DEVICE_ZONE => {
                    let parsed = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap();
                    let epoch = chrono::NaiveDate::from_ymd_opt(1970, 1, 1).unwrap();
                    let days = (parsed - epoch).num_days();
                    facts.insert(query, ZoneFact::Instant(days * 24 * HOUR_MS + OFFSET_MS));
                }
                _ => {}
            }
        }
        facts
    }

    fn item(id: &str, context: Option<&str>, stage: &str, deadline: Option<&str>) -> PaneItemFacts {
        PaneItemFacts {
            id: id.to_string(),
            title: format!("{id} title"),
            deadline: deadline.map(str::to_string),
            scheduled_date: None,
            stage: stage.to_string(),
            context: context.map(str::to_string),
            description: Some(format!("{id} notes")),
            created_at: 0,
        }
    }

    fn homework(id: &str, deadline: Option<&str>) -> PaneItemFacts {
        item(id, Some(HOMEWORK_CONTEXT), "ready", deadline)
    }

    fn inputs(now_ms: i64, items: Vec<PaneItemFacts>) -> PaneInputs {
        PaneInputs {
            now_ms,
            items,
            ..PaneInputs::default()
        }
    }

    /// Answer + facts against a host that resolved everything the core
    /// asked for — the round trip every test below drives.
    fn answered(inputs: &PaneInputs) -> (PaneAnswerCore, HomeworkFacts) {
        let facts = resolve(&homework_zone_queries(inputs));
        let resolved = match homework_facts(inputs, &facts) {
            HomeworkResolved::Facts(resolved) => resolved,
            HomeworkResolved::Gap { gap } => panic!("unexpected gap {gap:?}"),
        };
        (homework_answer(inputs, &facts), resolved)
    }

    // ------------------------------------------------------------ matching

    #[test]
    fn matches_the_context_exactly_after_a_trim_and_ignoring_case() {
        assert!(is_homework(Some("@homework")));
        assert!(is_homework(Some("  @homework  ")));
        assert!(is_homework(Some("@HomeWork")));
        assert!(!is_homework(Some("@homework-ish")));
        assert!(!is_homework(Some("homework")));
        assert!(!is_homework(Some("")));
        assert!(!is_homework(None));
    }

    #[test]
    fn counts_every_open_stage_and_never_a_done_one() {
        let now_ms = at(2026, 8, 21, 9, 0);
        for stage in ["triage", "grilling", "ready", "in_progress", "blocked"] {
            let inputs = inputs(
                now_ms,
                vec![item(
                    "hw",
                    Some(HOMEWORK_CONTEXT),
                    stage,
                    Some("2026-08-24"),
                )],
            );
            assert_eq!(open_homework(&inputs).len(), 1, "{stage}");
        }
        let done = inputs(
            now_ms,
            vec![item(
                "hw",
                Some(HOMEWORK_CONTEXT),
                "done",
                Some("2026-08-24"),
            )],
        );
        assert!(open_homework(&done).is_empty());
    }

    #[test]
    fn reads_no_other_context_however_close() {
        let now_ms = at(2026, 8, 21, 9, 0);
        let inputs = inputs(
            now_ms,
            vec![
                item("garden", Some("@garden"), "ready", Some("2026-08-22")),
                item("none", None, "ready", Some("2026-08-22")),
                homework("hw", Some("2026-08-24")),
            ],
        );
        let open = open_homework(&inputs);
        assert_eq!(
            open.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(),
            ["hw"]
        );
    }

    // ------------------------------------------------------------ ordering

    #[test]
    fn the_soonest_deadline_wins() {
        let now_ms = at(2026, 8, 21, 9, 0);
        let inputs = inputs(
            now_ms,
            vec![
                homework("far", Some("2026-09-01")),
                homework("soon", Some("2026-08-22")),
                homework("middle", Some("2026-08-25")),
            ],
        );
        let (_, facts) = answered(&inputs);
        assert_eq!(facts.winner.as_ref().map(|w| w.id.as_str()), Some("soon"));
        assert_eq!(
            facts
                .others
                .iter()
                .map(|i| i.id.as_str())
                .collect::<Vec<_>>(),
            ["middle", "far"],
        );
    }

    #[test]
    fn a_deadline_less_item_never_beats_an_open_dated_sibling() {
        let now_ms = at(2026, 8, 21, 9, 0);
        // The undated one is newer, and further down the input list would
        // not save it either — the dated/undated split is the first key.
        let mut undated = homework("undated", None);
        undated.created_at = now_ms;
        let inputs = inputs(now_ms, vec![undated, homework("dated", Some("2027-01-01"))]);
        let (answer, facts) = answered(&inputs);
        assert_eq!(facts.winner.as_ref().map(|w| w.id.as_str()), Some("dated"));
        assert_eq!(answer.band, Band::Distant);
    }

    #[test]
    fn among_deadline_less_items_the_newest_wins() {
        let now_ms = at(2026, 8, 21, 9, 0);
        let mut older = homework("older", None);
        older.created_at = 1_000;
        let mut newer = homework("newer", None);
        newer.created_at = 2_000;
        let inputs = inputs(now_ms, vec![older, newer]);
        let (answer, facts) = answered(&inputs);
        assert_eq!(facts.winner.as_ref().map(|w| w.id.as_str()), Some("newer"));
        assert_eq!(
            facts
                .others
                .iter()
                .map(|i| i.id.as_str())
                .collect::<Vec<_>>(),
            ["older"]
        );
        // There *is* homework — it simply has no date on it.
        assert_eq!(answer.band, Band::Distant);
        assert_eq!(answer.answer_state, AnswerState::Answered);
        assert_eq!(answer.within_band, None);
        assert_eq!(facts.days_away, None);
    }

    // ---------------------------------------------------------- band table

    #[test]
    fn bands_an_overdue_item_live() {
        let now_ms = at(2026, 8, 21, 9, 0);
        let inputs = inputs(now_ms, vec![homework("hw", Some("2026-08-19"))]);
        let (answer, facts) = answered(&inputs);
        assert_eq!(answer.band, Band::Live);
        assert_eq!(facts.days_away, Some(-2));
    }

    #[test]
    fn bands_an_item_due_today_live() {
        let now_ms = at(2026, 8, 21, 9, 0);
        let inputs = inputs(now_ms, vec![homework("hw", Some("2026-08-21"))]);
        let (answer, facts) = answered(&inputs);
        assert_eq!(answer.band, Band::Live);
        assert_eq!(facts.days_away, Some(0));
    }

    #[test]
    fn bands_tomorrow_imminent_and_three_days_out_near() {
        let now_ms = at(2026, 8, 21, 9, 0);
        for (deadline, band, days) in [
            ("2026-08-22", Band::Imminent, 1),
            ("2026-08-23", Band::Near, 2),
            ("2026-08-24", Band::Near, 3),
            ("2026-08-25", Band::Distant, 4),
        ] {
            let inputs = inputs(now_ms, vec![homework("hw", Some(deadline))]);
            let (answer, facts) = answered(&inputs);
            assert_eq!(answer.band, band, "{deadline}");
            assert_eq!(facts.days_away, Some(days), "{deadline}");
        }
    }

    #[test]
    fn bands_by_the_device_s_own_day_never_utc_s() {
        // 2026-08-21 16:30 at the device (UTC-7) is 2026-08-21T23:30 UTC —
        // still the same UTC day. Push it to 18:00 local and UTC has
        // already rolled over to the 22nd, so an item due "today" would
        // read as *yesterday* under instant subtraction against a UTC day.
        let now_ms = at(2026, 8, 21, 18, 0);
        let inputs = inputs(now_ms, vec![homework("hw", Some("2026-08-21"))]);
        let (answer, facts) = answered(&inputs);
        assert_eq!(facts.days_away, Some(0));
        assert_eq!(answer.band, Band::Live);
    }

    #[test]
    fn bands_dormant_with_nothing_open_which_is_an_answer_and_not_a_gap() {
        let now_ms = at(2026, 8, 21, 9, 0);
        let inputs = inputs(
            now_ms,
            vec![item("hw", Some(HOMEWORK_CONTEXT), "done", None)],
        );
        let (answer, facts) = answered(&inputs);
        assert_eq!(answer.answer_state, AnswerState::Answered);
        assert_eq!(answer.band, Band::Dormant);
        assert_eq!(answer.within_band, None);
        assert_eq!(facts.winner, None);
        assert!(facts.others.is_empty());
    }

    // ---------------------------------------------------------- within_band

    #[test]
    fn orders_within_its_band_by_the_winning_deadline_s_own_instant() {
        let now_ms = at(2026, 8, 21, 9, 0);
        let inputs = inputs(now_ms, vec![homework("hw", Some("2026-08-22"))]);
        let (answer, _) = answered(&inputs);
        // A day-only deadline means the end of that day, at the device.
        assert_eq!(answer.within_band, Some(at(2026, 8, 22, 23, 59)));
    }

    // ----------------------------------------------------------------- gaps

    #[test]
    fn an_unresolvable_device_zone_is_a_gap_never_a_utc_fallback() {
        let now_ms = at(2026, 8, 21, 9, 0);
        let inputs = inputs(now_ms, vec![homework("hw", Some("2026-08-22"))]);
        let empty = ZoneFacts::default();
        assert_eq!(
            homework_facts(&inputs, &empty),
            HomeworkResolved::Gap {
                gap: HomeworkGap::UnresolvableZone
            },
        );
        let answer = homework_answer(&inputs, &empty);
        assert_eq!(answer.answer_state, AnswerState::BoundButUnacquired);
        assert_eq!(answer.band, Band::Dormant);
    }

    #[test]
    fn an_empty_mirror_with_an_unresolvable_zone_still_says_so_rather_than_dormant() {
        let inputs = inputs(at(2026, 8, 21, 9, 0), Vec::new());
        assert_eq!(
            homework_facts(&inputs, &ZoneFacts::default()),
            HomeworkResolved::Gap {
                gap: HomeworkGap::UnresolvableZone
            },
        );
    }

    // ----------------------------------------------------------- the facts

    #[test]
    fn carries_the_winner_s_notes_and_lists_the_rest_by_title_alone() {
        let now_ms = at(2026, 8, 21, 9, 0);
        let inputs = inputs(
            now_ms,
            vec![
                homework("first", Some("2026-08-22")),
                homework("second", None),
            ],
        );
        let (_, facts) = answered(&inputs);
        let winner = facts.winner.unwrap();
        assert_eq!(winner.title, "first title");
        assert_eq!(winner.description.as_deref(), Some("first notes"));
        assert_eq!(facts.others[0].title, "second title");
        assert_eq!(facts.others[0].description, None);
    }

    // ---------------------------------------------------------- the queries

    #[test]
    fn asks_for_today_and_one_midnight_per_distinct_deadline_day() {
        let now_ms = at(2026, 8, 21, 9, 0);
        let inputs = inputs(
            now_ms,
            vec![
                homework("a", Some("2026-08-22")),
                homework("b", Some("2026-08-22T09:00")),
                homework("c", Some("2026-08-25")),
                homework("d", None),
            ],
        );
        let keys: Vec<String> = homework_zone_queries(&inputs)
            .iter()
            .map(|q| q.key())
            .collect();
        assert_eq!(
            keys,
            [
                format!("civil:{DEVICE_ZONE}:{now_ms}"),
                format!("midnight:{DEVICE_ZONE}:2026-08-22"),
                format!("midnight:{DEVICE_ZONE}:2026-08-25"),
            ],
        );
    }

    #[test]
    fn asks_only_about_today_when_nothing_is_open() {
        let inputs = inputs(at(2026, 8, 21, 9, 0), Vec::new());
        assert_eq!(homework_zone_queries(&inputs).len(), 1);
    }
}
