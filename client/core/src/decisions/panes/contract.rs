//! ADR-0015's **pane shell contract**, minus its rendering half — the sunk
//! part of `client/web/src/screens/questions/contract.ts` (#533/M4).
//!
//! The split ADR-0025 draws through that file: a question's *answer state*,
//! its *band*, its *`within_band`* instant and its *identity* are decisions
//! (two clients disagreeing about any of them is a bug), so they are here;
//! its headline, its glyphs, its `QuestionDef` and its `Expanded` component
//! are renderings, so they stay per-client. [`PaneAnswerCore`] is
//! `PaneAnswer` with exactly that cut made — **no headline, no glyphs**.
//!
//! Every wire name here matches the TS union it was ported from, spelled
//! once via serde rename rather than twice.

use serde::{Deserialize, Serialize};

use super::zone::CivilDate;

/// Whether this question has an answer at all, and if not, why not. Three
/// states rather than "answered or not": a question whose binding is set
/// but whose data has not landed is something the reader can act on, while
/// a question nobody has bound yet was never asked. Rendering both as
/// "nothing to show" is the quietly-empty answer ADR-0015 rules out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AnswerState {
    Answered,
    BoundButUnacquired,
    Unbound,
}

/// ADR-0015's five-band salience vocabulary — how soon this answer matters.
/// A closed word list rather than a number: the bands are what make two
/// questions with unrelated units (a race in 3 days, a bin collection
/// tonight) comparable at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Band {
    Live,
    Imminent,
    Near,
    Distant,
    Dormant,
}

/// Bands in salience order, most pressing first. [`super::sort::order_panes`]
/// reads this; so does the web's `collapse.ts` default rule.
pub const BAND_ORDER: [Band; 5] =
    [Band::Live, Band::Imminent, Band::Near, Band::Distant, Band::Dormant];

/// ADR-0017's second axis: which ranked region a question renders into.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Surface {
    Now,
    Status,
}

impl Surface {
    pub fn parse(s: &str) -> Option<Surface> {
        match s {
            "now" => Some(Surface::Now),
            "status" => Some(Surface::Status),
            _ => None,
        }
    }
}

/// Every standing question this build can render. A closed vocabulary, on
/// the same reasoning as `BindingKey`: "every question, in a declared
/// order" is then a fact the type system checks rather than a list someone
/// has to remember to update.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StandingQuestion {
    Homework,
    Waste,
    Weekend,
    Vacation,
    Race,
    Kimi,
    Github,
    Uptime,
    Reachability,
}

impl StandingQuestion {
    pub fn as_str(self) -> &'static str {
        match self {
            StandingQuestion::Homework => "homework",
            StandingQuestion::Waste => "waste",
            StandingQuestion::Weekend => "weekend",
            StandingQuestion::Vacation => "vacation",
            StandingQuestion::Race => "race",
            StandingQuestion::Kimi => "kimi",
            StandingQuestion::Github => "github",
            StandingQuestion::Uptime => "uptime",
            StandingQuestion::Reachability => "reachability",
        }
    }
}

/// Declared display order — the fourth axis of the cross-pane sort, and the
/// order the wiring unions its sources in. Declaration order, not
/// alphabetical, so a question's place does not move when another is
/// renamed. The four infra questions (ADR-0017, #311) are declared after
/// Now's own.
///
/// `Homework` is declared **first** (#675, the operator's call): it is the
/// only question about work the reader personally owes, so when two panes
/// tie on band and `within_band` it is the one that should be read first.
pub const QUESTION_ORDER: [StandingQuestion; 9] = [
    StandingQuestion::Homework,
    StandingQuestion::Waste,
    StandingQuestion::Weekend,
    StandingQuestion::Vacation,
    StandingQuestion::Race,
    StandingQuestion::Kimi,
    StandingQuestion::Github,
    StandingQuestion::Uptime,
    StandingQuestion::Reachability,
];

/// One question's answer about one subject, **as a decision** — `PaneAnswer`
/// (`contract.ts`) minus `collapsedHeadline` and `icon`, which are the two
/// fields ADR-0025 leaves per-client.
///
/// `within_band` is **epoch ms of this answer's next relevant moment**,
/// device clock — an absolute instant, never a duration, so the sort reads
/// no clock at all and a captured value cannot go stale between renders.
/// `None` is "nothing to order by", and sorts after every `Some`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaneAnswerCore {
    pub answer_state: AnswerState,
    pub band: Band,
    pub within_band: Option<i64>,
}

/// One pane, ranked: which question, which subject, and the answer that
/// placed it.
///
/// **`question` is a `String`, not a [`StandingQuestion`]**, and that is the
/// port being faithful rather than sloppy. `sort.ts`'s `orderPanes` takes
/// `questionOrder: readonly string[]` and ranks whatever strings it is
/// handed — its own suite declares a synthetic two-question order — so the
/// sort is total over question names it has never heard of, and narrowing
/// here would make the sunk function refuse inputs the TS one answers.
/// [`StandingQuestion`] remains the vocabulary; it is what a *producer*
/// (`rank_panes`) names its records with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RankedPaneRecord {
    pub question: String,
    pub subject_key: String,
    pub pane_key: String,
    pub answer: PaneAnswerCore,
}

/// The stable per-pane identity — question and subject, never position.
pub fn pane_key(question: &str, subject_key: &str) -> String {
    format!("{question}:{subject_key}")
}

/// One calendar-arm request: the interval a question needs from the
/// calendar mirror, in both of the arms [`crate::calendar::Interval`] takes
/// — instants for timed events, the reader's own civil dates (exclusive
/// end) for all-day ones. Neither half is derived from the other, here or
/// anywhere: deriving one would need the tzdb this crate deliberately does
/// not carry (ADR-0015's 2026-08-10 amendment).
///
/// The web builds the same shape in each question's `calendarRequests`
/// (`registry.ts`'s `requiredCalendarRequests`); #564 gave the two that
/// build one from a resolved zone — weekend and vacation — a second caller,
/// so the computation sank and this is what it answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarInterval {
    pub start_ms: i64,
    pub end_ms: i64,
    pub start_date: CivilDate,
    /// **Exclusive** — the day after the last day the window covers.
    pub end_date: CivilDate,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wire_names_are_the_ts_unions_spelling() {
        assert_eq!(serde_json::to_string(&AnswerState::Answered).unwrap(), "\"answered\"");
        assert_eq!(
            serde_json::to_string(&AnswerState::BoundButUnacquired).unwrap(),
            "\"bound-but-unacquired\"",
        );
        assert_eq!(serde_json::to_string(&AnswerState::Unbound).unwrap(), "\"unbound\"");
        assert_eq!(serde_json::to_string(&Band::Live).unwrap(), "\"live\"");
        assert_eq!(serde_json::to_string(&Band::Dormant).unwrap(), "\"dormant\"");
        assert_eq!(serde_json::to_string(&Surface::Now).unwrap(), "\"now\"");
        assert_eq!(serde_json::to_string(&StandingQuestion::Github).unwrap(), "\"github\"");
    }

    #[test]
    fn the_declared_orders_are_the_ts_arrays_verbatim() {
        assert_eq!(
            serde_json::to_string(&BAND_ORDER).unwrap(),
            r#"["live","imminent","near","distant","dormant"]"#,
        );
        assert_eq!(
            serde_json::to_string(&QUESTION_ORDER).unwrap(),
            r#"["homework","waste","weekend","vacation","race","kimi","github","uptime","reachability"]"#,
        );
    }

    #[test]
    fn a_pane_key_is_question_then_subject_and_never_a_position() {
        assert_eq!(pane_key("waste", "collection"), "waste:collection");
    }

    #[test]
    fn an_answer_crosses_as_the_three_decided_fields_and_nothing_else() {
        let json = serde_json::to_string(&PaneAnswerCore {
            answer_state: AnswerState::Answered,
            band: Band::Imminent,
            within_band: Some(17),
        })
        .unwrap();
        assert_eq!(json, r#"{"answerState":"answered","band":"imminent","withinBand":17}"#);
    }
}
