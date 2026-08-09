//! The wire DTOs of the authority's API (ADR-0008): create, patch,
//! delta-read, and the error shapes.

use serde::{Deserialize, Deserializer, Serialize};

use crate::item::{Energy, Item, Size, Stage};

/// `POST /api/items` body. `id` is the client-supplied deterministic id the
/// create is idempotent by; the server stamps `seq`, timestamps and
/// `version` — they cannot be supplied. `deny_unknown_fields` makes a typo'd
/// (or server-stamped) field a 400, not a silent no-op.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateItem {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Defaults to `triage` — capture lands in the inbox.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage: Option<Stage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<Size>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub energy: Option<Energy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    /// Defaults to 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_pos: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due_date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheduled_date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
}

/// Marks JSON presence: absent field deserializes to `None` (untouched),
/// `"field": null` to `Some(None)` (clear), a value to `Some(Some(v))`.
fn touched<'de, T, D>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

/// `NOT NULL` columns cannot be cleared: an explicit JSON `null` is a
/// deserialize error, not a silent skip.
fn non_null<'de, T, D>(deserializer: D, field: &'static str) -> Result<Option<T>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    match Option::<T>::deserialize(deserializer)? {
        Some(v) => Ok(Some(v)),
        None => Err(serde::de::Error::custom(format!("{field} may not be null"))),
    }
}

fn non_null_title<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    non_null(d, "title")
}

fn non_null_stage<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Stage>, D::Error> {
    non_null(d, "stage")
}

fn non_null_priority<'de, D: Deserializer<'de>>(d: D) -> Result<Option<i64>, D::Error> {
    non_null(d, "priority")
}

/// `PATCH /api/items/:id` body: `expected_version` plus absolute-value
/// sets. Every mutation states the entire new value of each field it
/// touches (ADR-0008) — S3's rebase-on-409 compares *touched* fields, so
/// absent-vs-null fidelity here is load-bearing.
///
/// Nullable columns are double-`Option`: outer = touched at all, inner =
/// the new value (`None` clears). `NOT NULL` columns are single-`Option`
/// and cannot be cleared — an explicit `null` on them is a 400.
/// `deny_unknown_fields` makes a typo'd field a 400, not a silent no-op.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemPatch {
    pub expected_version: i64,
    #[serde(default, deserialize_with = "non_null_title", skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, deserialize_with = "touched", skip_serializing_if = "Option::is_none")]
    pub description: Option<Option<String>>,
    #[serde(default, deserialize_with = "non_null_stage", skip_serializing_if = "Option::is_none")]
    pub stage: Option<Stage>,
    #[serde(default, deserialize_with = "touched", skip_serializing_if = "Option::is_none")]
    pub size: Option<Option<Size>>,
    #[serde(default, deserialize_with = "touched", skip_serializing_if = "Option::is_none")]
    pub energy: Option<Option<Energy>>,
    #[serde(default, deserialize_with = "touched", skip_serializing_if = "Option::is_none")]
    pub context: Option<Option<String>>,
    #[serde(default, deserialize_with = "non_null_priority", skip_serializing_if = "Option::is_none")]
    pub priority: Option<i64>,
    #[serde(default, deserialize_with = "touched", skip_serializing_if = "Option::is_none")]
    pub project_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "touched", skip_serializing_if = "Option::is_none")]
    pub project_pos: Option<Option<i64>>,
    #[serde(default, deserialize_with = "touched", skip_serializing_if = "Option::is_none")]
    pub due_date: Option<Option<String>>,
    #[serde(default, deserialize_with = "touched", skip_serializing_if = "Option::is_none")]
    pub scheduled_date: Option<Option<String>>,
    #[serde(default, deserialize_with = "touched", skip_serializing_if = "Option::is_none")]
    pub archived_at: Option<Option<i64>>,
}

/// `GET /api/changes?since=N` response: the workspace version and every
/// item row whose `version` is above the cursor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChangesResponse {
    pub version: i64,
    pub items: Vec<Item>,
}

/// Every non-2xx body except the 409: `{"error": code, "message": …}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiError {
    pub error: String,
    pub message: String,
}

pub const VERSION_CONFLICT: &str = "version_conflict";

/// The 409 body: a stale `expected_version` write is answered with the
/// current entity so the client can rebase (ADR-0008).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConflictResponse {
    /// Always [`VERSION_CONFLICT`].
    pub error: String,
    pub current: Item,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patch_absent_vs_null_vs_value() {
        let p: ItemPatch = serde_json::from_str(
            r#"{"expected_version": 3, "description": null, "context": "@calls"}"#,
        )
        .unwrap();
        assert_eq!(p.expected_version, 3);
        assert_eq!(p.description, Some(None), "explicit null = clear");
        assert_eq!(p.context, Some(Some("@calls".into())), "value = set");
        assert_eq!(p.due_date, None, "absent = untouched");
        assert_eq!(p.title, None);
    }

    #[test]
    fn create_defaults() {
        let c: CreateItem = serde_json::from_str(r#"{"id": "x", "title": "t"}"#).unwrap();
        assert_eq!(c.stage, None);
        assert_eq!(c.priority, None);
    }

    #[test]
    fn patch_requires_expected_version() {
        assert!(serde_json::from_str::<ItemPatch>(r#"{"title": "t"}"#).is_err());
    }

    /// The S2/S3 client serializes these DTOs: an untouched field must stay
    /// off the wire entirely, never appear as `null`.
    #[test]
    fn item_patch_default_serializes_to_a_wire_noop() {
        assert_eq!(
            serde_json::to_string(&ItemPatch::default()).unwrap(),
            r#"{"expected_version":0}"#
        );
    }
}
