//! The binding-generator entry point (`cargo run --features bindgen --bin
//! uniffi-bindgen -- generate --library …`), library-mode per the UniFFI
//! manual: bindings are derived from the built `cdylib`'s metadata, so the
//! exported surface in `lib.rs` is the single source of truth — no `.udl`.

fn main() {
    uniffi::uniffi_bindgen_main()
}
