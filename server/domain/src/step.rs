//! Below-item records of the owned schema (ADR-0009): `steps` (ticking one
//! is a scalar CAS write — the operation whose impossibility under Linear
//! triggered ADR-0008) and `blocked_by` (native sequencing edges).

use serde::{Deserialize, Serialize};

/// One Step of an item's checklist, exactly the `steps` columns.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Step {
    pub id: String,
    pub item_id: String,
    pub body: String,
    /// Stored as INTEGER 0/1 in the DDL.
    pub done: bool,
    pub position: i64,
    /// ms epoch; `None` = live. Flagged, never erased.
    pub deleted_at: Option<i64>,
    pub version: i64,
}

/// One sequencing edge: `item_id` is blocked by `blocker_id`. The identity
/// is the composite key; removal is flagged via `removed_at`, never DELETE,
/// and a re-add clears the flag on the same row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlockedBy {
    pub item_id: String,
    pub blocker_id: String,
    pub version: i64,
    /// ms epoch; `None` = live edge.
    pub removed_at: Option<i64>,
}
