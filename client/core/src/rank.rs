//! `rank()`: a pure, deterministic ranking of candidate items (issue
//! #162), split out of the `/next-up-personal` skill's six ranking steps
//! so a device can pick "what to do right now" offline. **No I/O, no
//! credentials, no clock read** — "now" arrives in [`Now`], exactly the
//! same discipline [`crate::calendar::query`] already uses for its two
//! read queries. Speaks only [`hummingbird_domain::Item`] (ADR-0009) and
//! [`crate::calendar::EventRecord`] (#70) — no Linear vocabulary, and
//! deliberately no translation layer onto either: [`Axes::context`] and
//! [`Axes::energy`]/[`Axes::size`] are matched directly against the
//! item's own fields, and priority is [`Item::priority`]'s raw `0..=4`,
//! consumed directly rather than through a re-derived label type.
//!
//! **Its caller is `client/next-up`** (#116): a member of this workspace
//! whose `next-up-rank` binary turns one `GET /api/sweep` payload into
//! candidates and calls [`rank`] on them, under the `/next-up-hb` skill.
//! That skill owns free-text axis parsing and the *why* line; this module
//! owns only the six steps and the reason codes that let it cite the
//! actual decisive rule. Delegation is **not** in that split any more —
//! the owned schema has no labels, no comments and so no way to express
//! #10's protocol at all (see `.claude/skills/next-up-hb/SKILL.md`).
//!
//! # The six steps, in order
//!
//! 1. **Context hard-filter** — drop candidates whose `context` is set and
//!    disagrees with the declared [`Axes::context`]. An untagged item
//!    always survives ([`context_survives`]).
//! 2. **Overdue / due-today** — computed against [`Now::local`], which is
//!    in the exact naive-local shape [`Item::deadline`] already uses, so
//!    the comparison goes through [`hummingbird_domain::deadline_sort_key`]
//!    the same way the mirror's own display sort does — one comparison
//!    key, never a second, divergent one built here.
//! 3. **In Progress bias** — an `InProgress` item outranks a fresh one,
//!    unless it fails the declared context (already gone by step 1) or its
//!    `energy` is the **opposite pole** of the declared energy. Unlabeled
//!    or adjacent energy keeps the bias; the bias is never a demotion below
//!    a same-ranked non-`InProgress` item, only the loss of a boost.
//! 4. **Priority** — [`Item::priority`]'s raw `0..=4`, ranked the one way
//!    this codebase already ranks it everywhere else (`client/core/src/
//!    task/item.rs`'s `Priority::rank`, `client/web/src/screens/
//!    priority.ts`'s `priorityRank`): 1..4 most-urgent-first, 0 last.
//! 5. **Energy / size fit, plus the 30-minute nudge** — a candidate whose
//!    `energy` matches the declared axis, or whose `size` **fits within**
//!    the declared time (no larger — `Quick` fits a declared `Normal`),
//!    outranks one that does not; within an otherwise-tied group, a
//!    `size: Quick` candidate is nudged ahead when [`next_start_ms`]
//!    (calendar-masking aware, cancelled instances excluded) is within 30
//!    minutes of `now`.
//! 6. **Oldest first** — `created_at` ascending, then `id` as the final,
//!    total-order tie-break so [`rank`] is byte-identical on a repeat call.

use hummingbird_domain::{deadline_sort_key, Energy, Item, Size, Stage};
use serde::{Deserialize, Serialize};

use crate::calendar::{is_actionable, CurrentOrNext, EventRecord, EventWhen};

/// The 30-minute window step 5's calendar nudge fires inside.
const NUDGE_WINDOW_MS: i64 = 30 * 60 * 1000;

/// The three declared axes, already parsed (free-text parsing is #116's
/// skill layer, out of scope here). `None` on any axis means "no
/// preference" for that axis, never "match nothing".
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Axes {
    /// Compared against [`Item::context`] with a leading `@` and case
    /// ignored on both sides — `"office"` and `"@Office"` are the same
    /// declared context.
    pub context: Option<String>,
    pub energy: Option<Energy>,
    pub size: Option<Size>,
}

/// "Now", caller-supplied — the one door through which the current instant
/// enters [`rank`]. Two shapes of the same instant, because the two things
/// `rank` compares it against are naive-local ([`Item::deadline`]) and a
/// real instant ([`EventWhen::Timed`]'s own milliseconds); deriving one
/// from the other would need a time zone this crate has no business
/// guessing at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Now {
    /// Naive local "now", in exactly [`Item::deadline`]'s own shape
    /// (`YYYY-MM-DD` or `YYYY-MM-DDTHH:MM`) — see
    /// [`hummingbird_domain::is_valid_deadline`].
    pub local: String,
    /// The same instant, as milliseconds since the Unix epoch.
    pub epoch_ms: i64,
}

/// Issue #70's calendar-context shape, unchanged: the in-progress-or-next
/// event, plus the day's events for the masking lookup step 5 needs.
#[derive(Debug)]
pub struct CalendarContext<'a> {
    pub current_or_next: CurrentOrNext<'a>,
    pub today: &'a [EventRecord],
}

/// The reason-code vocabulary: an output contract, not an implementation
/// detail. Names the rule that applied, never an internal step index, so
/// #116's skill can cite it directly. A candidate carries every rule that
/// applied to it, in step order — not just the one that happened to be
/// decisive against its neighbors — so the caller has enough to explain
/// the winning pick in one line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "rule", content = "value", rename_all = "snake_case")]
pub enum ReasonCode {
    /// Step 2: past `now`.
    Overdue,
    /// Step 2: falls on `now`'s calendar day, not yet overdue.
    DueToday,
    /// Step 3: `InProgress` and not opposite-pole demoted.
    InProgressBias,
    /// Step 4: `Item::priority`'s raw value, whenever it is set (non-zero).
    Priority(i64),
    /// Step 5: `Item::energy` matches the declared [`Axes::energy`].
    EnergyMatch,
    /// Step 5: `Item::size` fits within the declared [`Axes::size`] — no
    /// larger than the declared time (`Quick < Normal < Deep`), not
    /// necessarily equal to it.
    SizeFits,
    /// Step 5: the 30-minute nudge, applied because this candidate is
    /// `size: Quick` and the next start is inside the window.
    QuickBeforeNextStart,
    /// Step 6: this candidate reached the final `created_at`-then-`id`
    /// tie-break. Carried by **every** ranked candidate — step 6 always
    /// runs — so it marks the terminal rule, never a claim that age was
    /// the decisive difference against a neighbor. #116's "why" line
    /// should cite it only when no earlier reason separates the pick from
    /// its runner-up.
    OldestFirst,
}

/// One ranked candidate: the item, unchanged, plus every reason code that
/// applied to it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RankedCandidate {
    pub item: Item,
    pub reasons: Vec<ReasonCode>,
}

/// Step 2's bucket. Declaration order is deliberately the sort order
/// (`derive(Ord)` on a fieldless enum ranks by declaration position):
/// overdue outranks due-today outranks everything else.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum DeadlineBucket {
    Overdue,
    DueToday,
    Other,
}

/// The one comparable key [`rank`] sorts candidates by — every field
/// already in "lower sorts first" form, so the final sort is a plain
/// ascending `sort_by_key`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct SortKey {
    deadline: DeadlineBucket,
    /// 0 = `InProgress` and boosted, 1 = everything else (including a
    /// demoted `InProgress` item — demotion loses the boost, it never
    /// ranks below a same-tier non-`InProgress` candidate).
    in_progress: u8,
    priority: i64,
    /// 0 = matches both declared energy and size, 1 = matches one, 2 =
    /// matches neither (and/or nothing was declared to match against).
    fit: u8,
    /// 0 = the 30-minute nudge applies to this `size: Quick` candidate.
    quick_nudge: u8,
    created_at: i64,
    id: String,
}

fn normalize_context(s: &str) -> String {
    s.trim().trim_start_matches('@').to_ascii_lowercase()
}

/// Step 1: an item with no context survives regardless of what is
/// declared; a tagged item survives only by matching.
fn context_survives(item: &Item, axes: &Axes) -> bool {
    match (&axes.context, &item.context) {
        (None, _) => true,
        (Some(_), None) => true,
        (Some(declared), Some(item_context)) => {
            normalize_context(declared) == normalize_context(item_context)
        }
    }
}

fn deadline_bucket(item: &Item, now: &Now) -> DeadlineBucket {
    let Some(deadline) = item.deadline.as_deref() else {
        return DeadlineBucket::Other;
    };
    // `get(..10)` is the total form of `[..10]`: `None` on a too-short
    // value *and* on a byte-10 char boundary a raw slice would panic on —
    // this function's contract is a bucket for any input, never a panic.
    let (Some(deadline_day), Some(today)) = (deadline.get(..10), now.local.get(..10)) else {
        return DeadlineBucket::Other;
    };
    if deadline_sort_key(deadline) < deadline_sort_key(&now.local) {
        DeadlineBucket::Overdue
    } else if deadline_day == today {
        DeadlineBucket::DueToday
    } else {
        DeadlineBucket::Other
    }
}

/// Whether `a` and `b` are opposite poles of the energy spectrum. `Medium`
/// is adjacent to both and opposite to neither.
fn is_opposite_pole(a: Energy, b: Energy) -> bool {
    matches!(
        (a, b),
        (Energy::Low, Energy::High) | (Energy::High, Energy::Low)
    )
}

/// Step 3: whether an `InProgress` item keeps its boost.
fn in_progress_boosted(item: &Item, axes: &Axes) -> bool {
    if item.stage != Stage::InProgress {
        return false;
    }
    !matches!(
        (item.energy, axes.energy),
        (Some(item_energy), Some(declared)) if is_opposite_pole(item_energy, declared)
    )
}

fn priority_rank(raw: i64) -> i64 {
    match raw {
        1 => 0, // Urgent
        2 => 1, // High
        3 => 2, // Medium
        4 => 3, // Low
        _ => 4, // 0 (No priority) or anything unrecognized — sorts last.
    }
}

fn energy_matches(item: &Item, axes: &Axes) -> bool {
    matches!((item.energy, axes.energy), (Some(a), Some(b)) if a == b)
}

/// The duration ordering behind [`size_fits`]: `Quick < Normal < Deep`.
/// Local because `hummingbird_domain::Size` deliberately derives no `Ord` —
/// how sizes compare is a consumer's ranking concern, not schema.
fn size_duration_rank(size: Size) -> u8 {
    match size {
        Size::Quick => 0,
        Size::Normal => 1,
        Size::Deep => 2,
    }
}

/// Step 5's size rule is a size that **fits** the declared time
/// (SKILL.md's "a `size` that fits the declared time"), never exact
/// equality: declare a normal-sized block and a `Quick` item fits it just as a
/// `Normal` one does, while a `Deep` one does not. Equality would invert
/// the skill's own worked example — the smaller item losing the fit to
/// the exactly-matching one.
fn size_fits(item: &Item, axes: &Axes) -> bool {
    matches!(
        (item.size, axes.size),
        (Some(a), Some(b)) if size_duration_rank(a) <= size_duration_rank(b)
    )
}

/// The start instant of `event`, or `None` for an all-day event.
///
/// **An all-day event can never fire the nudge**, and that is a property of
/// the shape rather than a rule applied over it (ADR-0015's 2026-08-10
/// amendment): [`EventWhen::AllDay`] carries civil dates and no instant, so
/// there is no moment to be thirty minutes before. Resolving one to an
/// instant here would need a time zone this crate has no business guessing
/// — the same reason [`Now`] arrives in two shapes.
fn timed_start_ms(event: &EventRecord) -> Option<i64> {
    match event.when {
        EventWhen::Timed { start_ms, .. } => Some(start_ms),
        EventWhen::AllDay { .. } => None,
    }
}

/// The next event start after `now`, reading `today` instead of
/// `current_or_next` whenever an in-progress event (timed or all-day —
/// [`CurrentOrNext::InProgress`] covers both) would otherwise mask it.
/// `None` when nothing timed is upcoming, or when no calendar context was
/// supplied at all.
///
/// The masked scan filters `today` through the exact predicate
/// [`crate::calendar::query`]'s own reads use ([`is_actionable`]) — the
/// snapshot keeps cancelled instances (`showDeleted=true`, stored as a
/// zero-length span), and that module's invariant is explicit: a cancelled
/// future instance must never become "Next" or bias task ranking.
fn next_start_ms(calendar: Option<&CalendarContext>, now: &Now) -> Option<i64> {
    let calendar = calendar?;
    match calendar.current_or_next {
        CurrentOrNext::Upcoming(event) => timed_start_ms(event),
        CurrentOrNext::InProgress(_) => calendar
            .today
            .iter()
            .filter(|event| is_actionable(event))
            .filter_map(|event| {
                timed_start_ms(event).map(|start_ms| (start_ms, event.provider_event_id.as_str()))
            })
            .filter(|(start_ms, _)| *start_ms > now.epoch_ms)
            .min()
            .map(|(start_ms, _)| start_ms),
        CurrentOrNext::None => None,
    }
}

/// Whether step 5's 30-minute nudge is active at all right now (independent
/// of any one candidate's `size`).
fn nudge_active(calendar: Option<&CalendarContext>, now: &Now) -> bool {
    match next_start_ms(calendar, now) {
        Some(start_ms) => {
            let delta = start_ms - now.epoch_ms;
            (0..=NUDGE_WINDOW_MS).contains(&delta)
        }
        None => false,
    }
}

/// Ranks `items` per the six steps documented on this module. Pure and
/// deterministic: the same inputs, in any order, produce byte-identical
/// output, because every field of [`SortKey`] is itself deterministic and
/// `id` is a total tie-break with nothing left to chance.
pub fn rank(
    items: &[Item],
    axes: &Axes,
    now: &Now,
    calendar: Option<&CalendarContext>,
) -> Vec<RankedCandidate> {
    let nudge_is_active = nudge_active(calendar, now);

    let mut candidates: Vec<(SortKey, RankedCandidate)> = items
        .iter()
        .filter(|item| context_survives(item, axes))
        .map(|item| {
            let deadline = deadline_bucket(item, now);
            let boosted = in_progress_boosted(item, axes);
            let energy_hit = energy_matches(item, axes);
            let size_hit = size_fits(item, axes);
            let fit = 2 - u8::from(energy_hit) - u8::from(size_hit);
            let quick_hit = nudge_is_active && item.size == Some(Size::Quick);

            let mut reasons = Vec::new();
            match deadline {
                DeadlineBucket::Overdue => reasons.push(ReasonCode::Overdue),
                DeadlineBucket::DueToday => reasons.push(ReasonCode::DueToday),
                DeadlineBucket::Other => {}
            }
            if boosted {
                reasons.push(ReasonCode::InProgressBias);
            }
            if item.priority != 0 {
                reasons.push(ReasonCode::Priority(item.priority));
            }
            if energy_hit {
                reasons.push(ReasonCode::EnergyMatch);
            }
            if size_hit {
                reasons.push(ReasonCode::SizeFits);
            }
            if quick_hit {
                reasons.push(ReasonCode::QuickBeforeNextStart);
            }
            reasons.push(ReasonCode::OldestFirst);

            let key = SortKey {
                deadline,
                in_progress: u8::from(!boosted),
                priority: priority_rank(item.priority),
                fit,
                quick_nudge: u8::from(!quick_hit),
                created_at: item.created_at,
                id: item.id.clone(),
            };

            (
                key,
                RankedCandidate {
                    item: item.clone(),
                    reasons,
                },
            )
        })
        .collect();

    candidates.sort_by(|(a, _), (b, _)| a.cmp(b));
    candidates
        .into_iter()
        .map(|(_, candidate)| candidate)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calendar::{EventStatus, EventWhen};

    fn item(id: &str) -> Item {
        Item {
            id: id.to_string(),
            seq: None,
            title: format!("item {id}"),
            description: None,
            stage: Stage::Ready,
            size: None,
            energy: None,
            context: None,
            priority: 0,
            project_id: None,
            project_pos: None,
            deadline: None,
            scheduled_date: None,
            source: None,
            source_key: None,
            source_url: None,
            vault_path: None,
            link_url: None,
            link_label: None,
            archived_at: None,
            agent: false,
            created_at: 1_000,
            updated_at: 1_000,
            version: 1,
        }
    }

    fn now(local: &str, epoch_ms: i64) -> Now {
        Now {
            local: local.to_string(),
            epoch_ms,
        }
    }

    fn timed_event(id: &str, start_ms: i64, end_ms: i64) -> EventRecord {
        EventRecord {
            provider_event_id: id.to_string(),
            calendar_id: "cal-primary".to_string(),
            title: id.to_string(),
            when: EventWhen::timed(start_ms, end_ms),
            recurrence_id: None,
            location: None,
            organizer: None,
            status: EventStatus::Confirmed,
            provider_updated_at_ms: start_ms,
            html_link: None,
            description: None,
        }
    }

    fn ids(candidates: &[RankedCandidate]) -> Vec<String> {
        candidates.iter().map(|c| c.item.id.clone()).collect()
    }

    // -- determinism -------------------------------------------------

    #[test]
    fn the_same_snapshot_ranked_twice_is_byte_identical() {
        let items = vec![
            {
                let mut i = item("a");
                i.priority = 2;
                i.created_at = 500;
                i
            },
            {
                let mut i = item("b");
                i.priority = 2;
                i.created_at = 200;
                i
            },
            {
                let mut i = item("c");
                i.stage = Stage::InProgress;
                i
            },
        ];
        let axes = Axes::default();
        let now = now("2026-08-10T09:00", 0);

        let first = rank(&items, &axes, &now, None);
        let second = rank(&items, &axes, &now, None);

        assert_eq!(
            serde_json::to_string(&first).unwrap(),
            serde_json::to_string(&second).unwrap()
        );
    }

    // -- step 1: context hard-filter ----------------------------------

    #[test]
    fn a_wrong_context_item_is_dropped_but_untagged_survives() {
        let mut office = item("office-only");
        office.context = Some("@office".to_string());
        let untagged = item("untagged");
        let mut matching = item("matching");
        matching.context = Some("@computer".to_string());

        let axes = Axes {
            context: Some("@computer".to_string()),
            ..Axes::default()
        };
        let now = now("2026-08-10T09:00", 0);

        let results = rank(&[office, untagged, matching], &axes, &now, None);

        let mut got = ids(&results);
        got.sort();
        assert_eq!(got, vec!["matching".to_string(), "untagged".to_string()]);
    }

    #[test]
    fn context_matches_regardless_of_leading_at_and_case() {
        let mut candidate = item("a");
        candidate.context = Some("Computer".to_string());
        let axes = Axes {
            context: Some("@COMPUTER".to_string()),
            ..Axes::default()
        };
        let now = now("2026-08-10T09:00", 0);

        let results = rank(&[candidate], &axes, &now, None);
        assert_eq!(ids(&results), vec!["a".to_string()]);
    }

    // -- step 2: overdue / due today -----------------------------------

    #[test]
    fn overdue_beats_due_today_beats_undated() {
        let mut overdue = item("overdue");
        overdue.deadline = Some("2026-08-09T12:00".to_string());
        let mut due_today = item("due-today");
        due_today.deadline = Some("2026-08-10T18:00".to_string());
        let undated = item("undated");

        let axes = Axes::default();
        let now = now("2026-08-10T09:00", 0);

        let results = rank(&[undated, due_today, overdue], &axes, &now, None);
        assert_eq!(
            ids(&results),
            vec![
                "overdue".to_string(),
                "due-today".to_string(),
                "undated".to_string()
            ]
        );
        assert!(results[0].reasons.contains(&ReasonCode::Overdue));
        assert!(results[1].reasons.contains(&ReasonCode::DueToday));
    }

    #[test]
    fn a_day_only_deadline_today_is_due_today_not_overdue() {
        let mut day_only = item("day-only");
        day_only.deadline = Some("2026-08-10".to_string());
        let axes = Axes::default();
        // "now" is late in the day, but a day-only deadline means end of
        // day (23:59) — still due today, never overdue.
        let now = now("2026-08-10T23:00", 0);

        let results = rank(&[day_only], &axes, &now, None);
        assert_eq!(results[0].reasons.first(), Some(&ReasonCode::DueToday));
    }

    /// `deadline_bucket`'s contract is totality: garbage in, `Other` out,
    /// never a panic. `"2026-08-1é!!"` puts a two-byte `é` across byte
    /// index 10, so a byte-length guard alone would let `deadline[..10]`
    /// split the char and panic.
    #[test]
    fn a_non_char_boundary_deadline_is_other_not_a_panic() {
        let mut garbled = item("garbled");
        garbled.deadline = Some("2026-08-1é!!".to_string());
        let axes = Axes::default();
        let now = now("2026-08-10T09:00", 0);

        let results = rank(&[garbled], &axes, &now, None);
        assert!(!results[0].reasons.contains(&ReasonCode::Overdue));
        assert!(!results[0].reasons.contains(&ReasonCode::DueToday));
    }

    // -- step 3: In Progress bias --------------------------------------

    #[test]
    fn an_in_progress_item_outranks_a_fresh_one_at_the_same_priority() {
        let mut fresh = item("fresh");
        fresh.created_at = 1;
        let mut started = item("started");
        started.stage = Stage::InProgress;
        started.created_at = 2;

        let axes = Axes::default();
        let now = now("2026-08-10T09:00", 0);

        let results = rank(&[fresh, started], &axes, &now, None);
        assert_eq!(
            ids(&results),
            vec!["started".to_string(), "fresh".to_string()]
        );
    }

    #[test]
    fn an_in_progress_item_with_opposite_pole_energy_is_demoted() {
        let mut low_declared_high_started = item("started-high");
        low_declared_high_started.stage = Stage::InProgress;
        low_declared_high_started.energy = Some(Energy::High);
        let fresh = item("fresh");

        let axes = Axes {
            energy: Some(Energy::Low),
            ..Axes::default()
        };
        let now = now("2026-08-10T09:00", 0);

        let results = rank(&[low_declared_high_started, fresh], &axes, &now, None);

        // Demoted, but never dropped — still an alternate, and it ties
        // with the fresh item rather than sorting behind it, so the
        // oldest-first tie-break (created_at 1000 for both) plus id
        // decides: "fresh" < "started-high" lexicographically.
        assert_eq!(
            ids(&results),
            vec!["fresh".to_string(), "started-high".to_string()]
        );
        assert!(!results
            .iter()
            .find(|c| c.item.id == "started-high")
            .unwrap()
            .reasons
            .contains(&ReasonCode::InProgressBias));
    }

    #[test]
    fn an_in_progress_item_with_unlabeled_energy_keeps_the_bias() {
        let mut started = item("started");
        started.stage = Stage::InProgress; // energy left None
        let fresh = item("fresh");

        let axes = Axes {
            energy: Some(Energy::Low),
            ..Axes::default()
        };
        let now = now("2026-08-10T09:00", 0);

        let results = rank(&[fresh, started], &axes, &now, None);
        assert_eq!(
            ids(&results),
            vec!["started".to_string(), "fresh".to_string()]
        );
    }

    #[test]
    fn an_in_progress_item_with_adjacent_energy_keeps_the_bias() {
        // Medium is adjacent to (not opposite) both Low and High.
        let mut started = item("started");
        started.stage = Stage::InProgress;
        started.energy = Some(Energy::Medium);
        let fresh = item("fresh");

        let axes = Axes {
            energy: Some(Energy::Low),
            ..Axes::default()
        };
        let now = now("2026-08-10T09:00", 0);

        let results = rank(&[fresh, started], &axes, &now, None);
        assert_eq!(
            ids(&results),
            vec!["started".to_string(), "fresh".to_string()]
        );
    }

    // -- step 4: priority ------------------------------------------------

    #[test]
    fn priority_ranks_urgent_first_and_no_priority_last() {
        let mut none = item("none");
        none.priority = 0;
        let mut urgent = item("urgent");
        urgent.priority = 1;
        let mut low = item("low");
        low.priority = 4;

        let axes = Axes::default();
        let now = now("2026-08-10T09:00", 0);

        let results = rank(&[none, low, urgent], &axes, &now, None);
        assert_eq!(
            ids(&results),
            vec!["urgent".to_string(), "low".to_string(), "none".to_string()]
        );
    }

    // -- step 5: energy/size fit -----------------------------------------

    #[test]
    fn a_matching_energy_and_size_outranks_a_non_matching_one() {
        let mut fits = item("fits");
        fits.energy = Some(Energy::Low);
        fits.size = Some(Size::Quick);
        let misses = item("misses");

        let axes = Axes {
            energy: Some(Energy::Low),
            size: Some(Size::Quick),
            ..Axes::default()
        };
        let now = now("2026-08-10T09:00", 0);

        let results = rank(&[misses, fits], &axes, &now, None);
        assert_eq!(
            ids(&results),
            vec!["fits".to_string(), "misses".to_string()]
        );
    }

    /// SKILL.md's worked example, decided *before* any calendar signal:
    /// with a short block declared, a `Quick` item **fits** the declared
    /// time and a larger one does not — "fits", not "equals". Under exact
    /// equality the smaller item would get no fit at all and lose to the
    /// older, larger one on step 6, the inverse of the source rule.
    #[test]
    fn a_smaller_item_size_fits_the_declared_time() {
        let quick = {
            let mut i = item("quick");
            i.size = Some(Size::Quick);
            i.created_at = 500; // newer — must win on fit, not on age
            i
        };
        let deep = {
            let mut i = item("deep");
            i.size = Some(Size::Deep);
            i.created_at = 100;
            i
        };

        let axes = Axes {
            size: Some(Size::Normal), // "I have 30 minutes"
            ..Axes::default()
        };
        let now = now("2026-08-10T09:00", 0);

        let results = rank(&[deep, quick], &axes, &now, None);
        assert_eq!(ids(&results), vec!["quick".to_string(), "deep".to_string()]);
        assert!(results[0].reasons.contains(&ReasonCode::SizeFits));
        assert!(!results[1].reasons.contains(&ReasonCode::SizeFits));
    }

    #[test]
    fn the_30_minute_nudge_promotes_a_quick_candidate_when_upcoming_is_close() {
        let quick = {
            let mut i = item("quick");
            i.size = Some(Size::Quick);
            i.created_at = 500;
            i
        };
        let medium = {
            let mut i = item("medium");
            i.size = Some(Size::Normal);
            i.created_at = 100; // older, would otherwise win step 6
            i
        };

        let axes = Axes::default(); // no declared time — step 5's base fit is moot
        let now = now("2026-08-10T09:00", 0);
        let upcoming = timed_event("review", 12 * 60 * 1000, 30 * 60 * 1000);
        let calendar = CalendarContext {
            current_or_next: CurrentOrNext::Upcoming(&upcoming),
            today: &[],
        };

        let results = rank(
            &[medium.clone(), quick.clone()],
            &axes,
            &now,
            Some(&calendar),
        );
        assert_eq!(
            ids(&results),
            vec!["quick".to_string(), "medium".to_string()]
        );
        assert!(results[0]
            .reasons
            .contains(&ReasonCode::QuickBeforeNextStart));

        // Without the calendar context, the older item wins on step 6.
        let no_calendar = rank(&[medium, quick], &axes, &now, None);
        assert_eq!(
            ids(&no_calendar),
            vec!["medium".to_string(), "quick".to_string()]
        );
    }

    #[test]
    fn the_nudge_never_drops_a_non_quick_candidate_from_the_list() {
        let quick = {
            let mut i = item("quick");
            i.size = Some(Size::Quick);
            i
        };
        let deep = {
            let mut i = item("deep");
            i.size = Some(Size::Deep);
            i
        };

        let axes = Axes::default();
        let now = now("2026-08-10T09:00", 0);
        let upcoming = timed_event("review", 5 * 60 * 1000, 30 * 60 * 1000);
        let calendar = CalendarContext {
            current_or_next: CurrentOrNext::Upcoming(&upcoming),
            today: &[],
        };

        let results = rank(&[deep, quick], &axes, &now, Some(&calendar));
        assert_eq!(results.len(), 2);
    }

    /// The 10:50-standup masking example from the brief: a standup is in
    /// progress at 10:50 with an 11:00 review right behind it.
    /// `current_or_next` alone reports `InProgress`, which would hide the
    /// upcoming review entirely — the nudge must read `today` instead.
    #[test]
    fn the_nudge_reads_the_next_start_off_today_when_in_progress_masks_it() {
        let ten_am = 10 * 60 * 60 * 1000_i64;
        let ten_fifty = ten_am + 50 * 60 * 1000;
        let eleven_am = 11 * 60 * 60 * 1000_i64;

        let standup = timed_event("standup", ten_am, eleven_am);
        let review = timed_event("review", eleven_am, eleven_am + 30 * 60 * 1000);

        let quick = {
            let mut i = item("quick");
            i.size = Some(Size::Quick);
            i.created_at = 500;
            i
        };
        let older_non_quick = {
            let mut i = item("older");
            i.size = Some(Size::Normal);
            i.created_at = 100;
            i
        };

        let axes = Axes::default();
        let now = now("2026-08-10T10:50", ten_fifty);
        let calendar = CalendarContext {
            current_or_next: CurrentOrNext::InProgress(&standup),
            today: &[standup.clone(), review],
        };

        let results = rank(&[older_non_quick, quick], &axes, &now, Some(&calendar));
        assert_eq!(
            ids(&results),
            vec!["quick".to_string(), "older".to_string()]
        );
    }

    /// The masking test one status field away: the 11:00 review is
    /// **cancelled**. The snapshot keeps it (`showDeleted=true`, stored
    /// with `start == end` at its original start), but
    /// `calendar::query`'s invariant is explicit — a cancelled future
    /// instance must never become "Next" *or bias task ranking* — so the
    /// masked `today` scan must skip it exactly as `current_or_next_event`
    /// does, and the nudge must not fire.
    #[test]
    fn a_cancelled_event_in_today_never_fires_the_nudge() {
        let ten_am = 10 * 60 * 60 * 1000_i64;
        let ten_fifty = ten_am + 50 * 60 * 1000;
        let eleven_am = 11 * 60 * 60 * 1000_i64;

        let standup = timed_event("standup", ten_am, eleven_am);
        let cancelled_review = {
            // Cancelled instances arrive with start == end at the
            // original start — a zero-length placeholder, not an event.
            let mut e = timed_event("review", eleven_am, eleven_am);
            e.status = EventStatus::Cancelled;
            e
        };

        let quick = {
            let mut i = item("quick");
            i.size = Some(Size::Quick);
            i.created_at = 500;
            i
        };
        let older_non_quick = {
            let mut i = item("older");
            i.size = Some(Size::Normal);
            i.created_at = 100;
            i
        };

        let axes = Axes::default();
        let now = now("2026-08-10T10:50", ten_fifty);
        let calendar = CalendarContext {
            current_or_next: CurrentOrNext::InProgress(&standup),
            today: &[standup.clone(), cancelled_review],
        };

        let results = rank(&[older_non_quick, quick], &axes, &now, Some(&calendar));
        // No live upcoming start: the nudge stays off and the older item
        // keeps its step-6 win.
        assert_eq!(
            ids(&results),
            vec!["older".to_string(), "quick".to_string()]
        );
        assert!(!results
            .iter()
            .any(|c| c.reasons.contains(&ReasonCode::QuickBeforeNextStart)));
    }

    fn all_day_event(id: &str, start_date: &str, end_date: &str) -> EventRecord {
        EventRecord {
            when: EventWhen::all_day(start_date, end_date),
            ..timed_event(id, 0, 0)
        }
    }

    #[test]
    fn an_all_day_event_masks_the_next_start_the_same_way_as_in_progress() {
        // An all-day event covering today reports `InProgress` (see
        // `calendar::query::current_or_next_event`), so it takes the exact
        // same masking path as a normal in-progress meeting: the nudge is
        // decided by the next TIMED start in `today`, not by the all-day
        // event itself.
        let all_day = all_day_event("conference", "2026-08-10", "2026-08-11");
        let lunch_meeting = timed_event("lunch", 12 * 60 * 60 * 1000, 13 * 60 * 60 * 1000);

        let quick = {
            let mut i = item("quick");
            i.size = Some(Size::Quick);
            i
        };

        let axes = Axes::default();
        let almost_noon = 12 * 60 * 60 * 1000 - 5 * 60 * 1000;
        let now = now("2026-08-10T11:55", almost_noon);
        let calendar = CalendarContext {
            current_or_next: CurrentOrNext::InProgress(&all_day),
            today: &[all_day.clone(), lunch_meeting],
        };

        let results = rank(&[quick], &axes, &now, Some(&calendar));
        assert!(results[0]
            .reasons
            .contains(&ReasonCode::QuickBeforeNextStart));
    }

    #[test]
    fn an_all_day_event_never_fires_the_nudge_itself() {
        // ADR-0015's amendment made this structural: an all-day event
        // carries civil dates and no instant, so there is no moment to be
        // thirty minutes before — whether it arrives as the upcoming event
        // or as the only other thing on today's calendar. Under the old
        // flattened shape it carried a local-midnight instant, which a
        // device reading in another zone could land inside the nudge
        // window for no reason anyone could see.
        let tomorrow_off = all_day_event("day-off", "2026-08-11", "2026-08-12");

        let quick = {
            let mut i = item("quick");
            i.size = Some(Size::Quick);
            i.created_at = 500;
            i
        };
        let older_non_quick = {
            let mut i = item("older");
            i.size = Some(Size::Normal);
            i.created_at = 100;
            i
        };

        let axes = Axes::default();
        let now = now("2026-08-10T23:40", 23 * 60 * 60 * 1000 + 40 * 60 * 1000);

        for calendar in [
            CalendarContext {
                current_or_next: CurrentOrNext::Upcoming(&tomorrow_off),
                today: std::slice::from_ref(&tomorrow_off),
            },
            CalendarContext {
                current_or_next: CurrentOrNext::InProgress(&tomorrow_off),
                today: std::slice::from_ref(&tomorrow_off),
            },
        ] {
            let results = rank(
                &[older_non_quick.clone(), quick.clone()],
                &axes,
                &now,
                Some(&calendar),
            );
            assert_eq!(
                ids(&results),
                vec!["older".to_string(), "quick".to_string()],
                "an all-day event must not promote the quick item"
            );
            assert!(!results
                .iter()
                .any(|c| c.reasons.contains(&ReasonCode::QuickBeforeNextStart)));
        }
    }

    #[test]
    fn beyond_30_minutes_the_nudge_does_not_fire() {
        let quick = {
            let mut i = item("quick");
            i.size = Some(Size::Quick);
            i.created_at = 500;
            i
        };
        let older_non_quick = {
            let mut i = item("older");
            i.size = Some(Size::Normal);
            i.created_at = 100;
            i
        };

        let axes = Axes::default();
        let now = now("2026-08-10T09:00", 0);
        let upcoming = timed_event("later", 45 * 60 * 1000, 60 * 60 * 1000);
        let calendar = CalendarContext {
            current_or_next: CurrentOrNext::Upcoming(&upcoming),
            today: &[],
        };

        let results = rank(&[older_non_quick, quick], &axes, &now, Some(&calendar));
        assert_eq!(
            ids(&results),
            vec!["older".to_string(), "quick".to_string()]
        );
    }

    // -- step 6: oldest first ---------------------------------------------

    #[test]
    fn oldest_created_at_wins_when_everything_else_ties() {
        let mut newer = item("newer");
        newer.created_at = 2_000;
        let mut older = item("older");
        older.created_at = 1_000;

        let axes = Axes::default();
        let now = now("2026-08-10T09:00", 0);

        let results = rank(&[newer, older], &axes, &now, None);
        assert_eq!(
            ids(&results),
            vec!["older".to_string(), "newer".to_string()]
        );
    }

    /// Pins [`ReasonCode::OldestFirst`]'s documented contract: a terminal
    /// marker on *every* ranked candidate (step 6 always runs), never a
    /// claim of decisiveness — an item that won outright on an earlier
    /// step still carries it, last.
    #[test]
    fn every_candidate_carries_oldest_first_as_its_final_reason() {
        let mut overdue = item("overdue");
        overdue.deadline = Some("2026-08-09T12:00".to_string());
        let plain = item("plain");

        let axes = Axes::default();
        let now = now("2026-08-10T09:00", 0);

        let results = rank(&[overdue, plain], &axes, &now, None);
        assert_eq!(results.len(), 2);
        for candidate in &results {
            assert_eq!(candidate.reasons.last(), Some(&ReasonCode::OldestFirst));
        }
    }

    // -- zero Linear vocabulary (structural) -------------------------------

    #[test]
    fn rank_input_and_output_types_carry_only_adr_0009_and_hash70_fields() {
        // This is a compile-time property as much as a runtime one: `Item`
        // is `hummingbird_domain::Item` verbatim (no wrapper struct with a
        // `priorityLabel` or a Linear state name), and `EventRecord` is
        // `crate::calendar::EventRecord` (#70) verbatim. The assertion
        // below just exercises that the two really are the same types the
        // rest of the crate/domain already use.
        let i = item("a");
        let _: hummingbird_domain::Item = i;
        let e = timed_event("b", 0, 1);
        let _: EventRecord = e;
    }
}
