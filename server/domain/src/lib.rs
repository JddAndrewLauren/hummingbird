//! `hummingbird-domain`: the owned-schema task model (ADR-0009) plus the
//! wire DTOs of the authority's API (ADR-0008). This is the standalone
//! crate both the server (`hummingbird-authority`, from S0/#113) and the
//! client (`hummingbird-core`, from S2/S3 — #100/#101) compile, so schema
//! agreement is a compiler guarantee.
//!
//! Deliberately not a move of the S1 model in `client/core/src/task/`: the
//! owned schema dissolves `labels` into typed `size`/`energy`/`context`,
//! `presence` into `archived_at`, `identifier` into the server-minted `seq`,
//! drops the `extra` passthrough, and adds the integer `version` every CAS
//! write pivots on. The client migrates onto these types at S2/S3.

mod api;
mod context;
mod deadline;
mod event;
mod item;
mod live;
mod project;
mod rule;
mod severity;
mod sources;
mod step;
mod token;

pub use api::{
    AlertIngest, AlertPatch, ApiError, BlockedByPatch, ChangesResponse, ConflictResponse,
    CreateBlockedBy, CreateFog, CreateItem, CreateProject, CreatePushTarget, CreateRule,
    CreateStep, FogPatch, ItemPatch, MintToken, ProjectPatch, PutSetting, RoutePatch, RulePatch,
    SnapshotIngest, StepPatch, VERSION_CONFLICT,
};
pub use context::{Alert, ContextSnapshot, EnvelopeProblem, Setting, SnapshotEnvelope};
pub use deadline::{
    deadline_sort_key, is_valid_deadline, now_as_deadline, parse_duration, shift, DurationUnit,
};
pub use event::{
    core_field_type, find_kind, kind_registry_json, Event, EventKindEntry, FieldDescriptor,
    FieldType, FieldValue, CORE_FIELDS, EVENT_KINDS,
};
pub use item::{Energy, Item, Size, Stage};
pub use live::{is_live, settled_at};
pub use project::{Fog, Project, Route};
pub use rule::{Condition, Delivery, Platform, PushTarget, Rule, Tier};
pub use severity::{higher_severity, severity_rank, SEVERITIES};
pub use sources::{
    city_waste_v1_key, city_waste_v2_key, find as find_source, github_v1_key, gmail_alert_v1_key,
    gmail_v1_key, google_calendar_v1_key, healthchecks_v1_key, home_assistant_v1_key,
    item_threshold_v1_key, m365_calendar_v1_key, m365_mail_v1_key, photo_site_v1_key,
    race_schedule_v1_key, Expiry, Shape, SourceEntry, Writes, CITY_WASTE_V2, GMAIL_V1,
    GOOGLE_CALENDAR_V1, ITEM_THRESHOLD_V1, M365_CALENDAR_V1, M365_MAIL_V1, RACE_SCHEDULE_V1,
    REGISTRY,
};
pub use step::{BlockedBy, Step};
pub use token::{MintedToken, Scope, TokenInfo};

#[cfg(test)]
mod tests {
    /// Mechanically enforces that this crate stays compilable on every
    /// target the client and server share — it must never gain a binding or
    /// runtime dependency (the ADR-0003 rule, extended to the server side).
    #[test]
    fn cargo_toml_has_no_binding_or_runtime_dependencies() {
        let manifest = include_str!("../Cargo.toml");
        for forbidden in ["uniffi", "wasm-bindgen", "wasm_bindgen", "js-sys", "worker"] {
            assert!(
                !manifest.contains(forbidden),
                "server/domain/Cargo.toml must not depend on `{forbidden}` — \
                 the domain crate is binding- and runtime-agnostic",
            );
        }
    }
}
