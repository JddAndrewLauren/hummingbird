//! `file_links` (ADR-0036): the many-per-item pointers at files in the
//! operator's Dropbox. Each row is one Dropbox-relative **path** — never a
//! URL, never a share link — and the machine-local folder it is relative to
//! is a fact each machine holds for itself (the `hummingbird-open` helper),
//! not a column and not a binding. The item lane's sibling of
//! [`crate::ProjectLink`], and the reason it is not in `project.rs`: it
//! references `items`, so unlike a project link it **is** one of
//! `hummingbird_authority::schema::FK_CHILDREN`, the tables an `items`
//! rebuild has to stand aside.

use serde::{Deserialize, Serialize};

/// One File link: a Dropbox-relative path an item points at (ADR-0036).
/// No label — the basename is the name — and no position: rows list in
/// insertion order. Added and removed whole, never re-pointed; removal is
/// flagged (`removed_at`), never deleted (ADR-0020).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileLink {
    pub id: String,
    pub item_id: String,
    pub path: String,
    /// ms epoch; `None` = live.
    pub removed_at: Option<i64>,
    pub version: i64,
}
