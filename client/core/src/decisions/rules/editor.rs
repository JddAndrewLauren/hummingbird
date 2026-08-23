//! The condition row's own cascade — sunk from
//! `client/web/src/screens/rules/condition-editor.ts`. Not one of #540's
//! five named items, and forced by them anyway: the phone's create-and-edit
//! form needs "pick a kind, then a field, then only the operators legal for
//! that field's type, then the value widget the type implies," and ADR-0025
//! forbids Kotlin holding a per-row decision function. See this family's
//! `mod.rs` header.

use hummingbird_domain::{Condition, FieldType};
use hummingbird_rules_engine::Operator;
use serde::{Deserialize, Serialize};

use super::operators::{default_operator_for, legal_operators};

/// The control one condition row's value is edited with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ValueWidget {
    Chips,
    Duration,
    Datetime,
    Boolean,
    Number,
    /// A pick from [`super::validity::KindRegistry::sources`] — the `source`
    /// core field's frozen vocabulary, rather than a text box a typo makes
    /// silently unmatchable. See [`widget_for`] for when it applies.
    Source,
    Text,
}

impl ValueWidget {
    pub const ALL: [ValueWidget; 7] = [
        ValueWidget::Chips,
        ValueWidget::Duration,
        ValueWidget::Datetime,
        ValueWidget::Boolean,
        ValueWidget::Number,
        ValueWidget::Source,
        ValueWidget::Text,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            ValueWidget::Chips => "chips",
            ValueWidget::Duration => "duration",
            ValueWidget::Datetime => "datetime",
            ValueWidget::Boolean => "boolean",
            ValueWidget::Number => "number",
            ValueWidget::Source => "source",
            ValueWidget::Text => "text",
        }
    }

    pub fn parse(s: &str) -> Option<ValueWidget> {
        ValueWidget::ALL.into_iter().find(|w| w.as_str() == s)
    }
}

/// The widget a condition row offers, given the field it targets and the
/// operator selected on it.
///
/// **Priority order matters and is deliberate.** `deadline` gets a
/// date/time picker even though its only legal operators are the
/// relative-time ones (ADR-0013: `deadline within_next '2h'`) — picking a
/// target date is the more natural gesture for a deadline specifically, and
/// [`super::deadline`] computes the equivalent duration underneath, so the
/// wire value is still an ordinary duration. Every *other*
/// `within_next`/`within_last` condition (e.g. `received_at within_last
/// '10m'`) gets the plain duration picker instead.
///
/// **`source` gets a pick list, but only under `eq`.** Its legal values are
/// a frozen registry (`hummingbird_domain::REGISTRY`), so a typed
/// string is a rule that silently matches nothing — the exact failure this
/// family exists to prevent. `contains` is substring matching, though: a
/// condition may legitimately name `city-waste` to reach both versions of
/// it, and a whole-value picker cannot express that. The engine's own
/// `retired_source_problems` draws the line in the same place and for the
/// same reason, which is what keeps the two from disagreeing about which
/// conditions the picker's vocabulary governs.
///
/// Placed after the `StringList` and relative-time guards, per the priority
/// discipline above: neither can collide with `source` today (it is a
/// `String` core field, and `eq` is not a relative-time operator), and the
/// order is what keeps that true if the catalogue moves.
pub fn widget_for(field_name: &str, field_type: FieldType, operator: Operator) -> ValueWidget {
    if field_type == FieldType::StringList {
        return ValueWidget::Chips;
    }
    if matches!(operator, Operator::WithinNext | Operator::WithinLast) {
        return if field_name == "deadline" {
            ValueWidget::Datetime
        } else {
            ValueWidget::Duration
        };
    }
    if field_name == "source" && operator == Operator::Eq {
        return ValueWidget::Source;
    }
    match field_type {
        FieldType::Bool => ValueWidget::Boolean,
        FieldType::Number => ValueWidget::Number,
        _ => ValueWidget::Text,
    }
}

/// The empty value a fresh (or re-typed) condition on `field_type` starts
/// at — a list for the chips widget, an empty string for everything else.
fn empty_value(field_type: FieldType) -> serde_json::Value {
    match field_type {
        FieldType::StringList => serde_json::Value::Array(Vec::new()),
        _ => serde_json::Value::String(String::new()),
    }
}

/// A fresh, legal condition on `field_name`: the operator defaults to the
/// first legal one for its type, `value` starts empty in the shape the
/// chosen widget expects, and `negate` starts off.
pub fn new_condition(field_name: &str, field_type: FieldType) -> Condition {
    Condition {
        field: field_name.to_string(),
        op: default_operator_for(field_type).as_str().to_string(),
        value: empty_value(field_type),
        negate: false,
    }
}

/// Re-legalises `condition` after its field's type changed (a field pick on
/// the same row): resets `op` to the new type's default when the old one is
/// no longer legal, and resets `value` to that type's empty shape. `negate`
/// survives — the row's "not" toggle is about the author's intent, not the
/// field's type.
///
/// **`None` means "already legal, leave it exactly as it is."** Not a copy
/// of the input: a host holding the condition as an object (the web's
/// `ConditionDTO`) then keeps its own identity rather than being handed a
/// structurally equal replacement, and a caller that re-renders on identity
/// does not re-render on a field pick that changed nothing.
pub fn retype_condition(condition: &Condition, new_field_type: FieldType) -> Option<Condition> {
    let still_legal = Operator::parse(&condition.op)
        .is_some_and(|op| legal_operators(new_field_type).contains(&op));
    if still_legal {
        return None;
    }
    Some(Condition {
        field: condition.field.clone(),
        op: default_operator_for(new_field_type).as_str().to_string(),
        value: empty_value(new_field_type),
        negate: condition.negate,
    })
}

/// Flips a condition's `negate` flag — the per-row "not" toggle, and
/// nothing else.
pub fn toggle_negate(condition: &Condition) -> Condition {
    Condition {
        negate: !condition.negate,
        ..condition.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_string_list_field_gets_chips_whatever_the_operator() {
        assert_eq!(
            widget_for("to", FieldType::StringList, Operator::Eq),
            ValueWidget::Chips,
        );
        assert_eq!(
            widget_for("labels", FieldType::StringList, Operator::Contains),
            ValueWidget::Chips,
        );
    }

    #[test]
    fn the_relative_time_pair_gets_a_duration_picker_on_an_ordinary_field() {
        assert_eq!(
            widget_for("received_at", FieldType::Timestamp, Operator::WithinLast),
            ValueWidget::Duration,
        );
        assert_eq!(
            widget_for("scheduled_date", FieldType::Date, Operator::WithinNext),
            ValueWidget::Duration,
        );
    }

    #[test]
    fn deadline_specifically_gets_the_date_time_picker() {
        assert_eq!(
            widget_for("deadline", FieldType::Timestamp, Operator::WithinNext),
            ValueWidget::Datetime,
        );
        assert_eq!(
            widget_for("deadline", FieldType::Timestamp, Operator::WithinLast),
            ValueWidget::Datetime,
        );
    }

    #[test]
    fn bool_number_and_string_get_their_own_controls() {
        assert_eq!(
            widget_for("has_attachment", FieldType::Bool, Operator::Is),
            ValueWidget::Boolean,
        );
        assert_eq!(
            widget_for("priority", FieldType::Number, Operator::Eq),
            ValueWidget::Number,
        );
        assert_eq!(
            widget_for("subject", FieldType::String, Operator::Contains),
            ValueWidget::Text,
        );
    }

    #[test]
    fn a_fresh_condition_starts_at_its_types_default_operator() {
        assert_eq!(new_condition("subject", FieldType::String).op, "eq");
        assert_eq!(new_condition("priority", FieldType::Number).op, "eq");
        assert_eq!(new_condition("has_attachment", FieldType::Bool).op, "is");
    }

    #[test]
    fn a_fresh_condition_starts_negate_off_and_value_empty_in_its_own_shape() {
        assert_eq!(
            new_condition("subject", FieldType::String),
            Condition {
                field: "subject".to_string(),
                op: "eq".to_string(),
                value: serde_json::Value::String(String::new()),
                negate: false,
            },
        );
        assert_eq!(
            new_condition("to", FieldType::StringList).value,
            serde_json::json!([]),
        );
    }

    #[test]
    fn retype_leaves_an_already_legal_operator_alone() {
        let condition = Condition {
            field: "subject".to_string(),
            op: "contains".to_string(),
            value: serde_json::json!("urgent"),
            negate: true,
        };
        assert_eq!(retype_condition(&condition, FieldType::String), None);
    }

    #[test]
    fn retype_resets_to_the_new_types_default_and_empty_value() {
        let condition = Condition {
            field: "subject".to_string(),
            op: "contains".to_string(),
            value: serde_json::json!("urgent"),
            negate: false,
        };
        assert_eq!(
            retype_condition(&condition, FieldType::Number),
            Some(Condition {
                field: "subject".to_string(),
                op: "eq".to_string(),
                value: serde_json::Value::String(String::new()),
                negate: false,
            }),
        );
    }

    #[test]
    fn retype_keeps_the_rows_negate_toggle() {
        let condition = Condition {
            field: "subject".to_string(),
            op: "contains".to_string(),
            value: serde_json::json!("urgent"),
            negate: true,
        };
        assert_eq!(
            retype_condition(&condition, FieldType::Number).map(|c| c.negate),
            Some(true),
        );
    }

    #[test]
    fn retype_replaces_an_unrecognised_operator() {
        let condition = Condition {
            field: "subject".to_string(),
            op: "not_an_operator".to_string(),
            value: serde_json::json!("urgent"),
            negate: false,
        };
        assert_eq!(
            retype_condition(&condition, FieldType::String).map(|c| c.op),
            Some("eq".to_string()),
        );
    }

    #[test]
    fn toggle_negate_flips_the_flag_and_touches_nothing_else() {
        let condition = Condition {
            field: "subject".to_string(),
            op: "contains".to_string(),
            value: serde_json::json!("urgent"),
            negate: false,
        };
        let flipped = toggle_negate(&condition);
        assert!(flipped.negate);
        assert_eq!(toggle_negate(&flipped), condition);
    }

    /// `source`'s legal values are a frozen registry, so an `eq` condition
    /// gets the vocabulary rather than a text box. `contains` does not:
    /// substring matching over a whole-value picker is not expressible, and
    /// the engine's own retired-source check draws the line at `eq` for the
    /// same reason.
    #[test]
    fn source_gets_the_registry_picker_under_eq_and_a_text_box_otherwise() {
        assert_eq!(
            widget_for("source", FieldType::String, Operator::Eq),
            ValueWidget::Source,
        );
        assert_eq!(
            widget_for("source", FieldType::String, Operator::Contains),
            ValueWidget::Text,
        );
    }

    /// The picker is `source`'s alone — an `eq` on any other string field
    /// keeps the text box it has always had.
    #[test]
    fn no_other_field_gets_the_source_picker() {
        assert_eq!(
            widget_for("subject", FieldType::String, Operator::Eq),
            ValueWidget::Text,
        );
        assert_eq!(
            widget_for("from", FieldType::String, Operator::Eq),
            ValueWidget::Text,
        );
    }

    #[test]
    fn widget_spelling_round_trips() {
        for widget in ValueWidget::ALL {
            assert_eq!(ValueWidget::parse(widget.as_str()), Some(widget));
        }
        assert_eq!(ValueWidget::parse("slider"), None);
    }
}
