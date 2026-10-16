//! **The fantasy-football question** (#717, the tracer bullet of #713's
//! fantasy lane) — born here, like [`super::homework`], rather than sunk
//! from a web `*.ts` file.
//!
//! # One question, two subjects per league
//!
//! ADR-0015's `Pane = one subject`: the question registers once and emits
//! **two panes per league** named in the `yahoo-leagues` binding —
//! `<league-key>:lineup` and `<league-key>:waivers` — the way
//! [`super::race`] emits one pane per followed series. The two are ranked
//! independently because they are different obligations with different
//! clocks: "waivers close in 2h" must be able to outrank "lineup fine,
//! first kickoff Sunday" (#713's own rejection of one swapping pane).
//!
//! Each subject reads its **own source**, because the two differ in shape
//! and `hummingbird_domain::SourceEntry::shape` is one field: lineup
//! validity is a state that resolves ([`LINEUP_SOURCE`]), a waiver deadline
//! is an occurrence that expires ([`WAIVERS_SOURCE`]).
//!
//! # What is decided here, and what is left
//!
//! **Nothing here talks to Yahoo.** Both subjects answer the two states a
//! device is in before any row lands: binding unset
//! ([`AnswerState::Unbound`], with the setup prompt, so the question is
//! discoverable) and bound but never polled
//! ([`AnswerState::BoundButUnacquired`]). Both are `dormant`.
//!
//! **The lineup reads its row (#855).** The `yahoo-lineup/v1` body is the
//! poller's to pin (`server/fantasy-poll/src/body.rs`, byte for byte in
//! `tests/fixtures/golden-lineup{,-valid}.json`, and this module's tests
//! read those same files). From it this module decides the
//! [`LineupFacts`]: the per-slot detail, the band, the
//! [`LineupHeadline`] and the lineup's own crunch-time freshness judgement
//! — see [`fantasy_lineup_answer`] for the thresholds. The `live` band, off
//! a live lineup alert, is #720's. The waivers row is still read only as
//! far as its envelope, so a present, well-formed one answers
//! [`FantasyGap::BodyNotRead`] until #719 pins its body — a gap, never an
//! invented answer.
//!
//! **The headline crosses as a decision, not a sentence** (ADR-0025): which
//! arm it is, the count of what is wrong and the one reason they share, the
//! lock instant, and how old a crunch-time answer is. "Sun 1:00pm" and
//! "in 4h" are device-local words for that instant (ADR-0015's Time), so a
//! client sets them. For the same reason [`fantasy_zone_queries`] still
//! asks the bridge for nothing: every threshold here is a duration from
//! now, never a civil date.
//!
//! **The discovered leagues** — the poller's second key under the lineup
//! source — are read here too ([`fantasy_discovered_leagues`]), so Settings
//! can show them beside the binding they are copied into.
//!
//! # The binding
//!
//! `yahoo-leagues` holds **comma-separated league keys inside the JSON
//! string** (`"449.l.123456"`, later `"449.l.123456,449.l.789012"`). A JSON
//! array would land as `BindingValue::Other`, which the binding editor
//! cannot write — `server/race-poll/src/binding.rs`'s header says why at
//! length, and [`leagues_from_binding`] — shared with the poller through
//! `hummingbird_domain` since #853 — mirrors its parse.

use serde::{Deserialize, Serialize};

use super::contract::{AnswerState, Band, PaneAnswerCore};
use super::inputs::{BindingValueFact, FreshnessFact, PaneEnvelopeFacts, PaneInputs, PaneSnapshotFacts};
use super::zone::ZoneQuery;

const HOUR_MS: i64 = 60 * 60 * 1000;

/// An invalid lineup whose lock is this close is `imminent`; further out it
/// is `near` (#713's bands).
pub const IMMINENT_MS: i64 = 24 * HOUR_MS;

/// How old a lineup answer may be before it is worth saying so — `2 ×` the
/// poller's declared six-hour cadence, ADR-0015's own rule, which is safe
/// here because that cadence is a floor (`server/fantasy-poll/src/body.rs`'s
/// `POLLED_EVERY_MS`).
pub const STALE_AFTER_MS: i64 = 12 * HOUR_MS;

/// The lineup's **own, tighter** judgement for the case that matters: an
/// invalid lineup inside 24h of its lock. The poller re-fetches every
/// `*/15` run in exactly that case (its `gate` module), so a row older than
/// an hour there means four or more runs missed — and "2 empty slots" read
/// off it may already have been fixed in Yahoo. So the headline carries the
/// age ([`LineupHeadline::Invalid`]'s `checked_ago_ms`) rather than reading
/// as current. An hour, not fifteen minutes: Actions' cron routinely starts
/// a scheduled run many minutes late, and a caveat on every jittered run
/// would teach the reader to ignore it.
pub const CRUNCH_STALE_AFTER_MS: i64 = HOUR_MS;

/// The discovered-leagues row's key under [`LINEUP_SOURCE`] — the poller's
/// `body::DISCOVERED_KEY`, agreeing by review (and by its golden file).
pub const DISCOVERED_KEY: &str = "discovered-leagues";

/// That row's envelope `schema` — the poller's `body::DISCOVERED_SCHEMA`.
pub const DISCOVERED_SCHEMA: &str = "yahoo-discovered-leagues/v1";

/// The lineup subject's source. This module's own constant, agreeing with
/// `hummingbird_domain::YAHOO_LINEUP_V1` by review — ADR-0015 forbids a pane
/// consulting the frozen registry (`race.rs`'s `SOURCE` says the same).
pub const LINEUP_SOURCE: &str = "yahoo-lineup/v1";

/// The waivers subject's source, [`LINEUP_SOURCE`]'s sibling.
pub const WAIVERS_SOURCE: &str = "yahoo-waivers/v1";

/// The binding that names which leagues are followed — kebab-case and
/// unversioned, so a `/v1 → /v2` source bump cannot orphan it.
pub const BINDING_KEY: &str = "yahoo-leagues";

/// The league half of the sentinel subjects an unbound (or not-yet-read)
/// question emits — never a real league key, which always carries `.l.`.
pub const SETUP_LEAGUE: &str = "setup";

/// Which of the two obligations a pane is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FantasySubject {
    Lineup,
    Waivers,
}

impl FantasySubject {
    /// Both, in the order a league's two panes are emitted.
    pub const BOTH: [FantasySubject; 2] = [FantasySubject::Lineup, FantasySubject::Waivers];

    pub fn as_str(self) -> &'static str {
        match self {
            FantasySubject::Lineup => "lineup",
            FantasySubject::Waivers => "waivers",
        }
    }

    /// The source this subject reads.
    pub fn source(self) -> &'static str {
        match self {
            FantasySubject::Lineup => LINEUP_SOURCE,
            FantasySubject::Waivers => WAIVERS_SOURCE,
        }
    }
}

/// `<league-key>:<subject>` — the one spelling of a pane's subject key.
pub fn subject_key(league: &str, subject: FantasySubject) -> String {
    format!("{league}:{}", subject.as_str())
}

/// The inverse of [`subject_key`]: the league and the subject a key names,
/// or `None` for a key that is not one of this question's. Splits on the
/// **last** colon, so a league key is taken whole whatever it contains.
pub fn parse_subject_key(key: &str) -> Option<(&str, FantasySubject)> {
    let (league, subject) = key.rsplit_once(':')?;
    if league.is_empty() {
        return None;
    }
    let subject = match subject {
        "lineup" => FantasySubject::Lineup,
        "waivers" => FantasySubject::Waivers,
        _ => return None,
    };
    Some((league, subject))
}

/// Whether the question has been asked at all — **four answers, not a
/// boolean**, [`super::race::RaceSetup`]'s shape for its reasons.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum FantasySetup {
    Bound { leagues: Vec<String> },
    /// The bindings table has not been read on this device yet.
    Unread,
    /// A row exists but holds something that is not text — a JSON array
    /// among them. Bound-but-unacquired, not a crash.
    Unusable,
    /// No row, or one holding nothing — the only arm that is genuinely
    /// `unbound`.
    Unset,
}

/// The followed leagues, read out of the `yahoo-leagues` binding's text:
/// trimmed, blanks dropped, repeats dropped, order kept. Lives in
/// `hummingbird_domain` since #853, so the Yahoo poller (`server/fantasy-poll`)
/// splits the binding through this one parse rather than a twin.
pub use hummingbird_domain::yahoo_leagues_from_binding as leagues_from_binding;

/// Whether the `yahoo-leagues` binding has been set, and if not, which kind
/// of not-set it is.
pub fn fantasy_setup(inputs: &PaneInputs) -> FantasySetup {
    if inputs.bindings.is_none() {
        return FantasySetup::Unread;
    }
    match inputs.binding(BINDING_KEY).map(|binding| &binding.value) {
        None | Some(BindingValueFact::Unset) => FantasySetup::Unset,
        Some(BindingValueFact::Other) => FantasySetup::Unusable,
        Some(BindingValueFact::Text { text }) => {
            let leagues = leagues_from_binding(text);
            // A row blanked to whitespace (or nothing but separators) is the
            // nearest thing `settings` has to a DELETE, and reads as unset.
            if leagues.is_empty() {
                FantasySetup::Unset
            } else {
                FantasySetup::Bound { leagues }
            }
        }
    }
}

/// This question's subjects: a lineup and a waivers pane per followed
/// league, in binding order — or, until a league is bound, the two
/// sentinel subjects under [`SETUP_LEAGUE`], so both obligations are
/// discoverable with their setup prompt (ADR-0015).
pub fn fantasy_subjects(inputs: &PaneInputs) -> Vec<String> {
    let leagues = match fantasy_setup(inputs) {
        FantasySetup::Bound { leagues } => leagues,
        _ => vec![SETUP_LEAGUE.to_string()],
    };
    leagues
        .iter()
        .flat_map(|league| FantasySubject::BOTH.map(|subject| subject_key(league, subject)))
        .collect()
}

/// Every `(zone, civil-date)` fact this question needs — **none**. Every
/// threshold is a duration from now and the lock crosses as an instant; see
/// the module header.
pub fn fantasy_zone_queries(_inputs: &PaneInputs) -> Vec<ZoneQuery> {
    Vec::new()
}

/// Why a pane has no answer — a **kind**, not a sentence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "gap", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum FantasyGap {
    /// No snapshot row at all: this league has never been polled.
    NotFetched,
    /// The envelope itself could not be read. `reason` is
    /// `hummingbird_domain::EnvelopeProblem`'s own wording, as data.
    Malformed { reason: String },
    /// A schema this build has never heard of — fixed by updating the app.
    UnknownSchema { schema: String },
    /// A well-formed row exists, and this build reads no body for this
    /// source yet: the waivers body is #719's to pin.
    BodyNotRead,
    /// A `yahoo-lineup/v1` row whose body is not a lineup — not JSON, or a
    /// shape the poller's golden file does not describe.
    BadBody,
}

/// Why a starting slot is invalid — the poller's `validity::Invalid`, read
/// from the body's `snake_case` and crossing in camelCase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase", deserialize = "snake_case"))]
pub enum SlotProblem {
    Empty,
    Bye,
    Out,
    InjuredReserve,
}

/// A starting slot's occupant, as the expanded pane shows him.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase", deserialize = "snake_case"))]
pub struct LineupOccupant {
    pub player_key: String,
    pub name: String,
    pub bye_week: Option<u8>,
    /// The body's own spelling (`questionable`, `doubtful`, `out`,
    /// `injured_reserve`), passed through as an open string: nothing here
    /// branches on it — whether a slot is invalid is the body's `invalid`,
    /// decided poller-side — and a designation a later poller adds must not
    /// fail the whole read.
    pub designation: Option<String>,
    /// His game's kickoff this week; `None` on a bye.
    pub kickoff_ms: Option<i64>,
}

/// Why a slot is invalid, and the last moment it can be fixed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase", deserialize = "snake_case"))]
pub struct InvalidSlot {
    pub reason: SlotProblem,
    /// The replacement deadline, decided by the poller as of its fetch;
    /// `None` when nothing on the roster can fill the slot.
    pub deadline_ms: Option<i64>,
}

/// One starting slot, as the body carries it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase", deserialize = "snake_case"))]
pub struct LineupSlot {
    pub position: String,
    /// `None` is an empty slot.
    pub occupant: Option<LineupOccupant>,
    /// `None` is a valid slot.
    pub invalid: Option<InvalidSlot>,
}

/// The `yahoo-lineup/v1` body, as far as this pane reads it.
#[derive(Debug, Clone, PartialEq, Deserialize)]
struct LineupBody {
    team_key: String,
    team_name: String,
    league_key: String,
    league_name: String,
    week: u8,
    slots: Vec<LineupSlot>,
}

/// The lineup pane's headline, **as a decision**: which arm, and the
/// numbers each arm's words are set from. No sentence crosses (ADR-0025).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum LineupHeadline {
    /// Every starting slot is valid — "Lineup set — locks Sun 1:00pm".
    /// `locks_at_ms` is the next starter's kickoff still ahead, `None` once
    /// every starter's game has begun.
    Set { locks_at_ms: Option<i64> },
    /// Something is wrong — "2 empty slots — locks in 4h".
    Invalid {
        /// How many starting slots are invalid.
        count: u32,
        /// The reason every invalid slot shares, or `None` when they differ
        /// (a client then counts slots rather than naming a reason).
        reason: Option<SlotProblem>,
        /// The lineup's deadline: the earliest of its invalid slots'
        /// replacement deadlines. `None` when no slot has one.
        locks_at_ms: Option<i64>,
        /// The answer's age, set **only** when it is inside the crunch
        /// window ([`IMMINENT_MS`]) and older than
        /// [`CRUNCH_STALE_AFTER_MS`] — "last checked 3h ago". `None` means
        /// the answer reads as current.
        checked_ago_ms: Option<i64>,
    },
}

/// Everything one answered lineup pane needs, decided once and read by the
/// answer, the collapsed headline and the expanded per-slot detail.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LineupFacts {
    pub league_key: String,
    pub league_name: String,
    pub team_key: String,
    pub team_name: String,
    pub week: u8,
    /// Every starting slot, in the body's order.
    pub slots: Vec<LineupSlot>,
    pub headline: LineupHeadline,
    /// Older than [`STALE_AFTER_MS`] (or of unknown age) — the general
    /// caveat every pane carries, beside the headline's crunch-time one.
    pub stale: bool,
    pub freshness: FreshnessFact,
}

/// The whole answered fact set for one pane, or the reason there is none.
/// The waivers arm arrives with #719.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum FantasyResolved {
    Lineup(LineupFacts),
    Gap { gap: FantasyGap },
}

/// Reads one row's envelope: the body text, or the gap that stops short of
/// it.
fn read_envelope<'a>(snapshot: Option<&'a PaneSnapshotFacts>, source: &str) -> Result<&'a str, FantasyGap> {
    let Some(snapshot) = snapshot else {
        return Err(FantasyGap::NotFetched);
    };
    match &snapshot.envelope {
        PaneEnvelopeFacts::Malformed { reason } => Err(FantasyGap::Malformed { reason: reason.clone() }),
        PaneEnvelopeFacts::Ok { schema, .. } if schema != source => {
            Err(FantasyGap::UnknownSchema { schema: schema.clone() })
        }
        PaneEnvelopeFacts::Ok { body, .. } => Ok(body),
    }
}

/// The lineup's headline from its slots, as of `now_ms`.
fn lineup_headline(slots: &[LineupSlot], freshness: FreshnessFact, now_ms: i64) -> LineupHeadline {
    let invalid: Vec<&InvalidSlot> = slots.iter().filter_map(|slot| slot.invalid.as_ref()).collect();
    if invalid.is_empty() {
        let locks_at_ms = slots
            .iter()
            .filter_map(|slot| slot.occupant.as_ref()?.kickoff_ms)
            .filter(|kickoff| *kickoff > now_ms)
            .min();
        return LineupHeadline::Set { locks_at_ms };
    }
    let reason = invalid[0].reason;
    let shared = invalid.iter().all(|slot| slot.reason == reason);
    let locks_at_ms = invalid.iter().filter_map(|slot| slot.deadline_ms).min();
    let in_crunch = locks_at_ms.is_some_and(|lock| lock - now_ms <= IMMINENT_MS);
    // `Unknown` has no hours to name: the general `stale` flag says so instead.
    let checked_ago_ms = match freshness {
        FreshnessFact::Age { age_ms, .. } if in_crunch && age_ms > CRUNCH_STALE_AFTER_MS => Some(age_ms),
        _ => None,
    };
    LineupHeadline::Invalid {
        count: invalid.len() as u32,
        reason: shared.then_some(reason),
        locks_at_ms,
        checked_ago_ms,
    }
}

/// One league's lineup facts, or why there are none.
fn lineup_facts(league: &str, inputs: &PaneInputs) -> FantasyResolved {
    let snapshot = inputs.snapshot(LINEUP_SOURCE, league);
    let body = match read_envelope(snapshot, LINEUP_SOURCE) {
        Ok(body) => body,
        Err(gap) => return FantasyResolved::Gap { gap },
    };
    let Ok(body) = serde_json::from_str::<LineupBody>(body) else {
        return FantasyResolved::Gap { gap: FantasyGap::BadBody };
    };
    // `read_envelope` returning `Ok` guarantees the row exists.
    let freshness = snapshot.expect("a read body implies a snapshot row").freshness;
    FantasyResolved::Lineup(LineupFacts {
        headline: lineup_headline(&body.slots, freshness, inputs.now_ms),
        league_key: body.league_key,
        league_name: body.league_name,
        team_key: body.team_key,
        team_name: body.team_name,
        week: body.week,
        slots: body.slots,
        stale: freshness.is_stale_beyond(STALE_AFTER_MS),
        freshness,
    })
}

/// One pane's facts. The snapshot row is keyed by the **league key** under
/// the subject's own source; a subject key that is not this question's
/// reads as never fetched rather than failing.
pub fn fantasy_facts(subject_key: &str, inputs: &PaneInputs) -> FantasyResolved {
    match parse_subject_key(subject_key) {
        Some((league, FantasySubject::Lineup)) => lineup_facts(league, inputs),
        Some((league, FantasySubject::Waivers)) => {
            let gap = match read_envelope(inputs.snapshot(WAIVERS_SOURCE, league), WAIVERS_SOURCE) {
                Ok(_) => FantasyGap::BodyNotRead,
                Err(gap) => gap,
            };
            FantasyResolved::Gap { gap }
        }
        None => FantasyResolved::Gap { gap: FantasyGap::NotFetched },
    }
}

/// One league the operator has a team in this season.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase", deserialize = "snake_case"))]
pub struct DiscoveredLeague {
    /// The key to copy into the `yahoo-leagues` binding.
    pub league_key: String,
    pub league_name: String,
}

#[derive(Deserialize)]
struct DiscoveredBody {
    game_key: String,
    leagues: Vec<DiscoveredLeague>,
}

/// What the poller found for this season, for Settings to show beside the
/// `yahoo-leagues` field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum FantasyDiscovered {
    /// No discovered-leagues row (or the source has not been read on this
    /// device): nothing to show, which is not a fault.
    None,
    Leagues { game_key: String, leagues: Vec<DiscoveredLeague> },
    /// A row exists and could not be read as a league list.
    Unreadable,
}

/// The poller's discovered leagues — the second key under [`LINEUP_SOURCE`].
pub fn fantasy_discovered_leagues(inputs: &PaneInputs) -> FantasyDiscovered {
    let body = match read_envelope(inputs.snapshot(LINEUP_SOURCE, DISCOVERED_KEY), DISCOVERED_SCHEMA) {
        Ok(body) => body,
        Err(FantasyGap::NotFetched) => return FantasyDiscovered::None,
        Err(_) => return FantasyDiscovered::Unreadable,
    };
    match serde_json::from_str::<DiscoveredBody>(body) {
        Ok(DiscoveredBody { game_key, leagues }) => FantasyDiscovered::Leagues { game_key, leagues },
        Err(_) => FantasyDiscovered::Unreadable,
    }
}

/// The answer every arm with no data shares: `dormant`, with no instant to
/// order by, in whichever answer state the setup names.
fn no_data(inputs: &PaneInputs) -> PaneAnswerCore {
    let answer_state = match fantasy_setup(inputs) {
        FantasySetup::Unset => AnswerState::Unbound,
        // Unread or unusable is neither answered nor unbound; bound with no
        // readable row yet is the never-polled gap.
        FantasySetup::Unread | FantasySetup::Unusable | FantasySetup::Bound { .. } => {
            AnswerState::BoundButUnacquired
        }
    };
    PaneAnswerCore { answer_state, band: Band::Dormant, within_band: None }
}

/// The band a lineup headline earns, and the instant it orders by within
/// that band. #713's bands, near-binary as `waste`'s are:
///
///   * `dormant` — every starting slot is valid. Nothing to do.
///   * `imminent` — invalid, and the lock is inside [`IMMINENT_MS`] (or has
///     already passed: still invalid, and there is no later band to fall to).
///   * `near` — invalid with the lock further out, or with no deadline at
///     all (nothing on the roster can fill the slot) — still something to
///     do, with no clock to rank it by.
///
/// `live`, off a live lineup alert, is #720's.
pub fn lineup_band(headline: &LineupHeadline, now_ms: i64) -> (Band, Option<i64>) {
    match *headline {
        LineupHeadline::Set { locks_at_ms } => (Band::Dormant, locks_at_ms),
        LineupHeadline::Invalid { locks_at_ms: None, .. } => (Band::Near, None),
        LineupHeadline::Invalid { locks_at_ms: Some(lock), .. } if lock - now_ms <= IMMINENT_MS => {
            (Band::Imminent, Some(lock))
        }
        LineupHeadline::Invalid { locks_at_ms: Some(lock), .. } => (Band::Near, Some(lock)),
    }
}

/// The lineup pane's answer for one league: a no-data arm until a readable
/// row lands, then [`lineup_band`] over its headline.
pub fn fantasy_lineup_answer(league: &str, inputs: &PaneInputs) -> PaneAnswerCore {
    if !matches!(fantasy_setup(inputs), FantasySetup::Bound { .. }) {
        return no_data(inputs);
    }
    match lineup_facts(league, inputs) {
        FantasyResolved::Lineup(facts) => {
            let (band, within_band) = lineup_band(&facts.headline, inputs.now_ms);
            PaneAnswerCore { answer_state: AnswerState::Answered, band, within_band }
        }
        FantasyResolved::Gap { .. } => no_data(inputs),
    }
}

/// The waivers pane's answer for one league — its no-data arms only (#717).
/// The countdown bands (never `distant`) are #719's.
pub fn fantasy_waivers_answer(_league: &str, inputs: &PaneInputs) -> PaneAnswerCore {
    no_data(inputs)
}

/// This question's answer for the shell, dispatched on the subject key.
pub fn fantasy_answer(subject_key: &str, inputs: &PaneInputs) -> PaneAnswerCore {
    match parse_subject_key(subject_key) {
        Some((league, FantasySubject::Lineup)) => fantasy_lineup_answer(league, inputs),
        Some((league, FantasySubject::Waivers)) => fantasy_waivers_answer(league, inputs),
        None => no_data(inputs),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inputs(bindings: serde_json::Value, pane_reads: serde_json::Value) -> PaneInputs {
        serde_json::from_value(serde_json::json!({
            "nowMs": 1_790_000_000_000i64,
            "bindings": bindings,
            "paneReads": pane_reads,
        }))
        .unwrap()
    }

    fn bound(text: &str) -> PaneInputs {
        inputs(
            serde_json::json!([{"key": BINDING_KEY, "value": {"state":"text","text":text}}]),
            serde_json::json!({}),
        )
    }

    // ----------------------------------------------------- the league list

    #[test]
    fn one_league_key_reads_as_one_league() {
        assert_eq!(leagues_from_binding("449.l.123456"), vec!["449.l.123456"]);
    }

    #[test]
    fn two_league_keys_read_in_binding_order() {
        assert_eq!(
            leagues_from_binding("449.l.123456,449.l.789012"),
            vec!["449.l.123456", "449.l.789012"],
        );
    }

    #[test]
    fn whitespace_around_keys_is_trimmed() {
        assert_eq!(
            leagues_from_binding("  449.l.123456 ,\t449.l.789012  "),
            vec!["449.l.123456", "449.l.789012"],
        );
    }

    #[test]
    fn a_trailing_comma_and_empty_entries_are_skipped() {
        assert_eq!(leagues_from_binding("449.l.123456,"), vec!["449.l.123456"]);
        assert_eq!(leagues_from_binding(",449.l.123456,,"), vec!["449.l.123456"]);
    }

    #[test]
    fn a_repeated_key_is_one_league() {
        assert_eq!(leagues_from_binding("449.l.1, 449.l.1"), vec!["449.l.1"]);
    }

    #[test]
    fn a_blanked_row_reads_as_unset() {
        for blank in ["", "   ", " , ,"] {
            assert_eq!(fantasy_setup(&bound(blank)), FantasySetup::Unset, "{blank:?}");
        }
    }

    #[test]
    fn an_other_row_is_unusable_rather_than_a_crash() {
        // A JSON array lands as `BindingValue::Other` — bound, unacquired.
        let unusable = inputs(
            serde_json::json!([{"key": BINDING_KEY, "value": {"state":"other"}}]),
            serde_json::json!({}),
        );
        assert_eq!(fantasy_setup(&unusable), FantasySetup::Unusable);
        assert_eq!(fantasy_subjects(&unusable), vec!["setup:lineup", "setup:waivers"]);
        for subject in fantasy_subjects(&unusable) {
            let answer = fantasy_answer(&subject, &unusable);
            assert_eq!(answer.answer_state, AnswerState::BoundButUnacquired);
            assert_eq!(answer.band, Band::Dormant);
        }
    }

    #[test]
    fn an_unread_table_is_not_the_setup_prompt() {
        let unread: PaneInputs = serde_json::from_value(serde_json::json!({"nowMs": 0})).unwrap();
        assert_eq!(fantasy_setup(&unread), FantasySetup::Unread);
        assert_eq!(fantasy_answer("setup:lineup", &unread).answer_state, AnswerState::BoundButUnacquired);
    }

    // --------------------------------------------------------- the subjects

    #[test]
    fn unset_emits_both_setup_subjects_and_both_read_unbound() {
        let unset = inputs(serde_json::json!([]), serde_json::json!({}));
        let subjects = fantasy_subjects(&unset);
        assert_eq!(subjects, vec!["setup:lineup", "setup:waivers"]);
        for subject in &subjects {
            let answer = fantasy_answer(subject, &unset);
            assert_eq!(answer.answer_state, AnswerState::Unbound, "{subject}");
            assert_eq!(answer.band, Band::Dormant);
            assert_eq!(answer.within_band, None);
        }
    }

    #[test]
    fn one_league_emits_two_never_polled_panes() {
        let one = bound("449.l.123456");
        let subjects = fantasy_subjects(&one);
        assert_eq!(subjects, vec!["449.l.123456:lineup", "449.l.123456:waivers"]);
        for subject in &subjects {
            let answer = fantasy_answer(subject, &one);
            assert_eq!(answer.answer_state, AnswerState::BoundButUnacquired, "{subject}");
            assert_eq!(answer.band, Band::Dormant);
            assert_eq!(
                fantasy_facts(subject, &one),
                FantasyResolved::Gap { gap: FantasyGap::NotFetched },
            );
        }
    }

    #[test]
    fn two_leagues_emit_four_panes_league_by_league() {
        assert_eq!(
            fantasy_subjects(&bound("449.l.1,449.l.2")),
            vec!["449.l.1:lineup", "449.l.1:waivers", "449.l.2:lineup", "449.l.2:waivers"],
        );
    }

    #[test]
    fn a_subject_key_round_trips_and_a_foreign_one_does_not_parse() {
        for subject in FantasySubject::BOTH {
            let key = subject_key("449.l.123456", subject);
            assert_eq!(parse_subject_key(&key), Some(("449.l.123456", subject)));
        }
        for foreign in ["449.l.1", "449.l.1:roster", ":lineup", ""] {
            assert_eq!(parse_subject_key(foreign), None, "{foreign:?}");
        }
    }

    // ------------------------------------------------------ a row present

    fn with_row(source: &str, envelope: serde_json::Value) -> PaneInputs {
        inputs(
            serde_json::json!([{"key": BINDING_KEY, "value": {"state":"text","text":"449.l.1"}}]),
            serde_json::json!({ source: { "snapshots": [{
                "key": "449.l.1",
                "envelope": envelope,
                "freshness": {"kind":"age","ageMs":60000,"declaredCadenceMs": 21_600_000},
            }], "liveAlerts": [] } }),
        )
    }

    #[test]
    fn each_subject_reads_its_own_source_keyed_by_league() {
        let lineup_row = with_row(LINEUP_SOURCE, serde_json::json!({"kind":"ok","schema":LINEUP_SOURCE,"body":"{}"}));
        // `{}` is not a lineup: a gap of its own, never an invented answer.
        assert_eq!(
            fantasy_facts("449.l.1:lineup", &lineup_row),
            FantasyResolved::Gap { gap: FantasyGap::BadBody },
        );
        assert_eq!(
            fantasy_answer("449.l.1:lineup", &lineup_row).answer_state,
            AnswerState::BoundButUnacquired,
        );
        // The waivers pane does not read the lineup's row.
        assert_eq!(
            fantasy_facts("449.l.1:waivers", &lineup_row),
            FantasyResolved::Gap { gap: FantasyGap::NotFetched },
        );
        // A well-formed waivers row is still unread until #719 pins it.
        let waivers_row = with_row(WAIVERS_SOURCE, serde_json::json!({"kind":"ok","schema":WAIVERS_SOURCE,"body":"{}"}));
        assert_eq!(
            fantasy_facts("449.l.1:waivers", &waivers_row),
            FantasyResolved::Gap { gap: FantasyGap::BodyNotRead },
        );
    }

    // ------------------------------------- the lineup, over the golden files

    /// The poller's own golden files — the bytes `server/fantasy-poll`'s
    /// `tests/golden.rs` pins its output to — so a body change there that
    /// this reader cannot follow fails here too.
    const GOLDEN_INVALID: &str =
        include_str!("../../../../../server/fantasy-poll/tests/fixtures/golden-lineup.json");
    const GOLDEN_VALID: &str =
        include_str!("../../../../../server/fantasy-poll/tests/fixtures/golden-lineup-valid.json");
    const GOLDEN_DISCOVERED: &str =
        include_str!("../../../../../server/fantasy-poll/tests/fixtures/golden-discovered.json");

    const MINUTE_MS: i64 = 60 * 1000;
    /// The fixtures' fetch instant, Tue 2026-09-29 15:00Z.
    const AS_OF: i64 = 1_790_694_000_000;
    /// The invalid golden's earliest replacement deadline (a bench player's
    /// Thursday-night game) — and the valid golden's first starter kickoff.
    const THURSDAY: i64 = 1_790_900_100_000;
    /// The Sunday early kickoff.
    const SUNDAY: i64 = 1_791_133_200_000;
    const LEAGUE: &str = "449.l.123456";

    /// The golden envelope's body, rewritten by `edit` — for the arms the
    /// two files do not carry as-is.
    fn golden_body(golden: &str, edit: impl FnOnce(&mut serde_json::Value)) -> String {
        let mut envelope: serde_json::Value = serde_json::from_str(golden).unwrap();
        edit(&mut envelope["body"]);
        envelope["body"].to_string()
    }

    fn lineup_inputs(now_ms: i64, body: &str, age_ms: i64) -> PaneInputs {
        serde_json::from_value(serde_json::json!({
            "nowMs": now_ms,
            "bindings": [{"key": BINDING_KEY, "value": {"state":"text","text":LEAGUE}}],
            "paneReads": { LINEUP_SOURCE: { "snapshots": [{
                "key": LEAGUE,
                "envelope": {"kind":"ok","schema":LINEUP_SOURCE,"body":body},
                "freshness": {"kind":"age","ageMs":age_ms,"declaredCadenceMs": 21_600_000},
            }], "liveAlerts": [] } },
        }))
        .unwrap()
    }

    fn lineup_of(inputs: &PaneInputs) -> LineupFacts {
        match fantasy_facts(&subject_key(LEAGUE, FantasySubject::Lineup), inputs) {
            FantasyResolved::Lineup(facts) => facts,
            other => panic!("expected lineup facts, got {other:?}"),
        }
    }

    fn lineup_answer(inputs: &PaneInputs) -> PaneAnswerCore {
        fantasy_answer(&subject_key(LEAGUE, FantasySubject::Lineup), inputs)
    }

    #[test]
    fn the_valid_golden_is_dormant_and_locks_at_the_first_starter_kickoff() {
        let inputs = lineup_inputs(AS_OF, &golden_body(GOLDEN_VALID, |_| {}), MINUTE_MS);
        let facts = lineup_of(&inputs);
        assert_eq!(facts.headline, LineupHeadline::Set { locks_at_ms: Some(THURSDAY) });
        assert_eq!(facts.league_name, "Twinion Family League");
        assert_eq!(facts.slots.len(), 8);
        assert_eq!(
            lineup_answer(&inputs),
            PaneAnswerCore { answer_state: AnswerState::Answered, band: Band::Dormant, within_band: Some(THURSDAY) },
        );
    }

    #[test]
    fn a_valid_lineup_locks_next_at_the_first_kickoff_still_ahead() {
        let inputs = lineup_inputs(THURSDAY + MINUTE_MS, &golden_body(GOLDEN_VALID, |_| {}), MINUTE_MS);
        assert_eq!(lineup_of(&inputs).headline, LineupHeadline::Set { locks_at_ms: Some(SUNDAY) });
        // Once every starter's game has begun there is no lock left to name.
        let after = lineup_inputs(SUNDAY + 30 * IMMINENT_MS, &golden_body(GOLDEN_VALID, |_| {}), MINUTE_MS);
        assert_eq!(lineup_of(&after).headline, LineupHeadline::Set { locks_at_ms: None });
        assert_eq!(lineup_answer(&after).band, Band::Dormant);
    }

    #[test]
    fn the_invalid_golden_more_than_a_day_from_its_lock_is_near() {
        let inputs = lineup_inputs(THURSDAY - IMMINENT_MS - MINUTE_MS, &golden_body(GOLDEN_INVALID, |_| {}), MINUTE_MS);
        assert_eq!(
            lineup_of(&inputs).headline,
            // Three slots, three different reasons: counted, not named.
            LineupHeadline::Invalid { count: 3, reason: None, locks_at_ms: Some(THURSDAY), checked_ago_ms: None },
        );
        assert_eq!(
            lineup_answer(&inputs),
            PaneAnswerCore { answer_state: AnswerState::Answered, band: Band::Near, within_band: Some(THURSDAY) },
        );
    }

    #[test]
    fn the_invalid_golden_within_a_day_of_its_lock_is_imminent() {
        let inputs = lineup_inputs(THURSDAY - 4 * HOUR_MS, &golden_body(GOLDEN_INVALID, |_| {}), 10 * MINUTE_MS);
        assert_eq!(
            lineup_answer(&inputs),
            PaneAnswerCore { answer_state: AnswerState::Answered, band: Band::Imminent, within_band: Some(THURSDAY) },
        );
        // Ten minutes old in crunch time reads as current.
        assert!(matches!(lineup_of(&inputs).headline, LineupHeadline::Invalid { checked_ago_ms: None, .. }));
        // A lock already passed is still invalid, and still the loudest band.
        let passed = lineup_inputs(THURSDAY + MINUTE_MS, &golden_body(GOLDEN_INVALID, |_| {}), MINUTE_MS);
        assert_eq!(lineup_answer(&passed).band, Band::Imminent);
    }

    #[test]
    fn one_shared_reason_is_named_with_its_count() {
        // Only the empty RB left invalid.
        let body = golden_body(GOLDEN_INVALID, |body| {
            for slot in body["slots"].as_array_mut().unwrap() {
                if slot["invalid"]["reason"] != "empty" {
                    slot["invalid"] = serde_json::Value::Null;
                }
            }
        });
        let inputs = lineup_inputs(AS_OF, &body, MINUTE_MS);
        assert_eq!(
            lineup_of(&inputs).headline,
            LineupHeadline::Invalid {
                count: 1,
                reason: Some(SlotProblem::Empty),
                locks_at_ms: Some(SUNDAY),
                checked_ago_ms: None,
            },
        );
    }

    #[test]
    fn an_invalid_slot_with_no_deadline_is_near_with_nothing_to_order_by() {
        let body = golden_body(GOLDEN_INVALID, |body| {
            for slot in body["slots"].as_array_mut().unwrap() {
                if slot["invalid"].is_object() {
                    slot["invalid"]["deadline_ms"] = serde_json::Value::Null;
                }
            }
        });
        let inputs = lineup_inputs(AS_OF, &body, MINUTE_MS);
        assert!(matches!(lineup_of(&inputs).headline, LineupHeadline::Invalid { locks_at_ms: None, .. }));
        assert_eq!(
            lineup_answer(&inputs),
            PaneAnswerCore { answer_state: AnswerState::Answered, band: Band::Near, within_band: None },
        );
    }

    // ------------------------------------------- the lineup's own freshness

    #[test]
    fn an_hours_old_invalid_answer_in_crunch_time_says_how_old_it_is() {
        let three_hours = 3 * HOUR_MS;
        let inputs = lineup_inputs(THURSDAY - 4 * HOUR_MS, &golden_body(GOLDEN_INVALID, |_| {}), three_hours);
        assert_eq!(
            lineup_of(&inputs).headline,
            LineupHeadline::Invalid {
                count: 3,
                reason: None,
                locks_at_ms: Some(THURSDAY),
                checked_ago_ms: Some(three_hours),
            },
        );
        // Not past the general 12h rule: only the crunch judgement fires.
        assert!(!lineup_of(&inputs).stale);
    }

    #[test]
    fn the_crunch_judgement_is_only_for_an_invalid_lineup_inside_a_day() {
        let three_hours = 3 * HOUR_MS;
        // The same age, more than a day out: a six-hourly poll is on time.
        let far = lineup_inputs(THURSDAY - 2 * IMMINENT_MS, &golden_body(GOLDEN_INVALID, |_| {}), three_hours);
        assert!(matches!(lineup_of(&far).headline, LineupHeadline::Invalid { checked_ago_ms: None, .. }));
        // An hour exactly is not yet worth saying.
        let edge = lineup_inputs(THURSDAY - HOUR_MS, &golden_body(GOLDEN_INVALID, |_| {}), CRUNCH_STALE_AFTER_MS);
        assert!(matches!(lineup_of(&edge).headline, LineupHeadline::Invalid { checked_ago_ms: None, .. }));
    }

    #[test]
    fn the_general_stale_flag_is_twice_the_declared_cadence() {
        let body = golden_body(GOLDEN_VALID, |_| {});
        assert!(!lineup_of(&lineup_inputs(AS_OF, &body, STALE_AFTER_MS)).stale);
        assert!(lineup_of(&lineup_inputs(AS_OF, &body, STALE_AFTER_MS + 1)).stale);
    }

    #[test]
    fn the_headline_and_slots_cross_as_data() {
        let inputs = lineup_inputs(THURSDAY - 4 * HOUR_MS, &golden_body(GOLDEN_INVALID, |_| {}), 3 * HOUR_MS);
        let crossed = serde_json::to_value(fantasy_facts("449.l.123456:lineup", &inputs)).unwrap();
        assert_eq!(crossed["kind"], "lineup");
        assert_eq!(
            crossed["headline"],
            serde_json::json!({"kind":"invalid","count":3,"reason":null,
                "locksAtMs":THURSDAY,"checkedAgoMs":3 * HOUR_MS}),
        );
        assert_eq!(
            crossed["slots"][5],
            serde_json::json!({"position":"W/R/T","occupant":{"playerKey":"449.p.31050","name":"Owen Sidelined",
                "byeWeek":12,"designation":"out","kickoffMs":SUNDAY},
                "invalid":{"reason":"out","deadlineMs":THURSDAY}}),
        );
        assert_eq!(
            serde_json::to_value(SlotProblem::InjuredReserve).unwrap(),
            serde_json::json!("injuredReserve"),
        );
    }

    // ------------------------------------------------- discovered leagues

    #[test]
    fn the_discovered_leagues_read_off_the_golden_row() {
        let golden: serde_json::Value = serde_json::from_str(GOLDEN_DISCOVERED).unwrap();
        let row = |schema: &str, body: String| -> PaneInputs {
            serde_json::from_value(serde_json::json!({
                "nowMs": AS_OF,
                "paneReads": { LINEUP_SOURCE: { "snapshots": [{
                    "key": DISCOVERED_KEY,
                    "envelope": {"kind":"ok","schema":schema,"body":body},
                    "freshness": {"kind":"age","ageMs":0,"declaredCadenceMs": 21_600_000},
                }], "liveAlerts": [] } },
            }))
            .unwrap()
        };
        assert_eq!(golden["schema"], DISCOVERED_SCHEMA);
        assert_eq!(
            fantasy_discovered_leagues(&row(DISCOVERED_SCHEMA, golden["body"].to_string())),
            FantasyDiscovered::Leagues {
                game_key: "449".into(),
                leagues: vec![
                    DiscoveredLeague { league_key: "449.l.123456".into(), league_name: "Twinion Family League".into() },
                    DiscoveredLeague { league_key: "449.l.789012".into(), league_name: "Office Pool".into() },
                ],
            },
        );
        assert_eq!(
            fantasy_discovered_leagues(&row(LINEUP_SOURCE, golden["body"].to_string())),
            FantasyDiscovered::Unreadable,
        );
        assert_eq!(fantasy_discovered_leagues(&row(DISCOVERED_SCHEMA, "[]".into())), FantasyDiscovered::Unreadable);
        assert_eq!(fantasy_discovered_leagues(&bound(LEAGUE)), FantasyDiscovered::None);
        // And the row never answers as a league's lineup.
        assert!(!DISCOVERED_KEY.contains(".l."));
    }

    #[test]
    fn a_broken_or_foreign_envelope_is_named_as_its_own_gap() {
        let malformed = with_row(WAIVERS_SOURCE, serde_json::json!({"kind":"malformed","reason":"no `schema`"}));
        assert_eq!(
            fantasy_facts("449.l.1:waivers", &malformed),
            FantasyResolved::Gap { gap: FantasyGap::Malformed { reason: "no `schema`".into() } },
        );
        let newer = with_row(WAIVERS_SOURCE, serde_json::json!({"kind":"ok","schema":"yahoo-waivers/v2","body":"{}"}));
        assert_eq!(
            fantasy_facts("449.l.1:waivers", &newer),
            FantasyResolved::Gap { gap: FantasyGap::UnknownSchema { schema: "yahoo-waivers/v2".into() } },
        );
    }

    #[test]
    fn the_binding_key_is_the_one_the_vocabulary_writes() {
        assert_eq!(BINDING_KEY, crate::bindings::BindingKey::Fantasy.as_str());
    }

    #[test]
    fn no_data_arm_asks_the_zone_bridge_for_anything() {
        assert!(fantasy_zone_queries(&bound("449.l.1")).is_empty());
    }

    #[test]
    fn resolved_crosses_as_a_kind_and_a_gap() {
        assert_eq!(
            serde_json::to_value(FantasyResolved::Gap { gap: FantasyGap::NotFetched }).unwrap(),
            serde_json::json!({"kind":"gap","gap":{"gap":"notFetched"}}),
        );
    }
}
