//! Fire-time rule evaluation (ADR-0013, #133). Reads the event and the
//! rule set as they are at this moment and never mutates either — a
//! verdict is a severity-on-alert plus a tier-on-delivery, nothing else.

use hummingbird_domain::{
    core_field_type, deadline_sort_key, find_kind, find_source, is_valid_deadline, parse_duration,
    shift, Condition, DurationUnit, Event, EventKindEntry, FieldType, FieldValue, Rule, Tier,
};

use crate::operator::Operator;

/// Why a rule (or one of its conditions) cannot be trusted to ever fire —
/// ADR-0013's "must be observable, not silently inert."
#[derive(Debug, Clone, PartialEq)]
pub enum RuleProblem {
    /// `field` is neither a core field nor declared by the rule's kind
    /// (or the rule names no kind and `field` is not core).
    UnknownField { field: String },
    /// The rule names an `event_kind` the registry does not know at all.
    UnknownKind { event_kind: String },
    /// `op` is not a recognized operator, or is not legal for `field`'s
    /// declared type.
    IllegalOperator { field: String, op: String },
    /// `condition.value` cannot possibly satisfy `op` against `field`'s
    /// declared type — a malformed duration literal, an `m`/`h` unit on a
    /// `date`-typed field (ADR-0013 rejects both "at save"), or a literal
    /// of the wrong JSON kind entirely (e.g. a number where `eq` needs a
    /// string). Reported here rather than left to evaluate silently to
    /// `false` — which, negated, would make the condition **always true**.
    MalformedValue { field: String, reason: String },
    /// `field` is `source`, `op` is `eq`, and the named literal resolves
    /// (via the ADR-0014 registry) to a source that has been retired in
    /// favor of `successor` — a condition naming it can never fire again.
    /// Distinct from an **unregistered** source, which is not a problem
    /// (`items.source`/`context_snapshots.source` values legitimately
    /// never appear in this registry at all).
    RetiredSource { source: String, successor: String },
}

/// One rule's outcome against one event.
#[derive(Debug, Clone, PartialEq)]
pub enum RuleOutcome {
    Matched(Verdict),
    NotMatched,
    /// The rule cannot be trusted — surfaced rather than silently treated
    /// as non-matching, per ADR-0013's invalid-rule requirement.
    Invalid(Vec<RuleProblem>),
}

/// What a matching rule stamps (ADR-0013): severity on the alert it mints,
/// tier on the delivery. `severity` is populated even for `mints: false`
/// kinds — a rule author always names one — but callers must ignore it
/// there ("`rules.severity` is simply unused for these", ADR-0013); this
/// crate does not know a kind's `mints` flag on its own and leaves that
/// check to the caller, which does (the pollers, #135-137; the DO alarm,
/// #138; the delivery leg, #139).
#[derive(Debug, Clone, PartialEq)]
pub struct Verdict {
    pub rule_id: String,
    pub severity: String,
    pub tier: Tier,
}

/// Validates a rule's conditions against its own declared `event_kind`,
/// independent of any particular event — the check #140 runs at load time
/// to flag an invalid rule before it is ever fired against anything.
pub fn validate_rule(rule: &Rule) -> Vec<RuleProblem> {
    let mut problems = Vec::new();
    let kind_entry = match &rule.event_kind {
        None => None,
        Some(k) => match find_kind(k) {
            Some(entry) => Some(entry),
            None => {
                problems.push(RuleProblem::UnknownKind { event_kind: k.clone() });
                None
            }
        },
    };
    for condition in &rule.conditions {
        match field_type_for(kind_entry, &condition.field) {
            Ok(field_type) => match Operator::parse(&condition.op) {
                Some(op) if op.is_legal_for(field_type) => {
                    if let Some(reason) = malformed_value_reason(field_type, op, &condition.value) {
                        problems.push(RuleProblem::MalformedValue {
                            field: condition.field.clone(),
                            reason,
                        });
                    } else {
                        problems.extend(retired_source_problems(&condition.field, op, &condition.value));
                    }
                }
                _ => problems.push(RuleProblem::IllegalOperator {
                    field: condition.field.clone(),
                    op: condition.op.clone(),
                }),
            },
            Err(problem) => problems.push(problem),
        }
    }
    problems
}

/// `Some(reason)` when `value` cannot possibly satisfy `op` against
/// `field_type` — ADR-0013's two save-time duration rules (a malformed
/// duration literal; `m`/`h` on a `date` field) plus a general check that
/// every operator's literal is at least the right JSON kind. Skipped
/// entirely for [`FieldType::Dynamic`] fields: `snapshot_change`'s
/// `value`/`previous` declare their real type per key, at wiring time
/// (#135-137), which this crate does not have — legality for those is
/// checked against the actual value at evaluation time instead (see
/// `evaluate_condition`).
fn malformed_value_reason(field_type: FieldType, op: Operator, value: &serde_json::Value) -> Option<String> {
    if field_type == FieldType::Dynamic {
        return None;
    }
    let literals = normalize_condition_values(value);
    if literals.is_empty() {
        return Some("value has no literal to compare against".to_string());
    }
    match op {
        Operator::WithinNext | Operator::WithinLast => {
            for literal in &literals {
                let Some(s) = literal.as_str() else {
                    return Some(format!("duration literal {literal} is not a string"));
                };
                let Some((_, unit)) = parse_duration(s) else {
                    return Some(format!(
                        "\"{s}\" is not a valid duration literal (an integer immediately \
                         followed by one unit letter m/h/d, no compounds, no sign)"
                    ));
                };
                if field_type == FieldType::Date && unit != DurationUnit::Days {
                    return Some(format!("\"{s}\": a `date`-typed field accepts `d` units only"));
                }
            }
            None
        }
        Operator::Eq | Operator::Contains => literals
            .iter()
            .all(|l| l.as_str().is_none())
            .then(|| "value must be a string or a list of strings".to_string()),
        Operator::Gt | Operator::Lt => literals
            .iter()
            .all(|l| l.as_f64().is_none())
            .then(|| "value must be a number or a list of numbers".to_string()),
        Operator::Is => literals
            .iter()
            .all(|l| l.as_bool().is_none())
            .then(|| "value must be a bool or a list of bools".to_string()),
    }
}

/// ADR-0014's second registry job: a rule naming a source that has since
/// been retired by a namespace bump matches nothing ever again — flagged
/// here rather than left silently non-firing (ADR-0013/0014). Only
/// `source` conditions using `eq` are checked: `contains` does substring
/// matching and cannot name one source unambiguously, and an
/// **unregistered** source is never itself a problem (`find_source`
/// returning `None` means "not an alert source," not "unknown" — see
/// `hummingbird_domain::sources`'s module doc). Independent of `negate`:
/// a dead condition must not read as "always true" once inverted.
fn retired_source_problems(field: &str, op: Operator, value: &serde_json::Value) -> Vec<RuleProblem> {
    if field != "source" || op != Operator::Eq {
        return Vec::new();
    }
    normalize_condition_values(value)
        .iter()
        .filter_map(|literal| literal.as_str())
        .filter_map(|source| {
            let entry = find_source(source)?;
            let successor = entry.retired_as?;
            Some(RuleProblem::RetiredSource {
                source: source.to_string(),
                successor: successor.to_string(),
            })
        })
        .collect()
}

/// Evaluates one rule against one event. `now` must be a valid
/// [`hummingbird_domain::is_valid_deadline`]-shaped date-time (minute
/// precision, naive local, e.g. `"2026-08-15T09:30"`) in the one
/// Worker-configured timezone (ADR-0013) — callers own producing it; this
/// crate has no clock of its own (the same caller-injects-the-clock rule
/// the client's sync engine follows).
pub fn evaluate_rule(rule: &Rule, event: &Event, now: &str) -> RuleOutcome {
    if !rule.enabled {
        return RuleOutcome::NotMatched;
    }
    let problems = validate_rule(rule);
    if !problems.is_empty() {
        return RuleOutcome::Invalid(problems);
    }
    // A NULL `event_kind` means "any kind" (ADR-0013); otherwise the rule
    // only applies to events of its own declared kind.
    if let Some(rule_kind) = &rule.event_kind {
        if event.event_kind.as_deref() != Some(rule_kind.as_str()) {
            return RuleOutcome::NotMatched;
        }
    }
    let kind_entry = rule.event_kind.as_deref().and_then(find_kind);
    for condition in &rule.conditions {
        match evaluate_condition(kind_entry, event, condition, now) {
            Ok(true) => continue,
            Ok(false) => return RuleOutcome::NotMatched,
            Err(problem) => return RuleOutcome::Invalid(vec![problem]),
        }
    }
    RuleOutcome::Matched(Verdict {
        rule_id: rule.id.clone(),
        severity: rule.severity.clone(),
        tier: rule.tier,
    })
}

/// Evaluates every rule in `rules` against `event`, in order, pairing each
/// rule id with its outcome. Conditions within a rule are ANDed; rules
/// across the set are ORed (ADR-0013) — that OR is the caller's to apply,
/// by collecting every [`RuleOutcome::Matched`] here rather than
/// short-circuiting on the first.
pub fn evaluate_rules(rules: &[Rule], event: &Event, now: &str) -> Vec<(String, RuleOutcome)> {
    rules.iter().map(|r| (r.id.clone(), evaluate_rule(r, event, now))).collect()
}

fn field_type_for(kind_entry: Option<&EventKindEntry>, field: &str) -> Result<FieldType, RuleProblem> {
    if let Some(t) = core_field_type(field) {
        return Ok(t);
    }
    if let Some(entry) = kind_entry {
        if let Some(descriptor) = entry.fields.iter().find(|f| f.name == field) {
            return Ok(descriptor.field_type);
        }
    }
    Err(RuleProblem::UnknownField { field: field.to_string() })
}

fn resolve_field_value(kind_entry: Option<&EventKindEntry>, event: &Event, field: &str) -> Option<FieldValue> {
    if core_field_type(field).is_some() {
        return event.core_value(field);
    }
    if kind_entry.is_some() {
        return event.extras.get(field).cloned();
    }
    None
}

fn evaluate_condition(
    kind_entry: Option<&EventKindEntry>,
    event: &Event,
    condition: &Condition,
    now: &str,
) -> Result<bool, RuleProblem> {
    // `validate_rule` already guarantees these succeed for a rule reaching
    // here — recomputed cheaply rather than threaded through as state.
    let field_type = field_type_for(kind_entry, &condition.field).expect("validated by validate_rule");
    let op = Operator::parse(&condition.op).expect("validated by validate_rule");

    // Evaluation rule 2 (ADR-0013): a missing field makes its condition
    // false, even negated. `Event::core_value` already resolves
    // `calendar_busy`'s default, so this is the one check for both cases.
    let Some(value) = resolve_field_value(kind_entry, event, &condition.field) else {
        return Ok(false);
    };

    // `Dynamic` fields (snapshot_change's `value`/`previous`) are gated at
    // the actual value's type, since the descriptor itself carries none.
    if field_type == FieldType::Dynamic && !op.is_legal_for(value.dynamic_type()) {
        return Err(RuleProblem::IllegalOperator {
            field: condition.field.clone(),
            op: condition.op.clone(),
        });
    }

    let raw_matched = match op {
        Operator::Eq | Operator::Contains => string_family_match(op, &value, &condition.value),
        Operator::Gt | Operator::Lt => numeric_match(op, &value, &condition.value),
        Operator::Is => bool_match(&value, &condition.value),
        Operator::WithinNext | Operator::WithinLast => {
            time_match(op, field_type, &value, &condition.value, now)
        }
    }
    .unwrap_or(false);

    Ok(raw_matched != condition.negate)
}

/// A condition's `value` is a scalar or a list — a list means any-of
/// (ADR-0013). Normalizes both shapes to a list of JSON values.
fn normalize_condition_values(value: &serde_json::Value) -> Vec<serde_json::Value> {
    match value.as_array() {
        Some(arr) => arr.clone(),
        None => vec![value.clone()],
    }
}

/// `eq`/`contains` on a `String` or `StringList` field: list fields take
/// the same two string ops applied to any element, and a list condition
/// value means any-of — so this is a cross-product any-any match, always
/// case-insensitive. Both operators fold through `to_lowercase()`
/// (Unicode case folding, not `eq_ignore_ascii_case`'s ASCII-only fold) —
/// one casing discipline for both, since ADR-0013 says "all string
/// comparison is case-insensitive" without carving out non-ASCII text.
/// `None` when the field's runtime value isn't a string family, or the
/// condition carries no usable string literal.
fn string_family_match(op: Operator, value: &FieldValue, condition_value: &serde_json::Value) -> Option<bool> {
    let field_strs: Vec<&str> = match value {
        FieldValue::Str(s) => vec![s.as_str()],
        FieldValue::StrList(list) => list.iter().map(String::as_str).collect(),
        _ => return None,
    };
    let literals = normalize_condition_values(condition_value);
    let cond_strs: Vec<&str> = literals.iter().filter_map(|v| v.as_str()).collect();
    if cond_strs.is_empty() {
        return None;
    }
    Some(field_strs.iter().any(|field_str| {
        cond_strs.iter().any(|cond_str| match op {
            Operator::Eq => field_str.to_lowercase() == cond_str.to_lowercase(),
            Operator::Contains => field_str.to_lowercase().contains(&cond_str.to_lowercase()),
            _ => false,
        })
    }))
}

fn numeric_match(op: Operator, value: &FieldValue, condition_value: &serde_json::Value) -> Option<bool> {
    let field_num = match value {
        FieldValue::Num(n) => *n,
        _ => return None,
    };
    let literals = normalize_condition_values(condition_value);
    let cond_nums: Vec<f64> = literals.iter().filter_map(|v| v.as_f64()).collect();
    if cond_nums.is_empty() {
        return None;
    }
    Some(cond_nums.iter().any(|cond_num| match op {
        Operator::Eq => (field_num - cond_num).abs() < f64::EPSILON,
        Operator::Gt => field_num > *cond_num,
        Operator::Lt => field_num < *cond_num,
        _ => false,
    }))
}

fn bool_match(value: &FieldValue, condition_value: &serde_json::Value) -> Option<bool> {
    let field_bool = match value {
        FieldValue::Bool(b) => *b,
        _ => return None,
    };
    let literals = normalize_condition_values(condition_value);
    let cond_bools: Vec<bool> = literals.iter().filter_map(|v| v.as_bool()).collect();
    if cond_bools.is_empty() {
        return None;
    }
    Some(cond_bools.contains(&field_bool))
}

/// `within_next`/`within_last` (ADR-0013): direction is the operator,
/// never inferred. Comparison stays string-based throughout, reusing
/// [`deadline_sort_key`] for a `Timestamp` field's day-only resolution —
/// the one comparison key ADR-0013 requires shared between the sort and
/// the evaluator — rather than parsing to a second, numeric notion of
/// "when this is."
fn time_match(
    op: Operator,
    field_type: FieldType,
    value: &FieldValue,
    condition_value: &serde_json::Value,
    now: &str,
) -> Option<bool> {
    let field_str = match value {
        FieldValue::Str(s) => s.as_str(),
        _ => return None,
    };
    let literals = normalize_condition_values(condition_value);
    let durations: Vec<&str> = literals.iter().filter_map(|v| v.as_str()).collect();
    if durations.is_empty() {
        return None;
    }
    Some(durations.iter().any(|duration| {
        single_time_match(op, field_type, field_str, duration, now).unwrap_or(false)
    }))
}

fn single_time_match(op: Operator, field_type: FieldType, field_str: &str, duration: &str, now: &str) -> Option<bool> {
    let (amount, unit) = parse_duration(duration)?;
    let signed = match op {
        // `within_next D` means `t <= now + D`, unbounded on the past
        // side — an overdue item still matches (ADR-0013, deliberately).
        Operator::WithinNext => amount,
        // Symmetric on the other side: `within_last D` means
        // `t >= now - D`, unbounded on the future side.
        Operator::WithinLast => -amount,
        _ => return None,
    };
    match field_type {
        FieldType::Date => {
            // `date` fields accept `d` units only (ADR-0013).
            if unit != DurationUnit::Days || field_str.len() != 10 {
                return None;
            }
            let now_date = now.get(..10)?;
            let cutoff = shift(now_date, signed, DurationUnit::Days)?;
            Some(match op {
                Operator::WithinNext => field_str <= cutoff.as_str(),
                Operator::WithinLast => field_str >= cutoff.as_str(),
                _ => unreachable!(),
            })
        }
        FieldType::Timestamp => {
            if !is_valid_deadline(field_str) {
                return None;
            }
            let resolved_field = deadline_sort_key(field_str);
            let raw_cutoff = shift(now, signed, unit)?;
            // `shift` only stays date-only for a whole-`Days` shift of a
            // date-only `now` (itself already an edge case here — `now`
            // is documented as always minute-precision). Running the
            // cutoff through the same `deadline_sort_key` resolution the
            // field went through guarantees both sides of this comparison
            // share one convention, however either one arrived as
            // date-only, rather than trusting the two callers to agree by
            // construction.
            let cutoff = deadline_sort_key(&raw_cutoff);
            Some(match op {
                Operator::WithinNext => resolved_field.as_ref() <= cutoff.as_ref(),
                Operator::WithinLast => resolved_field.as_ref() >= cutoff.as_ref(),
                _ => unreachable!(),
            })
        }
        _ => None,
    }
}
