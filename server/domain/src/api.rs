//! The wire DTOs of the authority's API (ADR-0008): create, patch,
//! delta-read, and the error shapes.

use serde::{Deserialize, Deserializer, Serialize};

use crate::item::{Energy, Item, Size, Stage};

/// `POST /api/items` body. `id` is the client-supplied deterministic id the
/// create is idempotent by; the server stamps `seq`, timestamps and
/// `version` — they cannot be supplied.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreateItem {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    /// Defaults to `triage` — capture lands in the inbox.
    #[serde(default)]
    pub stage: Option<Stage>,
    #[serde(default)]
    pub size: Option<Size>,
    #[serde(default)]
    pub energy: Option<Energy>,
    #[serde(default)]
    pub context: Option<String>,
    /// Defaults to 0.
    #[serde(default)]
    pub priority: Option<i64>,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub project_pos: Option<i64>,
    #[serde(default)]
    pub due_date: Option<String>,
    #[serde(default)]
    pub scheduled_date: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub source_key: Option<String>,
    #[serde(default)]
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

/// `PATCH /api/items/:id` body: `expected_version` plus absolute-value
/// sets. Every mutation states the entire new value of each field it
/// touches (ADR-0008) — S3's rebase-on-409 compares *touched* fields, so
/// absent-vs-null fidelity here is load-bearing.
///
/// Nullable columns are double-`Option`: outer = touched at all, inner =
/// the new value (`None` clears). `NOT NULL` columns are single-`Option`
/// and cannot be cleared.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ItemPatch {
    pub expected_version: i64,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default, deserialize_with = "touched", skip_serializing_if = "Option::is_none")]
    pub description: Option<Option<String>>,
    #[serde(default)]
    pub stage: Option<Stage>,
    #[serde(default, deserialize_with = "touched", skip_serializing_if = "Option::is_none")]
    pub size: Option<Option<Size>>,
    #[serde(default, deserialize_with = "touched", skip_serializing_if = "Option::is_none")]
    pub energy: Option<Option<Energy>>,
    #[serde(default, deserialize_with = "touched", skip_serializing_if = "Option::is_none")]
    pub context: Option<Option<String>>,
    #[serde(default)]
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
}
