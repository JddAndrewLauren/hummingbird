//! The context lanes and workspace preferences of ADR-0009: `alerts`
//! (pushed context, upserted on `(source, source_key)`), `context_snapshots`
//! (server-polled gauges replaced wholesale), and `settings` (small
//! cross-device binding facts).
//!
//! Amended by ADR-0015: `alerts` gains the nullable `subject_key` that
//! joins an alert to the snapshot row a pane is reading, and every
//! `context_snapshots.payload` gains the common [`SnapshotEnvelope`].

use std::fmt;

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
    /// Identity within the source; re-raise upserts. **Occurrence identity
    /// and nothing else** (ADR-0015) — never a join key, never parsed.
    pub source_key: String,
    /// The `context_snapshots.key` this alert belongs to, or `None` for an
    /// alert that names no subject (ADR-0015). The pane join is
    /// `(source, subject_key)` ↔ `(source, key)`, and it is *additive*: an
    /// alert whose `subject_key` matches no pane is not dropped, it simply
    /// appears in `AlertsScreen` alone. `#[serde(default)]` because absence
    /// is the honest reading of every alert minted before this field
    /// existed — `item-threshold/v1`'s sweep still writes none, since an
    /// item is not a standing question and has no pane.
    #[serde(default)]
    pub subject_key: Option<String>,
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
/// `payload` mirrors the TEXT column: **enveloped** JSON, source-shaped
/// inside (ADR-0015 amending ADR-0009's "source-shaped" flat payload) —
/// see [`SnapshotEnvelope`]. Neither the server nor this crate reads the
/// envelope's `body`; the client that renders the tile does.
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

/// The common envelope every [`ContextSnapshot::payload`] carries
/// (ADR-0015):
///
/// ```json
/// { "schema": "city-waste/v2", "polled_every_ms": 21600000, "body": { … } }
/// ```
///
/// It exists for two reasons that arrived independently: freshness needs a
/// **declared cadence** (no cadence column exists anywhere in ADR-0009's
/// schema, and `polled_every_ms` was already hand-rolled in two prototypes
/// under two names), and the known-to-be-provisional payload shapes need a
/// **version discriminator**.
///
/// The parse is deliberately shallow — `schema` and `polled_every_ms` are
/// read, `body` is carried through opaque. In particular an **unrecognised
/// `schema` is not an error**: it is passed through untouched, and the pane
/// renders its own "this device is behind" state. This type must never
/// grow a check against [`crate::REGISTRY`]; a source the reader's build
/// has not heard of is a fact about the build, not a broken payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SnapshotEnvelope {
    /// The payload shape's own versioned name. Not validated here.
    pub schema: String,
    /// How often the source says it polls. `None` is a **legitimate
    /// state**, not a failure — the client's `Freshness` reads it as
    /// `Age { declared_cadence_ms: None }`: we know the age, but not what
    /// normal looks like for this source.
    pub polled_every_ms: Option<i64>,
    /// The source-shaped payload, as its own JSON text. Opaque by
    /// construction — re-serialized rather than sliced, so key order is
    /// serde's, not the wire's; nothing downstream may depend on its bytes.
    pub body: String,
}

impl SnapshotEnvelope {
    /// Parses the envelope, and only the envelope.
    ///
    /// A broken envelope is a **typed malformed-with-a-reason**, never a
    /// quietly empty answer (ADR-0015: "visibly broken, never quietly
    /// empty"). This is the same rule `shell/sync-status.ts` carries for
    /// sync outcomes — a read that did not happen must never render as one
    /// that did — so every rejection below names what was wrong in a form a
    /// pane can put on screen.
    pub fn parse(payload: &str) -> Result<SnapshotEnvelope, EnvelopeProblem> {
        let value: serde_json::Value =
            serde_json::from_str(payload).map_err(|e| EnvelopeProblem::NotJson(e.to_string()))?;
        let Some(object) = value.as_object() else {
            return Err(EnvelopeProblem::NotAnObject);
        };

        let schema = match object.get("schema") {
            None | Some(serde_json::Value::Null) => {
                return Err(EnvelopeProblem::MissingField("schema"))
            }
            Some(serde_json::Value::String(s)) => s.clone(),
            Some(_) => {
                return Err(EnvelopeProblem::WrongType { field: "schema", expected: "a string" })
            }
        };

        // Absent and explicit `null` mean the same thing: no declared
        // cadence. A *present but unusable* one does not — a zero or
        // negative interval would make every freshness threshold derived
        // from it nonsense, and silently reading it as `None` would dress
        // a broken source up as a source that simply never declared.
        let polled_every_ms = match object.get("polled_every_ms") {
            None | Some(serde_json::Value::Null) => None,
            Some(serde_json::Value::Number(n)) => match n.as_i64() {
                Some(ms) if ms > 0 => Some(ms),
                _ => {
                    return Err(EnvelopeProblem::WrongType {
                        field: "polled_every_ms",
                        expected: "a positive whole number of milliseconds",
                    })
                }
            },
            Some(_) => {
                return Err(EnvelopeProblem::WrongType {
                    field: "polled_every_ms",
                    expected: "a positive whole number of milliseconds",
                })
            }
        };

        let body = match object.get("body") {
            None | Some(serde_json::Value::Null) => {
                return Err(EnvelopeProblem::MissingField("body"))
            }
            Some(body) => body.to_string(),
        };

        Ok(SnapshotEnvelope { schema, polled_every_ms, body })
    }
}

/// Why an envelope could not be read — the "reason" half of ADR-0015's
/// "malformed *with a reason*". Every arm carries enough to say what was
/// wrong without quoting the payload back at the reader.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EnvelopeProblem {
    /// Not JSON at all; carries serde's own message (line/column included).
    NotJson(String),
    /// Valid JSON, but not an object — an array or a bare scalar.
    NotAnObject,
    /// A required member is absent or `null`: `schema` or `body`.
    MissingField(&'static str),
    WrongType { field: &'static str, expected: &'static str },
}

impl fmt::Display for EnvelopeProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EnvelopeProblem::NotJson(message) => write!(f, "payload is not JSON: {message}"),
            EnvelopeProblem::NotAnObject => write!(f, "payload is not a JSON object"),
            EnvelopeProblem::MissingField(field) => write!(f, "`{field}` is missing"),
            EnvelopeProblem::WrongType { field, expected } => {
                write!(f, "`{field}` must be {expected}")
            }
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_full_envelope_parses_and_carries_body_through_opaque() {
        let env = SnapshotEnvelope::parse(
            r#"{"schema": "city-waste/v2", "polled_every_ms": 21600000,
                "body": {"collectedOn": "2026-08-12", "streams": ["landfill"]}}"#,
        )
        .expect("a well-formed envelope parses");
        assert_eq!(env.schema, "city-waste/v2");
        assert_eq!(env.polled_every_ms, Some(21_600_000));
        // Carried whole, and still JSON — the pane parses it, not us.
        let body: serde_json::Value = serde_json::from_str(&env.body).expect("body stays JSON");
        assert_eq!(body["collectedOn"], "2026-08-12");
    }

    #[test]
    fn an_unrecognised_schema_is_passed_through_untouched() {
        // ADR-0015: a source this build has not heard of is a fact about
        // the build. Never a rejection, and never checked against REGISTRY.
        let env = SnapshotEnvelope::parse(r#"{"schema": "not-a-real-source/v9", "body": {}}"#)
            .expect("an unknown schema is not an error");
        assert_eq!(env.schema, "not-a-real-source/v9");
    }

    #[test]
    fn a_missing_polled_every_ms_is_a_legitimate_state() {
        for payload in [
            r#"{"schema": "s/v1", "body": {}}"#,
            r#"{"schema": "s/v1", "polled_every_ms": null, "body": {}}"#,
        ] {
            let env = SnapshotEnvelope::parse(payload).expect("no declared cadence is not an error");
            assert_eq!(env.polled_every_ms, None, "{payload}");
        }
    }

    #[test]
    fn a_body_of_any_json_shape_is_carried() {
        // The envelope has no opinion about what a source's body looks
        // like — only that it said something.
        for body in ["[1,2,3]", r#""a string""#, "0", "false"] {
            let payload = format!(r#"{{"schema": "s/v1", "body": {body}}}"#);
            let env = SnapshotEnvelope::parse(&payload).expect("any JSON body is carried");
            assert_eq!(env.body, body, "{payload}");
        }
    }

    #[test]
    fn every_broken_envelope_is_malformed_with_a_reason() {
        // Never quietly empty: each of these must name what was wrong.
        let cases = [
            ("not json at all", "payload is not JSON"),
            ("[]", "payload is not a JSON object"),
            (r#"{"body": {}}"#, "`schema` is missing"),
            (r#"{"schema": null, "body": {}}"#, "`schema` is missing"),
            (r#"{"schema": 7, "body": {}}"#, "`schema` must be a string"),
            (r#"{"schema": "s/v1"}"#, "`body` is missing"),
            (r#"{"schema": "s/v1", "body": null}"#, "`body` is missing"),
            (
                r#"{"schema": "s/v1", "polled_every_ms": "6h", "body": {}}"#,
                "`polled_every_ms` must be a positive",
            ),
            (
                r#"{"schema": "s/v1", "polled_every_ms": 0, "body": {}}"#,
                "`polled_every_ms` must be a positive",
            ),
            (
                r#"{"schema": "s/v1", "polled_every_ms": -1, "body": {}}"#,
                "`polled_every_ms` must be a positive",
            ),
            (
                r#"{"schema": "s/v1", "polled_every_ms": 1.5, "body": {}}"#,
                "`polled_every_ms` must be a positive",
            ),
        ];
        for (payload, expected_reason) in cases {
            let problem = SnapshotEnvelope::parse(payload).expect_err("{payload} must be rejected");
            let reason = problem.to_string();
            assert!(
                reason.starts_with(expected_reason),
                "{payload} → {reason:?}, expected a reason starting {expected_reason:?}",
            );
        }
    }

    #[test]
    fn an_alert_deserializes_without_a_subject_key() {
        // Absence is the honest reading of every alert minted before
        // ADR-0015 — a stored mirror snapshot must not fail to load.
        let alert: Alert = serde_json::from_str(
            r#"{"id": "a", "source": "hc/v1", "source_key": "k", "title": "t", "body": null,
                "url": null, "severity": null, "raised_at": 1, "resolved_at": null,
                "dismissed_at": null, "expires_at": null, "version": 1}"#,
        )
        .expect("a pre-0015 alert still deserializes");
        assert_eq!(alert.subject_key, None);
    }
}
