//! Per-writer bearer auth (ADR-0008): token scopes and the two shapes the
//! admin endpoints return. The `token_hash` column never leaves the server —
//! neither type carries it.

use serde::{Deserialize, Serialize};

/// What a bearer token may do. The serde string of each variant is
/// byte-for-byte the DDL `CHECK` literal — `hummingbird-authority` has a
/// test holding the two lists together.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    /// A full task client: every task-lane write, alert dismiss, and reads.
    Device,
    /// The capture sweeper: `POST /api/items` only.
    Sweeper,
    /// A webhook source: `POST /api/alerts` only.
    Ingest,
}

impl Scope {
    pub const ALL: [Scope; 3] = [Scope::Device, Scope::Sweeper, Scope::Ingest];

    pub fn as_str(self) -> &'static str {
        match self {
            Scope::Device => "device",
            Scope::Sweeper => "sweeper",
            Scope::Ingest => "ingest",
        }
    }

    pub fn parse(s: &str) -> Option<Scope> {
        Scope::ALL.into_iter().find(|v| v.as_str() == s)
    }
}

/// Token metadata as the admin list/replay endpoints return it — everything
/// in the `tokens` row except the hash.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TokenInfo {
    pub id: String,
    /// 'pixel-9', 'sweeper', 'home-assistant', …
    pub name: String,
    pub scope: Scope,
    pub created_at: i64,
    pub last_seen: Option<i64>,
    pub revoked_at: Option<i64>,
}

/// The 201 mint response: [`TokenInfo`]'s identity fields plus the
/// plaintext token, shown exactly once — only its sha256 is stored, so a
/// replayed mint returns [`TokenInfo`] without it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MintedToken {
    pub id: String,
    pub name: String,
    pub scope: Scope,
    pub created_at: i64,
    pub token: String,
}
