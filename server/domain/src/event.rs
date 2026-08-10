//! The Event core and the kind registry (ADR-0013, #133): the one shape
//! every rule-eligible thing presents at the rule engine's door, and the
//! catalogue of what each `event_kind` adds on top of it.
//!
//! **The registry is a `domain` artifact — code, not a table.** ADR-0013 is
//! explicit about why: a data-driven catalogue can drift from what an
//! adapter actually emits, and the symptom of that drift is a rule that
//! silently never fires — undetectable in a default-deny lane. Declaring it
//! here, in the crate both the rule engine (`hummingbird-rules-engine`) and
//! the client compile, means [`kind_registry_json`] and the engine's field
//! lookups are always the same definition; adding a kind cannot update one
//! without the other. #140's rules UI renders its field/operator dropdowns
//! from the JSON export.

use serde::Serialize;
use std::collections::BTreeMap;

/// The typed catalogue ADR-0013 gates operators by. `Dynamic` exists only
/// for `snapshot_change`'s `value`/`previous`: ADR-0013 declares those
/// "per key at wiring time" rather than fixed by kind, so the registry
/// still names them (unknown-field detection still works) but operator
/// legality for them is checked against the value actually present on the
/// event, not a fixed descriptor type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldType {
    String,
    StringList,
    Number,
    Bool,
    Timestamp,
    Date,
    Dynamic,
}

/// One resolved field value. `Timestamp`- and `Date`-typed fields are
/// carried as `Str` (their wire representation is a string in the deadline
/// format, ADR-0013's "deadlines gain a time"); which typed comparison
/// applies is decided from the field's declared [`FieldType`], not from
/// this enum's shape.
#[derive(Debug, Clone, PartialEq)]
pub enum FieldValue {
    Str(String),
    StrList(Vec<String>),
    Num(f64),
    Bool(bool),
}

impl FieldValue {
    /// The type this value would resolve to if its descriptor were
    /// [`FieldType::Dynamic`]. `Str` maps to `String` here rather than
    /// `Timestamp`/`Date` — a bare value carries no signal distinguishing
    /// those, so relative-time operators are not supported on a `Dynamic`
    /// field in v1; only `eq`/`contains`/`gt`/`lt`/`is` are reachable
    /// through this mapping.
    pub fn dynamic_type(&self) -> FieldType {
        match self {
            FieldValue::Str(_) => FieldType::String,
            FieldValue::StrList(_) => FieldType::StringList,
            FieldValue::Num(_) => FieldType::Number,
            FieldValue::Bool(_) => FieldType::Bool,
        }
    }
}

/// One field a kind adds on top of the Event core.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct FieldDescriptor {
    pub name: &'static str,
    pub field_type: FieldType,
}

const fn field(name: &'static str, field_type: FieldType) -> FieldDescriptor {
    FieldDescriptor { name, field_type }
}

/// One registry entry: a kind's extra fields, plus ADR-0013's `mints` flag
/// (raw stream vs. an event that already is an alert).
#[derive(Debug, Clone, Copy, Serialize)]
pub struct EventKindEntry {
    pub key: &'static str,
    pub mints: bool,
    pub fields: &'static [FieldDescriptor],
}

/// The five kinds ADR-0013 launches with. Frozen shape per kind — see the
/// ADR's field tables; this is their code form, and the only form, per the
/// "registry is a `domain` artifact" decision above.
///
/// **Core fields always win a name collision.** `email.body`,
/// `calendar_event.title` and `item_threshold.title` repeat a name the
/// Event core already declares (ADR-0013's per-kind tables list them
/// again for #140's dropdown, since a rule author browsing "calendar_event
/// fields" expects to see `title` there); the engine's field lookup checks
/// [`core_field_type`] first, so these kind-level entries are never the
/// ones actually resolved — they exist for the JSON export's UI listing,
/// not for evaluation. This is harmless only because every repeated name
/// shares its core type (`String`); a kind declaring the same name at a
/// *different* type would be silently shadowed rather than rejected, so a
/// future kind must not do that.
pub const EVENT_KINDS: &[EventKindEntry] = &[
    EventKindEntry {
        key: "email",
        mints: true,
        fields: &[
            field("from", FieldType::String),
            field("to", FieldType::StringList),
            field("cc", FieldType::StringList),
            field("subject", FieldType::String),
            field("body", FieldType::String),
            field("labels", FieldType::StringList),
            field("received_at", FieldType::Timestamp),
            field("has_attachment", FieldType::Bool),
        ],
    },
    EventKindEntry {
        key: "calendar_event",
        mints: true,
        fields: &[
            field("calendar", FieldType::String),
            field("title", FieldType::String),
            field("organizer", FieldType::String),
            field("location", FieldType::String),
            field("attendees", FieldType::StringList),
            field("starts_at", FieldType::Timestamp),
            field("ends_at", FieldType::Timestamp),
            field("is_all_day", FieldType::Bool),
            field("response", FieldType::String),
        ],
    },
    EventKindEntry {
        key: "item_threshold",
        mints: true,
        fields: &[
            field("deadline", FieldType::Timestamp),
            field("scheduled_date", FieldType::Date),
            field("title", FieldType::String),
            field("stage", FieldType::String),
            field("size", FieldType::String),
            field("energy", FieldType::String),
            field("context", FieldType::String),
            field("priority", FieldType::Number),
            field("project", FieldType::String),
        ],
    },
    EventKindEntry {
        key: "snapshot_change",
        mints: true,
        fields: &[
            field("key", FieldType::String),
            field("changed_at", FieldType::Timestamp),
            field("value", FieldType::Dynamic),
            field("previous", FieldType::Dynamic),
        ],
    },
    EventKindEntry {
        key: "alert_raised",
        mints: false,
        fields: &[],
    },
];

/// Field names present on every Event, always — ADR-0013's core.
pub const CORE_FIELDS: &[&str] = &[
    "source",
    "source_key",
    "occurred_at",
    "title",
    "body",
    "url",
    "severity",
    "calendar_busy",
];

pub fn find_kind(key: &str) -> Option<&'static EventKindEntry> {
    EVENT_KINDS.iter().find(|k| k.key == key)
}

/// The type of a core field, or `None` if `name` is not one.
pub fn core_field_type(name: &str) -> Option<FieldType> {
    match name {
        "source" | "source_key" | "title" | "body" | "url" | "severity" => Some(FieldType::String),
        "occurred_at" => Some(FieldType::Timestamp),
        "calendar_busy" => Some(FieldType::Bool),
        _ => None,
    }
}

/// The registry, exported as JSON from the exact same definition the
/// engine evaluates against — the property ADR-0013 requires so a new kind
/// cannot update one consumer without the other.
pub fn kind_registry_json() -> serde_json::Value {
    serde_json::to_value(EVENT_KINDS).expect("EVENT_KINDS is always representable as JSON")
}

/// One Event: the ADR-0013 core, present on every event from every source,
/// plus whatever `event_kind` declares in `extras`.
#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub source: String,
    pub source_key: String,
    pub occurred_at: String,
    pub title: String,
    /// Present but possibly absent-valued (ADR-0013): `None` behaves
    /// exactly like a field a kind never declared — evaluation rule 2
    /// makes its condition false, even negated.
    pub body: Option<String>,
    pub url: Option<String>,
    pub severity: Option<String>,
    /// `None` = the calendar snapshot is missing or stale. Resolved to
    /// `false` (not busy) by [`Event::core_value`] — the sole exception to
    /// "missing makes the condition false" (ADR-0013).
    pub calendar_busy: Option<bool>,
    /// `None` = no kind; a rule whose own `event_kind` is also `None`
    /// evaluates against the core alone, for an event of any kind.
    pub event_kind: Option<String>,
    pub extras: BTreeMap<String, FieldValue>,
}

impl Event {
    /// Resolve one of the eight ADR-0013 core field names, or `None` if
    /// `name` does not name a core field. `calendar_busy`'s default-to-false
    /// is applied here, so callers never see it as "missing."
    pub fn core_value(&self, name: &str) -> Option<FieldValue> {
        match name {
            "source" => Some(FieldValue::Str(self.source.clone())),
            "source_key" => Some(FieldValue::Str(self.source_key.clone())),
            "occurred_at" => Some(FieldValue::Str(self.occurred_at.clone())),
            "title" => Some(FieldValue::Str(self.title.clone())),
            "body" => self.body.clone().map(FieldValue::Str),
            "url" => self.url.clone().map(FieldValue::Str),
            "severity" => self.severity.clone().map(FieldValue::Str),
            "calendar_busy" => Some(FieldValue::Bool(self.calendar_busy.unwrap_or(false))),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_key_is_unique() {
        let mut keys: Vec<&str> = EVENT_KINDS.iter().map(|k| k.key).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), EVENT_KINDS.len());
    }

    #[test]
    fn alert_raised_does_not_mint() {
        let entry = find_kind("alert_raised").unwrap();
        assert!(!entry.mints);
    }

    #[test]
    fn the_other_four_launch_kinds_mint() {
        for key in ["email", "calendar_event", "item_threshold", "snapshot_change"] {
            assert!(find_kind(key).unwrap().mints, "{key} should mint");
        }
    }

    #[test]
    fn registry_json_round_trips_every_kind_and_field() {
        let json = kind_registry_json();
        let kinds = json.as_array().unwrap();
        assert_eq!(kinds.len(), EVENT_KINDS.len());
        let email = kinds.iter().find(|k| k["key"] == "email").unwrap();
        assert_eq!(email["mints"], true);
        let fields = email["fields"].as_array().unwrap();
        assert!(fields.iter().any(|f| f["name"] == "subject" && f["field_type"] == "string"));
        assert!(fields.iter().any(|f| f["name"] == "to" && f["field_type"] == "string_list"));
    }

    #[test]
    fn calendar_busy_defaults_to_false_when_missing() {
        let event = Event {
            source: "s".into(),
            source_key: "k".into(),
            occurred_at: "2026-08-15T09:00".into(),
            title: "t".into(),
            body: None,
            url: None,
            severity: None,
            calendar_busy: None,
            event_kind: None,
            extras: BTreeMap::new(),
        };
        assert_eq!(event.core_value("calendar_busy"), Some(FieldValue::Bool(false)));
    }

    #[test]
    fn a_null_body_resolves_to_none_not_empty_string() {
        let event = Event {
            source: "s".into(),
            source_key: "k".into(),
            occurred_at: "2026-08-15T09:00".into(),
            title: "t".into(),
            body: None,
            url: None,
            severity: None,
            calendar_busy: Some(true),
            event_kind: None,
            extras: BTreeMap::new(),
        };
        assert_eq!(event.core_value("body"), None);
    }
}
