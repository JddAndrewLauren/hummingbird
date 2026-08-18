//! The kind → field cascade and the invalid-rule read — sunk from
//! `client/web/src/screens/rules/{registry.ts,validity.ts}`.
//!
//! **Why this takes the registry as an argument rather than reading
//! [`hummingbird_domain::EVENT_KINDS`] directly.** The registry *is* a
//! domain artifact (ADR-0013 is explicit), and reading it here would be one
//! fewer thing to pass. But the catalogue a client edits against is the one
//! the **authority it is synced with** exports (`GET`'s `kindRegistry`
//! push), not the one this binary happened to compile: a phone or a browser
//! running a build older or newer than the server would otherwise flag a
//! rule invalid — or fail to flag one — against a catalogue the server never
//! used. `invalid_fields` is a trust signal, so it must answer against the
//! registry the reader was actually shown.
//!
//! Everything here is display-only. It never blocks a save (#133's
//! `validate_rule` already does that, server-side, at save time) and never
//! mutates a rule — it only tells a screen to say so. The Agent Brief's own
//! words: "It must never silently stop firing — a silently-dead rule is the
//! failure mode that erodes trust in the whole channel."

use hummingbird_domain::{Condition, FieldType};
use serde::{Deserialize, Serialize};

/// One field a kind (or the Event core) declares — the registry export's
/// own shape (`hummingbird_domain::FieldDescriptor`, camelCased on the
/// wire).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KindField {
    pub name: String,
    pub field_type: FieldType,
}

/// One registry entry — `hummingbird_domain::EventKindEntry`'s wire shape.
/// `mints` is ADR-0013's raw-stream-vs-already-an-alert flag; a
/// `mints: false` kind (`alert_raised`) is still selectable, since a rule
/// can react to an alert even though it never re-mints one.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KindEntry {
    pub key: String,
    #[serde(default)]
    pub mints: bool,
    #[serde(default)]
    pub fields: Vec<KindField>,
}

/// The kind registry as a client holds it (#133/#140, ADR-0013).
/// `alarm_interval_ms` and `severities` are carried through untouched —
/// no decision here reads them, but the mobile seam ships them to a form
/// in the same crossing rather than opening a second one.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KindRegistry {
    #[serde(default)]
    pub kinds: Vec<KindEntry>,
    #[serde(default)]
    pub core_fields: Vec<KindField>,
    #[serde(default)]
    pub alarm_interval_ms: i64,
    #[serde(default)]
    pub severities: Vec<String>,
}

/// The field list a condition editor offers for `event_kind`.
///
/// `None` ("any kind") narrows the list to the Event core alone — ADR-0013's
/// own rule. A named kind offers the Event core plus that kind's declared
/// fields, **core first**: core fields always win a name collision at
/// evaluation time (`hummingbird_domain::event`'s own doc), so listing them
/// first mirrors which one actually resolves, and a kind field sharing a
/// core name is skipped rather than listed twice.
///
/// A kind this registry has never heard of still offers the core alone —
/// ADR-0013's open registry key. Flagging the rule itself is
/// [`invalid_fields`]'s job, not this lookup's.
pub fn fields_for_kind(registry: &KindRegistry, event_kind: Option<&str>) -> Vec<KindField> {
    let Some(entry) = event_kind.and_then(|key| registry.kinds.iter().find(|k| k.key == key))
    else {
        return registry.core_fields.clone();
    };
    let mut fields = registry.core_fields.clone();
    fields.extend(
        entry
            .fields
            .iter()
            .filter(|field| !registry.core_fields.iter().any(|core| core.name == field.name))
            .cloned(),
    );
    fields
}

/// One field's declared type within the list `event_kind` offers — `None`
/// if `field_name` is not in that list (the invalid-rule case
/// [`invalid_fields`] surfaces).
pub fn field_type(
    registry: &KindRegistry,
    event_kind: Option<&str>,
    field_name: &str,
) -> Option<FieldType> {
    fields_for_kind(registry, event_kind)
        .into_iter()
        .find(|field| field.name == field_name)
        .map(|field| field.field_type)
}

/// Every condition field the rule's `event_kind` no longer declares —
/// empty when the rule is entirely valid against `registry`. Checked
/// against [`fields_for_kind`], the exact list the editor itself offers, so
/// "invalid" here means precisely "this editor could not have produced this
/// condition today." Order follows the conditions, and a field named twice
/// is reported twice: the badge counts rows, not distinct names.
pub fn invalid_fields(
    conditions: &[Condition],
    event_kind: Option<&str>,
    registry: &KindRegistry,
) -> Vec<String> {
    let legal = fields_for_kind(registry, event_kind);
    conditions
        .iter()
        .filter(|condition| !legal.iter().any(|field| field.name == condition.field))
        .map(|condition| condition.field.clone())
        .collect()
}

/// Whether a rule is valid against `registry` — the boolean an
/// invalid-rule badge gates on.
pub fn is_rule_valid(
    conditions: &[Condition],
    event_kind: Option<&str>,
    registry: &KindRegistry,
) -> bool {
    invalid_fields(conditions, event_kind, registry).is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(name: &str, field_type: FieldType) -> KindField {
        KindField {
            name: name.to_string(),
            field_type,
        }
    }

    fn registry() -> KindRegistry {
        KindRegistry {
            core_fields: vec![
                field("source", FieldType::String),
                field("occurred_at", FieldType::Timestamp),
            ],
            kinds: vec![
                KindEntry {
                    key: "email".to_string(),
                    mints: true,
                    fields: vec![
                        field("subject", FieldType::String),
                        field("to", FieldType::StringList),
                        // A core-name collision: must not be listed twice.
                        field("source", FieldType::String),
                    ],
                },
                KindEntry {
                    key: "alert_raised".to_string(),
                    mints: false,
                    fields: vec![],
                },
            ],
            alarm_interval_ms: 900_000,
            severities: vec!["low".to_string(), "normal".to_string()],
        }
    }

    fn condition(field: &str) -> Condition {
        Condition {
            field: field.to_string(),
            op: "eq".to_string(),
            value: serde_json::Value::String("x".to_string()),
            negate: false,
        }
    }

    fn names(fields: &[KindField]) -> Vec<&str> {
        fields.iter().map(|f| f.name.as_str()).collect()
    }

    #[test]
    fn any_kind_narrows_to_the_event_core() {
        assert_eq!(fields_for_kind(&registry(), None), registry().core_fields);
    }

    #[test]
    fn a_named_kind_offers_the_core_first_then_its_own() {
        let fields = fields_for_kind(&registry(), Some("email"));
        assert_eq!(names(&fields), ["source", "occurred_at", "subject", "to"]);
    }

    #[test]
    fn a_kind_field_colliding_with_a_core_name_is_never_listed_twice() {
        let fields = fields_for_kind(&registry(), Some("email"));
        assert_eq!(fields.iter().filter(|f| f.name == "source").count(), 1);
    }

    #[test]
    fn an_unknown_kind_falls_back_to_the_core_alone() {
        assert_eq!(
            fields_for_kind(&registry(), Some("weather_alert")),
            registry().core_fields,
        );
    }

    #[test]
    fn field_type_resolves_within_the_offered_list_only() {
        let registry = registry();
        assert_eq!(
            field_type(&registry, Some("email"), "subject"),
            Some(FieldType::String),
        );
        assert_eq!(
            field_type(&registry, Some("email"), "source"),
            Some(FieldType::String),
        );
        assert_eq!(field_type(&registry, None, "subject"), None);
    }

    #[test]
    fn a_rule_naming_only_declared_fields_is_valid() {
        let registry = registry();
        let conditions = [condition("subject")];
        assert_eq!(invalid_fields(&conditions, Some("email"), &registry), Vec::<String>::new());
        assert!(is_rule_valid(&conditions, Some("email"), &registry));
    }

    #[test]
    fn a_core_field_is_valid_without_a_kind_specific_match() {
        assert!(is_rule_valid(&[condition("source")], Some("email"), &registry()));
    }

    #[test]
    fn a_field_the_kind_no_longer_declares_is_flagged() {
        let registry = registry();
        let conditions = [condition("removed_field")];
        assert_eq!(
            invalid_fields(&conditions, Some("email"), &registry),
            ["removed_field"],
        );
        assert!(!is_rule_valid(&conditions, Some("email"), &registry));
    }

    #[test]
    fn an_any_kind_rule_flags_a_kind_only_field() {
        assert!(!is_rule_valid(&[condition("subject")], None, &registry()));
    }

    #[test]
    fn the_registry_deserialises_from_the_camel_case_wire_shape() {
        let parsed: KindRegistry = serde_json::from_str(
            r#"{"kinds":[{"key":"email","mints":true,"fields":[{"name":"subject","fieldType":"string"}]}],
                "coreFields":[{"name":"source","fieldType":"string"}],
                "alarmIntervalMs":900000,"severities":["low"]}"#,
        )
        .expect("the registry export is this shape");
        assert_eq!(
            names(&fields_for_kind(&parsed, Some("email"))),
            ["source", "subject"],
        );
        assert_eq!(parsed.alarm_interval_ms, 900_000);
    }
}
