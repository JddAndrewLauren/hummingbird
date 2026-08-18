//! Which operators a rules editor may offer for one field type — the
//! decision `client/web/src/screens/rules/operators.ts` used to hold as a
//! hand-maintained twin of [`hummingbird_rules_engine::Operator::is_legal_for`].
//!
//! That module's own header asked for the two tables to be "kept
//! byte-identical," with nothing mechanical connecting them: the registry
//! export carries field *types*, never legal operators, so a drift was
//! silent on both sides. There is no second table here — every answer below
//! is derived by asking the engine's own gating function, in the engine's
//! own declaration order ([`Operator::ALL`]).

use hummingbird_domain::FieldType;
use hummingbird_rules_engine::Operator;

/// The concrete types a [`FieldType::Dynamic`] field's value can actually
/// resolve to at evaluation time — exactly the arms of
/// [`hummingbird_domain::FieldValue::dynamic_type`].
const DYNAMIC_RESOLVABLE: [FieldType; 4] = [
    FieldType::String,
    FieldType::StringList,
    FieldType::Number,
    FieldType::Bool,
];

/// The operators legal for one field type, in [`Operator::ALL`]'s order.
///
/// [`FieldType::Dynamic`] is the one type the gating table answers `true`
/// to for *every* operator — its legality is decided at evaluation time
/// against the concrete value, not the descriptor. An editor offering all
/// seven there would offer two controls that can never match anything:
/// `FieldValue::dynamic_type` never resolves a bare value to
/// `Timestamp`/`Date`, so `within_next`/`within_last` are unreachable
/// through it. This returns the **reachable** subset instead — derived by
/// asking `is_legal_for` about each type a dynamic value can actually take,
/// so it narrows with that mapping rather than restating its result.
pub fn legal_operators(field_type: FieldType) -> Vec<Operator> {
    Operator::ALL
        .into_iter()
        .filter(|op| match field_type {
            FieldType::Dynamic => DYNAMIC_RESOLVABLE.iter().any(|t| op.is_legal_for(*t)),
            concrete => op.is_legal_for(concrete),
        })
        .collect()
}

/// The operator a fresh condition on a field of `field_type` starts at —
/// always the first of [`legal_operators`], so a newly added row is never
/// in an illegal state. Falls back to [`Operator::Eq`] rather than
/// panicking for a type the gating table legalises nothing for; no such
/// type exists today, and an editor row that opens on a harmless operator
/// is a better failure than a crashed screen.
pub fn default_operator_for(field_type: FieldType) -> Operator {
    legal_operators(field_type)
        .first()
        .copied()
        .unwrap_or(Operator::Eq)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(field_type: FieldType) -> Vec<&'static str> {
        legal_operators(field_type)
            .into_iter()
            .map(Operator::as_str)
            .collect()
    }

    #[test]
    fn string_and_string_list_get_eq_and_contains_only() {
        assert_eq!(names(FieldType::String), ["eq", "contains"]);
        assert_eq!(names(FieldType::StringList), ["eq", "contains"]);
    }

    #[test]
    fn number_gets_eq_gt_lt_only() {
        assert_eq!(names(FieldType::Number), ["eq", "gt", "lt"]);
    }

    #[test]
    fn bool_gets_is_only() {
        assert_eq!(names(FieldType::Bool), ["is"]);
    }

    #[test]
    fn timestamp_and_date_get_the_relative_time_pair_only() {
        assert_eq!(names(FieldType::Timestamp), ["within_next", "within_last"]);
        assert_eq!(names(FieldType::Date), ["within_next", "within_last"]);
    }

    #[test]
    fn dynamic_gets_the_reachable_subset_never_the_relative_time_pair() {
        assert_eq!(names(FieldType::Dynamic), ["eq", "contains", "gt", "lt", "is"]);
    }

    #[test]
    fn every_offered_operator_passes_the_engine_s_own_gate() {
        for field_type in [
            FieldType::String,
            FieldType::StringList,
            FieldType::Number,
            FieldType::Bool,
            FieldType::Timestamp,
            FieldType::Date,
            FieldType::Dynamic,
        ] {
            for op in legal_operators(field_type) {
                assert!(
                    op.is_legal_for(field_type),
                    "{op:?} offered for {field_type:?} but the engine rejects it",
                );
            }
        }
    }

    #[test]
    fn the_default_is_always_the_first_legal_operator() {
        for field_type in [
            FieldType::String,
            FieldType::StringList,
            FieldType::Number,
            FieldType::Bool,
            FieldType::Timestamp,
            FieldType::Date,
            FieldType::Dynamic,
        ] {
            assert_eq!(
                Some(default_operator_for(field_type)),
                legal_operators(field_type).first().copied(),
            );
        }
    }
}
