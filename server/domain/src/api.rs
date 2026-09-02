//! The wire DTOs of the authority's API (ADR-0008): creates, patches,
//! delta-read, token minting, and the error shapes.
//!
//! Every create is idempotent by its client-supplied id; every patch is
//! `expected_version` plus absolute-value sets. Server-stamped fields
//! (`seq`, timestamps, `version`) never appear in a request body, and
//! `deny_unknown_fields` makes supplying one — or a typo — a 400, not a
//! silent no-op.

use serde::{Deserialize, Deserializer, Serialize};

use crate::context::{Alert, ContextSnapshot, Setting};
use crate::grill::{GrillVerdict, GrillWithoutTranscript};
use crate::item::{Energy, Item, Size, Stage};
use crate::project::{Fog, Project, ProjectLink, Route};
use crate::rule::{Condition, Platform, Rule, Tier};
use crate::step::{BlockedBy, Step};
use crate::token::Scope;

/// `POST /api/items` body. `id` is the client-supplied deterministic id the
/// create is idempotent by; the server stamps `seq`, timestamps and
/// `version` — they cannot be supplied.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateItem {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Defaults to `triage` — capture lands in the inbox.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage: Option<Stage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<Size>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub energy: Option<Energy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    /// Defaults to 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_pos: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deadline: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheduled_date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    /// #771's vault-relative Obsidian path. Plain `Option`, like every other
    /// nullable field here: a create names no prior value a `null` could be
    /// clearing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vault_path: Option<String>,
    /// #10's delegation axis. Defaults to false — the human does it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<bool>,
}

/// Marks JSON presence: absent field deserializes to `None` (untouched),
/// `"field": null` to `Some(None)` (clear), a value to `Some(Some(v))`.
fn touched<'de, T, D>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

/// `NOT NULL` columns cannot be cleared: an explicit JSON `null` is a
/// deserialize error, not a silent skip.
fn non_null<'de, T, D>(deserializer: D, field: &'static str) -> Result<Option<T>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    match Option::<T>::deserialize(deserializer)? {
        Some(v) => Ok(Some(v)),
        None => Err(serde::de::Error::custom(format!("{field} may not be null"))),
    }
}

/// `deserialize_with` needs a named path, so each `NOT NULL` patch field
/// gets a shim binding [`non_null`] to its field name.
macro_rules! non_null_shim {
    ($fn_name:ident, $ty:ty, $field:literal) => {
        fn $fn_name<'de, D: Deserializer<'de>>(d: D) -> Result<Option<$ty>, D::Error> {
            non_null(d, $field)
        }
    };
}

non_null_shim!(non_null_title, String, "title");
non_null_shim!(non_null_stage, Stage, "stage");
non_null_shim!(non_null_priority, i64, "priority");
non_null_shim!(non_null_name, String, "name");
non_null_shim!(non_null_question, String, "question");
non_null_shim!(non_null_url, String, "url");
non_null_shim!(non_null_position, i64, "position");
non_null_shim!(non_null_body, String, "body");
non_null_shim!(non_null_done, bool, "done");
non_null_shim!(non_null_conditions, Vec<Condition>, "conditions");
non_null_shim!(non_null_severity, String, "severity");
non_null_shim!(non_null_tier, Tier, "tier");
non_null_shim!(non_null_enabled, bool, "enabled");
non_null_shim!(non_null_agent, bool, "agent");

/// `PATCH /api/items/:id` body: `expected_version` plus absolute-value
/// sets. Every mutation states the entire new value of each field it
/// touches (ADR-0008) — S3's rebase-on-409 compares *touched* fields, so
/// absent-vs-null fidelity here is load-bearing.
///
/// Nullable columns are double-`Option`: outer = touched at all, inner =
/// the new value (`None` clears). `NOT NULL` columns are single-`Option`
/// and cannot be cleared — an explicit `null` on them is a 400.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemPatch {
    pub expected_version: i64,
    #[serde(default, deserialize_with = "non_null_title", skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, deserialize_with = "touched", skip_serializing_if = "Option::is_none")]
    pub description: Option<Option<String>>,
    #[serde(default, deserialize_with = "non_null_stage", skip_serializing_if = "Option::is_none")]
    pub stage: Option<Stage>,
    #[serde(default, deserialize_with = "touched", skip_serializing_if = "Option::is_none")]
    pub size: Option<Option<Size>>,
    #[serde(default, deserialize_with = "touched", skip_serializing_if = "Option::is_none")]
    pub energy: Option<Option<Energy>>,
    #[serde(default, deserialize_with = "touched", skip_serializing_if = "Option::is_none")]
    pub context: Option<Option<String>>,
    #[serde(default, deserialize_with = "non_null_priority", skip_serializing_if = "Option::is_none")]
    pub priority: Option<i64>,
    #[serde(default, deserialize_with = "touched", skip_serializing_if = "Option::is_none")]
    pub project_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "touched", skip_serializing_if = "Option::is_none")]
    pub project_pos: Option<Option<i64>>,
    #[serde(default, deserialize_with = "touched", skip_serializing_if = "Option::is_none")]
    pub deadline: Option<Option<String>>,
    #[serde(default, deserialize_with = "touched", skip_serializing_if = "Option::is_none")]
    pub scheduled_date: Option<Option<String>>,
    #[serde(default, deserialize_with = "touched", skip_serializing_if = "Option::is_none")]
    pub archived_at: Option<Option<i64>>,
    /// #771's vault-relative Obsidian path — and **the one nullable
    /// `items` column outside the provenance trio that this patch can
    /// touch**. `source`/`source_key`/`source_url` are deliberately absent
    /// here because provenance belongs to whatever captured the item; a
    /// vault path is an operator *choice*, set and re-set and cleared from
    /// the item panel long after capture, so that exclusion does not reach
    /// it. The asymmetry is deliberate, not an oversight.
    #[serde(default, deserialize_with = "touched", skip_serializing_if = "Option::is_none")]
    pub vault_path: Option<Option<String>>,
    #[serde(default, deserialize_with = "non_null_agent", skip_serializing_if = "Option::is_none")]
    pub agent: Option<bool>,
}

/// `POST /api/projects` body. Creating a project also creates its empty
/// Route row — the 1:1 invariant is structural, so there is no
/// `POST /api/routes`. `github_repo`/`default_context` are plain `Option`,
/// not the patch's double-`Option`: a create names no prior value a `null`
/// could be clearing, the same reasoning [`CreateItem`]'s own nullable
/// fields carry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateProject {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github_repo: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_context: Option<String>,
}

/// `PATCH /api/projects/:id` body. `github_repo`/`default_context` are
/// double-`Option` (#625) — the same [`touched`] contract every other
/// nullable patch field here carries.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectPatch {
    pub expected_version: i64,
    #[serde(default, deserialize_with = "non_null_name", skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, deserialize_with = "touched", skip_serializing_if = "Option::is_none")]
    pub github_repo: Option<Option<String>>,
    #[serde(default, deserialize_with = "touched", skip_serializing_if = "Option::is_none")]
    pub default_context: Option<Option<String>>,
    #[serde(default, deserialize_with = "touched", skip_serializing_if = "Option::is_none")]
    pub archived_at: Option<Option<i64>>,
}

/// `PATCH /api/routes/:project_id` body.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoutePatch {
    pub expected_version: i64,
    #[serde(default, deserialize_with = "touched", skip_serializing_if = "Option::is_none")]
    pub destination: Option<Option<String>>,
    #[serde(default, deserialize_with = "touched", skip_serializing_if = "Option::is_none")]
    pub notes: Option<Option<String>>,
}

/// `POST /api/fog` body.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateFog {
    pub id: String,
    pub project_id: String,
    pub question: String,
    pub position: i64,
}

/// `PATCH /api/fog/:id` body.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FogPatch {
    pub expected_version: i64,
    #[serde(default, deserialize_with = "non_null_question", skip_serializing_if = "Option::is_none")]
    pub question: Option<String>,
    #[serde(default, deserialize_with = "non_null_position", skip_serializing_if = "Option::is_none")]
    pub position: Option<i64>,
    #[serde(default, deserialize_with = "touched", skip_serializing_if = "Option::is_none")]
    pub resolved_at: Option<Option<i64>>,
}

/// `POST /api/project_links` body (#626, ADR-0030 decision 4). `url` is
/// non-nullable — a link with no url would be nothing to click — `label` is
/// plain `Option`, not the patch's double-`Option`, same [`CreateProject`]
/// reasoning: a create names no prior value a `null` could be clearing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateProjectLink {
    pub id: String,
    pub project_id: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub position: i64,
}

/// `PATCH /api/project_links/:id` body. `url` is `NOT NULL` and cannot be
/// cleared, same as [`FogPatch::question`]; `label` and `removed_at` are
/// double-`Option` — the removal flag double-`Option` the brief asks for,
/// so un-removing (clearing `removed_at` with an explicit `null`) is the
/// same gesture [`FogPatch::resolved_at`] already carries.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectLinkPatch {
    pub expected_version: i64,
    #[serde(default, deserialize_with = "non_null_url", skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, deserialize_with = "touched", skip_serializing_if = "Option::is_none")]
    pub label: Option<Option<String>>,
    #[serde(default, deserialize_with = "non_null_position", skip_serializing_if = "Option::is_none")]
    pub position: Option<i64>,
    #[serde(default, deserialize_with = "touched", skip_serializing_if = "Option::is_none")]
    pub removed_at: Option<Option<i64>>,
}

/// `POST /api/steps` body.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateStep {
    pub id: String,
    pub item_id: String,
    pub body: String,
    pub position: i64,
}

/// `POST /api/grills` body: the outcome of one completed Grill (ADR-0023
/// decision 2), plus the transcript that produced it. `resulting_stage` is
/// deliberately absent — the handler computes it from the target item's
/// *current* stage via `hummingbird_domain::resulting_stage` (#352), never
/// accepted as a caller-supplied value the handler would have to trust.
/// `completed_at` is likewise absent: a timestamp, server-stamped like
/// every other create's (this module's own doc — "timestamps… never
/// appear in a request body"), unlike `AlertIngest.raised_at`'s documented
/// exception for a source that knows an event's time better than the
/// server clock. A Grill's completion has no such external clock; it
/// completes exactly when this request lands.
///
/// **#354's atomic-completion fields.** `expected_version` is the target
/// item's CAS token — the same contract as every other patch DTO — so this
/// create can also carry the item mutation ADR-0023 promises ("the Grill
/// record is created, the reviewed item patch is applied, and the stage
/// moves — all together, or not at all"). There is no generic item-field
/// patch here: the only item mutation a Grill completion ever makes is the
/// verdict→stage move (`hummingbird_domain::resulting_stage`, never
/// re-derived) and, gated by `delete_unticked_plan`, soft-deleting the
/// item's still-unticked Plan. `delete_unticked_plan` defaults to `false`
/// (old callers, and every existing fixture, still deserialize) — the
/// **Replace** glossary entry's explicit-gesture rule: absent the flag,
/// steps are never touched, ticked or not.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateGrill {
    pub id: String,
    pub item_id: String,
    pub expected_version: i64,
    pub transcript: String,
    pub summary: String,
    pub verdict: GrillVerdict,
    pub model_proposal: String,
    pub applied_patch: String,
    #[serde(default)]
    pub delete_unticked_plan: bool,
}

/// `PATCH /api/steps/:id` body. Ticking a Step is `{expected_version,
/// done: true}` — the scalar CAS write ADR-0008 exists for.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StepPatch {
    pub expected_version: i64,
    #[serde(default, deserialize_with = "non_null_body", skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(default, deserialize_with = "non_null_done", skip_serializing_if = "Option::is_none")]
    pub done: Option<bool>,
    #[serde(default, deserialize_with = "non_null_position", skip_serializing_if = "Option::is_none")]
    pub position: Option<i64>,
    #[serde(default, deserialize_with = "touched", skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<Option<i64>>,
}

/// `POST /api/blocked_by` body. The edge's identity is the pair itself, so
/// the create is idempotent by construction; re-adding a removed edge
/// clears `removed_at` on the same row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateBlockedBy {
    pub item_id: String,
    pub blocker_id: String,
}

/// `PATCH /api/blocked_by/:item_id/:blocker_id` body — removal (set
/// `removed_at`) and un-removal (`null` clears) under CAS.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlockedByPatch {
    pub expected_version: i64,
    #[serde(default, deserialize_with = "touched", skip_serializing_if = "Option::is_none")]
    pub removed_at: Option<Option<i64>>,
}

/// `POST /api/rules` body. `id` is client-supplied so the create is
/// idempotent by it, same rule as [`CreateItem`]. `event_kind` is nullable
/// with no closed vocabulary (ADR-0013): `None` matches any kind.
/// `enabled` defaults to `true` — a saved rule starts live.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateRule {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_kind: Option<String>,
    pub conditions: Vec<Condition>,
    pub severity: String,
    pub tier: Tier,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

/// `POST /api/push_targets` body (#139): idempotent create by client id,
/// the same shape as [`CreateRule`]. No patch surface — revocation
/// (`DELETE /api/push_targets/:id`) is the only mutation a registered
/// target ever gets.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatePushTarget {
    pub id: String,
    /// 'pixel-9', 'pixel-watch', …
    pub name: String,
    pub platform: Platform,
    pub fcm_token: String,
}

/// `PATCH /api/rules/:id` body: `expected_version` plus absolute-value
/// sets, the same CAS contract as [`ItemPatch`]. `event_kind` and
/// `deleted_at` are the two nullable columns — double-`Option`, `null`
/// clears them (to "any kind", and to a live rule respectively) — every
/// other field is `NOT NULL` and rejects an explicit `null`.
///
/// **Deleting a rule is this patch**, not a route of its own
/// ([`StepPatch::deleted_at`]'s precedent): one flagged column on the one
/// CAS write, so there is no second HTTP surface and no second piece of
/// auth reasoning.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RulePatch {
    pub expected_version: i64,
    #[serde(default, deserialize_with = "non_null_name", skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, deserialize_with = "touched", skip_serializing_if = "Option::is_none")]
    pub event_kind: Option<Option<String>>,
    #[serde(default, deserialize_with = "non_null_conditions", skip_serializing_if = "Option::is_none")]
    pub conditions: Option<Vec<Condition>>,
    #[serde(default, deserialize_with = "non_null_severity", skip_serializing_if = "Option::is_none")]
    pub severity: Option<String>,
    #[serde(default, deserialize_with = "non_null_tier", skip_serializing_if = "Option::is_none")]
    pub tier: Option<Tier>,
    #[serde(default, deserialize_with = "non_null_enabled", skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, deserialize_with = "touched", skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<Option<i64>>,
}

/// `PUT /api/settings/:key` body. `expected_version: 0` is the create case
/// (idempotent: a replay against the identical stored value is a no-op 200).
/// `value` is typed JSON on the wire and stored as its canonical
/// serialization — the stored text is valid JSON by construction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PutSetting {
    pub expected_version: i64,
    pub value: serde_json::Value,
}

/// `POST /api/snapshots` body (server-polled context, `ingest` scope only).
/// Identity is `(source, key)` and the row is replaced wholesale each poll
/// — ADR-0009's gauge lane, so there is no `expected_version` here for
/// [`AlertIngest`]'s reason: a poller tracks no versions and is
/// authoritative for its own row.
///
/// `payload` is typed JSON on the wire and stored as its canonical
/// serialization, like [`PutSetting::value`]. It must parse as ADR-0015's
/// [`SnapshotEnvelope`]; the handler rejects a broken one with the
/// [`EnvelopeProblem`]'s own wording, rather than storing a row every pane
/// reading it will render as a gap.
///
/// **`fetched_at` is required, and that is load-bearing.** Defaulting it to
/// the server clock would make an exact replay undecidable, and the replay
/// no-op is the only thing standing between a daily poll and a version bump
/// pushed to every device every morning for the life of an unchanged
/// answer. It is also *part of the value*: a fresh poll that read the same
/// answer still moves the stamp, because the stamp is what
/// `Freshness::of_snapshot` reads, and freezing it would make "poller fine,
/// nothing changed" indistinguishable from "poller dead".
///
/// [`SnapshotEnvelope`]: crate::SnapshotEnvelope
/// [`EnvelopeProblem`]: crate::EnvelopeProblem
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotIngest {
    pub source: String,
    pub key: String,
    pub payload: serde_json::Value,
    /// When the source was actually read, on the poller's clock.
    pub fetched_at: i64,
}

/// `POST /api/alerts` body (webhook ingest, `ingest` scope only). No
/// `expected_version`: webhook sources cannot track versions, and the
/// upsert on `(source, source_key)` is inherently absolute — the source is
/// authoritative for its own fields (ADR-0009 rule 3). Every source-owned
/// field is set from this payload on each raise (absent = NULL), with one
/// exception: an absent `raised_at` keeps the stored stamp on a re-raise, so
/// an identical replay is a byte-identical no-op. `dismissed_at` is
/// human-owned and never touched by ingest.
///
/// **`severity` is not a second exception, and was until #188.** It kept the
/// stored value while the alert was live, and could only ratchet up
/// (ADR-0014's ratchet, as originally written). ADR-0014's 2026-08-12
/// amendment scopes that ratchet to what it was aimed at — the mint-time
/// fold over concurrent rule verdicts, which every minting caller does
/// before it ever posts here — and moves monotonicity to the delivery layer,
/// which rings only on an escalation. So severity is now a *reading* like
/// any other source-owned field: a source may lower it on its own still-live
/// occurrence and may clear it by omission, and neither rings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AlertIngest {
    pub source: String,
    pub source_key: String,
    /// Optional (ADR-0015): the `context_snapshots.key` this alert is
    /// *about*, when the source has one — the pane join's own half. Absent
    /// is the norm and always legal; most sources answer no standing
    /// question, and `sweep_tick` deliberately never sends it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject_key: Option<String>,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub severity: Option<String>,
    /// Absent: the server clock on first raise; on a re-raise the stored
    /// stamp is kept, so an identical replayed payload is a no-op.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raised_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<i64>,
    /// Opt in to "re-raise only when something actually changed" (#120).
    /// Default `false`, so no shipped source moves.
    ///
    /// It exists for a **repeatedly-polled** source, where the same
    /// occurrence is re-reported every run for days. Such a source needs
    /// both halves of one rule and cannot express either on its own: a
    /// daily re-poll of an unchanged holiday slide must *not* restamp
    /// `raised_at` (`is_live` compares it against `dismissed_at`, so
    /// restamping undoes the human's dismissal every morning), while a
    /// genuine correction — Tuesday's slide moving to Wednesday — must
    /// re-ring even over that dismissal, because it is new information.
    ///
    /// The decision has to be the server's: an ingest token cannot read the
    /// alert back, so the poller has no way to know whether its write
    /// changes anything. With this set, the handler stamps `raised_at` with
    /// its own **write clock** on exactly the raises that change a
    /// source-owned field. The write clock, and not the poll's nominal cron
    /// slot, is load-bearing: a correction stamped at an 06:00 bucket lands
    /// *before* an 08:00 dismissal made the same morning and stays silently
    /// quiet.
    ///
    /// Mutually exclusive with an explicit `raised_at` (400): the two are
    /// contradictory instructions about the same field.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub restamp_on_change: bool,
}

/// `PATCH /api/alerts/:id` body (`device` scope): the human-owned dismiss
/// flag, set or cleared under CAS. The only alert field a device may write.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AlertPatch {
    pub expected_version: i64,
    #[serde(default, deserialize_with = "touched", skip_serializing_if = "Option::is_none")]
    pub dismissed_at: Option<Option<i64>>,
}

/// `POST /api/admin/tokens` body. `id` is client-supplied so the mint is
/// idempotent — but the plaintext token is shown only on the 201; a replay
/// returns the stored metadata without it (only the hash is kept).
///
/// `source` binds an `ingest` token to exactly one webhook source (#145,
/// ADR-0008's "one token per ingest source"): required when `scope` is
/// `ingest`, rejected for every other scope. The mint handler validates the
/// pairing; this shape just carries it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MintToken {
    pub id: String,
    pub name: String,
    pub scope: Scope,
    #[serde(default)]
    pub source: Option<String>,
}

/// `GET /api/changes?since=N` (and `GET /api/sweep`, which is the same
/// query with `since = 0`) response: the workspace version and every synced
/// row whose `version` is above the cursor.
///
/// `tokens` and `meta` are deliberately absent: tokens are per-writer
/// machinery that never syncs to clients, and `meta`'s counter is the
/// `version` field itself. `push_targets` and `deliveries` are absent too,
/// for the same reason as `tokens`: neither carries a `version` column
/// (#131) — they are server-side machinery, not delta-pulled records.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChangesResponse {
    pub version: i64,
    pub projects: Vec<Project>,
    pub routes: Vec<Route>,
    pub fog: Vec<Fog>,
    /// `project_links` (#626, ADR-0030 decision 4). `#[serde(default)]` on
    /// the same #131 precedent as `rules`/`grills`: a response predating
    /// this slice carries no `project_links` key at all and must still
    /// deserialize.
    #[serde(default)]
    pub project_links: Vec<ProjectLink>,
    pub items: Vec<Item>,
    pub steps: Vec<Step>,
    pub blocked_by: Vec<BlockedBy>,
    pub alerts: Vec<Alert>,
    pub context_snapshots: Vec<ContextSnapshot>,
    pub settings: Vec<Setting>,
    /// `#[serde(default)]`: a server response predating #131 (or a fixture
    /// written before it) carries no `rules` key at all — absent, not
    /// empty, and must still deserialize rather than fail the whole pull.
    #[serde(default)]
    pub rules: Vec<Rule>,
    /// The `grills` table (#353, ADR-0023 decision 4): every column except
    /// `transcript` — that rides `GET /api/grills/:id` only, never the
    /// sweep. `#[serde(default)]` on the same #131 precedent: a response
    /// predating this slice carries no `grills` key at all.
    #[serde(default)]
    pub grills: Vec<GrillWithoutTranscript>,
}

impl ChangesResponse {
    /// The unchanged-workspace response: the current version, every table
    /// empty.
    pub fn empty(version: i64) -> ChangesResponse {
        ChangesResponse {
            version,
            projects: vec![],
            routes: vec![],
            fog: vec![],
            project_links: vec![],
            items: vec![],
            steps: vec![],
            blocked_by: vec![],
            alerts: vec![],
            context_snapshots: vec![],
            settings: vec![],
            rules: vec![],
            grills: vec![],
        }
    }
}

/// `POST /api/google/calendar_token` 200 response (#577/#582): a Google
/// `calendar.readonly` access token the authority minted from its
/// server-held refresh token, and the absolute wall-clock millisecond it
/// expires. `expires_at_ms` already carries the authority's expiry slack —
/// the client does no arithmetic on it, dropping it straight into
/// `TokenResult.expiresAtMs`/`msUntilRotation`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalendarTokenResponse {
    pub access_token: String,
    pub expires_at_ms: i64,
}

/// Every non-2xx body except the 409 and the empty-bodied 401/403:
/// `{"error": code, "message": …}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiError {
    pub error: String,
    pub message: String,
}

pub const VERSION_CONFLICT: &str = "version_conflict";

/// The 409 body: a stale `expected_version` write is answered with the
/// current entity so the client can rebase (ADR-0008). Generic over the
/// entity; defaults to [`Item`] so S0-era call sites keep compiling.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConflictResponse<T = Item> {
    /// Always [`VERSION_CONFLICT`].
    pub error: String,
    pub current: T,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patch_absent_vs_null_vs_value() {
        let p: ItemPatch = serde_json::from_str(
            r#"{"expected_version": 3, "description": null, "context": "@calls"}"#,
        )
        .unwrap();
        assert_eq!(p.expected_version, 3);
        assert_eq!(p.description, Some(None), "explicit null = clear");
        assert_eq!(p.context, Some(Some("@calls".into())), "value = set");
        assert_eq!(p.deadline, None, "absent = untouched");
        assert_eq!(p.title, None);
    }

    #[test]
    fn create_defaults() {
        let c: CreateItem = serde_json::from_str(r#"{"id": "x", "title": "t"}"#).unwrap();
        assert_eq!(c.stage, None);
        assert_eq!(c.priority, None);
    }

    #[test]
    fn patch_requires_expected_version() {
        assert!(serde_json::from_str::<ItemPatch>(r#"{"title": "t"}"#).is_err());
    }

    /// The S2/S3 client serializes these DTOs: an untouched field must stay
    /// off the wire entirely, never appear as `null`.
    #[test]
    fn item_patch_default_serializes_to_a_wire_noop() {
        assert_eq!(
            serde_json::to_string(&ItemPatch::default()).unwrap(),
            r#"{"expected_version":0}"#
        );
    }

    /// Every entity patch honours the same absent/null/value contract.
    #[test]
    fn entity_patches_share_the_touched_contract() {
        let p: StepPatch =
            serde_json::from_str(r#"{"expected_version": 1, "done": true, "deleted_at": null}"#)
                .unwrap();
        assert_eq!(p.done, Some(true));
        assert_eq!(p.deleted_at, Some(None), "explicit null = clear");
        assert_eq!(p.body, None, "absent = untouched");

        assert!(
            serde_json::from_str::<StepPatch>(r#"{"expected_version": 1, "done": null}"#).is_err(),
            "NOT NULL column rejects explicit null"
        );
        assert!(
            serde_json::from_str::<FogPatch>(r#"{"expected_version": 1, "question": null}"#)
                .is_err()
        );

        let p: RoutePatch =
            serde_json::from_str(r#"{"expected_version": 2, "destination": "shipped"}"#).unwrap();
        assert_eq!(p.destination, Some(Some("shipped".into())));
        assert_eq!(p.notes, None);
    }

    /// #626: `url` is `NOT NULL` and rejects an explicit `null`, `label` and
    /// `removed_at` are double-`Option` — the same contract [`FogPatch`]
    /// carries for `question`/`resolved_at`.
    #[test]
    fn project_link_patch_shares_the_touched_contract() {
        assert!(
            serde_json::from_str::<ProjectLinkPatch>(r#"{"expected_version": 1, "url": null}"#)
                .is_err(),
            "url is NOT NULL"
        );
        let p: ProjectLinkPatch = serde_json::from_str(
            r#"{"expected_version": 1, "label": null, "removed_at": 7000}"#,
        )
        .unwrap();
        assert_eq!(p.label, Some(None), "explicit null = clear");
        assert_eq!(p.removed_at, Some(Some(7000)));
        assert_eq!(p.url, None, "absent = untouched");
    }

    #[test]
    fn create_project_link_defaults_label_to_none() {
        let c: CreateProjectLink = serde_json::from_str(
            r#"{"id": "l-1", "project_id": "p-1", "url": "https://example.com", "position": 1}"#,
        )
        .unwrap();
        assert_eq!(c.label, None);
    }

    #[test]
    fn scope_round_trips_and_rejects_unknown() {
        for scope in Scope::ALL {
            let json = serde_json::to_string(&scope).unwrap();
            assert_eq!(json, format!("\"{}\"", scope.as_str()));
            assert_eq!(Scope::parse(scope.as_str()), Some(scope));
        }
        assert_eq!(Scope::parse("admin"), None, "admin is a secret, not a scope");
        assert!(serde_json::from_str::<MintToken>(
            r#"{"id": "t", "name": "n", "scope": "admin"}"#
        )
        .is_err());
    }

    #[test]
    fn conflict_response_is_generic_over_the_entity() {
        let body = ConflictResponse {
            error: VERSION_CONFLICT.to_string(),
            current: crate::Step {
                id: "s-1".into(),
                item_id: "a-1".into(),
                body: "tick me".into(),
                done: false,
                position: 1,
                deleted_at: None,
                version: 4,
            },
        };
        let json = serde_json::to_string(&body).unwrap();
        let back: ConflictResponse<crate::Step> = serde_json::from_str(&json).unwrap();
        assert_eq!(back, body);
    }

    /// Pins the synced-table list: adding a table to the schema without
    /// adding it here (and to the sweep) would silently break "the mirror
    /// is the export".
    #[test]
    fn changes_response_carries_exactly_the_synced_tables() {
        let empty = ChangesResponse::empty(0);
        let value = serde_json::to_value(&empty).unwrap();
        // serde_json::Map sorts keys, so compare the set, not the order.
        let keys: Vec<&str> = value.as_object().unwrap().keys().map(|k| k.as_str()).collect();
        let mut expected = vec![
            "version",
            "projects",
            "routes",
            "fog",
            "project_links",
            "items",
            "steps",
            "blocked_by",
            "alerts",
            "context_snapshots",
            "settings",
            "rules",
            "grills",
        ];
        expected.sort_unstable();
        assert_eq!(keys, expected);
    }

    /// The acceptance criterion: a response minted before this slice (or a
    /// fixture written before it) carries no `grills` key at all, and must
    /// still deserialize — the same #131 precedent `rules` set.
    #[test]
    fn changes_response_without_a_grills_key_still_deserializes() {
        let pre_slice = r#"{
            "version": 1, "projects": [], "routes": [], "fog": [], "items": [],
            "steps": [], "blocked_by": [], "alerts": [], "context_snapshots": [],
            "settings": [], "rules": []
        }"#;
        let parsed: ChangesResponse = serde_json::from_str(pre_slice).unwrap();
        assert!(parsed.grills.is_empty());
    }

    /// The acceptance criterion for `project_links` (#626): a response
    /// minted before this slice — carrying no `project_links` key, same as
    /// the pre-grills fixture above — still deserializes.
    #[test]
    fn changes_response_without_a_project_links_key_still_deserializes() {
        let pre_slice = r#"{
            "version": 1, "projects": [], "routes": [], "fog": [], "items": [],
            "steps": [], "blocked_by": [], "alerts": [], "context_snapshots": [],
            "settings": [], "rules": [], "grills": []
        }"#;
        let parsed: ChangesResponse = serde_json::from_str(pre_slice).unwrap();
        assert!(parsed.project_links.is_empty());
    }

    /// `project_links` participates in the delta pull like any other entity
    /// — it must round-trip on the wire, not sit beside it.
    #[test]
    fn changes_response_carries_project_links() {
        let link = ProjectLink {
            id: "l-1".into(),
            project_id: "p-1".into(),
            url: "https://example.com".into(),
            label: Some("Example".into()),
            position: 1,
            removed_at: None,
            version: 1,
        };
        let response = ChangesResponse {
            project_links: vec![link.clone()],
            ..ChangesResponse::empty(1)
        };
        let json = serde_json::to_string(&response).unwrap();
        let back: ChangesResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back.project_links, vec![link]);
    }

    /// The acceptance criterion for the anti-goal: `grills` participates in
    /// the delta pull like any other entity, and its wire shape never
    /// carries a transcript.
    #[test]
    fn changes_response_carries_grills_without_a_transcript() {
        use crate::grill::{Grill, GrillVerdict};

        let grill = Grill {
            id: "g-1".into(),
            item_id: "a-1".into(),
            transcript: "secret turn-by-turn text".into(),
            summary: "s".into(),
            verdict: GrillVerdict::Resolved,
            model_proposal: "p".into(),
            applied_patch: "p".into(),
            resulting_stage: Stage::Ready,
            completed_at: 10,
            version: 1,
        };
        let response = ChangesResponse {
            grills: vec![(&grill).into()],
            ..ChangesResponse::empty(1)
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(!json.contains("secret turn-by-turn text"), "the transcript must never ride the wire: {json}");
        let back: ChangesResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back.grills.len(), 1);
        assert_eq!(back.grills[0].id, "g-1");
    }

    #[test]
    fn create_grill_round_trips_and_rejects_unknown_fields() {
        use crate::grill::GrillVerdict;

        let body = r#"{
            "id": "g-1", "item_id": "a-1", "expected_version": 1, "transcript": "t", "summary": "s",
            "verdict": "resolved", "model_proposal": "p", "applied_patch": "p"
        }"#;
        let create: CreateGrill = serde_json::from_str(body).unwrap();
        assert_eq!(create.verdict, GrillVerdict::Resolved);
        assert!(!create.delete_unticked_plan, "defaults to false");

        assert!(
            serde_json::from_str::<CreateGrill>(
                r#"{"id": "g", "item_id": "a", "expected_version": 1, "transcript": "t", "summary": "s",
                    "verdict": "resolved", "model_proposal": "p", "applied_patch": "p",
                    "resulting_stage": "ready"}"#
            )
            .is_err(),
            "resulting_stage is server-computed, never caller-supplied",
        );
    }

    /// #354: the CAS token that lets a completion's item mutation rebase
    /// like every other patch DTO — and the explicit plan-deletion gesture,
    /// which old bytes (pre-#354) still deserialize into `false`.
    #[test]
    fn create_grill_expected_version_is_required_and_delete_unticked_plan_defaults_false() {
        assert!(
            serde_json::from_str::<CreateGrill>(
                r#"{"id": "g", "item_id": "a", "transcript": "t", "summary": "s",
                    "verdict": "resolved", "model_proposal": "p", "applied_patch": "p"}"#
            )
            .is_err(),
            "expected_version has no default — a completion always names the item's CAS token",
        );

        let with_flag: CreateGrill = serde_json::from_str(
            r#"{"id": "g", "item_id": "a", "expected_version": 3, "transcript": "t", "summary": "s",
                "verdict": "resolved", "model_proposal": "p", "applied_patch": "p",
                "delete_unticked_plan": true}"#,
        )
        .unwrap();
        assert!(with_flag.delete_unticked_plan);
    }

    /// The acceptance criterion: `rules` participates in the delta pull like
    /// any other entity — it must appear on the wire, not sit beside it.
    #[test]
    fn changes_response_carries_rules() {
        let rule = Rule {
            id: "r-1".into(),
            name: "n".into(),
            event_kind: None,
            conditions: vec![],
            severity: "high".into(),
            tier: Tier::Urgent,
            enabled: true,
            updated_at: 0,
            version: 1,
            deleted_at: None,
        };
        let response = ChangesResponse {
            rules: vec![rule.clone()],
            ..ChangesResponse::empty(1)
        };
        let json = serde_json::to_string(&response).unwrap();
        let back: ChangesResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back.rules, vec![rule]);
    }

    #[test]
    fn alert_ingest_has_no_expected_version() {
        let a: AlertIngest = serde_json::from_str(
            r#"{"source": "healthchecks/v1", "source_key": "sweeper", "title": "down"}"#,
        )
        .unwrap();
        assert_eq!(a.raised_at, None, "server clock fills it");
        assert!(
            serde_json::from_str::<AlertIngest>(
                r#"{"source": "s", "source_key": "k", "title": "t", "expected_version": 1}"#
            )
            .is_err(),
            "ingest is version-blind by design"
        );
    }
}
