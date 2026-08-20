//! The project lane of the owned schema (ADR-0009): `projects`, `routes`,
//! `fog` — a Route is 1:1 with its project (created with it, patched only),
//! and Fog rows are the segments not yet definable as actions.

use serde::{Deserialize, Serialize};

/// One project, exactly the `projects` columns of ADR-0009, as amended by
/// ADR-0030 decision 2 (`github_repo`, `default_context`, #625).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Project {
    /// uuid, client-supplied.
    pub id: String,
    pub name: String,
    /// `owner/repo` only, never a URL — validated by
    /// [`crate::is_valid_github_repo`] at every write door. `None` when the
    /// project names no repo.
    pub github_repo: Option<String>,
    /// Copied onto a context-less item at the three entry points ADR-0030
    /// decision 3 names (triage promotion, `/to-actions` minting, project
    /// assignment) — never read live, never joined. `None` when the
    /// project names no default.
    pub default_context: Option<String>,
    /// ms epoch; `None` = live. Rows are never deleted, only flagged.
    pub archived_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
    /// CAS target + delta cursor, stamped from the workspace counter.
    pub version: i64,
}

/// The Route of a project (glossary: Destination, Fog, Notes, ordered
/// actions). 1:1 with `projects` — the row is inserted by project create
/// and only ever patched. Its content is **shared-owned** (ADR-0030
/// decision 1, which withdrew the sole-ownership claim this comment used to
/// make for `/to-actions`): the skill and any client may both write it, and
/// the ordinary CAS `expected_version` arbitrates.
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
