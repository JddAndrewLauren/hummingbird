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
mod task_host;

pub use calendar_host::{CalendarHostCore, CalendarListResponse, CurrentNextResponse};
pub use task_host::{
    CaptureResponse, DeadLetterEntryDTO, DeadLetterFieldDTO, DeadLettersResponse,
    IsPendingResponse, ItemListResponse, MirrorSnapshotResponse, QueueDepthResponse, RunResponse,
    TaskEventDTO, TaskHostCore,
};

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

    // ------------------------------------------------------------ TaskHost

    use super::task_host::TaskHostCore;

    /// Whatever a synchronous setter had to defer because [`TaskShared`]'s
    /// host was checked out — mutually exclusive, since only the most
    /// recent request matters: whichever of push/rehydrate/clear landed
    /// *last* while checked out is what [`TaskShared::check_in`] applies,
    /// matching what would have happened had the host not been busy.
    enum PendingApiKeyOp {
        Push(String),
        /// Issue #196 (shape 2): the rehydration counterpart to `Push` —
        /// see [`TaskShared::rehydrate_api_key`].
        Rehydrate(String),
        Clear,
    }

    /// The same check-out/check-in shape as [`Shared`] above, generic over
    /// which host it wraps rather than a second copy of the borrow-safety
    /// logic — see `Shared`'s own docs for why this exists at all (a wasm
    /// panic from a `RefCell` borrow held across an await poisons the whole
    /// module, not just one call).
    ///
    /// `pending_op` mirrors [`Shared`]'s own `Pending` slot (PR #171
    /// round-1 review): a push/rehydrate/clear requested while a
    /// `run`/`capture` call holds the host would otherwise be silently
    /// dropped on the floor rather than merely delayed, and nothing
    /// upstream of #106 re-sends it — the host has no way to know its
    /// request was lost. Applied at [`TaskShared::check_in`], same as
    /// [`Shared::check_in`] applies a pending token/selection.
    struct TaskShared {
        host: RefCell<Option<TaskHostCore>>,
        pending_op: RefCell<Option<PendingApiKeyOp>>,
    }

    impl TaskShared {
        fn new(host: TaskHostCore) -> Self {
            Self {
                host: RefCell::new(Some(host)),
                pending_op: RefCell::new(None),
            }
        }

        fn check_out(&self) -> Option<TaskHostCore> {
            self.host.borrow_mut().take()
        }

        fn check_in(&self, mut host: TaskHostCore) {
            match self.pending_op.borrow_mut().take() {
                Some(PendingApiKeyOp::Clear) => host.clear_api_key(),
                Some(PendingApiKeyOp::Push(api_key)) => host.push_api_key(api_key),
                Some(PendingApiKeyOp::Rehydrate(api_key)) => host.rehydrate_api_key(api_key),
                None => {}
            }
            *self.host.borrow_mut() = Some(host);
        }

        /// Pushes immediately if the host is present, or queues for the next
        /// [`TaskShared::check_in`] if it is currently checked out — never
        /// silently drops the key either way. A queued push supersedes any
        /// other queued op: this is the more recent request.
        fn push_api_key(&self, api_key: String) {
            match self.host.borrow_mut().as_mut() {
                Some(host) => host.push_api_key(api_key),
                None => *self.pending_op.borrow_mut() = Some(PendingApiKeyOp::Push(api_key)),
            }
        }

        /// Issue #196 (shape 2): the rehydration counterpart to
        /// [`TaskShared::push_api_key`] — applies immediately if the host is
        /// present, or queues otherwise, same as a push, but never resumes a
        /// hold either way. See [`TaskHostCore::rehydrate_api_key`].
        fn rehydrate_api_key(&self, api_key: String) {
            match self.host.borrow_mut().as_mut() {
                Some(host) => host.rehydrate_api_key(api_key),
                None => *self.pending_op.borrow_mut() = Some(PendingApiKeyOp::Rehydrate(api_key)),
            }
        }

        /// "Forget token" (#106/S8): clears immediately if the host is
        /// present, or queues for the next [`TaskShared::check_in`]
        /// otherwise. A queued clear supersedes any other queued op.
        fn clear_api_key(&self) {
            match self.host.borrow_mut().as_mut() {
                Some(host) => host.clear_api_key(),
                None => *self.pending_op.borrow_mut() = Some(PendingApiKeyOp::Clear),
            }
        }
    }

    /// `"busy"` for every JSON response shape this host resolves to, when
    /// the core is checked out — the read-only getters' fallback (they never
    /// await, but must still answer *something* if a concurrent async call
    /// happens to be mid-flight; see `TaskHost::frontier`).
    const BUSY_ITEM_LIST: &str = r#"{"kind":"busy","items":[]}"#;
    const BUSY_IS_PENDING: &str = r#"{"kind":"busy","pending":false}"#;
    const BUSY_CAPTURE: &str = r#"{"kind":"busy","id":null,"error":null}"#;
    const BUSY_RUN: &str = r#"{"kind":"busy","retry_after_ms":null,"active_item_count":null,"was_full_sweep":null,"dead_lettered":null}"#;
    const BUSY_QUEUE_DEPTH: &str = r#"{"kind":"busy","depth":0}"#;
    const BUSY_DEAD_LETTERS: &str = r#"{"kind":"busy","entries":[]}"#;
    const BUSY_MIRROR_SNAPSHOT: &str = r#"{"kind":"busy","mirror":null}"#;

    /// The owned-schema task binding, wrapped for TypeScript (#105/S7) — the
    /// shape [`CalendarHost`] above already proved, one door into #104's
    /// `Core` instead of #72's `ContextPoller`.
    #[wasm_bindgen]
    pub struct TaskHost {
        inner: Rc<TaskShared>,
    }

    /// Constructs and durably loads a [`TaskHost`] — async because
    /// [`TaskHostCore::init`] is (it loads whatever a previous session left
    /// durable). `wasm-bindgen` turns a `pub async fn` returning
    /// `Result<T, JsValue>` into a function returning a rejected/resolved
    /// `Promise` automatically; nothing here needs `future_to_promise`
    /// itself. Rejects if the durable load fails (a corrupt snapshot) — the
    /// worker surfaces that the same way it surfaces a wasm import failure
    /// (see `core.worker.ts`).
    #[wasm_bindgen(js_name = createTaskHost)]
    pub async fn create_task_host(
        namespace: String,
        base_url: String,
        api_key: String,
    ) -> Result<TaskHost, JsValue> {
        let host = TaskHostCore::init(namespace, base_url, api_key)
            .await
            .map_err(|error| JsValue::from_str(&error.to_string()))?;
        Ok(TaskHost {
            inner: Rc::new(TaskShared::new(host)),
        })
    }

    #[wasm_bindgen]
    impl TaskHost {
        /// The host calls this once a device token is known (startup, or a
        /// rotation) — the API key crosses main -> worker -> here exactly
        /// once per push and is never read back out by any method on this
        /// type. Queued rather than dropped if the host is currently
        /// checked out mid-`run`/`capture` — see [`TaskShared`]'s doc.
        #[wasm_bindgen(js_name = pushApiKey)]
        pub fn push_api_key(&self, api_key: String) {
            self.inner.push_api_key(api_key);
        }

        /// Issue #196 (shape 2): the host calls this — never `pushApiKey` —
        /// to rehydrate whatever device token it already has stored: at
        /// core start, and every time a view reaches `ready` under #126's
        /// one-shared-core-per-origin (`useTaskTokenWiring.ts`'s core-start
        /// effect). Unlike `pushApiKey`, this NEVER resumes a hold and never
        /// retracts a pending re-auth prompt — a later view rehydrating the
        /// very token that just got rejected must not be able to trigger a
        /// retry of a credential already known to be dead. Only a genuinely
        /// re-entered or changed token, submitted through `pushApiKey`,
        /// resumes. Queued rather than dropped if the host is currently
        /// checked out mid-`run`/`capture` — see [`TaskShared`]'s doc.
        #[wasm_bindgen(js_name = rehydrateApiKey)]
        pub fn rehydrate_api_key(&self, api_key: String) {
            self.inner.rehydrate_api_key(api_key);
        }

        /// "Forget token" (#106/S8): clears the in-memory credential. Never
        /// touches anything durable — the key was never persisted — and
        /// posts no response; the caller (`task-worker.ts`) fires this and
        /// moves on. Queued rather than dropped if the host is currently
        /// checked out mid-`run`/`capture` — see [`TaskShared`]'s doc.
        #[wasm_bindgen(js_name = clearApiKey)]
        pub fn clear_api_key(&self) {
            self.inner.clear_api_key();
        }

        /// The frontier, as JSON: `{"kind": "ok"|"busy", "items": [Item]}`.
        pub fn frontier(&self) -> String {
            match self.inner.host.borrow().as_ref() {
                Some(host) => {
                    serde_json::to_string(&host.frontier()).expect("ItemListResponse serializes")
                }
                None => BUSY_ITEM_LIST.to_string(),
            }
        }

        /// The triage inbox, as JSON: same shape as [`TaskHost::frontier`].
        #[wasm_bindgen(js_name = triageInbox)]
        pub fn triage_inbox(&self) -> String {
            match self.inner.host.borrow().as_ref() {
                Some(host) => serde_json::to_string(&host.triage_inbox())
                    .expect("ItemListResponse serializes"),
                None => BUSY_ITEM_LIST.to_string(),
            }
        }

        /// Whether `item_id` has an unconfirmed capture overlaid, as JSON:
        /// `{"kind": "ok"|"busy", "pending": bool}`.
        #[wasm_bindgen(js_name = isPending)]
        pub fn is_pending(&self, item_id: String) -> String {
            match self.inner.host.borrow().as_ref() {
                Some(host) => {
                    serde_json::to_string(&host.is_pending(&item_id)).expect("IsPendingResponse serializes")
                }
                None => BUSY_IS_PENDING.to_string(),
            }
        }

        /// Drains every credential-needed event since the last drain, as
        /// JSON: `[{"kind": "credential_needed", "at_ms": number}]`.
        #[wasm_bindgen(js_name = takeEvents)]
        pub fn take_events(&self) -> String {
            let events = match self.inner.host.borrow_mut().as_mut() {
                Some(host) => host.take_events(),
                None => Vec::new(),
            };
            serde_json::to_string(&events).expect("TaskEventDTO serializes")
        }

        /// Captures a new item. Resolves to JSON:
        /// `{"kind": "ok"|"failed"|"busy", "id": string|null, "error": string|null}`.
        pub fn capture(&self, seed: String, title: String, stage: String, now_ms: f64) -> js_sys::Promise {
            let inner = self.inner.clone();
            future_to_promise(async move {
                let Some(mut host) = inner.check_out() else {
                    return Ok(JsValue::from_str(BUSY_CAPTURE));
                };
                let response = host.capture(&seed, &title, &stage, now_ms as i64).await;
                inner.check_in(host);
                Ok(JsValue::from_str(
                    &serde_json::to_string(&response).expect("CaptureResponse serializes"),
                ))
            })
        }

        /// Runs one sync cycle. Resolves to JSON:
        /// `{"kind": string, "retry_after_ms": number|null, "active_item_count": number|null, "was_full_sweep": bool|null, "dead_lettered": number|null}`.
        #[wasm_bindgen(js_name = runSync)]
        pub fn run_sync(
            &self,
            now_ms: f64,
            trigger: String,
            force_full_sweep: bool,
            jitter_unit: f64,
        ) -> js_sys::Promise {
            let inner = self.inner.clone();
            future_to_promise(async move {
                let Some(mut host) = inner.check_out() else {
                    return Ok(JsValue::from_str(BUSY_RUN));
                };
                let response = host
                    .run(now_ms as i64, &trigger, force_full_sweep, jitter_unit)
                    .await;
                inner.check_in(host);
                Ok(JsValue::from_str(
                    &serde_json::to_string(&response).expect("RunResponse serializes"),
                ))
            })
        }

        /// The outbound queue's current depth, as JSON:
        /// `{"kind": "ok"|"busy", "depth": number}`. S9's sync-status
        /// "queued" figure.
        #[wasm_bindgen(js_name = queueDepth)]
        pub fn queue_depth(&self) -> String {
            match self.inner.host.borrow().as_ref() {
                Some(host) => {
                    serde_json::to_string(&host.queue_depth()).expect("QueueDepthResponse serializes")
                }
                None => BUSY_QUEUE_DEPTH.to_string(),
            }
        }

        /// Every dead-lettered entry, as JSON:
        /// `{"kind": "ok"|"busy", "entries": [DeadLetterEntryDTO]}`. S9's "1
        /// edit didn't apply" affordance.
        #[wasm_bindgen(js_name = deadLetters)]
        pub fn dead_letters(&self) -> String {
            match self.inner.host.borrow().as_ref() {
                Some(host) => {
                    serde_json::to_string(&host.dead_letters()).expect("DeadLettersResponse serializes")
                }
                None => BUSY_DEAD_LETTERS.to_string(),
            }
        }

        /// The local mirror, as JSON: `{"kind": "ok"|"busy", "mirror":
        /// object|null}`. S9's mirror download button.
        #[wasm_bindgen(js_name = mirrorSnapshot)]
        pub fn mirror_snapshot(&self) -> String {
            match self.inner.host.borrow().as_ref() {
                Some(host) => serde_json::to_string(&host.mirror_snapshot())
                    .expect("MirrorSnapshotResponse serializes"),
                None => BUSY_MIRROR_SNAPSHOT.to_string(),
            }
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
