//! `hummingbird-ffi-web`: `#[wasm_bindgen]` wrappers over `hummingbird-core`
//! for TypeScript, building for `wasm32-unknown-unknown` (ADR-0003).
//!
//! [`CalendarHostCore`] (issue #73) is the web host's one door into #72's
//! `ContextPoller`/#71's Google adapter: push a token, drive the selected
//! calendars, trigger a poll, and drain credential events. It lives in
//! `hummingbird_core::calendar::host` since #564 gave it a second caller
//! (`ffi-mobile`), and is re-exported here under its old name so this
//! crate's own surface did not change; it is plain, `wasm_bindgen`-free
//! Rust, testable with `cargo test --workspace` on any
//! target; [`CalendarHost`] below is the thin `#[wasm_bindgen]` shim over it
//! that only compiles for `wasm32` — `js_sys`/`wasm-bindgen-futures`'s JS
//! interop has no working implementation to test against outside an actual
//! JS host, so nothing in this crate's native test run exercises it. The
//! `wasm32` build itself is what CI's `cargo build --target
//! wasm32-unknown-unknown -p hummingbird-ffi-web` step gates.

//! [`decisions`] is the exception to all of that: free functions over
//! scalars and JSON, native-tested, and the one part of this crate the web
//! instantiates a **second** time on its main thread (ADR-0025/#141) rather
//! than reaching through the SharedWorker. Its header states what may be
//! added to it without breaking that arrangement.

pub mod decisions;
mod task_host;

pub use hummingbird_core::calendar::{
    CalendarEventsResponse, CalendarListResponse, CALENDAR_POLL_INTERVAL_MS,
};

/// This crate's [`hummingbird_core::calendar::CalendarHostCore`]: one
/// resolved over the store `hummingbird_core` picks for this target
/// (IndexedDB on `wasm32`), so the shim below reads exactly as it did
/// before the move.
pub type CalendarHostCore = hummingbird_core::calendar::CalendarHostCore<hummingbird_core::CoreStore>;
pub use task_host::{
    ActResponse, BlockedEntryDTO, BlockedListResponse, CaptureFields, CaptureResponse, CoreFieldDTO,
    CreateRuleResponse, DeadLetterEntryDTO, DeadLetterFieldDTO, DeadLettersResponse,
    FreshnessResponse, FrontierItemDTO, IsPendingResponse, ItemListResponse, KindRegistryResponse,
    MirrorSnapshotResponse, PaneReadResponse, PatchRuleResponse, ProjectListResponse,
    QueueDepthResponse, RuleListResponse, RunResponse, StepListResponse, TaskEventDTO,
    TaskHostCore, TriageEdits, TriageResponse,
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

    use hummingbird_core::calendar::{outcome_name, CalendarSelection};
    use hummingbird_core::storage::IndexedDbSnapshotStore;

    use super::CalendarHostCore;

    /// Parses the host's selection JSON — `[{"id": "...", "horizon":
    /// "standard"|"long"}]` (#121). Unparseable text is an **empty**
    /// selection rather than a panic: a wasm panic poisons the whole module,
    /// and the host's own protocol test is what pins the shape. An empty
    /// selection is already a legitimate steady state here (nothing picked
    /// yet), and the poller keeps whatever it last held until the next
    /// trigger.
    fn parse_selections(json: &str) -> Vec<CalendarSelection> {
        serde_json::from_str(json).unwrap_or_default()
    }

    /// Everything a synchronous setter has to defer while the core is
    /// checked out for a poll. Applied, in this order, at check-in.
    #[derive(Default)]
    struct Pending {
        token: Option<String>,
        selections: Option<Vec<CalendarSelection>>,
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
            if let Some(selections) = pending.selections {
                core.set_calendar_selections(selections);
            }
            *self.core.borrow_mut() = Some(core);
        }

        fn push_token(&self, token: String) {
            match self.core.borrow_mut().as_mut() {
                Some(core) => core.push_token(token),
                None => self.pending.borrow_mut().token = Some(token),
            }
        }

        fn set_calendar_selections(&self, selections: Vec<CalendarSelection>) {
            match self.core.borrow().as_ref() {
                Some(core) => core.set_calendar_selections(selections),
                None => self.pending.borrow_mut().selections = Some(selections),
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

    /// The "no new information" answer for `listCalendars`: the host
    /// leaves the picker's existing options alone rather than emptying it.
    const BUSY_CALENDAR_LIST: &str = r#"{"kind":"busy","calendars":[]}"#;

    /// The "nothing was attempted" answer for `eventsInInterval` (issue
    /// #267) — the third state alongside `CalendarEventsResponse`'s own
    /// `"not_read"`/`"read"`, added here because only the wasm wrapper can
    /// ever find the core checked out. Never `[]` events read as a real
    /// answer: a busy core has no standing to say "nothing scheduled".
    const BUSY_CALENDAR_EVENTS: &str = r#"{"kind":"busy","events":[],"freshness":null}"#;

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
        /// path/namespace). `selections_json` is the picker's initial
        /// selection as JSON text (`[{"id","horizon"}]`, #121); `"[]"` is a
        /// valid steady state (never-opted-in / nothing picked yet).
        ///
        /// JSON text rather than a `Vec<String>`: a selection now carries a
        /// per-entry horizon, which no positional `wasm_bindgen` argument
        /// shape can express without a second, separately-lengthed array —
        /// the same reasoning `TriageEdits` records for its own one-string
        /// seam.
        #[wasm_bindgen(constructor)]
        pub fn new(namespace: String, selections_json: String) -> CalendarHost {
            CalendarHost {
                inner: Rc::new(Shared::new(CalendarHostCore::new(
                    // The database name is `namespace` itself, unprefixed —
                    // `core.worker.ts`'s `"hummingbird-calendar"`, unchanged
                    // by #564's move of this type into `hummingbird-core`.
                    // Deriving it instead would rename the database and
                    // orphan every already-polled mirror.
                    IndexedDbSnapshotStore::new(namespace),
                    parse_selections(&selections_json),
                ))),
            }
        }

        /// The host calls this at init and on every token rotation (silent
        /// re-mint or a re-connect round-trip).
        #[wasm_bindgen(js_name = pushToken)]
        pub fn push_token(&self, token: String) {
            self.inner.push_token(token);
        }

        /// The calendar picker's current selection, as JSON text (see the
        /// constructor); takes effect on the next poll trigger.
        #[wasm_bindgen(js_name = setCalendarSelections)]
        pub fn set_calendar_selections(&self, selections_json: String) {
            self.inner
                .set_calendar_selections(parse_selections(&selections_json));
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

        /// Issue #267: every non-cancelled event overlapping `[start_ms,
        /// end_ms)`, as JSON: `{"kind": "not_read"|"read"|"busy",
        /// "events": [...], "freshness": null|{"state":...}}`. Same
        /// check-out/check-in dance as the poll triggers — this is a plain
        /// `&self` read on [`CalendarHostCore`], but the underlying snapshot
        /// store load still awaits, so a poll already in flight must not
        /// see a second borrow.
        ///
        /// `start_date`/`end_date` are the same window as `start_ms`/
        /// `end_ms`, in the reader's own civil dates (`YYYY-MM-DD`,
        /// exclusive end) — the arm all-day events are asked about. Both
        /// halves are the caller's: the core owns no tzdb and cannot
        /// derive either from the other.
        #[wasm_bindgen(js_name = eventsInInterval)]
        pub fn events_in_interval(
            &self,
            start_ms: f64,
            end_ms: f64,
            start_date: String,
            end_date: String,
            now_ms: f64,
        ) -> js_sys::Promise {
            let inner = self.inner.clone();
            future_to_promise(async move {
                let Some(core) = inner.check_out() else {
                    return Ok(JsValue::from_str(BUSY_CALENDAR_EVENTS));
                };
                let response = core
                    .events_in_interval(
                        start_ms as i64,
                        end_ms as i64,
                        start_date,
                        end_date,
                        now_ms as i64,
                    )
                    .await;
                inner.check_in(core);
                Ok(JsValue::from_str(
                    &serde_json::to_string(&response).expect("CalendarEventsResponse serializes"),
                ))
            })
        }
    }

    // ------------------------------------------------------------ TaskHost

    use super::task_host::{CaptureFields, CaptureResponse, TaskHostCore, TriageEdits, TriageResponse};

    /// Whatever a synchronous setter had to defer because [`TaskShared`]'s
    /// host was checked out. NOT simple last-wins: `Push` and `Clear` always
    /// supersede whatever is pending, but a queued `Rehydrate` must never
    /// supersede a queued `Push` (round-2 review of #196's PR #202).
    ///
    /// A same-target-object simulation makes the reason concrete. "As if not
    /// busy" — the host applies each call the instant it arrives — Push then
    /// Rehydrate resumes: `push_api_key` sets the key, clears `held`, and
    /// drops the pending prompt; `rehydrate_api_key` merely re-sets the same
    /// key afterwards, leaving the resume intact. Under plain last-wins,
    /// only the queued `Rehydrate` would apply at check-in, and
    /// `rehydrate_api_key` never touches `held` — so the credential would
    /// stay held with no pending prompt to explain why, reachable in
    /// practice through `serial-queue.ts`'s abandon-on-timeout
    /// (`TASK_REQUEST_TIMEOUT_MS`) racing a 401 re-submit against a
    /// still-running cycle. Dropping the queued `Rehydrate` in favour of the
    /// `Push` is safe: `check_in`'s `Push` already sets the (newer, or
    /// identical) key the `Rehydrate` would have re-set.
    enum PendingApiKeyOp {
        Push(String),
        /// Issue #196 (shape 2): the rehydration counterpart to `Push` —
        /// see [`TaskShared::rehydrate_api_key`]. Deliberately the only
        /// variant that does NOT unconditionally supersede whatever is
        /// already queued — see this enum's own doc.
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
        /// other queued op unconditionally, including a queued rehydration —
        /// see [`PendingApiKeyOp`]'s doc for why that is the correct, not
        /// merely convenient, choice.
        fn push_api_key(&self, api_key: String) {
            match self.host.borrow_mut().as_mut() {
                Some(host) => host.push_api_key(api_key),
                None => *self.pending_op.borrow_mut() = Some(PendingApiKeyOp::Push(api_key)),
            }
        }

        /// Issue #196 (shape 2): the rehydration counterpart to
        /// [`TaskShared::push_api_key`] — applies immediately if the host is
        /// present, or queues otherwise, but never resumes a hold either
        /// way. Deliberately does NOT overwrite an already-queued `Push`:
        /// see [`PendingApiKeyOp`]'s doc for the failure this avoids — a
        /// queued resume silently downgraded to a non-resuming rehydration
        /// would leave the credential held with no prompt to explain why.
        /// See [`TaskHostCore::rehydrate_api_key`].
        fn rehydrate_api_key(&self, api_key: String) {
            match self.host.borrow_mut().as_mut() {
                Some(host) => host.rehydrate_api_key(api_key),
                None => {
                    let mut pending = self.pending_op.borrow_mut();
                    if !matches!(*pending, Some(PendingApiKeyOp::Push(_))) {
                        *pending = Some(PendingApiKeyOp::Rehydrate(api_key));
                    }
                }
            }
        }

        /// "Forget token" (#106/S8): clears immediately if the host is
        /// present, or queues for the next [`TaskShared::check_in`]
        /// otherwise. A queued clear supersedes any other queued op
        /// unconditionally, same as a queued push.
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
    const BUSY_BLOCKED_LIST: &str = r#"{"kind":"busy","entries":[]}"#;
    const BUSY_STEP_LIST: &str = r#"{"kind":"busy","steps":[]}"#;
    // Dropped by the host rather than stored, like the pane read: an empty
    // ledger renders as "nothing has ever been tracked" — a claim a core
    // that has not loaded may not make.
    const BUSY_LEDGER_LIST: &str = r#"{"kind":"busy","rows":[]}"#;
    // Same drop-not-store contract as `BUSY_LEDGER_LIST` — Recall's rows
    // *and* its `total` (#478).
    const BUSY_SEARCH: &str = r#"{"kind":"busy","rows":[],"total":0}"#;
    const BUSY_PROJECT_LIST: &str = r#"{"kind":"busy","projects":[]}"#;
    // #624: same three-way shape as BUSY_CREATE_RULE — busy is "no answer",
    // distinct from a create this seam refused.
    const BUSY_CREATE_PROJECT: &str = r#"{"kind":"busy","id":null,"error":null}"#;
    // #625: same shape as BUSY_PATCH_RULE — busy is "no answer", distinct
    // from a patch this seam refused (a malformed github_repo).
    const BUSY_PATCH_PROJECT: &str = r#"{"kind":"busy","error":null}"#;
    // #626: same three-way shape as BUSY_PROJECT_LIST above.
    const BUSY_PROJECT_LINK_LIST: &str = r#"{"kind":"busy","links":[]}"#;
    // #626: same shape as BUSY_CREATE_PROJECT — busy is "no answer", distinct
    // from a create this seam refused (an empty url).
    const BUSY_CREATE_PROJECT_LINK: &str = r#"{"kind":"busy","id":null,"error":null}"#;
    // #626: same shape as BUSY_PATCH_PROJECT.
    const BUSY_PATCH_PROJECT_LINK: &str = r#"{"kind":"busy","error":null}"#;
    // #627: `route: null` here means the same thing it means when the
    // mirror simply hasn't pulled the row yet ([`RouteResponse`]'s own
    // doc) — a busy answer is one more reason this device cannot say.
    const BUSY_ROUTE: &str = r#"{"kind":"busy","route":null}"#;
    // #627: same shape as BUSY_PATCH_PROJECT.
    const BUSY_PATCH_ROUTE: &str = r#"{"kind":"busy","error":null}"#;
    // #628: same three-way shape as BUSY_PROJECT_LINK_LIST above.
    const BUSY_FOG_LIST: &str = r#"{"kind":"busy","fog":[]}"#;
    // #628: same shape as BUSY_CREATE_PROJECT_LINK — busy is "no answer",
    // distinct from a create this seam refused (an empty question).
    const BUSY_CREATE_FOG: &str = r#"{"kind":"busy","id":null,"error":null}"#;
    // #628: same shape as BUSY_PATCH_PROJECT_LINK.
    const BUSY_PATCH_FOG: &str = r#"{"kind":"busy","error":null}"#;
    // #629: [`TaskHostCore::project_actions`] carries the same `ItemListResponse`
    // shape as `frontier`/`triageInbox`, so it reuses BUSY_ITEM_LIST rather
    // than minting a second identical constant.
    // #629: same shape as BUSY_PATCH_PROJECT.
    const BUSY_PATCH_ACTION_POSITION: &str = r#"{"kind":"busy","error":null}"#;
    // #629: same shape as BUSY_CREATE_PROJECT_LINK.
    const BUSY_CREATE_STEP: &str = r#"{"kind":"busy","id":null,"error":null}"#;
    // #629: same shape as BUSY_PATCH_PROJECT_LINK.
    const BUSY_PATCH_STEP: &str = r#"{"kind":"busy","error":null}"#;
    const BUSY_IS_PENDING: &str = r#"{"kind":"busy","pending":false}"#;
    // #118: an empty binding list would read as "nothing is bound", which
    // is an answer — and the wrong one. Busy says nothing at all.
    const BUSY_BINDINGS: &str = r#"{"kind":"busy","bindings":[]}"#;
    const BUSY_SET_BINDING: &str = r#"{"kind":"busy","error":null}"#;
    // #140: same "no answer, never an empty one" contract as BUSY_BINDINGS.
    const BUSY_RULES: &str = r#"{"kind":"busy","rules":[]}"#;
    const BUSY_CREATE_RULE: &str = r#"{"kind":"busy","id":null,"error":null}"#;
    const BUSY_PATCH_RULE: &str = r#"{"kind":"busy","error":null}"#;
    // ADR-0015: a core that has not loaded has measured nothing, so busy is
    // `unknown` — never `{"age_ms":0}`, which would render as fresh.
    const BUSY_FRESHNESS: &str = r#"{"kind":"busy","freshness":{"state":"unknown"}}"#;
    // #245: same reading one step up — an empty pane read is the claim
    // "nothing is due", and busy is no answer rather than that one. The
    // host drops it (`task-worker.ts`'s `mapPaneRead`) instead of storing
    // the empty lists this shape has to carry.
    const BUSY_PANE_READ: &str = r#"{"kind":"busy","snapshots":[],"alerts":[]}"#;
    const BUSY_CAPTURE: &str = r#"{"kind":"busy","id":null,"error":null}"#;
    const BUSY_ACT: &str = r#"{"kind":"busy","error":null}"#;
    const BUSY_TRIAGE: &str = r#"{"kind":"busy","error":null}"#;
    const BUSY_COMPLETE_GRILL: &str = r#"{"kind":"busy","id":null,"error":null}"#;
    const BUSY_SAVE_GRILL_DRAFT: &str = r#"{"kind":"busy","error":null}"#;
    const BUSY_DISCARD_GRILL_DRAFT: &str = r#"{"kind":"busy","error":null}"#;
    // #356: no answer, not an empty one — an item with no draft and an item
    // this core has not loaded yet must not read the same, same "no answer,
    // not an empty answer" contract BUSY_BINDINGS documents.
    const BUSY_GRILL_DRAFT: &str = r#"{"kind":"busy","exists":false,"turns":null}"#;
    const BUSY_GRILL_DRAFT_ITEM_IDS: &str = r#"{"kind":"busy","item_ids":[]}"#;
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

        /// The frontier, as JSON: `{"kind": "ok"|"busy", "items": [Item & {"pending": bool}]}`
        /// — each item's own fields flattened alongside `pending` (issue
        /// #108's "a pending item is marked as such").
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

        /// Items already grilled once and still foggy, as JSON: same shape
        /// as [`TaskHost::frontier`] (#357).
        #[wasm_bindgen(js_name = grillingItems)]
        pub fn grilling_items(&self) -> String {
            match self.inner.host.borrow().as_ref() {
                Some(host) => serde_json::to_string(&host.grilling_items())
                    .expect("ItemListResponse serializes"),
                None => BUSY_ITEM_LIST.to_string(),
            }
        }

        /// Items on an external wait (`Stage::Blocked`), as JSON: same
        /// shape as [`TaskHost::frontier`]. Pane inputs only — no screen
        /// lists these (#675).
        #[wasm_bindgen(js_name = externallyBlocked)]
        pub fn externally_blocked(&self) -> String {
            match self.inner.host.borrow().as_ref() {
                Some(host) => serde_json::to_string(&host.externally_blocked())
                    .expect("ItemListResponse serializes"),
                None => BUSY_ITEM_LIST.to_string(),
            }
        }

        /// Relation-blocked items with the reason visible, as JSON:
        /// `{"kind": "ok"|"busy", "entries": [{"item": Item & {"pending": bool}, "blocked_by": [Item & {"pending": bool}]}]}`.
        pub fn blocked(&self) -> String {
            match self.inner.host.borrow().as_ref() {
                Some(host) => {
                    serde_json::to_string(&host.blocked()).expect("BlockedListResponse serializes")
                }
                None => BUSY_BLOCKED_LIST.to_string(),
            }
        }

        /// One item's Steps, as JSON: `{"kind": "ok"|"busy", "steps": [Step]}`.
        pub fn steps(&self, item_id: String) -> String {
            match self.inner.host.borrow().as_ref() {
                Some(host) => {
                    serde_json::to_string(&host.steps(&item_id)).expect("StepListResponse serializes")
                }
                None => BUSY_STEP_LIST.to_string(),
            }
        }

        /// Every live project, as JSON: `{"kind": "ok"|"busy", "projects": [Project]}`
        /// — resolves the frontier's "grouped by project" display to real
        /// names (issue #108, PR #200 review).
        pub fn projects(&self) -> String {
            match self.inner.host.borrow().as_ref() {
                Some(host) => {
                    serde_json::to_string(&host.projects()).expect("ProjectListResponse serializes")
                }
                None => BUSY_PROJECT_LIST.to_string(),
            }
        }

        /// Creates a project (#624). Resolves to JSON:
        /// `{"kind": "ok"|"failed"|"busy", "id": string|null, "error": string|null}`.
        /// The name is trimmed and an empty one refused before `Core` is
        /// reached ([`TaskHostCore::create_project`]). `"ok"` means
        /// *enqueued*, not *saved* — no optimistic overlay, so `projects()`
        /// keeps answering the old list until a cycle completes.
        #[wasm_bindgen(js_name = createProject)]
        pub fn create_project(&self, seed: String, name: String, now_ms: f64) -> js_sys::Promise {
            let inner = self.inner.clone();
            future_to_promise(async move {
                let Some(mut host) = inner.check_out() else {
                    return Ok(JsValue::from_str(BUSY_CREATE_PROJECT));
                };
                let response = host.create_project(&seed, &name, now_ms as i64).await;
                inner.check_in(host);
                Ok(JsValue::from_str(
                    &serde_json::to_string(&response).expect("CreateProjectResponse serializes"),
                ))
            })
        }

        /// Patches a project (#625) — the dossier's properties card sets and
        /// clears `github_repo`/`default_context`, alongside renaming and
        /// archiving, through this one entry point. Resolves to JSON:
        /// `{"kind": "ok"|"failed"|"busy", "error": string|null}`.
        /// `current_json` is the caller's own last-known [`Project`] (from
        /// [`TaskHost::projects`]), as JSON — the `base` a 409's rebase diffs
        /// against. `github_repo_touched`/`default_context_touched`/
        /// `archived_at_touched` each distinguish "leave this field alone"
        /// (`false`) from "set it, possibly to `null`" (`true`, with the
        /// paired value carrying the new value or `None`) — the same
        /// double-`Option` [`hummingbird_domain::ProjectPatch`] itself
        /// carries, flattened for the wasm boundary exactly like
        /// [`TaskHost::patch_rule`]'s `event_kind_touched`. A malformed
        /// `github_repo` is refused before `Core` is reached
        /// ([`TaskHostCore::patch_project`]).
        #[wasm_bindgen(js_name = patchProject)]
        #[allow(clippy::too_many_arguments)]
        pub fn patch_project(
            &self,
            seed: String,
            current_json: String,
            name: Option<String>,
            github_repo_touched: bool,
            github_repo: Option<String>,
            default_context_touched: bool,
            default_context: Option<String>,
            archived_at_touched: bool,
            archived_at: Option<f64>,
            now_ms: f64,
        ) -> js_sys::Promise {
            let inner = self.inner.clone();
            future_to_promise(async move {
                let current: hummingbird_domain::Project = match serde_json::from_str(&current_json) {
                    Ok(project) => project,
                    Err(error) => {
                        return Ok(JsValue::from_str(&format!(
                            r#"{{"kind":"failed","error":"malformed project: {error}"}}"#
                        )))
                    }
                };
                let Some(mut host) = inner.check_out() else {
                    return Ok(JsValue::from_str(BUSY_PATCH_PROJECT));
                };
                let response = host
                    .patch_project(
                        &seed,
                        &current,
                        name,
                        github_repo_touched,
                        github_repo,
                        default_context_touched,
                        default_context,
                        archived_at_touched,
                        archived_at.map(|v| v as i64),
                        now_ms as i64,
                    )
                    .await;
                inner.check_in(host);
                Ok(JsValue::from_str(
                    &serde_json::to_string(&response).expect("PatchProjectResponse serializes"),
                ))
            })
        }

        /// Every live Link on one project, as JSON: `{"kind": "ok"|"busy",
        /// "links": [ProjectLink]}` — the dossier aside's read (#626,
        /// ADR-0030 decision 4).
        #[wasm_bindgen(js_name = projectLinks)]
        pub fn project_links(&self, project_id: String) -> String {
            match self.inner.host.borrow().as_ref() {
                Some(host) => serde_json::to_string(&host.project_links(&project_id))
                    .expect("ProjectLinkListResponse serializes"),
                None => BUSY_PROJECT_LINK_LIST.to_string(),
            }
        }

        /// Creates a project Link (#626). Resolves to JSON:
        /// `{"kind": "ok"|"failed"|"busy", "id": string|null, "error": string|null}`.
        /// The url is trimmed and an empty one refused before `Core` is
        /// reached ([`TaskHostCore::create_project_link`]). `"ok"` means
        /// *enqueued*, not *saved* — no optimistic overlay, so
        /// `projectLinks()` keeps answering the old list until a cycle
        /// completes.
        #[wasm_bindgen(js_name = createProjectLink)]
        #[allow(clippy::too_many_arguments)]
        pub fn create_project_link(
            &self,
            seed: String,
            project_id: String,
            url: String,
            label: Option<String>,
            position: f64,
            now_ms: f64,
        ) -> js_sys::Promise {
            let inner = self.inner.clone();
            future_to_promise(async move {
                let Some(mut host) = inner.check_out() else {
                    return Ok(JsValue::from_str(BUSY_CREATE_PROJECT_LINK));
                };
                let response = host
                    .create_project_link(&seed, &project_id, &url, label, position as i64, now_ms as i64)
                    .await;
                inner.check_in(host);
                Ok(JsValue::from_str(
                    &serde_json::to_string(&response).expect("CreateProjectLinkResponse serializes"),
                ))
            })
        }

        /// Patches a project Link (#626) — editing its url/label,
        /// reordering it, or flagging/clearing its removal, all through
        /// this one entry point. Resolves to JSON:
        /// `{"kind": "ok"|"failed"|"busy", "error": string|null}`.
        /// `current_json` is the caller's own last-known [`ProjectLink`]
        /// (from [`TaskHost::projectLinks`]), as JSON — the `base` a 409's
        /// rebase diffs against. `label_touched`/`removed_at_touched` each
        /// distinguish "leave this field alone" (`false`) from "set it,
        /// possibly to `null`" (`true`, with the paired value carrying the
        /// new value or `None`) — the same double-`Option`
        /// [`hummingbird_domain::ProjectLinkPatch`] itself carries,
        /// flattened for the wasm boundary exactly like
        /// [`TaskHost::patchProject`]'s touched-flag shape.
        #[wasm_bindgen(js_name = patchProjectLink)]
        #[allow(clippy::too_many_arguments)]
        pub fn patch_project_link(
            &self,
            seed: String,
            current_json: String,
            url: Option<String>,
            label_touched: bool,
            label: Option<String>,
            position: Option<f64>,
            removed_at_touched: bool,
            removed_at: Option<f64>,
            now_ms: f64,
        ) -> js_sys::Promise {
            let inner = self.inner.clone();
            future_to_promise(async move {
                let current: hummingbird_domain::ProjectLink = match serde_json::from_str(&current_json) {
                    Ok(link) => link,
                    Err(error) => {
                        return Ok(JsValue::from_str(&format!(
                            r#"{{"kind":"failed","error":"malformed project link: {error}"}}"#
                        )))
                    }
                };
                let Some(mut host) = inner.check_out() else {
                    return Ok(JsValue::from_str(BUSY_PATCH_PROJECT_LINK));
                };
                let response = host
                    .patch_project_link(
                        &seed,
                        &current,
                        url,
                        label_touched,
                        label,
                        position.map(|v| v as i64),
                        removed_at_touched,
                        removed_at.map(|v| v as i64),
                        now_ms as i64,
                    )
                    .await;
                inner.check_in(host);
                Ok(JsValue::from_str(
                    &serde_json::to_string(&response).expect("PatchProjectLinkResponse serializes"),
                ))
            })
        }

        /// One project's Route, as JSON: `{"kind": "ok"|"busy", "route":
        /// Route|null}` — the dossier's reading column's read (#627,
        /// ADR-0030 decision 1). `route: null` means "not read yet" both
        /// when `"ok"` and when `"busy"`: every project has exactly one
        /// Route, created structurally by [`TaskHost::createProject`].
        pub fn route(&self, project_id: String) -> String {
            match self.inner.host.borrow().as_ref() {
                Some(host) => {
                    serde_json::to_string(&host.route(&project_id)).expect("RouteResponse serializes")
                }
                None => BUSY_ROUTE.to_string(),
            }
        }

        /// Patches a project's Route (#627, ADR-0030 decision 1) — the
        /// dossier's reading column, editing `destination`/`notes` through
        /// this one entry point. Resolves to JSON: `{"kind":
        /// "ok"|"failed"|"busy", "error": string|null}`. `current_json` is
        /// the caller's own last-known [`hummingbird_domain::Route`] (from
        /// [`TaskHost::route`]), as JSON — the `base` a 409's rebase diffs
        /// against. `destination_touched`/`notes_touched` each distinguish
        /// "leave this field alone" (`false`) from "set it, possibly to
        /// `null`" (`true`, with the paired value carrying the new value or
        /// `None`) — the same double-`Option`
        /// [`hummingbird_domain::RoutePatch`] itself carries, flattened for
        /// the wasm boundary exactly like [`TaskHost::patchProject`]'s
        /// touched-flag shape. A 409 here is an ordinary outcome (ADR-0030
        /// decision 1: the route's content is shared-owned with
        /// `/to-actions`), handled by the same rebase-and-retry machinery
        /// and dead-letter journal every other CAS write here uses — this
        /// seam adds no bespoke conflict surface.
        #[wasm_bindgen(js_name = patchRoute)]
        #[allow(clippy::too_many_arguments)]
        pub fn patch_route(
            &self,
            seed: String,
            current_json: String,
            destination_touched: bool,
            destination: Option<String>,
            notes_touched: bool,
            notes: Option<String>,
            now_ms: f64,
        ) -> js_sys::Promise {
            let inner = self.inner.clone();
            future_to_promise(async move {
                let current: hummingbird_domain::Route = match serde_json::from_str(&current_json) {
                    Ok(route) => route,
                    Err(error) => {
                        return Ok(JsValue::from_str(&format!(
                            r#"{{"kind":"failed","error":"malformed route: {error}"}}"#
                        )))
                    }
                };
                let Some(mut host) = inner.check_out() else {
                    return Ok(JsValue::from_str(BUSY_PATCH_ROUTE));
                };
                let response = host
                    .patch_route(
                        &seed,
                        &current,
                        destination_touched,
                        destination,
                        notes_touched,
                        notes,
                        now_ms as i64,
                    )
                    .await;
                inner.check_in(host);
                Ok(JsValue::from_str(
                    &serde_json::to_string(&response).expect("PatchRouteResponse serializes"),
                ))
            })
        }

        /// Every open Fog segment on one project, as JSON: `{"kind":
        /// "ok"|"busy", "fog": [Fog]}` — the dossier reading column's read
        /// (#628, ADR-0030 decision 1). A resolved row is retained but
        /// never appears here ([`Core::open_fog_for`]'s own doc).
        ///
        /// **No web caller since 2026-08-21**: the project dossier's centre
        /// column became the frontier board, and the fog card, ordered
        /// action list and inline Steps checklist that called into here went
        /// with it (ADR-0030's Consequences amendment). The seam is kept —
        /// the records are still shared-owned and still written by
        /// `/to-actions` — but nothing in `client/web` reaches it today.
        #[wasm_bindgen(js_name = openFog)]
        pub fn open_fog(&self, project_id: String) -> String {
            match self.inner.host.borrow().as_ref() {
                Some(host) => {
                    serde_json::to_string(&host.open_fog(&project_id)).expect("FogListResponse serializes")
                }
                None => BUSY_FOG_LIST.to_string(),
            }
        }

        /// Creates a Fog segment (#628). Resolves to JSON: `{"kind":
        /// "ok"|"failed"|"busy", "id": string|null, "error": string|null}`.
        /// The question is trimmed and an empty one refused before `Core`
        /// is reached ([`TaskHostCore::create_fog`]). `"ok"` means
        /// *enqueued*, not *saved* — no optimistic overlay, so `openFog()`
        /// keeps answering the old list until a cycle completes.
        ///
        /// **No web caller since 2026-08-21**: the project dossier's centre
        /// column became the frontier board, and the fog card, ordered
        /// action list and inline Steps checklist that called into here went
        /// with it (ADR-0030's Consequences amendment). The seam is kept —
        /// the records are still shared-owned and still written by
        /// `/to-actions` — but nothing in `client/web` reaches it today.
        #[wasm_bindgen(js_name = createFog)]
        pub fn create_fog(
            &self,
            seed: String,
            project_id: String,
            question: String,
            position: f64,
            now_ms: f64,
        ) -> js_sys::Promise {
            let inner = self.inner.clone();
            future_to_promise(async move {
                let Some(mut host) = inner.check_out() else {
                    return Ok(JsValue::from_str(BUSY_CREATE_FOG));
                };
                let response = host
                    .create_fog(&seed, &project_id, &question, position as i64, now_ms as i64)
                    .await;
                inner.check_in(host);
                Ok(JsValue::from_str(
                    &serde_json::to_string(&response).expect("CreateFogResponse serializes"),
                ))
            })
        }

        /// Patches a Fog segment (#628) — rewording its question,
        /// repositioning it, or resolving/reopening it, all through this
        /// one entry point. Resolves to JSON: `{"kind":
        /// "ok"|"failed"|"busy", "error": string|null}`. `current_json` is
        /// the caller's own last-known [`hummingbird_domain::Fog`] (from
        /// [`TaskHost::openFog`]), as JSON — the `base` a 409's rebase
        /// diffs against. `resolved_at_touched` distinguishes "leave this
        /// field alone" (`false`) from "set it, possibly to `null`"
        /// (`true`, with the paired value carrying the new value or
        /// `None`) — the same double-`Option`
        /// [`hummingbird_domain::FogPatch`] itself carries, flattened for
        /// the wasm boundary exactly like [`TaskHost::patchProjectLink`]'s
        /// `removed_at_touched`.
        ///
        /// **No web caller since 2026-08-21**: the project dossier's centre
        /// column became the frontier board, and the fog card, ordered
        /// action list and inline Steps checklist that called into here went
        /// with it (ADR-0030's Consequences amendment). The seam is kept —
        /// the records are still shared-owned and still written by
        /// `/to-actions` — but nothing in `client/web` reaches it today.
        #[wasm_bindgen(js_name = patchFog)]
        #[allow(clippy::too_many_arguments)]
        pub fn patch_fog(
            &self,
            seed: String,
            current_json: String,
            question: Option<String>,
            position: Option<f64>,
            resolved_at_touched: bool,
            resolved_at: Option<f64>,
            now_ms: f64,
        ) -> js_sys::Promise {
            let inner = self.inner.clone();
            future_to_promise(async move {
                let current: hummingbird_domain::Fog = match serde_json::from_str(&current_json) {
                    Ok(fog) => fog,
                    Err(error) => {
                        return Ok(JsValue::from_str(&format!(
                            r#"{{"kind":"failed","error":"malformed fog: {error}"}}"#
                        )))
                    }
                };
                let Some(mut host) = inner.check_out() else {
                    return Ok(JsValue::from_str(BUSY_PATCH_FOG));
                };
                let response = host
                    .patch_fog(
                        &seed,
                        &current,
                        question,
                        position.map(|v| v as i64),
                        resolved_at_touched,
                        resolved_at.map(|v| v as i64),
                        now_ms as i64,
                    )
                    .await;
                inner.check_in(host);
                Ok(JsValue::from_str(
                    &serde_json::to_string(&response).expect("PatchFogResponse serializes"),
                ))
            })
        }

        /// Every live Action on a project, as JSON: `{"kind": "ok"|"busy",
        /// "items": [Item & {"pending": bool}]}` — the dossier's ordered
        /// action list (#629). Same shape as [`TaskHost::frontier`]: an
        /// Action is an ordinary item, so it carries the same pending flag.
        ///
        /// **No web caller since 2026-08-21**: the project dossier's centre
        /// column became the frontier board, and the fog card, ordered
        /// action list and inline Steps checklist that called into here went
        /// with it (ADR-0030's Consequences amendment). The seam is kept —
        /// the records are still shared-owned and still written by
        /// `/to-actions` — but nothing in `client/web` reaches it today.
        #[wasm_bindgen(js_name = projectActions)]
        pub fn project_actions(&self, project_id: String) -> String {
            match self.inner.host.borrow().as_ref() {
                Some(host) => serde_json::to_string(&host.project_actions(&project_id))
                    .expect("ItemListResponse serializes"),
                None => BUSY_ITEM_LIST.to_string(),
            }
        }

        /// Moves one Action's `project_pos` (#629) — the dossier's reorder
        /// control. Resolves to JSON: `{"kind": "ok"|"failed"|"busy",
        /// "error": string|null}`. `current_json` is the caller's own
        /// last-known [`hummingbird_domain::Item`] (from
        /// [`TaskHost::projectActions`]), as JSON — the `base` a 409's
        /// rebase diffs against. A 409 here is an ordinary outcome
        /// (ADR-0030 decision 1: `project_pos` is shared-owned with
        /// `/to-actions`), handled by the same rebase-and-retry machinery
        /// and dead-letter journal every other CAS write here uses.
        ///
        /// **No web caller since 2026-08-21**: the project dossier's centre
        /// column became the frontier board, and the fog card, ordered
        /// action list and inline Steps checklist that called into here went
        /// with it (ADR-0030's Consequences amendment). The seam is kept —
        /// the records are still shared-owned and still written by
        /// `/to-actions` — but nothing in `client/web` reaches it today.
        #[wasm_bindgen(js_name = patchActionPosition)]
        pub fn patch_action_position(
            &self,
            seed: String,
            current_json: String,
            position: f64,
            now_ms: f64,
        ) -> js_sys::Promise {
            let inner = self.inner.clone();
            future_to_promise(async move {
                let current: hummingbird_domain::Item = match serde_json::from_str(&current_json) {
                    Ok(item) => item,
                    Err(error) => {
                        return Ok(JsValue::from_str(&format!(
                            r#"{{"kind":"failed","error":"malformed item: {error}"}}"#
                        )))
                    }
                };
                let Some(mut host) = inner.check_out() else {
                    return Ok(JsValue::from_str(BUSY_PATCH_ACTION_POSITION));
                };
                let response = host
                    .patch_action_position(&seed, &current, position as i64, now_ms as i64)
                    .await;
                inner.check_in(host);
                Ok(JsValue::from_str(
                    &serde_json::to_string(&response).expect("PatchActionPositionResponse serializes"),
                ))
            })
        }

        /// Creates a Step on an item's checklist (#629). Resolves to JSON:
        /// `{"kind": "ok"|"failed"|"busy", "id": string|null,
        /// "error": string|null}`. The body is trimmed and an empty one
        /// refused before `Core` is reached
        /// ([`TaskHostCore::create_step`]). `"ok"` means *enqueued*, not
        /// *saved* — no optimistic overlay, so `steps()` keeps answering
        /// the old checklist until a cycle completes.
        ///
        /// **No web caller since 2026-08-21**: the project dossier's centre
        /// column became the frontier board, and the fog card, ordered
        /// action list and inline Steps checklist that called into here went
        /// with it (ADR-0030's Consequences amendment). The seam is kept —
        /// the records are still shared-owned and still written by
        /// `/to-actions` — but nothing in `client/web` reaches it today.
        #[wasm_bindgen(js_name = createStep)]
        pub fn create_step(
            &self,
            seed: String,
            item_id: String,
            body: String,
            position: f64,
            now_ms: f64,
        ) -> js_sys::Promise {
            let inner = self.inner.clone();
            future_to_promise(async move {
                let Some(mut host) = inner.check_out() else {
                    return Ok(JsValue::from_str(BUSY_CREATE_STEP));
                };
                let response = host.create_step(&seed, &item_id, &body, position as i64, now_ms as i64).await;
                inner.check_in(host);
                Ok(JsValue::from_str(
                    &serde_json::to_string(&response).expect("CreateStepResponse serializes"),
                ))
            })
        }

        /// Patches a Step (#629) — ticking, rewording, repositioning, or
        /// flagging/clearing its deletion, all through this one entry
        /// point. Resolves to JSON: `{"kind": "ok"|"failed"|"busy",
        /// "error": string|null}`. `current_json` is the caller's own
        /// last-known [`hummingbird_domain::Step`] (from
        /// [`TaskHost::steps`]), as JSON — the `base` a 409's rebase diffs
        /// against. `deleted_at_touched` distinguishes "leave this field
        /// alone" (`false`) from "set it, possibly to `null`" (`true`,
        /// with the paired value carrying the new value or `None`) — the
        /// same double-`Option` [`hummingbird_domain::StepPatch::deleted_at`]
        /// itself carries, flattened for the wasm boundary exactly like
        /// [`TaskHost::patchFog`]'s `resolved_at_touched`.
        ///
        /// **No web caller since 2026-08-21**: the project dossier's centre
        /// column became the frontier board, and the fog card, ordered
        /// action list and inline Steps checklist that called into here went
        /// with it (ADR-0030's Consequences amendment). The seam is kept —
        /// the records are still shared-owned and still written by
        /// `/to-actions` — but nothing in `client/web` reaches it today.
        #[wasm_bindgen(js_name = patchStep)]
        #[allow(clippy::too_many_arguments)]
        pub fn patch_step(
            &self,
            seed: String,
            current_json: String,
            body: Option<String>,
            done: Option<bool>,
            position: Option<f64>,
            deleted_at_touched: bool,
            deleted_at: Option<f64>,
            now_ms: f64,
        ) -> js_sys::Promise {
            let inner = self.inner.clone();
            future_to_promise(async move {
                let current: hummingbird_domain::Step = match serde_json::from_str(&current_json) {
                    Ok(step) => step,
                    Err(error) => {
                        return Ok(JsValue::from_str(&format!(
                            r#"{{"kind":"failed","error":"malformed step: {error}"}}"#
                        )))
                    }
                };
                let Some(mut host) = inner.check_out() else {
                    return Ok(JsValue::from_str(BUSY_PATCH_STEP));
                };
                let response = host
                    .patch_step(
                        &seed,
                        &current,
                        body,
                        done,
                        position.map(|v| v as i64),
                        deleted_at_touched,
                        deleted_at.map(|v| v as i64),
                        now_ms as i64,
                    )
                    .await;
                inner.check_in(host);
                Ok(JsValue::from_str(
                    &serde_json::to_string(&response).expect("PatchStepResponse serializes"),
                ))
            })
        }

        /// The complete retained roster (the Ledger screen's read), as JSON:
        /// `{"kind": "ok"|"busy", "rows": [Item & {"pending": bool,
        /// "absent_since_ms": number|null, "dead_lettered": bool,
        /// "has_live_alert": bool}]}` — every item this device's mirror has
        /// ever known, archived rows included and labelled. `busy` carries an
        /// empty list because the shape demands one, and the host drops the
        /// whole answer on it rather than storing it (see
        /// `BUSY_LEDGER_LIST`). `now_ms` is host-supplied and resolves alert
        /// liveness.
        pub fn ledger(&self, now_ms: f64) -> String {
            match self.inner.host.borrow().as_ref() {
                Some(host) => serde_json::to_string(&host.ledger(now_ms as i64))
                    .expect("LedgerListResponse serializes"),
                None => BUSY_LEDGER_LIST.to_string(),
            }
        }

        /// **Recall** (#478): re-find one item across the whole retained
        /// roster by remembered words or by handle, as JSON: `{"kind":
        /// "ok"|"busy", "rows": [Item & {"pending": bool, "group":
        /// "live"|"done"|"archived"}], "total": number}`. `busy` carries an
        /// empty list and a zero `total` because the shape demands both,
        /// and the host drops the whole answer on it rather than storing
        /// it, same contract as
        /// [`TaskHost::ledger`]. `now_ms` is host-supplied and resolves the
        /// same alert-liveness read `ledger` does (`search` shares its
        /// corpus with `ledger`).
        pub fn search(&self, query: String, now_ms: f64) -> String {
            match self.inner.host.borrow().as_ref() {
                Some(host) => serde_json::to_string(&host.search(&query, now_ms as i64))
                    .expect("SearchResponse serializes"),
                None => BUSY_SEARCH.to_string(),
            }
        }

        /// Every live `Done` item (the Done screen's read), as JSON: same
        /// shape as [`TaskHost::frontier`].
        pub fn done(&self) -> String {
            match self.inner.host.borrow().as_ref() {
                Some(host) => {
                    serde_json::to_string(&host.done()).expect("ItemListResponse serializes")
                }
                None => BUSY_ITEM_LIST.to_string(),
            }
        }

        /// How old this device's answer to one standing question is
        /// (ADR-0015), as JSON: `{"kind": "ok"|"busy", "freshness":
        /// {"state":"unknown"} | {"state":"age","age_ms":number,
        /// "declared_cadence_ms":number|null}}`. `now_ms` is host-supplied.
        #[wasm_bindgen(js_name = snapshotFreshness)]
        pub fn snapshot_freshness(&self, source: String, key: String, now_ms: f64) -> String {
            match self.inner.host.borrow().as_ref() {
                Some(host) => {
                    serde_json::to_string(&host.snapshot_freshness(&source, &key, now_ms as i64))
                        .expect("FreshnessResponse serializes")
                }
                None => BUSY_FRESHNESS.to_string(),
            }
        }

        /// One source's whole pane-facing read (#245, ADR-0015), as JSON:
        /// `{"kind": "ok"|"busy", "snapshots": [{"source": string, "key":
        /// string, "fetched_at": number, "version": number, "freshness":
        /// …, "envelope": {"state":"parsed","schema":string,
        /// "polled_every_ms":number|null,"body":string} |
        /// {"state":"malformed","reason":string}}], "alerts": [Alert]}`.
        ///
        /// `busy` carries empty lists because the shape demands lists, and
        /// the host **drops** the whole answer on it rather than storing
        /// them: an empty pane read renders as "nothing is due", which a
        /// core that has not loaded has no standing to claim. `now_ms` is
        /// host-supplied, and decides both the ages and which alerts are
        /// live.
        #[wasm_bindgen(js_name = paneRead)]
        pub fn pane_read(&self, source: String, now_ms: f64) -> String {
            match self.inner.host.borrow().as_ref() {
                Some(host) => serde_json::to_string(&host.pane_read(&source, now_ms as i64))
                    .expect("PaneReadResponse serializes"),
                None => BUSY_PANE_READ.to_string(),
            }
        }

        /// Every standing-question binding (#118), as JSON:
        /// `{"kind": "ok"|"busy", "bindings": [{"key": string, "known":
        /// bool, "pending": bool, "value": {"state":"unset"} |
        /// {"state":"text","text":string} | {"state":"other","raw":string}}]}`.
        pub fn bindings(&self) -> String {
            match self.inner.host.borrow().as_ref() {
                Some(host) => serde_json::to_string(&host.bindings())
                    .expect("BindingListResponse serializes"),
                None => BUSY_BINDINGS.to_string(),
            }
        }

        /// Sets one binding (#118), as one absolute-value CAS `PUT`.
        /// Resolves to JSON:
        /// `{"kind": "ok"|"unknown_key"|"failed"|"busy", "error": string|null}`.
        /// `key` is the kebab-case binding name, resolved by name — never a
        /// raw `settings` key a caller invented.
        #[wasm_bindgen(js_name = setBinding)]
        pub fn set_binding(
            &self,
            seed: String,
            key: String,
            value: String,
            now_ms: f64,
        ) -> js_sys::Promise {
            let inner = self.inner.clone();
            future_to_promise(async move {
                let Some(mut host) = inner.check_out() else {
                    return Ok(JsValue::from_str(BUSY_SET_BINDING));
                };
                let response = host.set_binding(&seed, &key, &value, now_ms as i64).await;
                inner.check_in(host);
                Ok(JsValue::from_str(
                    &serde_json::to_string(&response).expect("SetBindingResponse serializes"),
                ))
            })
        }

        /// Every rule (#140), as JSON: `{"kind": "ok"|"busy", "rules": [Rule]}`.
        pub fn rules(&self) -> String {
            match self.inner.host.borrow().as_ref() {
                Some(host) => serde_json::to_string(&host.rules()).expect("RuleListResponse serializes"),
                None => BUSY_RULES.to_string(),
            }
        }

        /// The kind registry export (#133/#140, ADR-0013), as JSON:
        /// `{"kind": "ok", "kinds": [EventKindEntry], "core_fields":
        /// [{"name": string, "field_type": string}], "alarm_interval_ms":
        /// number, "severities": string[]}`. Never `"busy"` —
        /// [`TaskHostCore::kind_registry`] needs no checked-out `Core` state
        /// at all, only static domain data.
        #[wasm_bindgen(js_name = kindRegistry)]
        pub fn kind_registry(&self) -> String {
            serde_json::to_string(&TaskHostCore::kind_registry())
                .expect("KindRegistryResponse serializes")
        }

        /// Creates a rule (#140). Resolves to JSON:
        /// `{"kind": "ok"|"failed"|"busy", "id": string|null, "error": string|null}`.
        /// `tier` is the wire's snake_case name (`"urgent"`/`"normal"`),
        /// resolved before it can reach `Core`. `conditions_json` is
        /// `Vec<Condition>`'s own JSON array — an open-ended list has no
        /// scalar-argument shape `wasm_bindgen` can carry.
        #[wasm_bindgen(js_name = createRule)]
        #[allow(clippy::too_many_arguments)]
        pub fn create_rule(
            &self,
            seed: String,
            name: String,
            event_kind: Option<String>,
            conditions_json: String,
            severity: String,
            tier: String,
            enabled: bool,
            now_ms: f64,
        ) -> js_sys::Promise {
            let inner = self.inner.clone();
            future_to_promise(async move {
                let Some(mut host) = inner.check_out() else {
                    return Ok(JsValue::from_str(BUSY_CREATE_RULE));
                };
                let response = host
                    .create_rule(
                        &seed,
                        &name,
                        event_kind,
                        &conditions_json,
                        &severity,
                        &tier,
                        enabled,
                        now_ms as i64,
                    )
                    .await;
                inner.check_in(host);
                Ok(JsValue::from_str(
                    &serde_json::to_string(&response).expect("CreateRuleResponse serializes"),
                ))
            })
        }

        /// Patches a rule (#140) — the enable/disable toggle and every other
        /// rule edit share this one entry point. Resolves to JSON:
        /// `{"kind": "ok"|"failed"|"busy", "error": string|null}`.
        /// `current_json` is the caller's own last-known [`Rule`] (from
        /// [`TaskHost::rules`]), as JSON — the `base` a 409's rebase diffs
        /// against. `event_kind_touched` distinguishes "leave `event_kind`
        /// alone" (`false`) from "set it, possibly to `null` for any kind"
        /// (`true`, with `event_kind` carrying the new value or `None`) —
        /// the same double-`Option` [`hummingbird_domain::RulePatch`]
        /// itself carries, flattened for the wasm boundary.
        /// `deleted_at_touched`/`deleted_at` is the same pair for the rule's
        /// soft-delete flag — **deleting a rule is this call**, not a route
        /// of its own, and `deleted_at_touched: true` with `deleted_at:
        /// None` is the explicit `null` that un-deletes.
        #[wasm_bindgen(js_name = patchRule)]
        #[allow(clippy::too_many_arguments)]
        pub fn patch_rule(
            &self,
            seed: String,
            current_json: String,
            name: Option<String>,
            event_kind_touched: bool,
            event_kind: Option<String>,
            conditions_json: Option<String>,
            severity: Option<String>,
            tier: Option<String>,
            enabled: Option<bool>,
            deleted_at_touched: bool,
            deleted_at: Option<f64>,
            now_ms: f64,
        ) -> js_sys::Promise {
            let inner = self.inner.clone();
            future_to_promise(async move {
                let current: hummingbird_domain::Rule = match serde_json::from_str(&current_json) {
                    Ok(rule) => rule,
                    Err(error) => {
                        return Ok(JsValue::from_str(&format!(
                            r#"{{"kind":"failed","error":"malformed rule: {error}"}}"#
                        )))
                    }
                };
                let Some(mut host) = inner.check_out() else {
                    return Ok(JsValue::from_str(BUSY_PATCH_RULE));
                };
                let response = host
                    .patch_rule(
                        &seed,
                        &current,
                        name,
                        event_kind_touched,
                        event_kind,
                        conditions_json,
                        severity,
                        tier,
                        enabled,
                        deleted_at_touched,
                        deleted_at.map(|ms| ms as i64),
                        now_ms as i64,
                    )
                    .await;
                inner.check_in(host);
                Ok(JsValue::from_str(
                    &serde_json::to_string(&response).expect("PatchRuleResponse serializes"),
                ))
            })
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
        /// `fields` is one JSON object ([`CaptureFields`]) rather than the
        /// positional scalars this took while there were three of them — the
        /// same call [`TaskHost::triage`] made for the same reason. Every key
        /// is optional and `null` means "not set": these are creation-time
        /// values on an item that does not exist yet, so there is nothing a
        /// `null` could be clearing. `size`/`energy` are the wire's snake_case
        /// vocabulary names (`"quick"`/`"normal"`/`"deep"` — ADR-0024 renamed
        /// the middle one — and `"low"`/`"medium"`/`"high"`), resolved by name
        /// rather than by a raw id.
        ///
        /// Malformed JSON or an unknown key is a `"failed"` answer carrying
        /// the parse error, refused before the host is ever checked out, on
        /// the same "reject before the seam" discipline `triage` uses.
        ///
        /// (`#[allow(clippy::too_many_arguments)]` stood here while the three
        /// scalars were positional; folding them into `fields` is what took
        /// the count back under the lint's threshold.)
        pub fn capture(
            &self,
            seed: String,
            title: String,
            stage: String,
            fields: String,
            now_ms: f64,
        ) -> js_sys::Promise {
            let inner = self.inner.clone();
            future_to_promise(async move {
                let fields: CaptureFields = match serde_json::from_str(&fields) {
                    Ok(fields) => fields,
                    Err(error) => {
                        return Ok(JsValue::from_str(
                            &serde_json::to_string(&CaptureResponse {
                                kind: "failed",
                                id: None,
                                error: Some(format!("unreadable capture fields: {error}")),
                            })
                            .expect("CaptureResponse serializes"),
                        ));
                    }
                };
                let Some(mut host) = inner.check_out() else {
                    return Ok(JsValue::from_str(BUSY_CAPTURE));
                };
                let response = host
                    .capture(&seed, &title, &stage, fields, now_ms as i64)
                    .await;
                inner.check_in(host);
                Ok(JsValue::from_str(
                    &serde_json::to_string(&response).expect("CaptureResponse serializes"),
                ))
            })
        }

        /// Acts on an already-existing item (S11/#109: start, complete,
        /// block, cancel). Resolves to JSON:
        /// `{"kind": "ok"|"not_found"|"failed"|"busy", "error": string|null}`.
        pub fn act(&self, seed: String, item_id: String, action: String, now_ms: f64) -> js_sys::Promise {
            let inner = self.inner.clone();
            future_to_promise(async move {
                let Some(mut host) = inner.check_out() else {
                    return Ok(JsValue::from_str(BUSY_ACT));
                };
                let response = host.act(&seed, &item_id, &action, now_ms as i64).await;
                inner.check_in(host);
                Ok(JsValue::from_str(
                    &serde_json::to_string(&response).expect("ActResponse serializes"),
                ))
            })
        }

        /// Triages an already-captured item (S13/#111: edit every field of it
        /// but the source, and optionally promote to Grilling or Ready), as
        /// one CAS `PATCH`. Resolves to JSON:
        /// `{"kind": "ok"|"not_found"|"failed"|"busy", "error": string|null}`.
        ///
        /// `edits` is a JSON object, not a positional list: it carries nine
        /// optional fields, and — more importantly — the difference between a
        /// key being absent ("leave this field alone") and explicitly `null`
        /// ("clear it"), which positional `Option<String>` arguments cannot
        /// express (see [`TriageEdits`]). Malformed JSON, an unknown key, or a
        /// `null` on a `NOT NULL` field is a `"failed"` answer carrying the
        /// parse error, never a partially applied edit. `destination` is
        /// `undefined`/`null` (#122) to leave `stage` untouched entirely —
        /// see [`TaskHostCore::triage`]'s doc.
        pub fn triage(
            &self,
            seed: String,
            item_id: String,
            destination: Option<String>,
            edits: String,
            now_ms: f64,
        ) -> js_sys::Promise {
            let inner = self.inner.clone();
            future_to_promise(async move {
                let edits: TriageEdits = match serde_json::from_str(&edits) {
                    Ok(edits) => edits,
                    Err(error) => {
                        return Ok(JsValue::from_str(
                            &serde_json::to_string(&TriageResponse {
                                kind: "failed",
                                error: Some(format!("unreadable triage edits: {error}")),
                            })
                            .expect("TriageResponse serializes"),
                        ));
                    }
                };
                let Some(mut host) = inner.check_out() else {
                    return Ok(JsValue::from_str(BUSY_TRIAGE));
                };
                let response = host
                    .triage(&seed, &item_id, destination.as_deref(), edits, now_ms as i64)
                    .await;
                inner.check_in(host);
                Ok(JsValue::from_str(
                    &serde_json::to_string(&response).expect("TriageResponse serializes"),
                ))
            })
        }

        /// Confirms a completed Grill interview (#355, ADR-0023): the
        /// review card's Confirm button. Resolves to JSON:
        /// `{"kind": "ok"|"not_found"|"item_done"|"needs_re_review"|"failed"|"busy", "id": string|null, "error": string|null}`.
        /// `session_steps` is a JSON array of `Step` — the review session's
        /// own captured snapshot, compared against live state by
        /// [`hummingbird_core::Core::complete_grill`] — and `verdict` is the
        /// wire's snake_case spelling (`"resolved"`/`"fog_remains"`); both
        /// are rejected as `"failed"` here, before the host is ever checked
        /// out, on the same "reject before the seam" discipline
        /// [`TaskHost::triage`] uses.
        // These are the boundary's required scalars, not an options bag; the
        // host method documents why grouping them would obscure the wire.
        #[allow(clippy::too_many_arguments)]
        #[wasm_bindgen(js_name = completeGrill)]
        pub fn complete_grill(
            &self,
            seed: String,
            item_id: String,
            session_steps: String,
            transcript: String,
            summary: String,
            verdict: String,
            model_proposal: String,
            applied_patch: String,
            delete_unticked_plan: bool,
            now_ms: f64,
        ) -> js_sys::Promise {
            let inner = self.inner.clone();
            future_to_promise(async move {
                let Some(mut host) = inner.check_out() else {
                    return Ok(JsValue::from_str(BUSY_COMPLETE_GRILL));
                };
                let response = host
                    .complete_grill(
                        &seed,
                        &item_id,
                        &session_steps,
                        transcript,
                        summary,
                        &verdict,
                        model_proposal,
                        applied_patch,
                        delete_unticked_plan,
                        now_ms as i64,
                    )
                    .await;
                inner.check_in(host);
                Ok(JsValue::from_str(
                    &serde_json::to_string(&response).expect("CompleteGrillResponse serializes"),
                ))
            })
        }

        /// Saves (or replaces) `item_id`'s Grill draft (#356, ADR-0023) —
        /// the takeover's own continuous save, called after every completed
        /// turn. `turns` is the caller's own opaque JSON array; unreadable
        /// JSON resolves to `"failed"` before the host is ever checked out.
        /// Resolves to JSON: `{"kind": "ok"|"failed"|"busy", "error": string|null}`.
        #[wasm_bindgen(js_name = saveGrillDraft)]
        pub fn save_grill_draft(&self, item_id: String, turns: String, now_ms: f64) -> js_sys::Promise {
            let inner = self.inner.clone();
            future_to_promise(async move {
                let Some(mut host) = inner.check_out() else {
                    return Ok(JsValue::from_str(BUSY_SAVE_GRILL_DRAFT));
                };
                let response = host.save_grill_draft(&item_id, &turns, now_ms as i64).await;
                inner.check_in(host);
                Ok(JsValue::from_str(
                    &serde_json::to_string(&response).expect("SaveGrillDraftResponse serializes"),
                ))
            })
        }

        /// Discards `item_id`'s Grill draft (#356) — the takeover's
        /// explicit, confirmed "Discard" gesture. Resolves to JSON:
        /// `{"kind": "ok"|"failed"|"busy", "error": string|null}`.
        #[wasm_bindgen(js_name = discardGrillDraft)]
        pub fn discard_grill_draft(&self, item_id: String, now_ms: f64) -> js_sys::Promise {
            let inner = self.inner.clone();
            future_to_promise(async move {
                let Some(mut host) = inner.check_out() else {
                    return Ok(JsValue::from_str(BUSY_DISCARD_GRILL_DRAFT));
                };
                let response = host.discard_grill_draft(&item_id, now_ms as i64).await;
                inner.check_in(host);
                Ok(JsValue::from_str(
                    &serde_json::to_string(&response).expect("DiscardGrillDraftResponse serializes"),
                ))
            })
        }

        /// `item_id`'s Grill draft, if any (#356). Resolves to JSON:
        /// `{"kind": "ok"|"busy", "exists": bool, "turns": array|null}`.
        #[wasm_bindgen(js_name = grillDraft)]
        pub fn grill_draft(&self, item_id: String) -> String {
            match self.inner.host.borrow().as_ref() {
                Some(host) => {
                    serde_json::to_string(&host.grill_draft(&item_id)).expect("GrillDraftResponse serializes")
                }
                None => BUSY_GRILL_DRAFT.to_string(),
            }
        }

        /// Every item id carrying a draft (#356) — the Triage inbox's
        /// "Resume grill" labels. Resolves to JSON:
        /// `{"kind": "ok"|"busy", "item_ids": [string]}`.
        #[wasm_bindgen(js_name = grillDraftItemIds)]
        pub fn grill_draft_item_ids(&self) -> String {
            match self.inner.host.borrow().as_ref() {
                Some(host) => serde_json::to_string(&host.grill_draft_item_ids())
                    .expect("GrillDraftItemIdsResponse serializes"),
                None => BUSY_GRILL_DRAFT_ITEM_IDS.to_string(),
            }
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
