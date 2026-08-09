//! The project lane of the owned schema (ADR-0009): `projects`, `routes`,
//! `fog` — a Route is 1:1 with its project (created with it, patched only),
//! and Fog rows are the segments not yet definable as actions.

use serde::{Deserialize, Serialize};

/// One project, exactly the `projects` columns of ADR-0009.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Project {
    /// uuid, client-supplied.
    pub id: String,
    pub name: String,
    /// ms epoch; `None` = live. Rows are never deleted, only flagged.
    pub archived_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
    /// CAS target + delta cursor, stamped from the workspace counter.
    pub version: i64,
}

/// The Route of a project (glossary: Destination, Fog, Notes, ordered
/// actions). 1:1 with `projects` — the row is inserted by project create
/// and only ever patched; `/to-actions` owns its content.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Route {
    pub project_id: String,
    pub destination: Option<String>,
    pub notes: Option<String>,
    pub updated_at: i64,
    pub version: i64,
}

/// One Fog segment: not yet definable as an action, carrying its open
/// question. Resolution is flagged, never deleted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fog {
    pub id: String,
    pub project_id: String,
    pub question: String,
    pub position: i64,
    /// ms epoch; `None` = open.
    pub resolved_at: Option<i64>,
    pub version: i64,
}
