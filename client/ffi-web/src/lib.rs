//! `hummingbird-ffi-web`: `#[wasm_bindgen]` wrappers over `hummingbird-core`
//! for TypeScript, building for `wasm32-unknown-unknown` (ADR-0003).
//!
//! [`calendar_host::CalendarHostCore`] (issue #73) is the web host's one
//! door into #72's `ContextPoller`/#71's Google adapter: push a token, drive
//! the selected calendars, trigger a poll, drain credential events, and
//! read the current/next event for the context tile. It is plain,
//! `wasm_bindgen`-free Rust, testable with `cargo test --workspace` on any
//! target; [`CalendarHost`] below is the thin `#[wasm_bindgen]` shim over it
//! that only compiles for `wasm32` — `js_sys`/`wasm-bindgen-futures`'s JS
//! interop has no working implementation to test against outside an actual
//! JS host, so nothing in this crate's native test run exercises it. The
//! `wasm32` build itself is what CI's `cargo build --target
//! wasm32-unknown-unknown -p hummingbird-ffi-web` step gates.

mod calendar_host;

pub use calendar_host::{CalendarHostCore, CurrentNextResponse};

use wasm_bindgen::prelude::*;

/// The public API version of the wrapped `hummingbird-core`, surfaced
/// verbatim to TypeScript (ADR-0001 seam rule 2).
#[wasm_bindgen]
pub fn core_api_version() -> u32 {
    hummingbird_core::Core::new().api_version()
}

#[cfg(target_arch = "wasm32")]
mod wasm_bindings {
    use std::cell::RefCell;
    use std::rc::Rc;

    use wasm_bindgen::prelude::*;
    use wasm_bindgen_futures::future_to_promise;

    use super::calendar_host::{outcome_name, CalendarHostCore};

    /// The Google Calendar context poller, wrapped for TypeScript.
    ///
    /// Held behind `Rc<RefCell<_>>` rather than exposed as `&mut self`
    /// methods: wasm-bindgen cannot export an async `&mut self` method (the
    /// borrow can't be proven to outlive a suspend point), and every
    /// trigger method here (`start`/`refresh`/`onTimer`) needs `&mut` for
    /// the poll attempt's duration. The host (the core Web Worker, #69)
    /// only ever calls one `CalendarHost` method at a time and awaits each
    /// before issuing the next, so the `RefCell` borrow never overlaps in
    /// practice; a concurrent call would panic rather than corrupt state.
    #[wasm_bindgen]
    pub struct CalendarHost {
        inner: Rc<RefCell<CalendarHostCore>>,
    }

    #[wasm_bindgen]
    impl CalendarHost {
        /// `namespace` becomes the IndexedDB database name (ADR-0003: the
        /// host contributes exactly one thing at init — a storage
        /// path/namespace). `calendar_ids` is the picker's initial
        /// selection; empty is a valid steady state (never-opted-in /
        /// nothing picked yet).
        #[wasm_bindgen(constructor)]
        pub fn new(namespace: String, calendar_ids: Vec<String>) -> CalendarHost {
            CalendarHost {
                inner: Rc::new(RefCell::new(CalendarHostCore::new(namespace, calendar_ids))),
            }
        }

        /// The host calls this at init and on every token rotation (silent
        /// re-mint or a re-connect round-trip).
        #[wasm_bindgen(js_name = pushToken)]
        pub fn push_token(&self, token: String) {
            self.inner.borrow_mut().push_token(token);
        }

        /// The calendar picker's current selection; takes effect on the
        /// next poll trigger.
        #[wasm_bindgen(js_name = setCalendarIds)]
        pub fn set_calendar_ids(&self, calendar_ids: Vec<String>) {
            self.inner.borrow().set_calendar_ids(calendar_ids);
        }

        /// Core-start trigger. Resolves to one of `"no_credential"`,
        /// `"held"`, `"succeeded"`, `"transient_failure"`, `"unauthorized"`.
        pub fn start(&self, now_ms: f64) -> js_sys::Promise {
            let inner = self.inner.clone();
            future_to_promise(async move {
                let outcome = inner.borrow_mut().start(now_ms as i64).await;
                Ok(JsValue::from_str(outcome_name(outcome)))
            })
        }

        /// Explicit user-invoked refresh.
        pub fn refresh(&self, now_ms: f64) -> js_sys::Promise {
            let inner = self.inner.clone();
            future_to_promise(async move {
                let outcome = inner.borrow_mut().refresh(now_ms as i64).await;
                Ok(JsValue::from_str(outcome_name(outcome)))
            })
        }

        /// The foreground 15-minute timer tick (ADR-0007); the host is
        /// responsible for only calling this while online and foregrounded.
        #[wasm_bindgen(js_name = onTimer)]
        pub fn on_timer(&self, now_ms: f64) -> js_sys::Promise {
            let inner = self.inner.clone();
            future_to_promise(async move {
                let outcome = inner.borrow_mut().on_timer(now_ms as i64).await;
                Ok(JsValue::from_str(outcome_name(outcome)))
            })
        }

        /// Drains every credential-needed event since the last drain, as a
        /// JSON array of `{"provider": string, "at_ms": number}`.
        #[wasm_bindgen(js_name = takeCredentialEvents)]
        pub fn take_credential_events(&self) -> String {
            let events = self.inner.borrow_mut().take_credential_events();
            serde_json::to_string(&events).expect("CredentialEvent serializes")
        }

        /// The current/next event as of `now_ms`, as JSON:
        /// `{"kind": "no_snapshot"|"none"|"in_progress"|"upcoming",
        ///   "event": EventRecord | null, "as_of_ms": number | null}`.
        #[wasm_bindgen(js_name = currentOrNext)]
        pub fn current_or_next(&self, now_ms: f64) -> js_sys::Promise {
            let inner = self.inner.clone();
            future_to_promise(async move {
                let response = inner.borrow().current_or_next(now_ms as i64).await;
                Ok(JsValue::from_str(
                    &serde_json::to_string(&response).expect("CurrentNextResponse serializes"),
                ))
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exported_stub_matches_core_api_version() {
        assert_eq!(core_api_version(), hummingbird_core::API_VERSION);
    }
}
