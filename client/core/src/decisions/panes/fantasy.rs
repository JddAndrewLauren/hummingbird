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
//! # What this slice decides, and what it leaves
//!
//! **Nothing here talks to Yahoo, and nothing has written a row yet.** This
//! module answers the two states a real device is in on day one: binding
//! unset ([`AnswerState::Unbound`], with the setup prompt, so the question
//! is discoverable) and bound but never polled
//! ([`AnswerState::BoundButUnacquired`]). Both are `dormant`. A row that
//! *does* exist is read only as far as its envelope: its body shape
//! belongs to the poller (#718 for the lineup, #719 for the waivers) and is
//! not guessed at here, so a present, well-formed row answers
//! [`FantasyGap::BodyNotRead`] until those slices pin it — still a gap,
//! never an invented answer.
//!
//! Neither no-data arm is civil-date reasoning, so [`fantasy_zone_queries`]
//! asks the bridge for nothing yet. The lock and the waiver deadline are,
//! and when their bands land they ask here, through the zone bridge —
//! never by subtracting milliseconds.
//!
//! # The binding
//!
//! `yahoo-leagues` holds **comma-separated league keys inside the JSON
//! string** (`"449.l.123456"`, later `"449.l.123456,449.l.789012"`). A JSON
//! array would land as `BindingValue::Other`, which the binding editor
//! cannot write — `server/race-poll/src/binding.rs`'s header says why at
//! length, and [`leagues_from_binding`] mirrors its parse.

use serde::{Deserialize, Serialize};

use super::contract::{AnswerState, Band, PaneAnswerCore};
use super::inputs::{BindingValueFact, PaneEnvelopeFacts, PaneInputs, PaneSnapshotFacts};
use super::zone::ZoneQuery;

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
/// trimmed, blanks dropped, repeats dropped, order kept.
///
/// `race-poll/src/binding.rs`'s reading, less its lowercasing: a race
/// series is a name a human types in any case, a league key is an
/// identifier copied from Yahoo and is taken exactly as written.
pub fn leagues_from_binding(text: &str) -> Vec<String> {
    let mut leagues: Vec<String> = Vec::new();
    for entry in text.split(',') {
        let key = entry.trim();
        if key.is_empty() || leagues.iter().any(|league| league == key) {
            continue;
        }
        leagues.push(key.to_string());
    }
    leagues
}

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

/// Every `(zone, civil-date)` fact this question needs — **none yet**. Both
/// no-data arms are decided without a date; see the module header for where
/// the first query will come from.
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
    /// source yet: the body's shape is the poller's to pin (#718/#719).
    BodyNotRead,
}

/// The whole answered fact set for one pane, or the reason there is none.
///
/// **Only the gap arm exists today** — the lineup and waivers fact sets
/// arrive with the slices that pin their bodies. It is an enum already so
/// that the seams, and every client switching on `kind`, need no reshaping
/// when they do.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum FantasyResolved {
    Gap { gap: FantasyGap },
}

/// Reads one row's envelope, and stops there — see [`FantasyGap::BodyNotRead`].
fn read_envelope(snapshot: Option<&PaneSnapshotFacts>, source: &str) -> FantasyGap {
    let Some(snapshot) = snapshot else {
        return FantasyGap::NotFetched;
    };
    match &snapshot.envelope {
        PaneEnvelopeFacts::Malformed { reason } => FantasyGap::Malformed { reason: reason.clone() },
        PaneEnvelopeFacts::Ok { schema, .. } if schema != source => {
            FantasyGap::UnknownSchema { schema: schema.clone() }
        }
        PaneEnvelopeFacts::Ok { .. } => FantasyGap::BodyNotRead,
    }
}

/// One pane's facts. The snapshot row is keyed by the **league key** under
/// the subject's own source; a subject key that is not this question's
/// reads as never fetched rather than failing.
pub fn fantasy_facts(subject_key: &str, inputs: &PaneInputs) -> FantasyResolved {
    let gap = match parse_subject_key(subject_key) {
        Some((league, subject)) => {
            read_envelope(inputs.snapshot(subject.source(), league), subject.source())
        }
        None => FantasyGap::NotFetched,
    };
    FantasyResolved::Gap { gap }
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

/// The lineup pane's answer for one league — its no-data arms only (#717).
/// The real bands (valid → dormant, invalid → near/imminent/live) are #718's.
pub fn fantasy_lineup_answer(_league: &str, inputs: &PaneInputs) -> PaneAnswerCore {
    no_data(inputs)
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
        assert_eq!(
            fantasy_facts("449.l.1:lineup", &lineup_row),
            FantasyResolved::Gap { gap: FantasyGap::BodyNotRead },
        );
        // The waivers pane does not read the lineup's row.
        assert_eq!(
            fantasy_facts("449.l.1:waivers", &lineup_row),
            FantasyResolved::Gap { gap: FantasyGap::NotFetched },
        );
        // And a present row is still a gap, never an invented answer.
        assert_eq!(
            fantasy_answer("449.l.1:lineup", &lineup_row).answer_state,
            AnswerState::BoundButUnacquired,
        );
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
