//! `POST /api/rules` and `PATCH /api/rules/:id` — the notification lane's
//! rule table (ADR-0012, amended by ADR-0013), on the same idempotent-create
//! and CAS-patch shape as `items`. `field`/`op`/`value` are validated
//! against the typed catalogue by #133's `hummingbird-rules-engine` before
//! either write persists (#186) — a malformed condition is rejected at save
//! rather than only discovered at fire time.

use hummingbird_domain::{CreateRule, Rule, RulePatch, Tier};
use hummingbird_rules_engine::{validate_rule, RuleProblem};

use super::{conflict, error, json, parse_body, read_meta_version, write_meta_version, ApiResponse};
use crate::codec::{bad_cell, RowReader, Sets};
use crate::sql::{Row, Sql, SqlError, SqlValue};

/// Problems worth rejecting a save over. `UnknownKind` is deliberately
/// excluded: `event_kind` is an open registry key, not a closed vocabulary
/// (ADR-0013; `hummingbird_domain::Rule::event_kind`'s own doc — "a kind
/// the code does not yet know is legal") — a rule may name a source that
/// hasn't been wired up yet. ADR-0013 only requires *field* and *value*
/// problems to be rejected at save ("an unknown field is rejected at
/// save", "[a malformed duration] rejected at save", "`m`/`h` on a `date`
/// field are rejected at save"); an unrecognized kind is instead flagged
/// invalid at load/fire time (`rules-engine`'s own `evaluate_rule`), never
/// blocked at save.
fn save_time_problems(rule: &Rule) -> Vec<RuleProblem> {
    validate_rule(rule)
        .into_iter()
        .filter(|p| !matches!(p, RuleProblem::UnknownKind { .. }))
        .collect()
}

fn describe_problem(problem: &RuleProblem) -> String {
    match problem {
        RuleProblem::UnknownField { field } => format!("unknown field `{field}`"),
        RuleProblem::UnknownKind { event_kind } => format!("unknown event_kind `{event_kind}`"),
        RuleProblem::IllegalOperator { field, op } => {
            format!("operator `{op}` is not legal for field `{field}`")
        }
        RuleProblem::MalformedValue { field, reason } => format!("field `{field}`: {reason}"),
        RuleProblem::RetiredSource { source, successor } => {
            format!("source `{source}` is retired (bumped to `{successor}`)")
        }
    }
}

/// `Some(400 response)` when `rule` has a save-time problem, `None` when
/// it is clean to persist.
fn validation_error(rule: &Rule) -> Option<ApiResponse> {
    let problems = save_time_problems(rule);
    if problems.is_empty() {
        return None;
    }
    let reason = problems.iter().map(describe_problem).collect::<Vec<_>>().join("; ");
    Some(error(400, "validation", &reason))
}

pub fn create(body: Option<&str>, now_ms: i64, sql: &dyn Sql) -> Result<ApiResponse, SqlError> {
    let create: CreateRule = match parse_body(body) {
        Ok(v) => v,
        Err(resp) => return Ok(resp),
    };
    if create.id.is_empty() {
        return Ok(error(400, "validation", "id must be non-empty"));
    }

    // Idempotent by client-supplied id, same rule as items::create: a
    // replay is answered with the current row, no write, no version bump.
    if let Some(row) = select_rule(sql, &create.id)? {
        return Ok(json(200, &rule_from_row(&row)?));
    }

    if create.name.is_empty() {
        return Ok(error(400, "validation", "name must be non-empty"));
    }
    if create.severity.is_empty() {
        return Ok(error(400, "validation", "severity must be non-empty"));
    }

    let version = read_meta_version(sql)? + 1;
    let rule = Rule {
        id: create.id,
        name: create.name,
        event_kind: create.event_kind,
        conditions: create.conditions,
        severity: create.severity,
        tier: create.tier,
        enabled: create.enabled.unwrap_or(true),
        updated_at: now_ms,
        version,
    };
    if let Some(resp) = validation_error(&rule) {
        return Ok(resp);
    }
    sql.exec(
        "INSERT INTO rules (id, name, event_kind, conditions, severity, tier, enabled, \
         updated_at, version) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        &rule_params(&rule)?,
    )?;
    write_meta_version(sql, version)?;
    Ok(json(201, &rule))
}

pub fn patch(
    id: &str,
    body: Option<&str>,
    now_ms: i64,
    sql: &dyn Sql,
) -> Result<ApiResponse, SqlError> {
    let patch: RulePatch = match parse_body(body) {
        Ok(v) => v,
        Err(resp) => return Ok(resp),
    };
    if patch.name.as_deref() == Some("") {
        return Ok(error(400, "validation", "name must be non-empty"));
    }
    if patch.severity.as_deref() == Some("") {
        return Ok(error(400, "validation", "severity must be non-empty"));
    }

    let Some(row) = select_rule(sql, id)? else {
        return Ok(error(404, "not_found", "no such rule"));
    };
    let current = rule_from_row(&row)?;
    if current.version != patch.expected_version {
        // A 409 carrying the current rule so the client can rebase
        // (ADR-0008), the same contract as every other CAS write.
        return Ok(conflict(&current));
    }

    // Validate the *post-patch* condition set, not just what changed — a
    // patch must not be able to smuggle in a condition a create would
    // reject (#186), e.g. patching only `event_kind` onto a rule whose
    // untouched `conditions` are only legal under the old kind.
    let candidate = Rule {
        name: patch.name.clone().unwrap_or_else(|| current.name.clone()),
        event_kind: patch.event_kind.clone().unwrap_or_else(|| current.event_kind.clone()),
        conditions: patch.conditions.clone().unwrap_or_else(|| current.conditions.clone()),
        severity: patch.severity.clone().unwrap_or_else(|| current.severity.clone()),
        tier: patch.tier.unwrap_or(current.tier),
        enabled: patch.enabled.unwrap_or(current.enabled),
        ..current.clone()
    };
    if let Some(resp) = validation_error(&candidate) {
        return Ok(resp);
    }

    // Compared typed against `current` (already the deserialized row) —
    // never a bare SQL-value `==`, which the integer/real hazard on a DO
    // cursor would make silently unequal on `f64` vs `Integer` (see the
    // sql module doc for `SqlValue::as_i64`). A field whose submitted
    // value already matches the stored one is left out of `sets` entirely,
    // same as an untouched field (#166, #221).
    let mut sets = Sets::new();
    if let Some(name) = &patch.name {
        if *name != current.name {
            sets.set("name", SqlValue::Text(name.clone()));
        }
    }
    if let Some(event_kind) = &patch.event_kind {
        if *event_kind != current.event_kind {
            sets.set("event_kind", SqlValue::from_opt_text(event_kind.as_deref()));
        }
    }
    if let Some(conditions) = &patch.conditions {
        if *conditions != current.conditions {
            let encoded = serde_json::to_string(conditions).map_err(|e| SqlError {
                message: format!("conditions did not encode: {e}"),
            })?;
            sets.set("conditions", SqlValue::Text(encoded));
        }
    }
    if let Some(severity) = &patch.severity {
        if *severity != current.severity {
            sets.set("severity", SqlValue::Text(severity.clone()));
        }
    }
    if let Some(tier) = patch.tier {
        if tier != current.tier {
            sets.set("tier", SqlValue::Text(tier.as_str().to_string()));
        }
    }
    if let Some(enabled) = patch.enabled {
        if enabled != current.enabled {
            sets.set("enabled", SqlValue::Integer(enabled as i64));
        }
    }
    if sets.is_empty() {
        return Ok(json(200, &current));
    }

    let version = read_meta_version(sql)? + 1;
    sets.set("updated_at", SqlValue::Integer(now_ms));
    sets.set("version", SqlValue::Integer(version));
    let update = sets.update_sql("rules", "id = ?");
    let mut params = sets.into_params();
    params.push(SqlValue::Text(id.to_string()));
    sql.exec(&update, &params)?;
    write_meta_version(sql, version)?;

    let row = select_rule(sql, id)?.ok_or_else(|| SqlError {
        message: "row vanished mid-update".into(),
    })?;
    Ok(json(200, &rule_from_row(&row)?))
}

/// Which `event_kind`(s) an evaluated-stream poller's `ingest` token may
/// read back through [`list`], keyed by its bound source (#135-137,
/// following the reviewer's item 2 on #264). A rule with `event_kind: None`
/// evaluates against events of **any** kind (`Rule`'s own "any kind"
/// meaning) and so is always included regardless of this table — a poller
/// must see it too, since it applies to its own events exactly as much as
/// anyone else's.
///
/// This is what keeps `GET /api/rules` from silently widening every
/// existing ingest token's reach to the operator's entire rule catalogue —
/// including `CITY_WASTE_INGEST_TOKEN`, which sits in GitHub Actions under
/// the recorded justification that its worst-case abuse is a wrong bin
/// day. An unrecognised or unbound source reads only the any-kind rules:
/// the safe default is nothing extra, never everything.
fn event_kinds_readable_by(source: &str) -> &'static [&'static str] {
    match source {
        hummingbird_domain::GMAIL_V1 => &["email"],
        hummingbird_domain::GOOGLE_CALENDAR_V1 => &["calendar_event"],
        _ => &[],
    }
}

/// `GET /api/rules` — every rule, enabled or not, for a device token; for
/// an `ingest` token, only the rules its bound source is entitled to see
/// (#135-137, [`event_kinds_readable_by`]). Ordinary device clients already
/// get the full set through `GET /api/changes`'s `rules` field; this route
/// exists for the evaluated-stream pollers, which hold an `ingest` token
/// and cannot reach the delta-pull lane at all (`auth::permitted`'s
/// device-only default). A poller evaluates every rule in memory itself
/// (`hummingbird_rules_engine::evaluate_rules` already skips a disabled
/// one), so disabled rules are included here too — filtering them here
/// would just be a second, easy-to-drift copy of the engine's own
/// `enabled` check.
///
/// A row this handler cannot parse propagates as a `SqlError`, exactly as
/// `changes.rs`'s `pull` does for the same table — silently dropping it
/// would be a rule that quietly stops existing, in the one consumer whose
/// entire job is firing rules.
pub fn list(token_source: Option<&str>, sql: &dyn Sql) -> Result<ApiResponse, SqlError> {
    let rows = sql.exec("SELECT * FROM rules ORDER BY id", &[])?;
    let rules: Vec<hummingbird_domain::Rule> =
        rows.iter().map(rule_from_row).collect::<Result<_, _>>()?;
    let rules = match token_source {
        // `None` is a device token (unrestricted) or, in principle, an
        // unbound ingest token — which cannot exist outside a raw seam
        // that bypassed the mint handler (`auth::Principal`'s own doc).
        None => rules,
        Some(source) => {
            let allowed = event_kinds_readable_by(source);
            rules
                .into_iter()
                .filter(|r| r.event_kind.as_deref().is_none_or(|k| allowed.contains(&k)))
                .collect()
        }
    };
    Ok(json(200, &rules))
}

fn select_rule(sql: &dyn Sql, id: &str) -> Result<Option<Row>, SqlError> {
    Ok(sql
        .exec(
            "SELECT * FROM rules WHERE id = ?",
            &[SqlValue::Text(id.to_string())],
        )?
        .into_iter()
        .next())
}

/// The INSERT's parameter list, in exactly its column order.
fn rule_params(rule: &Rule) -> Result<Vec<SqlValue>, SqlError> {
    let conditions = serde_json::to_string(&rule.conditions).map_err(|e| SqlError {
        message: format!("conditions did not encode: {e}"),
    })?;
    Ok(vec![
        SqlValue::Text(rule.id.clone()),
        SqlValue::Text(rule.name.clone()),
        SqlValue::from_opt_text(rule.event_kind.as_deref()),
        SqlValue::Text(conditions),
        SqlValue::Text(rule.severity.clone()),
        SqlValue::Text(rule.tier.as_str().to_string()),
        SqlValue::Integer(rule.enabled as i64),
        SqlValue::Integer(rule.updated_at),
        SqlValue::Integer(rule.version),
    ])
}

pub(crate) fn rule_from_row(row: &Row) -> Result<Rule, SqlError> {
    let r = RowReader(row);
    let tier_text = r.text("tier")?;
    let conditions_text = r.text("conditions")?;
    let conditions = serde_json::from_str(&conditions_text).map_err(|_| bad_cell("conditions"))?;
    Ok(Rule {
        id: r.text("id")?,
        name: r.text("name")?,
        event_kind: r.opt_text("event_kind"),
        conditions,
        severity: r.text("severity")?,
        tier: Tier::parse(&tier_text).ok_or_else(|| bad_cell("tier"))?,
        enabled: r.bool_int("enabled")?,
        updated_at: r.int("updated_at")?,
        version: r.int("version")?,
    })
}
