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

pub use calendar_host::{CalendarHostCore, CalendarListResponse, CurrentNextResponse};

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

    /// Everything a synchronous setter has to defer while the core is
    /// checked out for a poll. Applied, in this order, at check-in.
    #[derive(Default)]
    struct Pending {
        token: Option<String>,
        calendar_ids: Option<Vec<String>>,
    }

    /// The core plus its check-out slot. `Option` is doing load-bearing
    /// work: `None` means "checked out by an in-flight async call", which is
    /// a state this type can answer for, unlike a `RefCell` borrow held
    /// across an await (that is a panic waiting to happen — see
    /// [`CalendarHost`]).
    struct Shared {
        core: RefCell<Option<CalendarHostCore>>,
        pending: RefCell<Pending>,
    }

    impl Shared {
        fn new(core: CalendarHostCore) -> Self {
            Self {
                core: RefCell::new(Some(core)),
                pending: RefCell::new(Pending::default()),
            }
        }

        /// Takes the core for the duration of one async call, or `None` if
        /// another call already holds it. Every borrow here is released
        /// before the caller's first await.
        fn check_out(&self) -> Option<CalendarHostCore> {
            self.core.borrow_mut().take()
        }

        /// Returns the core and applies whatever landed while it was out.
        /// A token pushed mid-poll therefore takes effect on the next
        /// attempt, which is the same semantics the poller documents for a
        /// selection change.
        fn check_in(&self, mut core: CalendarHostCore) {
            let pending = std::mem::take(&mut *self.pending.borrow_mut());
            if let Some(token) = pending.token {
                core.push_token(token);
            }
            if let Some(calendar_ids) = pending.calendar_ids {
                core.set_calendar_ids(calendar_ids);
            }
            *self.core.borrow_mut() = Some(core);
        }

        fn push_token(&self, token: String) {
            match self.core.borrow_mut().as_mut() {
                Some(core) => core.push_token(token),
                None => self.pending.borrow_mut().token = Some(token),
            }
        }

        fn set_calendar_ids(&self, calendar_ids: Vec<String>) {
            match self.core.borrow().as_ref() {
                Some(core) => core.set_calendar_ids(calendar_ids),
                None => self.pending.borrow_mut().calendar_ids = Some(calendar_ids),
            }
        }

        /// Drains credential events, or yields none while the core is
        /// checked out — the events stay queued in the core and the next
        /// drain gets them. In practice the host drains right after a poll
        /// promise resolves, which is after check-in.
        fn take_credential_events(&self) -> String {
            let events = match self.core.borrow_mut().as_mut() {
                Some(core) => core.take_credential_events(),
                None => Vec::new(),
            };
            serde_json::to_string(&events).expect("CredentialEvent serializes")
        }
    }

    /// The Google Calendar context poller, wrapped for TypeScript.
    ///
    /// Held behind `Rc<_>` rather than exposed as `&mut self` methods:
    /// wasm-bindgen cannot export an async `&mut self` method (the borrow
    /// can't be proven to outlive a suspend point), and every trigger method
    /// here (`start`/`refresh`/`onTimer`) needs `&mut` for the poll
    /// attempt's duration.
    ///
    /// The obvious shape — `Rc<RefCell<CalendarHostCore>>` with
    /// `borrow_mut().start(..).await` — is wrong, and was: that holds the
    /// borrow across the poll's network await, so any second call arriving
    /// while a poll is in flight panics on the `RefCell`, and a wasm panic
    /// poisons the whole module rather than failing one call. The worker's
    /// request queue (`worker/calendar-worker.ts`) makes such a call
    /// impossible today, but "no caller does this" is not a property this
    /// module can enforce, so it no longer depends on it: an async call
    /// checks the core *out* of the cell, awaits with no borrow held, and
    /// checks it back in. An overlapping call finds it absent and resolves
    /// to `"busy"` instead of panicking.
    #[wasm_bindgen]
    pub struct CalendarHost {
        inner: Rc<Shared>,
    }

    /// What a poll trigger resolves to when the core is already checked out.
    /// Distinct from `"held"`/`"transient_failure"`: nothing was attempted
    /// and nothing is wrong with the credential.
    const BUSY_OUTCOME: &str = "busy";

    /// What `currentOrNext` resolves to when the core is already checked
    /// out. The host treats it as "no new information" and leaves the tile
    /// as it stands, rather than blanking it.
    const BUSY_CURRENT_NEXT: &str = r#"{"kind":"busy","event":null,"as_of_ms":null}"#;

    /// The same "no new information" answer for `listCalendars`: the host
    /// leaves the picker's existing options alone rather than emptying it.
    const BUSY_CALENDAR_LIST: &str = r#"{"kind":"busy","calendars":[]}"#;

    /// Which `ContextPoller` trigger a `poll` call stands for. The three
    /// exported triggers differ only in this, so they share one body rather
    /// than three copies of the check-out/await/check-in dance.
    enum Trigger {
        Start,
        Refresh,
        Timer,
    }

    /// One poll attempt with no `RefCell` borrow held across the await.
    async fn poll(inner: &Rc<Shared>, now_ms: f64, trigger: Trigger) -> JsValue {
        let Some(mut core) = inner.check_out() else {
            return JsValue::from_str(BUSY_OUTCOME);
        };
        let now_ms = now_ms as i64;
        let outcome = match trigger {
            Trigger::Start => core.start(now_ms).await,
            Trigger::Refresh => core.refresh(now_ms).await,
            Trigger::Timer => core.on_timer(now_ms).await,
        };
        inner.check_in(core);
        JsValue::from_str(outcome_name(outcome))
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
                inner: Rc::new(Shared::new(CalendarHostCore::new(namespace, calendar_ids))),
            }
        }

        /// The host calls this at init and on every token rotation (silent
        /// re-mint or a re-connect round-trip).
        #[wasm_bindgen(js_name = pushToken)]
        pub fn push_token(&self, token: String) {
            self.inner.push_token(token);
        }

        /// The calendar picker's current selection; takes effect on the
        /// next poll trigger.
        #[wasm_bindgen(js_name = setCalendarIds)]
        pub fn set_calendar_ids(&self, calendar_ids: Vec<String>) {
            self.inner.set_calendar_ids(calendar_ids);
        }

        /// Core-start trigger. Resolves to one of `"no_credential"`,
        /// `"held"`, `"succeeded"`, `"transient_failure"`, `"unauthorized"`,
        /// `"busy"`.
        pub fn start(&self, now_ms: f64) -> js_sys::Promise {
            let inner = self.inner.clone();
            future_to_promise(async move { Ok(poll(&inner, now_ms, Trigger::Start).await) })
        }

        /// Explicit user-invoked refresh.
        pub fn refresh(&self, now_ms: f64) -> js_sys::Promise {
            let inner = self.inner.clone();
            future_to_promise(async move { Ok(poll(&inner, now_ms, Trigger::Refresh).await) })
        }

        /// The foreground 15-minute context-poll timer tick (#46, under
        /// ADR-0005); the host is responsible for only calling this while
        /// online and foregrounded.
        #[wasm_bindgen(js_name = onTimer)]
        pub fn on_timer(&self, now_ms: f64) -> js_sys::Promise {
            let inner = self.inner.clone();
            future_to_promise(async move { Ok(poll(&inner, now_ms, Trigger::Timer).await) })
        }

        /// The calendars this device's credential can read, as JSON:
        /// `{"kind": "ok"|"no_credential"|"failed"|"busy",
        ///   "calendars": [{"id": string, "summary": string}]}`.
        ///
        /// No token parameter: the core already holds the one it polls with
        /// (`pushToken`), so this endpoint costs the host nothing and the
        /// credential never crosses the boundary a second time.
        #[wasm_bindgen(js_name = listCalendars)]
        pub fn list_calendars(&self) -> js_sys::Promise {
            let inner = self.inner.clone();
            future_to_promise(async move {
                let Some(core) = inner.check_out() else {
                    return Ok(JsValue::from_str(BUSY_CALENDAR_LIST));
                };
                let response = core.list_calendars().await;
                inner.check_in(core);
                Ok(JsValue::from_str(
                    &serde_json::to_string(&response).expect("CalendarListResponse serializes"),
                ))
            })
        }

        /// Drains every credential-needed event since the last drain, as a
        /// JSON array of `{"provider": string, "at_ms": number}`.
        #[wasm_bindgen(js_name = takeCredentialEvents)]
        pub fn take_credential_events(&self) -> String {
            self.inner.take_credential_events()
        }

        /// The current/next event as of `now_ms`, as JSON:
        /// `{"kind": "no_snapshot"|"none"|"in_progress"|"upcoming"|"busy",
        ///   "event": EventRecord | null, "as_of_ms": number | null}`.
        #[wasm_bindgen(js_name = currentOrNext)]
        pub fn current_or_next(&self, now_ms: f64) -> js_sys::Promise {
            let inner = self.inner.clone();
            future_to_promise(async move {
                let Some(core) = inner.check_out() else {
                    return Ok(JsValue::from_str(BUSY_CURRENT_NEXT));
                };
                let response = core.current_or_next(now_ms as i64).await;
                inner.check_in(core);
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
