//! The ADR-0013 operator vocabulary and its type-gating table.

use hummingbird_domain::FieldType;

/// The wire operator strings from `Condition.op`, parsed and typed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operator {
    Eq,
    Contains,
    Gt,
    Lt,
    Is,
    WithinNext,
    WithinLast,
}

impl Operator {
    pub fn parse(s: &str) -> Option<Operator> {
        match s {
            "eq" => Some(Operator::Eq),
            "contains" => Some(Operator::Contains),
            "gt" => Some(Operator::Gt),
            "lt" => Some(Operator::Lt),
            "is" => Some(Operator::Is),
            "within_next" => Some(Operator::WithinNext),
            "within_last" => Some(Operator::WithinLast),
            _ => None,
        }
    }

    /// ADR-0013's operator/type gating table. `Dynamic` fields
    /// (`snapshot_change.value`/`previous`) are gated separately, against
    /// the runtime value's own type — see [`hummingbird_domain::FieldValue::dynamic_type`].
    pub fn is_legal_for(self, field_type: FieldType) -> bool {
        match field_type {
            FieldType::String => matches!(self, Operator::Eq | Operator::Contains),
            FieldType::StringList => matches!(self, Operator::Eq | Operator::Contains),
            FieldType::Number => matches!(self, Operator::Eq | Operator::Gt | Operator::Lt),
            FieldType::Bool => matches!(self, Operator::Is),
            FieldType::Timestamp => matches!(self, Operator::WithinNext | Operator::WithinLast),
            FieldType::Date => matches!(self, Operator::WithinNext | Operator::WithinLast),
            // A `Dynamic` field's legality is decided at evaluation time
            // against the concrete value found on the event, never here.
            FieldType::Dynamic => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_wire_operator() {
        for (s, op) in [
            ("eq", Operator::Eq),
            ("contains", Operator::Contains),
            ("gt", Operator::Gt),
            ("lt", Operator::Lt),
            ("is", Operator::Is),
            ("within_next", Operator::WithinNext),
            ("within_last", Operator::WithinLast),
        ] {
            assert_eq!(Operator::parse(s), Some(op));
        }
        assert_eq!(Operator::parse("not_a_real_op"), None);
    }

    #[test]
    fn string_gets_eq_and_contains_only() {
        assert!(Operator::Eq.is_legal_for(FieldType::String));
        assert!(Operator::Contains.is_legal_for(FieldType::String));
        assert!(!Operator::Gt.is_legal_for(FieldType::String));
        assert!(!Operator::WithinNext.is_legal_for(FieldType::String));
    }

    #[test]
    fn number_gets_eq_gt_lt_only() {
        assert!(Operator::Eq.is_legal_for(FieldType::Number));
        assert!(Operator::Gt.is_legal_for(FieldType::Number));
        assert!(Operator::Lt.is_legal_for(FieldType::Number));
        assert!(!Operator::Contains.is_legal_for(FieldType::Number));
        assert!(!Operator::Is.is_legal_for(FieldType::Number));
    }

    #[test]
    fn bool_gets_is_only() {
        assert!(Operator::Is.is_legal_for(FieldType::Bool));
        assert!(!Operator::Eq.is_legal_for(FieldType::Bool));
    }

    #[test]
    fn timestamp_and_date_get_relative_time_only() {
        for op in [Operator::WithinNext, Operator::WithinLast] {
            assert!(op.is_legal_for(FieldType::Timestamp));
            assert!(op.is_legal_for(FieldType::Date));
        }
        assert!(!Operator::Eq.is_legal_for(FieldType::Timestamp));
        assert!(!Operator::Gt.is_legal_for(FieldType::Date));
    }
}
