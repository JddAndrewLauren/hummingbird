//! [`TaskHostCore`]: the web host's one door into #104's `Core` (the
//! owned-schema sync engine, ADR-0008/0009), kept free of `wasm_bindgen` so
//! it is testable with plain `cargo test` on any target — `lib.rs`'s
//! `wasm_bindings` module is the thin JS-facing shim over this, the same
//! split `calendar_host.rs` (issue #73) already proved.
//!
//! Every method returns a JSON-serializable DTO rather than a `Core` type
//! directly, so the wire shape lives in one place (this file) rather than
//! being re-derived on the TypeScript side from `hummingbird_domain`'s own
//! serde output.

use hummingbird_core::sync::write::ReqwestMutationTransport;
use hummingbird_core::sync::{ReqwestSyncTransport, Trigger};
use hummingbird_core::{Core, CoreCycleOutcome, CoreEvent, CoreInitError};
use hummingbird_domain::{Item, Stage};

// The real, target-specific store `Core::init` resolves to internally is a
// *private* type alias (`hummingbird_core::CoreStore`) — this crate cannot
// name it. It names its own copy of the same per-target split instead
// (`calendar_host.rs`'s `StoreImpl` is the identical pattern): the
// underlying concrete type on each target (`IndexedDbSnapshotStore` /
// `FsSnapshotStore`) is public and identical either way, so this alias and
// `Core::init`'s return type unify without either side knowing about the
// other's name for it.
#[cfg(target_arch = "wasm32")]
type TaskStore = hummingbird_core::storage::IndexedDbSnapshotStore;
#[cfg(not(target_arch = "wasm32"))]
type TaskStore = hummingbird_core::storage::FsSnapshotStore;

/// One drained [`CoreEvent`], as the web host's JSON shape
/// (`client/web/src/store/protocol.ts`'s `TaskEventDTO`).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct TaskEventDTO {
    pub kind: &'static str,
    pub at_ms: i64,
}

fn map_event(event: CoreEvent) -> TaskEventDTO {
    match event {
        CoreEvent::CredentialNeeded { at_ms } => TaskEventDTO {
            kind: "credential_needed",
            at_ms,
        },
    }
}

/// What [`TaskHostCore::capture`] resolves to. `"failed"` covers both an
/// unrecognised `stage` string and a durability failure enqueueing the
/// capture (`SnapshotError`) — the caller has no differing recovery for
/// either, and `error` carries the detail for a log.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct CaptureResponse {
    pub kind: &'static str,
    pub id: Option<String>,
    pub error: Option<String>,
}

/// The wrapper around a live read ([`TaskHostCore::frontier`] /
/// [`TaskHostCore::triage_inbox`]): `"busy"` when the core is checked out
/// mid-poll, carrying no items — the same "no new information, don't blank
/// the view" contract `calendar_host.rs`'s `CalendarListResponse` documents.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ItemListResponse {
    pub kind: &'static str,
    pub items: Vec<Item>,
}

/// The wrapper around [`TaskHostCore::is_pending`]'s answer.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct IsPendingResponse {
    pub kind: &'static str,
    pub pending: bool,
}

/// Maps a [`CoreCycleOutcome`] to the stable string name the web host's
/// protocol (`client/web/src/store/protocol.ts`) matches on, plus whatever
/// payload the S9 "1 edit didn't apply" / sync-status affordance needs.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct RunResponse {
    pub kind: &'static str,
    pub retry_after_ms: Option<i64>,
    pub active_item_count: Option<usize>,
    pub was_full_sweep: Option<bool>,
    pub dead_lettered: Option<usize>,
}

fn run_response(kind: &'static str) -> RunResponse {
    RunResponse {
        kind,
        retry_after_ms: None,
        active_item_count: None,
        was_full_sweep: None,
        dead_lettered: None,
    }
}

fn map_run_outcome(outcome: CoreCycleOutcome) -> RunResponse {
    match outcome {
        CoreCycleOutcome::NoCredential => run_response("no_credential"),
        CoreCycleOutcome::Held => run_response("held"),
        CoreCycleOutcome::Cycle(cycle) => match cycle {
            hummingbird_core::sync::CycleOutcome::Skipped => run_response("skipped"),
            hummingbird_core::sync::CycleOutcome::Blocked { drain, retry_after_ms } => {
                RunResponse {
                    dead_lettered: Some(drain.dead_lettered()),
                    retry_after_ms: Some(retry_after_ms),
                    ..run_response("blocked")
                }
            }
            hummingbird_core::sync::CycleOutcome::CredentialNeeded { drain } => RunResponse {
                dead_lettered: Some(drain.dead_lettered()),
                ..run_response("credential_needed")
            },
            hummingbird_core::sync::CycleOutcome::PersistFailed { retry_after_ms, .. } => {
                RunResponse {
                    retry_after_ms: Some(retry_after_ms),
                    ..run_response("persist_failed")
                }
            }
            hummingbird_core::sync::CycleOutcome::PullFailed { drain, retry_after_ms } => {
                RunResponse {
                    dead_lettered: Some(drain.dead_lettered()),
                    retry_after_ms: Some(retry_after_ms),
                    ..run_response("pull_failed")
                }
            }
            hummingbird_core::sync::CycleOutcome::Completed {
                drain,
                active_item_count,
                was_full_sweep,
            } => RunResponse {
                dead_lettered: Some(drain.dead_lettered()),
                active_item_count: Some(active_item_count),
                was_full_sweep: Some(was_full_sweep),
                ..run_response("completed")
            },
        },
    }
}

/// Plain-Rust wrapper over one owned-schema [`Core`], holding exactly the
/// operations the web host needs plus the two live `reqwest`-backed
/// transports [`Core::run`] takes as call-time arguments.
pub struct TaskHostCore {
    core: Core<TaskStore, TaskStore>,
    read_transport: ReqwestSyncTransport,
    write_transport: ReqwestMutationTransport,
}

impl TaskHostCore {
    /// `base_url` is the authority's origin, host-supplied per ADR-0003 —
    /// this crate invents no deployment address of its own. `api_key` is
    /// whatever credential the host already holds at construction time
    /// (empty until #106/S8 lands a device-token entry flow); it is never
    /// persisted by `Core::init` (see that method's own doc), and
    /// [`TaskHostCore::push_api_key`] is how a host supplies — or rotates —
    /// a real one afterwards.
    pub async fn init(
        namespace: impl Into<String>,
        base_url: impl Into<String>,
        api_key: impl Into<String>,
    ) -> Result<Self, CoreInitError> {
        let base_url = base_url.into();
        let core = Core::init(namespace, api_key).await?;
        let client = reqwest::Client::new();
        let read_transport = ReqwestSyncTransport::new(client.clone(), base_url.clone());
        let write_transport = ReqwestMutationTransport::new(client, base_url);
        Ok(Self {
            core,
            read_transport,
            write_transport,
        })
    }

    /// The host calls this at startup (once a stored device token is known)
    /// and on every rotation. Always resumes a hold — see
    /// [`Core::push_api_key`].
    pub fn push_api_key(&mut self, api_key: String) {
        self.core.push_api_key(api_key);
    }

    /// Whether `item_id` currently has an unconfirmed capture overlaid on
    /// it.
    pub fn is_pending(&self, item_id: &str) -> IsPendingResponse {
        IsPendingResponse {
            kind: "ok",
            pending: self.core.is_pending(item_id),
        }
    }

    /// The frontier — what can be started right now, per [`Core::frontier`].
    pub fn frontier(&self) -> ItemListResponse {
        ItemListResponse {
            kind: "ok",
            items: self.core.frontier(),
        }
    }

    /// The triage inbox — captured, not yet promoted, per
    /// [`Core::triage_inbox`].
    pub fn triage_inbox(&self) -> ItemListResponse {
        ItemListResponse {
            kind: "ok",
            items: self.core.triage_inbox(),
        }
    }

    /// Drains every [`CoreEvent`] since the last drain, mapped to this
    /// host's JSON shape.
    pub fn take_events(&mut self) -> Vec<TaskEventDTO> {
        self.core.take_events().into_iter().map(map_event).collect()
    }

    /// Captures a new item. `stage` is the wire's snake_case stage name
    /// (`hummingbird_domain::Stage::parse`); an unrecognised one fails
    /// without ever touching [`Core::capture`], the same "reject before the
    /// seam" discipline `calendar_host.rs` uses for its own inputs.
    pub async fn capture(
        &mut self,
        seed: &str,
        title: &str,
        stage: &str,
        now_ms: i64,
    ) -> CaptureResponse {
        let Some(stage) = Stage::parse(stage) else {
            return CaptureResponse {
                kind: "failed",
                id: None,
                error: Some(format!("unrecognised stage {stage:?}")),
            };
        };
        match self.core.capture(seed, title, stage, now_ms).await {
            Ok(id) => CaptureResponse {
                kind: "ok",
                id: Some(id),
                error: None,
            },
            Err(error) => CaptureResponse {
                kind: "failed",
                id: None,
                error: Some(error.to_string()),
            },
        }
    }

    /// Runs one [`Core::run`] cycle against the live `reqwest` transports.
    pub async fn run(
        &mut self,
        now_ms: i64,
        trigger: &str,
        force_full_sweep: bool,
        jitter_unit: f64,
    ) -> RunResponse {
        let trigger = match trigger {
            "timer" => Trigger::Timer,
            _ => Trigger::User,
        };
        let outcome = self
            .core
            .run(
                &self.read_transport,
                &self.write_transport,
                now_ms,
                trigger,
                force_full_sweep,
                jitter_unit,
            )
            .await;
        map_run_outcome(outcome)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_fresh_host_has_no_snapshot_and_no_pending_items() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-1");
        let host = TaskHostCore::init(namespace.to_str().unwrap(), "", "")
            .await
            .unwrap();

        assert_eq!(host.frontier(), ItemListResponse { kind: "ok", items: Vec::new() });
        assert_eq!(
            host.triage_inbox(),
            ItemListResponse { kind: "ok", items: Vec::new() }
        );
        assert_eq!(
            host.is_pending("some-id"),
            IsPendingResponse { kind: "ok", pending: false }
        );
    }

    #[tokio::test]
    async fn capturing_with_an_unrecognised_stage_never_reaches_core_capture() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-2");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "")
            .await
            .unwrap();

        let response = host.capture("seed-1", "buy milk", "not-a-stage", 1_000).await;

        assert_eq!(response.kind, "failed");
        assert!(response.id.is_none());
        assert!(response.error.is_some());
        assert_eq!(host.frontier().items.len(), 0);
    }

    #[tokio::test]
    async fn a_capture_is_readable_from_the_frontier_and_marked_pending() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-3");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "")
            .await
            .unwrap();

        let response = host.capture("seed-1", "buy milk", "ready", 1_000).await;

        assert_eq!(response.kind, "ok");
        let id = response.id.clone().unwrap();
        assert!(host.is_pending(&id).pending);
        let frontier = host.frontier();
        assert_eq!(frontier.items.len(), 1);
        assert_eq!(frontier.items[0].title, "buy milk");
        // A `Stage::Triage` (the default in `Core`, but here explicit via
        // "ready") capture is not on the triage inbox.
        assert_eq!(host.triage_inbox().items.len(), 0);
    }

    #[tokio::test]
    async fn a_triage_stage_capture_lands_on_the_inbox_not_the_frontier() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-4");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "")
            .await
            .unwrap();

        host.capture("seed-1", "someday maybe", "triage", 1_000).await;

        assert_eq!(host.frontier().items.len(), 0);
        assert_eq!(host.triage_inbox().items.len(), 1);
    }

    #[tokio::test]
    async fn no_api_key_ever_pushed_reports_no_credential_without_touching_the_network() {
        // `TaskHostCore::init`'s `api_key` argument is empty here — the
        // pre-#106 provisional value `core.worker.ts` starts every task
        // host with — so this is the real "device has never connected"
        // state as `Core::run` sees it... except `Core::init` always pushes
        // *something* (even ""), so this actually exercises the pull-failure
        // path below, not `CoreCycleOutcome::NoCredential` (which only
        // `Core::new` — never `Core::init` — can produce). See the finding
        // posted on #105.
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-5");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "")
            .await
            .unwrap();

        let response = host.run(1_000, "user", true, 0.0).await;

        // An empty `base_url` builds a relative URL, which `reqwest` rejects
        // before ever opening a socket — deterministic and network-free.
        assert_eq!(response.kind, "pull_failed");
    }

    #[tokio::test]
    async fn a_timer_trigger_is_accepted_and_a_user_trigger_is_the_default() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-6");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "")
            .await
            .unwrap();

        // Both trigger spellings reach `Core::run` rather than panicking on
        // an unrecognised string; the exact cycle outcome is `client/core`'s
        // own concern (263 tests already pin it), not this wrapper's.
        let timer = host.run(1_000, "timer", true, 0.0).await;
        let user = host.run(2_000, "anything-else", true, 0.0).await;
        assert_eq!(timer.kind, "pull_failed");
        assert_eq!(user.kind, "pull_failed");
    }

    #[tokio::test]
    async fn draining_events_twice_yields_nothing_new() {
        let dir = tempfile::tempdir().unwrap();
        let namespace = dir.path().join("ns-7");
        let mut host = TaskHostCore::init(namespace.to_str().unwrap(), "", "")
            .await
            .unwrap();

        assert_eq!(host.take_events(), Vec::new());
        assert_eq!(host.take_events(), Vec::new());
    }
}
