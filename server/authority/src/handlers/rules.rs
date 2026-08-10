//! `POST /api/rules` and `PATCH /api/rules/:id` — the notification lane's
//! rule table (ADR-0012, amended by ADR-0013), on the same idempotent-create
//! and CAS-patch shape as `items`. Validating `field`/`op`/`value` against
//! the typed catalogue is #133's job, not this handler's: `conditions` is
//! stored and returned exactly as given.

use hummingbird_domain::{CreateRule, Rule, RulePatch, Tier};

use super::{conflict, error, json, parse_body, read_meta_version, write_meta_version, ApiResponse};
use crate::codec::{bad_cell, RowReader, Sets};
use crate::sql::{Row, Sql, SqlError, SqlValue};

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

    let mut sets = Sets::new();
    if let Some(name) = &patch.name {
        sets.set("name", SqlValue::Text(name.clone()));
    }
    if let Some(event_kind) = &patch.event_kind {
        sets.set("event_kind", SqlValue::from_opt_text(event_kind.as_deref()));
    }
    if let Some(conditions) = &patch.conditions {
        let encoded = serde_json::to_string(conditions).map_err(|e| SqlError {
            message: format!("conditions did not encode: {e}"),
        })?;
        sets.set("conditions", SqlValue::Text(encoded));
    }
    if let Some(severity) = &patch.severity {
        sets.set("severity", SqlValue::Text(severity.clone()));
    }
    if let Some(tier) = patch.tier {
        sets.set("tier", SqlValue::Text(tier.as_str().to_string()));
    }
    if let Some(enabled) = patch.enabled {
        sets.set("enabled", SqlValue::Integer(enabled as i64));
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

pub(super) fn rule_from_row(row: &Row) -> Result<Rule, SqlError> {
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
