//! The standing-question **bindings** (#118, ADR-0015): the small
//! cross-device facts a pane needs before it can answer anything — which
//! race series, which calendar holds the trips, which page the waste
//! schedule is scraped from — and, since the homework pane's standing link,
//! one fact that names no source at all ([`BindingKey::HomeworkLink`]): a
//! value that must not live in a public repo, held here for the same
//! sync-it-once reason rather than because a pane queries it.
//!
//! A binding is one `settings` row (ADR-0009's KV table), so it rides the
//! ordinary delta pull and the full sweep like every other table, and is
//! written as an ordinary entity-level CAS set. There is no bespoke sync
//! path here and there must never be one: `SyncMirror` already carries
//! `settings`, and [`crate::Core::set_binding`] enqueues through
//! [`crate::sync::SyncCycle::enqueue`] exactly as `capture`/`act`/`triage`
//! do.
//!
//! **Keys are kebab-case and unversioned** (ADR-0015, decided in the seam
//! grilling that opened this slice). Deliberately unversioned: a source
//! bump — `city-waste/v1` → `city-waste/v2` — must not orphan a binding in
//! a table that has no DELETE, and a key carrying a version string would do
//! exactly that. The *source* is versioned; the question the human answered
//! once ("which page is my collection schedule on") is not.
//!
//! **The vocabulary is closed** ([`BindingKey`]), and that is the point of
//! this module: `settings` is a generic KV table with no DELETE, so a UI
//! able to mint arbitrary keys can only ever grow it. Every key crossing a
//! seam is resolved by name through [`BindingKey::parse`] before it reaches
//! `Core`, the same discipline `ffi-web`'s `parse_action`/`parse_destination`
//! already apply to their own wire strings. A key this build has never
//! heard of is still *readable* — see [`Binding::known`] — because a row
//! written by a newer build is a fact about this build, not a broken row
//! (the same reading ADR-0015 gives an unrecognised snapshot `schema`).
//!
//! **The collapse state of a pane is not a binding** (ADR-0015, again
//! explicitly): it is device-local and band-scoped, and belongs in the
//! injectable-`storage` idiom `calendar/persistence.ts` / `theme/useTheme.ts`
//! already use. Nothing in this module is a view preference.

use serde::Serialize;

/// Every binding this build knows how to write. Closed, kebab-case,
/// unversioned — see the module docs for all three reasons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BindingKey {
    /// Which motorsport series the next-race pane follows (#119).
    RaceSeries,
    /// The dedicated Trips calendar the vacation countdown reads (#121) —
    /// a calendar id, since the calendar stays the authority for its own
    /// events (ADR-0005).
    TripsCalendar,
    /// The council page the waste schedule is polled from (#120).
    CityWastePage,
    /// The standing meeting link the homework pane offers (#675's
    /// follow-on).
    ///
    /// **Unlike its three siblings this names no source.** `race-series`,
    /// `trips-calendar` and `city-waste-page` each say *which thing a pane
    /// reads*, and an unset one leaves that pane `AnswerState::Unbound`
    /// with a setup prompt. This one names a **display affordance** the
    /// homework pane renders and never queries: unset, the pane answers
    /// exactly as it does today and simply draws no button (ADR-0015,
    /// amended).
    ///
    /// It is a binding rather than a literal beside `HOMEWORK_CONTEXT` for
    /// one reason: the value is a meeting URL with its passcode embedded in
    /// it, and this repo is public. A binding is one `settings` row on the
    /// operator's own authority, set once and synced to every device —
    /// never in git.
    HomeworkLink,
}

impl BindingKey {
    /// Every key, in the order [`crate::Core::bindings`] presents them —
    /// declaration order, not alphabetical, so the editor's rows stay put
    /// as the vocabulary grows.
    pub const ALL: &'static [BindingKey] = &[
        BindingKey::RaceSeries,
        BindingKey::TripsCalendar,
        BindingKey::CityWastePage,
        BindingKey::HomeworkLink,
    ];

    /// The `settings.key` this binding is stored under — the wire spelling
    /// in both directions.
    pub fn as_str(self) -> &'static str {
        match self {
            BindingKey::RaceSeries => "race-series",
            BindingKey::TripsCalendar => "trips-calendar",
            BindingKey::CityWastePage => "city-waste-page",
            BindingKey::HomeworkLink => "homework-link",
        }
    }

    /// Resolves a wire key by name. `None` is a key this build does not
    /// know how to *write* — never a reason to hide a row it can read.
    pub fn parse(key: &str) -> Option<BindingKey> {
        BindingKey::ALL
            .iter()
            .copied()
            .find(|candidate| candidate.as_str() == key)
    }
}

/// What a binding is currently set to, as three distinct states rather than
/// a nullable string.
///
/// The split exists for the same reason [`crate::freshness::Freshness`]
/// refuses to be a boolean: "nobody has set this yet" and "this holds
/// something that is not text" are different facts, and collapsing the
/// second into the first would let the editor silently offer to overwrite a
/// value it never showed anyone. [`BindingValue::Other`] carries the stored
/// JSON verbatim so a reader can put it on screen — visibly odd, never
/// quietly empty (ADR-0015).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum BindingValue {
    /// No `settings` row for this key (or the row has gone absent —
    /// ADR-0003's demotion, which reads the same way here: this device has
    /// no current answer).
    Unset,
    /// The stored JSON is a string — the shape every binding this slice
    /// writes uses, and the only one [`crate::Core::set_binding`] produces.
    Text { text: String },
    /// The stored JSON is something else (a number, an object, a value a
    /// newer build wrote). Carried as its canonical JSON text so it can be
    /// displayed; deliberately not parsed further and not editable here.
    Other { raw: String },
}

impl BindingValue {
    /// Reads one stored `settings.value` — canonical JSON text, per
    /// `hummingbird_domain::Setting`'s own doc — into the three states
    /// above. Unparseable JSON lands in [`BindingValue::Other`] with the
    /// bytes as stored rather than being dropped: the row exists, and
    /// saying nothing about it would be the quietly-empty answer.
    pub fn from_stored(raw: &str) -> BindingValue {
        match serde_json::from_str::<serde_json::Value>(raw) {
            Ok(serde_json::Value::String(text)) => BindingValue::Text { text },
            _ => BindingValue::Other {
                raw: raw.to_string(),
            },
        }
    }
}

/// One binding as a reader sees it: every [`BindingKey`] this build knows
/// (set or not), plus every other live `settings` row, so the editor can
/// show what is actually in the table rather than only what it can write.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Binding {
    pub key: String,
    /// Whether [`BindingKey::parse`] resolved this key — i.e. whether this
    /// build can write it. A `false` row is display-only.
    pub known: bool,
    /// Whether an unconfirmed local write is currently overlaid on this key
    /// — the same read-time overlay fact `Core::is_pending` reports for an
    /// item, never a stored column.
    pub pending: bool,
    pub value: BindingValue,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_key_is_kebab_case_and_carries_no_version_suffix() {
        // ADR-0015: a `/vN` in a binding key would orphan the binding on a
        // source bump, in a table with no DELETE.
        for key in BindingKey::ALL {
            let spelling = key.as_str();
            assert!(
                spelling
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c == '-'),
                "{spelling} must be kebab-case"
            );
            assert!(!spelling.contains('/'), "{spelling} must carry no version");
            assert!(
                !spelling.chars().any(|c| c.is_ascii_digit()),
                "{spelling} must carry no version"
            );
        }
    }

    #[test]
    fn the_vocabulary_is_the_three_adr_0015_sources_then_the_homework_link() {
        // Declaration order is the editor's row order, so the fourth is
        // APPENDED — the three source bindings keep their rows. It is also
        // deliberately last in a second sense: it is not a source binding
        // at all (see `BindingKey::HomeworkLink`), so nothing above it
        // becomes harder to read by its arrival.
        let spellings: Vec<&str> = BindingKey::ALL.iter().map(|key| key.as_str()).collect();
        assert_eq!(
            spellings,
            vec![
                "race-series",
                "trips-calendar",
                "city-waste-page",
                "homework-link"
            ]
        );
    }

    #[test]
    fn a_key_round_trips_by_name_and_an_unknown_one_is_rejected() {
        for key in BindingKey::ALL {
            assert_eq!(BindingKey::parse(key.as_str()), Some(*key));
        }
        assert_eq!(BindingKey::parse("race-series/v1"), None);
        assert_eq!(BindingKey::parse("raceSeries"), None);
        assert_eq!(BindingKey::parse(""), None);
    }

    #[test]
    fn a_stored_value_reads_as_text_only_when_it_actually_is_one() {
        assert_eq!(
            BindingValue::from_stored("\"f1\""),
            BindingValue::Text {
                text: "f1".to_string()
            }
        );
        // A number, an object and outright garbage are all "something else
        // is in there", shown verbatim rather than reported as unset.
        for raw in ["7", "{\"a\":1}", "null", "not json"] {
            assert_eq!(
                BindingValue::from_stored(raw),
                BindingValue::Other {
                    raw: raw.to_string()
                },
                "{raw}"
            );
        }
    }

    #[test]
    fn the_value_states_serialize_as_a_tagged_union_never_a_nullable_string() {
        assert_eq!(
            serde_json::to_string(&BindingValue::Unset).unwrap(),
            r#"{"state":"unset"}"#
        );
        assert_eq!(
            serde_json::to_string(&BindingValue::Text {
                text: "f1".to_string()
            })
            .unwrap(),
            r#"{"state":"text","text":"f1"}"#
        );
        assert_eq!(
            serde_json::to_string(&BindingValue::Other {
                raw: "7".to_string()
            })
            .unwrap(),
            r#"{"state":"other","raw":"7"}"#
        );
    }
}
