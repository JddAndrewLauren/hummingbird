//! The notification lane's three tables (ADR-0012, amended by ADR-0013):
//! `rules` (structured predicates, individually CAS-editable and
//! delta-pulled like everything else), `push_targets` (FCM registrations,
//! individually revocable), and `deliveries` (the delivery log #139's
//! dedupe reads). Evaluating rules and the kind registry are #133; sending
//! to FCM and the `push_targets` HTTP surface are #139; both are out of
//! scope here — this module carries only the owned-schema shapes.

use serde::{Deserialize, Serialize};

/// One ANDed field test (ADR-0013): `field`, `op` and `value` are validated
/// against the typed catalogue by the rule engine (#133), not here — this
/// crate only needs the shape to round-trip. `negate` is the ADR-0013
/// amendment: a `not` toggle instead of a mirrored operator set.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Condition {
    pub field: String,
    pub op: String,
    pub value: serde_json::Value,
    #[serde(default)]
    pub negate: bool,
}

/// The two notification tiers (ADR-0012), mapping to Android notification
/// channels and FCM priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    Urgent,
    Normal,
}

impl Tier {
    pub const ALL: [Tier; 2] = [Tier::Urgent, Tier::Normal];

    pub fn as_str(self) -> &'static str {
        match self {
            Tier::Urgent => "urgent",
            Tier::Normal => "normal",
        }
    }

    pub fn parse(s: &str) -> Option<Tier> {
        Tier::ALL.into_iter().find(|v| v.as_str() == s)
    }
}

/// One rule, exactly the `rules` columns as amended by ADR-0013: `event_kind`
/// is nullable and carries no `CHECK` (an open registry key; `None` = any
/// kind), and `conditions` is `[{field, op, value, negate}]`. `severity` is
/// unused for `mints: false` kinds, but stays `NOT NULL` here too — a rule
/// author always names one, even where the engine ignores it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rule {
    pub id: String,
    pub name: String,
    /// `None` = any kind. A registry key, not a closed vocabulary — no
    /// `CHECK` in the DDL, so a kind the code does not yet know is legal.
    pub event_kind: Option<String>,
    pub conditions: Vec<Condition>,
    /// Stamped on the alert this rule mints; unused for `mints: false` kinds.
    pub severity: String,
    pub tier: Tier,
    pub enabled: bool,
    pub updated_at: i64,
    pub version: i64,
    /// ms epoch; `None` = a live rule. A soft delete, exactly
    /// `Step::deleted_at`: the flagged row still rides the delta pull, so
    /// every device learns the rule is gone rather than keeping it on
    /// screen until a full sweep. A deleted rule never fires
    /// (`load_enabled` filters it) and never lists.
    #[serde(default)]
    pub deleted_at: Option<i64>,
}

/// One FCM registration, exactly the `push_targets` columns. Individually
/// revocable — losing one device (`revoked_at`) must not disturb the
/// others. No `version`: registrations are server-side machinery like
/// `tokens`, outside the delta-pull contract by construction; the HTTP
/// surface that creates/revokes them is #139.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    Android,
    Ios,
}

impl Platform {
    pub const ALL: [Platform; 2] = [Platform::Android, Platform::Ios];

    pub fn as_str(self) -> &'static str {
        match self {
            Platform::Android => "android",
            Platform::Ios => "ios",
        }
    }

    pub fn parse(s: &str) -> Option<Platform> {
        Platform::ALL.into_iter().find(|v| v.as_str() == s)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PushTarget {
    pub id: String,
    /// 'pixel-9', 'pixel-watch', …
    pub name: String,
    pub platform: Platform,
    pub fcm_token: String,
    pub created_at: i64,
    pub last_seen: Option<i64>,
    /// ms epoch; `None` = live registration.
    pub revoked_at: Option<i64>,
}

/// One delivery-log row, exactly the `deliveries` columns. The dedupe key
/// `(alert_id, rule_id, generation, severity)` — enforced as a `UNIQUE`
/// constraint in the DDL — is what #139's transitions-only dedupe reads: a
/// delivery is warranted only when an alert enters live-unacked or its
/// severity escalates, never on an identical re-raise of the *same rule*.
/// `rule_id` is in the key deliberately (ADR-0014): a delivery is one
/// rule's ring, not the alert's, so two rules matching one event — even
/// agreeing on severity — must both persist as distinct rows. No
/// `version`: an append-only log, outside the delta-pull contract like
/// `push_targets`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Delivery {
    pub id: String,
    pub alert_id: String,
    pub rule_id: String,
    /// The alert's lifecycle generation at send (its `raised_at`, ADR-0012
    /// amended by ADR-0014).
    pub generation: i64,
    pub severity: String,
    pub tier: Tier,
    pub sent_at: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tier_strings_round_trip_through_serde_and_parse() {
        for tier in Tier::ALL {
            let json = serde_json::to_string(&tier).unwrap();
            assert_eq!(json, format!("\"{}\"", tier.as_str()));
            assert_eq!(Tier::parse(tier.as_str()), Some(tier));
        }
        assert_eq!(Tier::parse("digest"), None);
    }

    #[test]
    fn platform_strings_round_trip_through_serde_and_parse() {
        for platform in Platform::ALL {
            let json = serde_json::to_string(&platform).unwrap();
            assert_eq!(json, format!("\"{}\"", platform.as_str()));
            assert_eq!(Platform::parse(platform.as_str()), Some(platform));
        }
        assert_eq!(Platform::parse("watchos"), None);
    }

    /// The acceptance criterion: a `conditions` payload round-trips with
    /// `negate` preserved on every condition, including the default when a
    /// condition omits it (an older payload written before ADR-0013).
    #[test]
    fn conditions_round_trip_with_negate_preserved() {
        let conditions = vec![
            Condition {
                field: "labels".into(),
                op: "contains".into(),
                value: serde_json::json!("alert-high"),
                negate: true,
            },
            Condition {
                field: "subject".into(),
                op: "contains".into(),
                value: serde_json::json!(["urgent", "asap"]),
                negate: false,
            },
        ];
        let json = serde_json::to_string(&conditions).unwrap();
        let back: Vec<Condition> = serde_json::from_str(&json).unwrap();
        assert_eq!(back, conditions);
        assert!(back[0].negate);
        assert!(!back[1].negate);

        // A condition omitting `negate` entirely (pre-ADR-0013 shape)
        // defaults to false rather than failing to parse.
        let legacy: Vec<Condition> =
            serde_json::from_str(r#"[{"field":"from","op":"contains","value":"twinion"}]"#)
                .unwrap();
        assert!(!legacy[0].negate);
    }

    #[test]
    fn rule_serde_round_trips() {
        let rule = Rule {
            id: "r-1".into(),
            name: "trash slide".into(),
            event_kind: None,
            conditions: vec![Condition {
                field: "source".into(),
                op: "eq".into(),
                value: serde_json::json!("city-waste/v1"),
                negate: false,
            }],
            severity: "high".into(),
            tier: Tier::Urgent,
            enabled: true,
            updated_at: 1,
            version: 1,
            deleted_at: None,
        };
        let json = serde_json::to_string(&rule).unwrap();
        let back: Rule = serde_json::from_str(&json).unwrap();
        assert_eq!(back, rule);
    }

    /// `event_kind: null` on the wire is the "any kind" state, not an
    /// absent field — the struct must accept and round-trip it.
    #[test]
    fn rule_event_kind_accepts_null_and_an_unknown_kind() {
        let known: Rule = serde_json::from_str(
            r#"{"id":"r","name":"n","event_kind":null,"conditions":[],"severity":"s","tier":"normal","enabled":true,"updated_at":0,"version":1}"#,
        )
        .unwrap();
        assert_eq!(known.event_kind, None);

        let unknown: Rule = serde_json::from_str(
            r#"{"id":"r","name":"n","event_kind":"weather_alert","conditions":[],"severity":"s","tier":"normal","enabled":true,"updated_at":0,"version":1}"#,
        )
        .unwrap();
        assert_eq!(unknown.event_kind, Some("weather_alert".into()));
    }
}
