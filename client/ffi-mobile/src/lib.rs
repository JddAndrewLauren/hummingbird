//! `hummingbird-ffi-mobile`: `#[uniffi::export]` wrappers over
//! `hummingbird-core` for Kotlin (Android, Wear OS) and Swift (iPad).
//!
//! [`MobileTaskHost`] is the native hosts' one door into `Core` — the same
//! wrapper-plus-live-transports split `ffi-web::task_host::TaskHostCore`
//! is, surfaced through UniFFI instead of `wasm_bindgen` (ADR-0001 seam
//! rule 2: both FFI crates surface `Core` verbatim, neither reimplements
//! it). The surface grows one screen at a time with the Android build
//! (#141), each screen's decision modules arriving in `core` first
//! (ADR-0025); M0 carried init, credential push, one sync cycle, and the
//! counters the proof screen shows. M1-5 (#503) added the free
//! [`can_submit_capture`] door onto [`hummingbird_core::decisions`] and
//! [`MobileTaskHost::capture`] — `CaptureActivity`'s whole surface, title
//! only, no capture-meta fields (out of scope until a later screen).
//!
//! **Async runs under uniffi's tokio runtime** (`async_runtime = "tokio"`),
//! because `Core::run`'s reqwest transports need a reactor and the host
//! (Kotlin coroutines / Swift concurrency) must never be the thing
//! providing one. Interior state is a `tokio::sync::Mutex` for the same
//! reason: it is held across the cycle's awaits.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use hummingbird_core::storage::FsSnapshotStore;
use hummingbird_core::sync::write::ReqwestMutationTransport;
use hummingbird_core::sync::{ReqwestSyncTransport, Trigger};
use hummingbird_core::{CaptureOptions, Core, CoreCycleOutcome, CoreEvent};
use hummingbird_domain::Stage;

/// Whether `draft` is worth submitting — the free door onto
/// [`hummingbird_core::decisions::can_submit_capture`] (ADR-0025), the
/// mobile twin of `ffi-web::decisions::can_submit_capture`. `CaptureActivity`
/// calls this before ever calling [`MobileTaskHost::capture`], and it is the
/// *only* place the refusal may be decided: a Kotlin `isBlank()` copy of
/// this rule is banned (M1-5/#503's own trap) precisely because it disagrees
/// with the real rule on a BOM-only draft — see
/// `hummingbird_core::decisions::capture`'s doc for that case.
#[uniffi::export]
pub fn can_submit_capture(draft: &str) -> bool {
    hummingbird_core::decisions::can_submit_capture(draft)
}

/// Mints a fresh seed for one [`MobileTaskHost::capture`] call. Only
/// uniqueness matters here, never unpredictability — the same "no clock or
/// RNG that panics on bare wasm32" story `sync::write::deterministic_id`'s
/// callers already tell, adapted for a host that supplies no seed of its own
/// (`useCaptureWiring.ts`'s `mintSeed` is the web's equivalent, reached from
/// JS instead of from here because that door takes a caller-supplied seed —
/// `MobileTaskHost::capture`'s narrower signature does not, so the mint moves
/// behind the seam). An in-process monotonic counter, paired with the
/// caller's own `now_ms`, is enough: two captures can never collide within
/// one process, and a fresh process starts the counter over only alongside a
/// fresh `now_ms`.
static CAPTURE_SEQ: AtomicU64 = AtomicU64::new(0);

fn mint_capture_seed(now_ms: i64) -> String {
    format!("mobile-capture:{now_ms}:{}", CAPTURE_SEQ.fetch_add(1, Ordering::Relaxed))
}

uniffi::setup_scaffolding!();

/// The public API version of the wrapped `hummingbird-core`, surfaced
/// verbatim to Kotlin/Swift (ADR-0001 seam rule 2). Predates
/// [`MobileTaskHost`] as the stub's smoke test; kept exported because it is
/// the cheapest end-to-end proof a host has that the generated binding and
/// the loaded `.so` agree — the Android instrumented test asserts it.
#[uniffi::export]
pub fn core_api_version() -> u32 {
    hummingbird_core::Core::new().api_version()
}

/// [`MobileTaskHost::init`] failed to load durable state under the given
/// namespace. One flat arm: the caller's only recovery is showing the
/// message (`CoreInitError` is already a message-carrying wrapper for the
/// same reason — see its doc).
#[derive(Debug, uniffi::Error)]
pub enum MobileInitError {
    // `detail`, not `message`: a UniFFI error variant field named
    // `message` generates a Kotlin `val message` that collides with
    // `kotlin.Exception.message` and fails to compile.
    InitFailed { detail: String },
}

impl std::fmt::Display for MobileInitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MobileInitError::InitFailed { detail } => write!(f, "{detail}"),
        }
    }
}

impl std::error::Error for MobileInitError {}

/// [`MobileTaskHost::capture`] failed to enqueue — a [`hummingbird_core`]
/// `SnapshotError`, surfaced the same message-wrapping way
/// [`MobileInitError`] is (see its doc for why `detail`, not `message`).
#[derive(Debug, uniffi::Error)]
pub enum MobileCaptureError {
    CaptureFailed { detail: String },
}

impl std::fmt::Display for MobileCaptureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MobileCaptureError::CaptureFailed { detail } => write!(f, "{detail}"),
        }
    }
}

impl std::error::Error for MobileCaptureError {}

/// One drained [`CoreEvent`], as the mobile hosts' shape — the same
/// `kind`/`at_ms` pair `ffi-web`'s `TaskEventDTO` carries, so the two FFI
/// surfaces stay nameable in one breath.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MobileEvent {
    pub kind: String,
    pub at_ms: i64,
}

fn map_event(event: CoreEvent) -> MobileEvent {
    match event {
        CoreEvent::CredentialNeeded { at_ms } => MobileEvent {
            kind: "credential_needed".to_string(),
            at_ms,
        },
    }
}

/// What one [`MobileTaskHost::run`] cycle did — `ffi-web`'s `RunResponse`
/// shape with the identical `kind` strings, so a host on either side of
/// the seam badges sync state from the same vocabulary.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct RunOutcome {
    pub kind: String,
    pub retry_after_ms: Option<i64>,
    pub active_item_count: Option<u32>,
    pub was_full_sweep: Option<bool>,
    pub dead_lettered: Option<u32>,
}

fn run_outcome(kind: &str) -> RunOutcome {
    RunOutcome {
        kind: kind.to_string(),
        retry_after_ms: None,
        active_item_count: None,
        was_full_sweep: None,
        dead_lettered: None,
    }
}

fn map_run_outcome(outcome: CoreCycleOutcome) -> RunOutcome {
    use hummingbird_core::sync::CycleOutcome;
    match outcome {
        CoreCycleOutcome::NoCredential => run_outcome("no_credential"),
        CoreCycleOutcome::Held => run_outcome("held"),
        CoreCycleOutcome::Cycle(cycle) => match cycle {
            CycleOutcome::Skipped => run_outcome("skipped"),
            CycleOutcome::Blocked {
                drain,
                retry_after_ms,
            } => RunOutcome {
                dead_lettered: Some(drain.dead_lettered() as u32),
                retry_after_ms: Some(retry_after_ms),
                ..run_outcome("blocked")
            },
            CycleOutcome::CredentialNeeded { drain } => RunOutcome {
                dead_lettered: Some(drain.dead_lettered() as u32),
                ..run_outcome("credential_needed")
            },
            CycleOutcome::PersistFailed { retry_after_ms, .. } => RunOutcome {
                retry_after_ms: Some(retry_after_ms),
                ..run_outcome("persist_failed")
            },
            CycleOutcome::PullFailed {
                drain,
                retry_after_ms,
            } => RunOutcome {
                dead_lettered: Some(drain.dead_lettered() as u32),
                retry_after_ms: Some(retry_after_ms),
                ..run_outcome("pull_failed")
            },
            CycleOutcome::Completed {
                drain,
                active_item_count,
                was_full_sweep,
            } => RunOutcome {
                dead_lettered: Some(drain.dead_lettered() as u32),
                active_item_count: Some(active_item_count as u32),
                was_full_sweep: Some(was_full_sweep),
                ..run_outcome("completed")
            },
        },
    }
}

struct Inner {
    core: Core<FsSnapshotStore, FsSnapshotStore>,
    read_transport: ReqwestSyncTransport,
    write_transport: ReqwestMutationTransport,
}

/// The native hosts' handle on one durable `Core` — `TaskHostCore`'s
/// mobile twin (see the module doc for the seam-rule symmetry).
#[derive(uniffi::Object)]
pub struct MobileTaskHost {
    inner: tokio::sync::Mutex<Inner>,
}

#[uniffi::export(async_runtime = "tokio")]
impl MobileTaskHost {
    /// Loads (or starts fresh) the durable sync state under `namespace` —
    /// a directory path on mobile targets; the host supplies its app-local
    /// files dir (ADR-0003: the namespace is the one thing the host
    /// contributes at init). `base_url` is the authority's origin,
    /// host-supplied for the same reason. `api_key` is whatever device
    /// token the host already holds — often empty at first launch; it is
    /// never persisted by the core ([`Core::init`]'s own doc), and
    /// [`MobileTaskHost::push_api_key`] /
    /// [`MobileTaskHost::rehydrate_api_key`] supply it afterwards.
    #[uniffi::constructor]
    pub async fn init(
        namespace: String,
        base_url: String,
        api_key: String,
    ) -> Result<Arc<Self>, MobileInitError> {
        let empty_key = api_key.is_empty();
        let mut core = Core::init(namespace, api_key)
            .await
            .map_err(|error| MobileInitError::InitFailed {
                detail: error.to_string(),
            })?;
        // `Core::init` holds whatever string it is given — including `""`,
        // which would then be *sent* as a bearer token. An empty key from
        // the host means "no token yet", so map it to the core's explicit
        // no-credential state: `run` answers `no_credential` and reaches no
        // transport, instead of `pull_failed` after a doomed request.
        if empty_key {
            core.clear_api_key();
        }
        let client = reqwest_client();
        let read_transport = ReqwestSyncTransport::new(client.clone(), base_url.clone());
        let write_transport = ReqwestMutationTransport::new(client, base_url);
        Ok(Arc::new(Self {
            inner: tokio::sync::Mutex::new(Inner {
                core,
                read_transport,
                write_transport,
            }),
        }))
    }

    /// The wrapped core's public API version — same value as the free
    /// [`core_api_version`], surfaced on the object so a host holding only
    /// the handle can show it.
    pub async fn api_version(&self) -> u32 {
        self.inner.lock().await.core.api_version()
    }

    /// The mirror's active-item population (ADR-0001's watchline figure) —
    /// the M0 proof screen's number.
    pub async fn active_item_count(&self) -> u32 {
        self.inner.lock().await.core.active_item_count() as u32
    }

    /// The outbound queue's current depth — the "queued" sync-status
    /// figure.
    pub async fn queue_depth(&self) -> u32 {
        self.inner.lock().await.core.queue_depth() as u32
    }

    /// A fresh device token from the person (first entry, or rotation
    /// after a `credential_needed` event). Always resumes a hold — see
    /// [`Core::push_api_key`].
    pub async fn push_api_key(&self, api_key: String) {
        self.inner.lock().await.core.push_api_key(api_key);
    }

    /// The host reloading a token it already had stored (app start), never
    /// resuming a hold — see [`Core::rehydrate_api_key`].
    pub async fn rehydrate_api_key(&self, api_key: String) {
        self.inner.lock().await.core.rehydrate_api_key(api_key);
    }

    /// "Forget token": clears the in-memory credential. Nothing durable to
    /// clean up — the core never persisted it.
    pub async fn clear_api_key(&self) {
        self.inner.lock().await.core.clear_api_key();
    }

    /// Runs one sync cycle against the live transports. `trigger` is the
    /// web protocol's string pair (`"timer"` gates on backoff, anything
    /// else is a deliberate `"user"` gesture); `now_ms` and `jitter_unit`
    /// come from the host clock and RNG, injected here exactly as on the
    /// web side so the core stays deterministic under test.
    pub async fn run(
        &self,
        now_ms: i64,
        trigger: String,
        force_full_sweep: bool,
        jitter_unit: f64,
    ) -> RunOutcome {
        let trigger = match trigger.as_str() {
            "timer" => Trigger::Timer,
            _ => Trigger::User,
        };
        let inner = &mut *self.inner.lock().await;
        let outcome = inner
            .core
            .run(
                &inner.read_transport,
                &inner.write_transport,
                now_ms,
                trigger,
                force_full_sweep,
                jitter_unit,
            )
            .await;
        map_run_outcome(outcome)
    }

    /// Captures `title` — `CaptureActivity`'s whole surface (M1-5/#503).
    /// **Local-first** (#128's own criterion): reaches
    /// [`hummingbird_core::Core::capture`], which durably enqueues via
    /// `SyncCycle::enqueue` and overlays the optimistic [`Item`] before any
    /// transport is ever touched, so the item exists in the local mirror the
    /// instant this returns — sync or no sync, online or offline. Always
    /// captures into `Stage::Triage` with every [`CaptureOptions`] field
    /// absent: raw text only, no capture-meta surface in M1 (say so at every
    /// call site rather than only here). Mints its own seed
    /// ([`mint_capture_seed`]) since the host supplies only `title`/`now_ms`.
    ///
    /// [`Item`]: hummingbird_domain::Item
    pub async fn capture(&self, title: String, now_ms: i64) -> Result<String, MobileCaptureError> {
        let seed = mint_capture_seed(now_ms);
        let mut inner = self.inner.lock().await;
        inner
            .core
            .capture(&seed, title, Stage::Triage, now_ms, CaptureOptions::default())
            .await
            .map_err(|error| MobileCaptureError::CaptureFailed {
                detail: error.to_string(),
            })
    }

    /// Drains queued [`CoreEvent`]s (today: `credential_needed`) — a
    /// pull-based drain, never a host-implemented callback (ADR-0003).
    pub async fn take_events(&self) -> Vec<MobileEvent> {
        self.inner
            .lock()
            .await
            .core
            .take_events()
            .into_iter()
            .map(map_event)
            .collect()
    }
}

/// One `reqwest::Client` per host, cloned into both transports — connection
/// pooling shared across reads and writes, exactly `TaskHostCore::init`'s
/// arrangement.
fn reqwest_client() -> reqwest::Client {
    reqwest::Client::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exported_stub_matches_core_api_version() {
        assert_eq!(core_api_version(), hummingbird_core::API_VERSION);
    }

    #[tokio::test]
    async fn init_creates_the_namespace_and_answers_the_proof_pair() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("m0-ns");
        let host = MobileTaskHost::init(
            namespace.to_str().unwrap().to_string(),
            "https://authority.example".to_string(),
            String::new(),
        )
        .await
        .unwrap();

        assert_eq!(host.api_version().await, hummingbird_core::API_VERSION);
        assert_eq!(host.active_item_count().await, 0);
        assert_eq!(host.queue_depth().await, 0);
    }

    #[tokio::test]
    async fn run_without_a_credential_reports_no_credential_and_touches_no_network() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("m0-ns-2");
        let host = MobileTaskHost::init(
            namespace.to_str().unwrap().to_string(),
            // A base URL that would fail DNS if any request were attempted:
            // `no_credential` short-circuits before the transports.
            "https://invalid.invalid".to_string(),
            String::new(),
        )
        .await
        .unwrap();

        let outcome = host.run(1_000, "user".to_string(), false, 0.0).await;
        assert_eq!(outcome.kind, "no_credential");
        assert!(host.take_events().await.is_empty());
    }

    // ------------------------------------------------------- M1-5 (#503)

    /// The exposure is a pass-through and nothing more — the rule itself is
    /// tested in `hummingbird_core::decisions::capture`. Mirrors
    /// `ffi-web::decisions`' identical pin, so the two FFI surfaces are
    /// proven to agree, not just each proven separately.
    #[test]
    fn the_capture_binding_is_the_core_rule_verbatim() {
        for draft in [
            "",
            "   ",
            "\t\n",
            "\u{feff}",
            "buy milk",
            "  buy milk  ",
            "\u{feff}buy milk",
        ] {
            assert_eq!(
                can_submit_capture(draft),
                hummingbird_core::decisions::can_submit_capture(draft),
                "{draft:?} disagreed across the binding",
            );
        }
    }

    /// #128's local-first criterion, proved the same way the M0 test above
    /// proves "no network reached": a base URL that would fail DNS, yet the
    /// capture still succeeds and is durably enqueued (`queue_depth`) — the
    /// mutation exists before any transport is ever reached.
    /// `active_item_count`/`frontier` stay at zero on purpose: a capture is
    /// born into `Stage::Triage`, which `Core::frontier`'s own
    /// Ready/InProgress filter never surfaces — `Core::triage_inbox` is the
    /// query that would show it, not yet exposed to a mobile host (deferred
    /// to the Now-list screen, M1-6).
    #[tokio::test]
    async fn capture_is_durable_before_any_network_is_touched() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("m1-5-capture-ns");
        let host = MobileTaskHost::init(
            namespace.to_str().unwrap().to_string(),
            "https://invalid.invalid".to_string(),
            String::new(),
        )
        .await
        .unwrap();

        let id = host.capture("buy milk".to_string(), 1_000).await.unwrap();
        assert!(!id.is_empty());
        assert_eq!(host.queue_depth().await, 1);
    }

    /// Two captures at the identical `now_ms` must mint distinct ids — the
    /// whole reason [`mint_capture_seed`] carries its own counter rather than
    /// deriving the seed from `now_ms` alone.
    #[tokio::test]
    async fn two_captures_in_the_same_millisecond_mint_distinct_ids() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("m1-5-capture-ns-2");
        let host = MobileTaskHost::init(
            namespace.to_str().unwrap().to_string(),
            "https://invalid.invalid".to_string(),
            String::new(),
        )
        .await
        .unwrap();

        let a = host.capture("first".to_string(), 5_000).await.unwrap();
        let b = host.capture("second".to_string(), 5_000).await.unwrap();
        assert_ne!(a, b);
        assert_eq!(host.queue_depth().await, 2);
    }

    /// `Core::capture` still has no opinion of its own on an empty title
    /// (`hummingbird_core::decisions::capture`'s own doc) — the refusal is
    /// entirely `CaptureActivity`'s job, reached through
    /// [`can_submit_capture`] before this is ever called. Pinned here so
    /// nobody "fixes" that by adding a check inside this method instead.
    #[tokio::test]
    async fn capture_itself_has_no_opinion_on_a_blank_title() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("m1-5-capture-ns-3");
        let host = MobileTaskHost::init(
            namespace.to_str().unwrap().to_string(),
            "https://invalid.invalid".to_string(),
            String::new(),
        )
        .await
        .unwrap();

        assert!(host.capture("   ".to_string(), 1_000).await.is_ok());
    }
}
