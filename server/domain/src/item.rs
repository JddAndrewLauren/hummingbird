//! The `items` row of the owned schema (ADR-0009), as a typed record.

use serde::{Deserialize, Serialize};

/// The six-stage lifecycle of the owned schema. The serde string of each
/// variant is byte-for-byte the DDL `CHECK` literal — `hummingbird-authority`
/// has a test holding the two lists together.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Triage,
    Grilling,
    Ready,
    InProgress,
    Blocked,
    Done,
}

impl Stage {
    pub const ALL: [Stage; 6] = [
        Stage::Triage,
        Stage::Grilling,
        Stage::Ready,
        Stage::InProgress,
        Stage::Blocked,
        Stage::Done,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Stage::Triage => "triage",
            Stage::Grilling => "grilling",
            Stage::Ready => "ready",
            Stage::InProgress => "in_progress",
            Stage::Blocked => "blocked",
            Stage::Done => "done",
        }
    }

    pub fn parse(s: &str) -> Option<Stage> {
        Stage::ALL.into_iter().find(|v| v.as_str() == s)
    }
}

/// How long an item takes, GTD-style: `quick`, `normal`, `deep`
/// (ADR-0024, #446 — the middle one was spelled `short` until schema 7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Size {
    Quick,
    /// `short` is the pre-schema-7 wire spelling, and it is accepted on the
    /// way in — here and in [`Size::parse`] — for as long as a device can
    /// still be holding one. The client sync engine persists its outbound
    /// queue (`client/core/src/sync/`), so a `CreateItem` or `ItemPatch`
    /// minted before the deploy can drain after it; without the alias those
    /// writes land in the dead-letter journal rather than the store. It is
    /// an inbound alias only — `as_str` emits `normal` and nothing writes
    /// the old word.
    #[serde(alias = "short")]
    Normal,
    Deep,
}

impl Size {
    pub const ALL: [Size; 3] = [Size::Quick, Size::Normal, Size::Deep];

    pub fn as_str(self) -> &'static str {
        match self {
            Size::Quick => "quick",
            Size::Normal => "normal",
            Size::Deep => "deep",
        }
    }

    pub fn parse(s: &str) -> Option<Size> {
        // The legacy spelling, for the same drained-queue reason as the
        // `serde` alias above. `as_str` never produces it, so this is the
        // only place the old word survives outside a migration.
        if s == "short" {
            return Some(Size::Normal);
        }
        Size::ALL.into_iter().find(|v| v.as_str() == s)
    }
}

/// The energy an item demands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Energy {
    Low,
    Medium,
    High,
}

impl Energy {
    pub const ALL: [Energy; 3] = [Energy::Low, Energy::Medium, Energy::High];

    pub fn as_str(self) -> &'static str {
        match self {
            Energy::Low => "low",
            Energy::Medium => "medium",
            Energy::High => "high",
        }
    }

    pub fn parse(s: &str) -> Option<Energy> {
        Energy::ALL.into_iter().find(|v| v.as_str() == s)
    }
}

/// One task item, exactly the `items` columns of ADR-0009. Every field the
/// server stamps (`seq`, `created_at`, `updated_at`, `version`) is
/// authoritative here; clients never supply them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Item {
    /// uuid; the sweeper's deterministic ids land here.
    pub id: String,
    /// HB-42 display handle, server-minted at create.
    pub seq: Option<i64>,
    pub title: String,
    /// The only free-prose field; never holds Steps.
    pub description: Option<String>,
    pub stage: Stage,
    pub size: Option<Size>,
    pub energy: Option<Energy>,
    /// '@computer', '@calls', … free vocab.
    pub context: Option<String>,
    /// 0..=4.
    pub priority: i64,
    pub project_id: Option<String>,
    /// Order within the Route's action list.
    pub project_pos: Option<i64>,
    /// A naive calendar date (`YYYY-MM-DD`) or minute-precision date-time
    /// (`YYYY-MM-DDTHH:MM`), set deliberately at triage only. See
    /// [`crate::is_valid_deadline`] — no seconds, no timezone.
    pub deadline: Option<String>,
    /// ISO do-date the human chose: a preference, never feeds urgency.
    pub scheduled_date: Option<String>,
    /// Frozen namespace: 'google-tasks/v1', 'gmail/v1', 'web/v1', … Every
    /// source carries a `/vN` suffix from the start (ADR-0014) — there is
    /// no bare name, on this table or any other provenance-carrying one.
    pub source: Option<String>,
    pub source_key: Option<String>,
    pub source_url: Option<String>,
    /// Where this item's thinking lives, outside the app (#771): one
    /// vault-relative path into the operator's Obsidian vault, e.g.
    /// `Hummingbird/Knee rehab.md`. A **path**, never a URI — the vault it
    /// is relative to is a per-device binding (`obsidian-vault`), so a vault
    /// rename leaves every stored value correct, and the values import
    /// directly into the owned notes lane #192 will hold. Deliberately not
    /// provenance (`source_url`) and not an attachment (ADR-0023): the
    /// operator chooses it, so unlike `source_url` it is editable and
    /// clearable.
    ///
    /// `#[serde(default)]` for `agent`'s own reason: a client mirror
    /// snapshot persisted before this column existed carries no such key and
    /// must still parse rather than force a `SYNC_MIRROR_SCHEMA_VERSION`
    /// bump.
    #[serde(default)]
    pub vault_path: Option<String>,
    /// The one **Link** an item points at (#782): an operator-chosen URL and
    /// an optional name for it. The third nullable `items` column that is
    /// neither provenance nor a path — distinct from `source_url` (where the
    /// item *came from*, system-written and immutable) and from
    /// `vault_path` (a path into one vault, never a URL). The operator
    /// chooses it, so like `vault_path` and unlike `source_url` it is
    /// editable and clearable. At most one per item: a *list* of links is
    /// what a project's `project_links` is for (ADR-0030 decision 4).
    ///
    /// `link_label` is the name shown for it; absent, a client draws the
    /// URL's host instead. A name without a URL is a 400 on both write
    /// doors, and clearing the URL clears the name — one row state, not two.
    ///
    /// `#[serde(default)]` on both for `vault_path`'s reason: a mirror
    /// snapshot persisted before these columns existed must still parse.
    #[serde(default)]
    pub link_url: Option<String>,
    #[serde(default)]
    pub link_label: Option<String>,
    /// ms epoch; `None` = live. Rows are never deleted, only flagged.
    pub archived_at: Option<i64>,
    /// #10's fourth axis, *who does this*: `true` = an agent could do this
    /// chore, `false` = the human. There is no `for-human` marker — the
    /// default is the human, which is why this is a plain `bool` and not an
    /// `Option`. Deliberately **not** `context`: context is the one hard
    /// filter (*where* can this be done) and folding delegation into it
    /// would break that filter's meaning.
    ///
    /// `#[serde(default)]` so a client mirror snapshot written before this
    /// column existed still parses, the same trick `ChangesResponse.rules`
    /// uses — which is what keeps `SYNC_MIRROR_SCHEMA_VERSION` where it is.
    #[serde(default)]
    pub agent: bool,
    pub created_at: i64,
    pub updated_at: i64,
    /// CAS target + delta cursor, stamped from the workspace counter.
    pub version: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `as_str`/`parse` and serde must agree — handlers go through
    /// `as_str`/`parse` for SQL while the wire goes through serde.
    #[test]
    fn enum_strings_round_trip_through_serde_and_parse() {
        for stage in Stage::ALL {
            let json = serde_json::to_string(&stage).unwrap();
            assert_eq!(json, format!("\"{}\"", stage.as_str()));
            assert_eq!(Stage::parse(stage.as_str()), Some(stage));
        }
        for size in Size::ALL {
            let json = serde_json::to_string(&size).unwrap();
            assert_eq!(json, format!("\"{}\"", size.as_str()));
            assert_eq!(Size::parse(size.as_str()), Some(size));
        }
        for energy in Energy::ALL {
            let json = serde_json::to_string(&energy).unwrap();
            assert_eq!(json, format!("\"{}\"", energy.as_str()));
            assert_eq!(Energy::parse(energy.as_str()), Some(energy));
        }
        assert_eq!(Stage::parse("backlog"), None);
    }

    /// The pre-schema-7 spelling of the middle size is accepted inbound and
    /// never produced outbound (ADR-0024, #446). A device that queued a
    /// write before the rename deploy drains it afterwards, and this is the
    /// only thing standing between that write and the dead-letter journal.
    #[test]
    fn the_legacy_short_size_is_accepted_inbound_and_never_written_back() {
        assert_eq!(Size::parse("short"), Some(Size::Normal));
        assert_eq!(
            serde_json::from_str::<Size>("\"short\"").unwrap(),
            Size::Normal,
        );
        assert_eq!(
            serde_json::to_string(&Size::Normal).unwrap(),
            "\"normal\"",
            "outbound is always the new word — the alias is one-way",
        );
        assert!(
            !Size::ALL.iter().any(|s| s.as_str() == "short"),
            "`short` is not a size any more, only a spelling that used to be one",
        );
    }

    #[test]
    fn item_serde_round_trips() {
        let item = Item {
            id: "a-1".into(),
            seq: Some(1),
            title: "hello".into(),
            description: None,
            stage: Stage::Triage,
            size: Some(Size::Quick),
            energy: None,
            context: Some("@computer".into()),
            priority: 2,
            project_id: None,
            project_pos: None,
            deadline: Some("2026-08-15".into()),
            scheduled_date: None,
            source: Some("web/v1".into()),
            source_key: None,
            source_url: None,
            vault_path: None,
            link_url: None,
            link_label: None,
            archived_at: None,
            agent: false,
            created_at: 1,
            updated_at: 2,
            version: 3,
        };
        let json = serde_json::to_string(&item).unwrap();
        let back: Item = serde_json::from_str(&json).unwrap();
        assert_eq!(back, item);
    }

    /// The reason `agent` carries `#[serde(default)]`: a mirror snapshot
    /// persisted before the column existed has no such key, and must still
    /// parse rather than force a `SYNC_MIRROR_SCHEMA_VERSION` bump that
    /// would discard every stored item.
    #[test]
    fn an_item_written_before_the_agent_column_still_parses_as_the_human_s() {
        let pre_agent = r#"{
            "id": "a-1", "seq": 1, "title": "hello", "description": null,
            "stage": "triage", "size": null, "energy": null, "context": null,
            "priority": 0, "project_id": null, "project_pos": null,
            "deadline": null, "scheduled_date": null,
            "source": null, "source_key": null, "source_url": null,
            "archived_at": null, "created_at": 1, "updated_at": 2, "version": 3
        }"#;
        let item: Item = serde_json::from_str(pre_agent).unwrap();
        assert!(!item.agent);
    }

    /// #782's pair carries `#[serde(default)]` for the same reason: a
    /// snapshot from before the Link existed points at nothing.
    #[test]
    fn an_item_written_before_the_link_columns_still_parses_with_no_link() {
        let pre_link = r#"{
            "id": "a-1", "seq": 1, "title": "hello", "description": null,
            "stage": "triage", "size": null, "energy": null, "context": null,
            "priority": 0, "project_id": null, "project_pos": null,
            "deadline": null, "scheduled_date": null,
            "source": null, "source_key": null, "source_url": null,
            "vault_path": null, "archived_at": null, "agent": false,
            "created_at": 1, "updated_at": 2, "version": 3
        }"#;
        let item: Item = serde_json::from_str(pre_link).unwrap();
        assert_eq!(item.link_url, None);
        assert_eq!(item.link_label, None);
    }
}
