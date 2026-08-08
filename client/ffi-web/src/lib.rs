//! `hummingbird-ffi-web`: `#[wasm_bindgen]` wrappers over `hummingbird-core`
//! for TypeScript, building for `wasm32-unknown-unknown` (ADR-0003).
//!
//! Stub only — proves the wasm-bindgen export path compiles and packages.
//! The real web client (#69) loads this in a Web Worker.

use wasm_bindgen::prelude::*;

/// The public API version of the wrapped `hummingbird-core`, surfaced
/// verbatim to TypeScript (ADR-0001 seam rule 2).
#[wasm_bindgen]
pub fn core_api_version() -> u32 {
    hummingbird_core::Core::new().api_version()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exported_stub_matches_core_api_version() {
        assert_eq!(core_api_version(), hummingbird_core::API_VERSION);
    }
}
