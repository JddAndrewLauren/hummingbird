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
mod item;
mod project;
mod rule;
mod step;
mod token;

pub use api::{
    AlertIngest, AlertPatch, ApiError, BlockedByPatch, ChangesResponse, ConflictResponse,
    CreateBlockedBy, CreateFog, CreateItem, CreateProject, CreateRule, CreateStep, FogPatch,
    ItemPatch, MintToken, ProjectPatch, PutSetting, RoutePatch, RulePatch, StepPatch,
    VERSION_CONFLICT,
};
pub use context::{Alert, ContextSnapshot, Setting};
pub use deadline::is_valid_deadline;
pub use item::{Energy, Item, Size, Stage};
pub use project::{Fog, Project, Route};
pub use rule::{Condition, Delivery, Platform, PushTarget, Rule, Tier};
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
