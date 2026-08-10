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

/// How long an item takes, GTD-style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Size {
    Quick,
    Short,
    Deep,
}

impl Size {
    pub const ALL: [Size; 3] = [Size::Quick, Size::Short, Size::Deep];

    pub fn as_str(self) -> &'static str {
        match self {
            Size::Quick => "quick",
            Size::Short => "short",
            Size::Deep => "deep",
        }
    }

    pub fn parse(s: &str) -> Option<Size> {
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
    /// Frozen namespace: 'google-tasks/v1', 'gmail/v1', 'web', …
    pub source: Option<String>,
    pub source_key: Option<String>,
    pub source_url: Option<String>,
    /// ms epoch; `None` = live. Rows are never deleted, only flagged.
    pub archived_at: Option<i64>,
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
            source: Some("web".into()),
            source_key: None,
            source_url: None,
            archived_at: None,
            created_at: 1,
            updated_at: 2,
            version: 3,
        };
        let json = serde_json::to_string(&item).unwrap();
        let back: Item = serde_json::from_str(&json).unwrap();
        assert_eq!(back, item);
    }
}
