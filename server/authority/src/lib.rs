//! `hummingbird-authority`: the pure handler logic of the owned authority
//! (ADR-0008) — routing, parsing, validation, CAS writes and version-gated
//! reads — over the [`Sql`] seam. No runtime, no bindings: the `workers-rs`
//! shim (`hummingbird-authority-worker`) supplies a [`Sql`] over the Durable
//! Object's SQLite and forwards requests; tests supply rusqlite. The same
//! discipline as `client/core`'s transport seam, in sync form (the DO is
//! single-threaded and its SQLite API synchronous).

mod codec;
mod delivery;
mod entropy;
mod handlers;
mod schema;
mod sql;
mod sweep;

pub use delivery::{deliver, DeliveryOutcome, PushNotification, SuppressReason};
pub use entropy::Entropy;
pub use handlers::{handle, ApiRequest, ApiResponse, HandleContext};
pub use schema::{init_schema, SCHEMA_VERSION};
pub use sql::{Row, Sql, SqlError, SqlValue};
pub use sweep::{tick as sweep_tick, TickMatch, ALARM_INTERVAL_MS};

#[cfg(test)]
mod tests {
    /// The pure crate must stay natively testable forever: no bindings, no
    /// runtime (the `client/core/src/lib.rs` guard, extended server-side).
    /// rusqlite is the sole exception, dev-only.
    #[test]
    fn cargo_toml_has_no_binding_or_runtime_dependencies() {
        let manifest = include_str!("../Cargo.toml");
        let dependencies = manifest
            .split("[dev-dependencies]")
            .next()
            .expect("split always yields one piece");
        for forbidden in ["uniffi", "wasm-bindgen", "wasm_bindgen", "js-sys", "\nworker"] {
            assert!(
                !dependencies.contains(forbidden),
                "server/authority/Cargo.toml must not depend on `{forbidden}` — \
                 handler logic is runtime-agnostic and natively testable",
            );
        }
    }

    /// The serde strings of the domain enums are byte-for-byte the DDL
    /// CHECK literals — the equivalence every write and read leans on.
    #[test]
    fn enum_strings_appear_in_the_ddl() {
        use crate::schema::{CREATE_ITEMS, CREATE_PUSH_TARGETS, CREATE_RULES, CREATE_TOKENS};
        use hummingbird_domain::{Energy, Platform, Scope, Size, Stage, Tier};

        for stage in Stage::ALL {
            assert!(
                CREATE_ITEMS.contains(&format!("'{}'", stage.as_str())),
                "stage `{}` missing from the items DDL CHECK",
                stage.as_str(),
            );
        }
        for size in Size::ALL {
            assert!(CREATE_ITEMS.contains(&format!("'{}'", size.as_str())));
        }
        for energy in Energy::ALL {
            assert!(CREATE_ITEMS.contains(&format!("'{}'", energy.as_str())));
        }
        for scope in Scope::ALL {
            assert!(
                CREATE_TOKENS.contains(&format!("'{}'", scope.as_str())),
                "scope `{}` missing from the tokens DDL CHECK",
                scope.as_str(),
            );
        }
        for tier in Tier::ALL {
            assert!(
                CREATE_RULES.contains(&format!("'{}'", tier.as_str())),
                "tier `{}` missing from the rules DDL CHECK",
                tier.as_str(),
            );
        }
        for platform in Platform::ALL {
            assert!(
                CREATE_PUSH_TARGETS.contains(&format!("'{}'", platform.as_str())),
                "platform `{}` missing from the push_targets DDL CHECK",
                platform.as_str(),
            );
        }
        // event_kind is deliberately absent from this test: ADR-0013 drops
        // its CHECK, and asserting a closed set here would reintroduce the
        // frozen registry the amendment removed.
    }
}
