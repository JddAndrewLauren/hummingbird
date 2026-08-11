//! [`ContextPoller`]: the poll trigger + credential seam for one context
//! provider (issue #72, per ADR-0005's placement decision).

#[cfg(test)]
use crate::storage::PersistableSealed;
use crate::storage::{
    load_snapshot_at_version, save_snapshot, Envelope, Persistable, SnapshotStore,
};

use super::credential::CredentialState;

/// The provider-specific half of a poll attempt: given the currently pushed
/// access token, fetch and return one complete snapshot, or classify the
/// failure. #71's [`crate::calendar::google::fetch_calendar_snapshot`] is
/// the Google implementation this seam is built for; #47's future M365
/// adapter fills the same shape unchanged.
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
pub trait ProviderPoller {
    type Snapshot: Persistable;

    async fn poll(&self, access_token: &str, now_ms: i64) -> Result<Self::Snapshot, PollFailure>;
}

/// How a [`ProviderPoller::poll`] call failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PollFailure {
    /// The credential was rejected (expired or a 401). Holds this
    /// provider's polling and surfaces one [`CredentialEvent`] — no retry
    /// against a dead credential.
    Unauthorized,
    /// Any other failure (network, partial response, offline, ...). The
    /// previous snapshot is left untouched and the next trigger simply
    /// tries again — no hold.
    Transient(String),
}

/// One host-visible signal: this provider's credential no longer works and
/// polling is held until [`ContextPoller::push_token`] supplies a fresh one.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct CredentialEvent {
    pub provider: String,
    pub at_ms: i64,
}

/// The observable result of one poll attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PollOutcome {
    /// No token has been pushed for this provider yet — a valid steady
    /// state (ADR-0005), not an error.
    NoCredential,
    /// Polling is held on a prior expiry/401; nothing was attempted.
    Held,
    /// The fetch succeeded and the snapshot was atomically persisted.
    Succeeded,
    /// The fetch failed transiently; the previous snapshot is untouched.
    TransientFailure,
    /// The fetch failed with an unauthorized error; polling now holds and a
    /// [`CredentialEvent`] was recorded.
    Unauthorized,
}

/// Poll trigger + credential seam for one context provider.
///
/// The host owns cadence (core start, explicit refresh, and #46's 15-min
/// foreground timer, under ADR-0005 — not ADR-0007's 60-second *task-sync*
/// timer) and calls the matching method at each trigger point;
/// this type owns what happens on each call — token lookup, the fetch, and
/// atomic persistence on success. `refresh` and the timer path both funnel
/// through the same attempt as `start`, deliberately: manual refresh gets no
/// special casing.
pub struct ContextPoller<P, S> {
    provider: String,
    fetcher: P,
    store: S,
    schema_version: u32,
    credential: CredentialState,
    pending_events: Vec<CredentialEvent>,
}

impl<P, S> ContextPoller<P, S>
where
    P: ProviderPoller,
    S: SnapshotStore,
{
    pub fn new(provider: impl Into<String>, fetcher: P, store: S, schema_version: u32) -> Self {
        Self {
            provider: provider.into(),
            fetcher,
            store,
            schema_version,
            credential: CredentialState::default(),
            pending_events: Vec::new(),
        }
    }

    /// The wrapped provider poller, for host wiring that needs to reach
    /// provider-specific configuration (#73's calendar picker driving
    /// [`crate::calendar::google::GoogleProviderPoller::set_calendar_selections`])
    /// without this type needing to know that method exists.
    pub fn fetcher(&self) -> &P {
        &self.fetcher
    }

    /// The token an *auxiliary* authenticated read against this provider
    /// should use, or `None` when none has been pushed or polling is held.
    ///
    /// This exists for reads that are not poll attempts and write no
    /// snapshot — #73's calendar-list lookup for the picker. It honours the
    /// hold for the same reason `attempt` does: a token Google has already
    /// rejected cannot succeed on a different endpoint. The credential still
    /// never leaves the core+FFI process boundary and is never persisted
    /// (ADR-0004/0005); no binding exports this.
    pub fn current_token(&self) -> Option<String> {
        self.credential.token().map(str::to_string)
    }

    /// The host calls this at init and on every token rotation. Always
    /// resumes polling, even if this provider was previously held — a fresh
    /// push is the only way out of a hold.
    pub fn push_token(&mut self, token: impl Into<String>) {
        self.credential.push(token.into());
    }

    /// The most recently persisted snapshot, with its `as_of`, or `None` if
    /// nothing has been successfully polled yet — the read side of the
    /// same store `attempt` writes through, for a host (#73's context tile)
    /// to render without needing its own store handle.
    ///
    /// **Version-discarding** ([`load_snapshot_at_version`], the storage
    /// module's documented idiom for a caller whose answer to a stale
    /// version is "throw it away"): a context mirror is disposable by
    /// construction — it is a copy of what the provider says, re-fetchable
    /// on the next poll — so a snapshot written at another
    /// `schema_version` reads as "nothing polled yet" and costs one poll
    /// interval of `not_read`, rather than failing the read outright.
    /// Reaching for the version-blind [`crate::storage::load_snapshot`] instead
    /// would parse the old payload as the new type and, on the commonest
    /// kind of change, error the read forever until storage is cleared by
    /// hand.
    pub async fn current_snapshot(&self) -> Option<Envelope<P::Snapshot>> {
        load_snapshot_at_version(&self.store, self.schema_version)
            .await
            .unwrap_or(None)
    }

    /// Drains every [`CredentialEvent`] recorded since the last drain. Poll-
    /// style rather than callback-delivered: ADR-0003 rules out a
    /// host-implemented async callback trait crossing the FFI boundary.
    pub fn take_credential_events(&mut self) -> Vec<CredentialEvent> {
        std::mem::take(&mut self.pending_events)
    }

    /// Core-start trigger.
    pub async fn start(&mut self, now_ms: i64) -> PollOutcome {
        self.attempt(now_ms).await
    }

    /// Explicit user-invoked refresh — routes through the identical attempt
    /// as every other trigger.
    pub async fn refresh(&mut self, now_ms: i64) -> PollOutcome {
        self.attempt(now_ms).await
    }

    /// The foreground 15-minute context-poll timer tick (#46, under
    /// ADR-0005). The host is
    /// responsible for calling this only while online and foregrounded, and
    /// only pausing/resuming the timer itself — this method carries no
    /// cadence logic of its own, so it, too, is the same attempt.
    pub async fn on_timer(&mut self, now_ms: i64) -> PollOutcome {
        self.attempt(now_ms).await
    }

    async fn attempt(&mut self, now_ms: i64) -> PollOutcome {
        if self.credential.is_held() {
            return PollOutcome::Held;
        }
        let Some(token) = self.credential.token().map(str::to_string) else {
            return PollOutcome::NoCredential;
        };

        match self.fetcher.poll(&token, now_ms).await {
            Ok(snapshot) => {
                // An atomic replace-or-untouched write (#68): a storage
                // failure here leaves the previous snapshot intact, same as
                // any other transient failure.
                match save_snapshot(&self.store, self.schema_version, now_ms as u64, &snapshot)
                    .await
                {
                    Ok(()) => PollOutcome::Succeeded,
                    Err(_) => PollOutcome::TransientFailure,
                }
            }
            Err(PollFailure::Unauthorized) => {
                self.credential.hold();
                self.pending_events.push(CredentialEvent {
                    provider: self.provider.clone(),
                    at_ms: now_ms,
                });
                PollOutcome::Unauthorized
            }
            Err(PollFailure::Transient(_)) => PollOutcome::TransientFailure,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{load_snapshot, MemorySnapshotStore};
    use serde::{Deserialize, Serialize};
    use std::sync::Mutex;

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    struct FakeSnapshot {
        events: Vec<String>,
    }
    impl PersistableSealed for FakeSnapshot {}
    impl Persistable for FakeSnapshot {}

    /// A scripted [`ProviderPoller`]: each call to `poll` pops the next
    /// scripted result, and records the token it was called with so tests
    /// can assert the push→poll wiring, not just outcomes.
    struct ScriptedPoller {
        results: Mutex<Vec<Result<FakeSnapshot, PollFailure>>>,
        seen_tokens: Mutex<Vec<String>>,
    }

    impl ScriptedPoller {
        fn new(results: Vec<Result<FakeSnapshot, PollFailure>>) -> Self {
            Self {
                results: Mutex::new(results),
                seen_tokens: Mutex::new(Vec::new()),
            }
        }
    }

    #[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
    #[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
    impl ProviderPoller for ScriptedPoller {
        type Snapshot = FakeSnapshot;

        async fn poll(
            &self,
            access_token: &str,
            _now_ms: i64,
        ) -> Result<FakeSnapshot, PollFailure> {
            self.seen_tokens
                .lock()
                .unwrap()
                .push(access_token.to_string());
            self.results
                .lock()
                .unwrap()
                .pop()
                .expect("scripted poller ran out of results")
        }
    }

    #[tokio::test]
    async fn start_with_no_pushed_token_reports_no_credential_and_persists_nothing() {
        let fetcher = ScriptedPoller::new(vec![]);
        let store = MemorySnapshotStore::default();
        let mut poller = ContextPoller::new("google_calendar", fetcher, store, 1);

        let outcome = poller.start(1_000).await;

        assert_eq!(outcome, PollOutcome::NoCredential);
        let saved: Option<crate::storage::Envelope<FakeSnapshot>> =
            load_snapshot(&poller.store).await.unwrap();
        assert_eq!(saved, None);
    }

    #[tokio::test]
    async fn a_successful_start_persists_the_snapshot_with_an_accurate_as_of() {
        let snapshot = FakeSnapshot {
            events: vec!["evt-1".to_string()],
        };
        let fetcher = ScriptedPoller::new(vec![Ok(snapshot.clone())]);
        let store = MemorySnapshotStore::default();
        let mut poller = ContextPoller::new("google_calendar", fetcher, store, 1);
        poller.push_token("access-token-1");

        let outcome = poller.start(5_000).await;

        assert_eq!(outcome, PollOutcome::Succeeded);
        let saved = load_snapshot::<FakeSnapshot, _>(&poller.store)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(saved.payload, snapshot);
        assert_eq!(saved.as_of, 5_000);
        assert_eq!(
            poller.fetcher.seen_tokens.lock().unwrap().as_slice(),
            &["access-token-1".to_string()]
        );
    }

    #[tokio::test]
    async fn unauthorized_holds_polling_and_records_exactly_one_credential_event() {
        let fetcher = ScriptedPoller::new(vec![Err(PollFailure::Unauthorized)]);
        let store = MemorySnapshotStore::default();
        let mut poller = ContextPoller::new("google_calendar", fetcher, store, 1);
        poller.push_token("stale-token");

        let outcome = poller.start(10_000).await;
        assert_eq!(outcome, PollOutcome::Unauthorized);

        let events = poller.take_credential_events();
        assert_eq!(
            events,
            vec![CredentialEvent {
                provider: "google_calendar".to_string(),
                at_ms: 10_000,
            }]
        );

        // A second trigger while still held must not call the fetcher
        // again — no retry against a dead credential — and must not
        // re-record the event.
        let second = poller.refresh(11_000).await;
        assert_eq!(second, PollOutcome::Held);
        assert_eq!(poller.take_credential_events(), Vec::new());
        assert_eq!(poller.fetcher.seen_tokens.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn the_auxiliary_token_follows_the_same_hold_as_a_poll_attempt() {
        let fetcher = ScriptedPoller::new(vec![Err(PollFailure::Unauthorized)]);
        let store = MemorySnapshotStore::default();
        let mut poller = ContextPoller::new("google_calendar", fetcher, store, 1);

        assert_eq!(poller.current_token(), None);

        poller.push_token("token-1");
        assert_eq!(poller.current_token(), Some("token-1".to_string()));

        // Held on a rejected credential: an auxiliary read must not go out
        // with the token Google just refused either.
        assert_eq!(poller.start(1_000).await, PollOutcome::Unauthorized);
        assert_eq!(poller.current_token(), None);

        poller.push_token("token-2");
        assert_eq!(poller.current_token(), Some("token-2".to_string()));
    }

    #[tokio::test]
    async fn a_fresh_push_after_a_hold_resumes_polling() {
        let fetcher = ScriptedPoller::new(vec![
            Ok(FakeSnapshot {
                events: vec!["evt-resumed".to_string()],
            }),
            Err(PollFailure::Unauthorized),
        ]);
        let store = MemorySnapshotStore::default();
        let mut poller = ContextPoller::new("google_calendar", fetcher, store, 1);
        poller.push_token("stale-token");

        assert_eq!(poller.start(1_000).await, PollOutcome::Unauthorized);
        assert_eq!(poller.refresh(2_000).await, PollOutcome::Held);

        poller.push_token("fresh-token");
        let outcome = poller.refresh(3_000).await;

        assert_eq!(outcome, PollOutcome::Succeeded);
        let saved = load_snapshot::<FakeSnapshot, _>(&poller.store)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(saved.payload.events, vec!["evt-resumed".to_string()]);
        assert_eq!(
            poller.fetcher.seen_tokens.lock().unwrap().as_slice(),
            &["stale-token".to_string(), "fresh-token".to_string()]
        );
    }

    #[tokio::test]
    async fn a_transient_failure_leaves_the_previous_snapshot_untouched_and_does_not_hold() {
        let first_good = FakeSnapshot {
            events: vec!["evt-good".to_string()],
        };
        let fetcher = ScriptedPoller::new(vec![
            Err(PollFailure::Transient("network error".to_string())),
            Ok(first_good.clone()),
        ]);
        let store = MemorySnapshotStore::default();
        let mut poller = ContextPoller::new("google_calendar", fetcher, store, 1);
        poller.push_token("token-1");

        assert_eq!(poller.start(1_000).await, PollOutcome::Succeeded);

        let outcome = poller.refresh(2_000).await;
        assert_eq!(outcome, PollOutcome::TransientFailure);

        // Still held-free: no credential event, and the next trigger is
        // attempted rather than short-circuited.
        assert_eq!(poller.take_credential_events(), Vec::new());
        let saved = load_snapshot::<FakeSnapshot, _>(&poller.store)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(saved.payload, first_good);
        assert_eq!(saved.as_of, 1_000);
    }

    #[tokio::test]
    async fn an_offline_initial_poll_leaves_no_snapshot_at_all_rather_than_a_partial_one() {
        let fetcher = ScriptedPoller::new(vec![Err(PollFailure::Transient(
            "device is offline".to_string(),
        ))]);
        let store = MemorySnapshotStore::default();
        let mut poller = ContextPoller::new("google_calendar", fetcher, store, 1);
        poller.push_token("token-1");

        let outcome = poller.start(1_000).await;

        assert_eq!(outcome, PollOutcome::TransientFailure);
        let saved: Option<crate::storage::Envelope<FakeSnapshot>> =
            load_snapshot(&poller.store).await.unwrap();
        assert_eq!(saved, None);
    }

    #[tokio::test]
    async fn current_snapshot_is_none_until_a_poll_succeeds_then_reflects_the_latest_write() {
        let fetcher = ScriptedPoller::new(vec![Ok(FakeSnapshot {
            events: vec!["evt-1".to_string()],
        })]);
        let store = MemorySnapshotStore::default();
        let mut poller = ContextPoller::new("google_calendar", fetcher, store, 1);

        assert_eq!(poller.current_snapshot().await, None);

        poller.push_token("token-1");
        assert_eq!(poller.start(9_000).await, PollOutcome::Succeeded);

        let snapshot = poller.current_snapshot().await.unwrap();
        assert_eq!(snapshot.as_of, 9_000);
        assert_eq!(snapshot.payload.events, vec!["evt-1".to_string()]);
    }

    #[tokio::test]
    async fn a_snapshot_written_at_another_schema_version_reads_as_nothing_polled_yet() {
        // The version-discard migration (#46's two-arm event shape bumped
        // the calendar lane 1 -> 2): a mirror written by an older build is
        // thrown away rather than parsed, and the next poll refills it.
        let store = MemorySnapshotStore::default();
        save_snapshot(
            &store,
            1,
            9_000,
            &FakeSnapshot {
                events: vec!["written-at-v1".to_string()],
            },
        )
        .await
        .unwrap();

        let poller = ContextPoller::new("google_calendar", ScriptedPoller::new(vec![]), store, 2);

        assert_eq!(poller.current_snapshot().await, None);
    }

    #[tokio::test]
    async fn manual_refresh_and_the_timer_tick_both_run_the_identical_attempt_as_start() {
        let fetcher = ScriptedPoller::new(vec![
            Ok(FakeSnapshot {
                events: vec!["evt-timer".to_string()],
            }),
            Ok(FakeSnapshot {
                events: vec!["evt-manual".to_string()],
            }),
        ]);
        let store = MemorySnapshotStore::default();
        let mut poller = ContextPoller::new("google_calendar", fetcher, store, 1);
        poller.push_token("token-1");

        assert_eq!(poller.refresh(1_000).await, PollOutcome::Succeeded);
        assert_eq!(poller.on_timer(2_000).await, PollOutcome::Succeeded);

        let saved = load_snapshot::<FakeSnapshot, _>(&poller.store)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(saved.payload.events, vec!["evt-timer".to_string()]);
        assert_eq!(saved.as_of, 2_000);
    }
}
