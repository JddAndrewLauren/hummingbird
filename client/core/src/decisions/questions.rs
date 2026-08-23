//! The **standing-question roster** (ADR-0034 decision 4, #714): every
//! question this build asks, named once — the question itself, its
//! operator-facing label, the surface it renders into, and the
//! [`BindingKey`]s that answer it.
//!
//! # Why this is a decision and not a rendering
//!
//! The question→binding relation already existed before this module, in the
//! wrong place and in the wrong language: `SettingsScreen.tsx`'s calendar
//! hint read *"Polled because it answers **How long to the next vacation**.
//! Change it under Standing questions"* — `trips-calendar → Vacation`
//! hand-written as English in one client. Two clients disagreeing about
//! which binding answers which question is a bug, so ADR-0025 puts the
//! relation here and leaves only its wording on screen per client.
//!
//! # Why it is derived, never maintained beside `SUNK`
//!
//! ADR-0034 decision 4 names this as the part not to trade away for speed:
//! if the roster were a per-client table, an eleventh question would mean
//! editing [`SUNK`] *and* remembering a file in another language, and the
//! failure mode is a question that polls, rings, and is invisible in
//! Settings — the ADR's own premise collapsing one slice after it was
//! accepted.
//!
//! So the surface comes straight out of [`SUNK`], the order straight out of
//! [`QUESTION_ORDER`], and the two per-question facts this module *does*
//! own — the label and the bindings — are wildcard-free `match`es over
//! [`StandingQuestion`]. **Adding a variant to that enum fails to compile
//! until both matches cover it**, which is the exhaustiveness gate in its
//! cheapest form: a test can be forgotten, a `match` arm cannot.
//!
//! # What that gate reaches, and the one thing it does not
//!
//! Mutation-checked, not assumed (#714). An eleventh variant fails to
//! compile in four places before any test runs: [`question_label`],
//! [`question_bindings`], `StandingQuestion::as_str` and
//! `panes::rank_panes`'s own match — plus `map_standing_question` on the
//! mobile seam. Every one of those sits in a file that also holds
//! [`SUNK`] or [`QUESTION_ORDER`], so an author adding a question is
//! looking straight at both arrays.
//!
//! Divergence *between* the two arrays is caught by this module's tests:
//! [`question_roster`] walks [`QUESTION_ORDER`] and takes each surface from
//! [`SUNK`], so a question in one and not the other changes the roster's
//! length, which `the_roster_is_in_question_order` and
//! `the_roster_covers_exactly_the_sunk_questions` compare in both
//! directions.
//!
//! **What no test here reaches is a variant added to the enum and to every
//! forced `match` but to neither array.** Rust cannot enumerate an enum's
//! variants without a derive (`strum::EnumIter` or similar), so a test that
//! iterates one of the arrays to prove the array is complete is checking
//! itself — a slot-index `match` written for exactly this was tried, and a
//! mutation showed it passing with the eleventh variant missing from both
//! arrays. It was deleted rather than kept. Closing this properly is a
//! dependency decision, not a test; the compile errors above are what
//! stands in the meantime.
//!
//! # What is deliberately not here
//!
//! The **enabled flag**. ADR-0034's toggle is #715, and it lands as its own
//! typed `settings` row per question (decision 2), read with the
//! absence-means-enabled and CAS contracts that slice carries. A boolean
//! added here ahead of it would get those wrong.

use serde::Serialize;

use crate::bindings::BindingKey;

use super::panes::contract::{StandingQuestion, Surface, QUESTION_ORDER};
use super::panes::SUNK;

/// One question, as Settings lists it.
///
/// Carries the **enums**, not their spellings: the mobile seam maps them
/// onto its own `uniffi` mirrors, and every one of them already serializes
/// as its wire string, so the web sees exactly the kebab-case JSON it would
/// have got from a `String` field. Serialize-only — nothing reads a roster
/// back in, and a borrowed `label` says so in the type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionRosterEntry {
    pub question: StandingQuestion,
    pub label: &'static str,
    pub surface: Surface,
    /// The keys that answer this question, in [`BindingKey::ALL`]'s
    /// declaration order. Empty for most questions — a question with no
    /// binding is a question nobody has to configure, not a question
    /// missing something.
    pub bindings: Vec<BindingKey>,
}

/// The operator-facing name of one question.
///
/// **The web registry's wording, taken as canonical** (#714), **with one
/// exception**: these strings were `QuestionDef.label` in
/// `client/web/src/screens/questions/`, and the web now reads them back
/// from here rather than declaring its own. `Reachability` was *This
/// device* there and is *Is this device reachable* here — the roster made
/// it a heading on Settings, directly above that screen's own **This
/// device** section, and one screen must not say the same phrase twice
/// about two different things. The new wording also matches the
/// interrogative register the Now questions already use, and the tile it
/// names on the Status board is the one whose headline carries no subject
/// of its own (`tile-copy.ts`: a separator-less headline keeps the
/// question's label as its name), so the label is what that tile reads.
/// Android
/// still spells its own shorter Now-pane labels (`NowScreen.kt`'s
/// `nowPaneLabel`) and keeps them until #716 renders this roster there — a
/// divergence ADR-0034 decision 4 enters on purpose, because that surface
/// owes a device run.
///
/// Wildcard-free by design; see the module header.
pub fn question_label(question: StandingQuestion) -> &'static str {
    match question {
        StandingQuestion::Homework => "What's my homework",
        StandingQuestion::Scps => "Next SCPS event",
        StandingQuestion::Waste => "Which cans go out",
        StandingQuestion::Weekend => "This weekend",
        StandingQuestion::Vacation => "Next vacation",
        StandingQuestion::Race => "When is the next race",
        StandingQuestion::Kimi => "Kimi balance",
        StandingQuestion::Github => "GitHub workflows",
        StandingQuestion::Uptime => "Uptime",
        StandingQuestion::Reachability => "Is this device reachable",
    }
}

/// The bindings that answer one question — the relation this module exists
/// to hold.
///
/// Most questions bind nothing: the status four and `weekend` read sources
/// nobody chose, and `homework` binds only a display affordance
/// ([`BindingKey::HomeworkLink`], which names no source — see its own doc).
/// The empty arms are therefore the normal case, not an omission.
///
/// Wildcard-free by design; see the module header.
pub fn question_bindings(question: StandingQuestion) -> &'static [BindingKey] {
    match question {
        StandingQuestion::Homework => &[BindingKey::HomeworkLink],
        StandingQuestion::Scps => &[BindingKey::ScpsQuest],
        StandingQuestion::Waste => &[BindingKey::CityWastePage],
        StandingQuestion::Weekend => &[],
        StandingQuestion::Vacation => &[BindingKey::TripsCalendar],
        StandingQuestion::Race => &[BindingKey::RaceSeries],
        StandingQuestion::Kimi => &[],
        StandingQuestion::Github => &[],
        StandingQuestion::Uptime => &[],
        StandingQuestion::Reachability => &[],
    }
}

/// Which surface a question renders into, read out of [`SUNK`] rather than
/// restated — the ADR's "derived, never maintained beside" in one line.
///
/// `None` is a question in the vocabulary that this build does not actually
/// render anywhere. No such question exists today and
/// [`the_roster_covers_exactly_the_sunk_questions`] pins that, but the
/// signature says so honestly instead of panicking on a state the type
/// system permits.
pub fn question_surface(question: StandingQuestion) -> Option<Surface> {
    SUNK.iter()
        .find(|(sunk, _)| *sunk == question)
        .map(|(_, surface)| *surface)
}

/// Every question Settings lists, in [`QUESTION_ORDER`].
///
/// A question with no bindings is present with an empty `bindings` — the
/// roster is the one place an unbound and an off question can be seen at
/// all (ADR-0034's consequences), so omitting it would be exactly the
/// invisible-question failure the ADR is guarding against.
pub fn question_roster() -> Vec<QuestionRosterEntry> {
    QUESTION_ORDER
        .iter()
        .filter_map(|question| {
            let surface = question_surface(*question)?;
            Some(QuestionRosterEntry {
                question: *question,
                label: question_label(*question),
                surface,
                bindings: question_bindings(*question).to_vec(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_roster_covers_exactly_the_sunk_questions() {
        // Both directions. A question in `SUNK` missing from the roster is
        // a question that polls and rings while being invisible in
        // Settings; a roster entry with no `SUNK` row is a question listed
        // on a surface that never renders it.
        let roster = question_roster();
        let listed: Vec<&str> = roster
            .iter()
            .map(|entry| entry.question.as_str())
            .collect();
        let sunk: Vec<&str> = SUNK.iter().map(|(question, _)| question.as_str()).collect();
        for question in &sunk {
            assert!(listed.contains(question), "{question} is sunk but not listed");
        }
        for question in &listed {
            assert!(sunk.contains(question), "{question} is listed but not sunk");
        }
    }

    #[test]
    fn the_roster_is_in_question_order() {
        let listed: Vec<StandingQuestion> = question_roster()
            .into_iter()
            .map(|entry| entry.question)
            .collect();
        assert_eq!(listed, QUESTION_ORDER.to_vec());
    }

    #[test]
    fn a_surface_is_read_from_sunk_and_never_restated() {
        for (question, surface) in SUNK {
            assert_eq!(question_surface(question), Some(surface), "{question:?}");
        }
        let roster = question_roster();
        let now: Vec<&str> = roster
            .iter()
            .filter(|entry| entry.surface == Surface::Now)
            .map(|entry| entry.question.as_str())
            .collect();
        assert_eq!(now, ["homework", "scps", "waste", "weekend", "vacation", "race"]);
        let status: Vec<&str> = roster
            .iter()
            .filter(|entry| entry.surface == Surface::Status)
            .map(|entry| entry.question.as_str())
            .collect();
        assert_eq!(status, ["kimi", "github", "uptime", "reachability"]);
    }

    #[test]
    fn the_five_bound_questions_carry_their_key_and_the_rest_carry_none() {
        let bound: Vec<(StandingQuestion, Vec<BindingKey>)> = question_roster()
            .into_iter()
            .filter(|entry| !entry.bindings.is_empty())
            .map(|entry| (entry.question, entry.bindings))
            .collect();
        assert_eq!(
            bound,
            vec![
                (StandingQuestion::Homework, vec![BindingKey::HomeworkLink]),
                (StandingQuestion::Scps, vec![BindingKey::ScpsQuest]),
                (StandingQuestion::Waste, vec![BindingKey::CityWastePage]),
                (StandingQuestion::Vacation, vec![BindingKey::TripsCalendar]),
                (StandingQuestion::Race, vec![BindingKey::RaceSeries]),
            ]
        );
    }

    #[test]
    fn every_binding_key_is_claimed_by_exactly_one_question() {
        // The other direction of the same relation: a key nothing claims
        // would render in Settings under no question at all, which is the
        // flat list this slice replaced.
        for key in BindingKey::ALL {
            let claimants: Vec<StandingQuestion> = QUESTION_ORDER
                .iter()
                .copied()
                .filter(|question| question_bindings(*question).contains(key))
                .collect();
            assert_eq!(claimants.len(), 1, "{} is claimed by {claimants:?}", key.as_str());
        }
    }

    #[test]
    fn every_label_is_present_and_distinct() {
        let mut labels: Vec<&str> = QUESTION_ORDER.iter().map(|q| question_label(*q)).collect();
        assert!(labels.iter().all(|label| !label.trim().is_empty()));
        labels.sort_unstable();
        let distinct = labels.len();
        labels.dedup();
        assert_eq!(labels.len(), distinct, "two questions share a label");
    }

    #[test]
    fn an_entry_crosses_as_the_four_contract_fields_and_nothing_else() {
        let race = question_roster()
            .into_iter()
            .find(|entry| entry.question == StandingQuestion::Race)
            .expect("race is in the roster");
        assert_eq!(
            serde_json::to_string(&race).unwrap(),
            r#"{"question":"race","label":"When is the next race","surface":"now","bindings":["race-series"]}"#
        );
    }
}
