//! The context lanes and workspace preferences of ADR-0009: `alerts`
//! (pushed context, upserted on `(source, source_key)`), `context_snapshots`
//! (server-polled gauges replaced wholesale), and `settings` (small
//! cross-device binding facts).

use serde::{Deserialize, Serialize};

/// One pushed-context alert, exactly the `alerts` columns. The source owns
/// `raised_at`/`resolved_at`/`expires_at`; `dismissed_at` is human-owned and
/// never touched by ingest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Alert {
    /// Server-minted deterministic id — a re-raise of the same
    /// `(source, source_key)` lands on the same row by construction.
    pub id: String,
    /// 'healthchecks/v1', 'home-assistant/v1', 'gmail-alert/v1', … — the
    /// versioned frozen namespace of [`crate::REGISTRY`], which registers
    /// every source that can appear here.
    pub source: String,
    /// Identity within the source; re-raise upserts.
    pub source_key: String,
    pub title: String,
    pub body: Option<String>,
    pub url: Option<String>,
    pub severity: Option<String>,
    pub raised_at: i64,
    /// The source said it's over (infra up-event).
    pub resolved_at: Option<i64>,
    /// The human waved it away (email, HA).
    pub dismissed_at: Option<i64>,
    /// Source-declared TTL, read at query time by [`Alert::is_live`]'s
    /// expiry clause — never written back as a dismissal (ADR-0014
    /// corrects ADR-0009's "auto-dismiss": a machine writing the
    /// human-owned `dismissed_at` would make an expired-then-re-raised
    /// occurrence indistinguishable from an acked one).
    pub expires_at: Option<i64>,
    pub version: i64,
}

impl Alert {
    /// ADR-0014's live predicate ([`crate::is_live`]), applied to this row.
    /// Call with the caller's clock, never a stored value — `now` matters
    /// only for the expiry clause.
    pub fn is_live(&self, now: i64) -> bool {
        crate::is_live(self.raised_at, self.resolved_at, self.dismissed_at, self.expires_at, now)
    }
}

/// One server-polled gauge, exactly the `context_snapshots` columns.
/// `payload` mirrors the TEXT column: source-shaped JSON, parsed by the
/// client that renders the tile, never by the server.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextSnapshot {
    /// 'anthropic-usage/v1', 'github-hummingbird/v1', 'photo-site/v1', … —
    /// the same frozen namespace as `items.source` (ADR-0009 rule 4), so
    /// the `/vN` suffix is mandatory here too. The suffix claims the slash:
    /// a source naming a sub-scope folds it into the name
    /// ('github-hummingbird/v1', never 'github/hummingbird', which would
    /// read as version 'hummingbird' — ADR-0014).
    pub source: String,
    /// Metric within the source: 'weekly_limit', 'open_prs', …
    pub key: String,
    pub payload: String,
    /// Drives the "as of…" staleness display (ADR-0002's alarm).
    pub fetched_at: i64,
    pub version: i64,
}

/// One workspace preference, exactly the `settings` columns. `value`
/// mirrors the TEXT column: canonical JSON, written from [`PutSetting`]'s
/// typed `value` and parsed by the consuming client.
///
/// [`PutSetting`]: crate::PutSetting
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Setting {
    pub key: String,
    pub value: String,
    pub updated_at: i64,
    pub version: i64,
}
