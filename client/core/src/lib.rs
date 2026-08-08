//! `hummingbird-core`: the binding-agnostic sync engine core (ADR-0003).
//!
//! This crate has zero binding-macro dependencies (`uniffi`, `wasm_bindgen`)
//! by design — see the `cargo_toml_has_no_binding_macro_dependencies` test
//! below, which enforces that mechanically rather than by convention.
//!
//! [`Core`] is the one storage/sync public API that both `ffi-mobile` and
//! `ffi-web` surface verbatim (ADR-0001 seam rule 2). It is a stub today;
//! persistence lands in a later issue (#68).

/// The public API version both FFI crates surface.
pub const API_VERSION: u32 = 1;

/// Handle for the sync engine.
///
/// Stubbed out for now — no persistence, no sync, no calendar logic. Those
/// land in later issues.
#[derive(Debug, Default)]
pub struct Core;

impl Core {
    /// Creates a new core handle.
    pub fn new() -> Self {
        Self
    }

    /// The public API version this core implements.
    pub fn api_version(&self) -> u32 {
        API_VERSION
    }
}

/// Performs an HTTP GET and returns the response status code.
///
/// A minimal placeholder proving the core owns HTTP via `reqwest` (ADR-0003):
/// one async HTTP path serves every client, including the browser via
/// `reqwest`'s Fetch-backed `wasm32` target.
pub async fn fetch_status(client: &reqwest::Client, url: &str) -> Result<u16, reqwest::Error> {
    let response = client.get(url).send().await?;
    Ok(response.status().as_u16())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_version_is_stable() {
        let core = Core::new();
        assert_eq!(core.api_version(), API_VERSION);
    }

    #[tokio::test]
    async fn fetch_status_compiles_and_is_callable() {
        // No network call in CI: this only proves the async reqwest-backed
        // HTTP path type-checks against an unroutable address rather than
        // depending on network access being available in the sandbox.
        let client = reqwest::Client::new();
        let result = fetch_status(&client, "http://127.0.0.1:0/").await;
        assert!(result.is_err());
    }

    /// Mechanically enforces the binding-agnostic rule from ADR-0003: this
    /// crate must never gain a `uniffi` or `wasm_bindgen` dependency,
    /// checked against the crate's own manifest text rather than convention.
    #[test]
    fn cargo_toml_has_no_binding_macro_dependencies() {
        let manifest = include_str!("../Cargo.toml");
        for forbidden in ["uniffi", "wasm-bindgen", "wasm_bindgen"] {
            assert!(
                !manifest.contains(forbidden),
                "client/core/Cargo.toml must not depend on `{forbidden}` — \
                 core is binding-agnostic (ADR-0003)",
            );
        }
    }
}
