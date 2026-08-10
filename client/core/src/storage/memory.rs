//! In-memory [`SnapshotStore`] — the cross-platform test default (ADR-0003).

use super::sealed::Sealed;
use std::convert::Infallible;
use std::sync::Mutex;

/// Holds the current snapshot bytes in memory. No device, no browser: this
/// is what makes the whole core testable, and it's the default `core` tests
/// run against.
#[derive(Debug, Default)]
pub struct MemorySnapshotStore {
    inner: Mutex<Option<Vec<u8>>>,
}

impl Sealed for MemorySnapshotStore {
    type Error = Infallible;

    async fn write(&self, bytes: Vec<u8>) -> Result<(), Infallible> {
        *self.inner.lock().expect("snapshot mutex poisoned") = Some(bytes);
        Ok(())
    }

    async fn read(&self) -> Result<Option<Vec<u8>>, Infallible> {
        Ok(self.inner.lock().expect("snapshot mutex poisoned").clone())
    }
}

/// The failure a [`InstrumentedSnapshotStore`] is asked to produce — a
/// distinct type rather than a `String` so a test can never mistake it for
/// a real medium error.
#[cfg(test)]
#[derive(Debug)]
pub struct SimulatedWriteFailure;

/// [`MemorySnapshotStore`] with two instruments bolted on: `write` can be
/// made to fail on demand, and every write's byte count is recorded.
///
/// Both exist because [`MemorySnapshotStore::Error`] is [`Infallible`], so
/// the `Err` arm of every `save_snapshot` call site in `core` — including
/// both branches that produce
/// [`crate::sync::CycleOutcome::PersistFailed`] — was unreachable and
/// therefore untested. The write/byte counters are the same instrument the
/// write-amplification question (#95) needs: how many times, and how many
/// bytes, one cycle re-serialises.
///
/// `#[cfg(test)]` and `core`-internal by necessity: [`super::sealed::Sealed`]
/// is unnameable outside this crate, so an integration test or a benchmark
/// could not implement one.
#[cfg(test)]
#[derive(Debug, Default)]
pub struct InstrumentedSnapshotStore {
    inner: Mutex<Option<Vec<u8>>>,
    failing: Mutex<bool>,
    write_sizes: Mutex<Vec<usize>>,
}

#[cfg(test)]
impl InstrumentedSnapshotStore {
    /// A store that accepts writes, counting them.
    pub fn new() -> Self {
        Self::default()
    }

    /// A store whose every write fails from the outset — for the "this
    /// state was never durable" cases.
    pub fn failing() -> Self {
        Self {
            failing: Mutex::new(true),
            ..Self::default()
        }
    }

    /// Starts (or stops) failing every subsequent write, so one test can
    /// seed durable state and *then* break the store.
    pub fn set_failing(&self, failing: bool) {
        *self.failing.lock().expect("snapshot mutex poisoned") = failing;
    }

    /// How many `write` calls this store saw — failed ones included, since
    /// an attempted re-serialisation costs the same either way.
    pub fn write_count(&self) -> usize {
        self.write_sizes.lock().expect("snapshot mutex poisoned").len()
    }

    /// Every write's payload size, in call order.
    pub fn write_sizes(&self) -> Vec<usize> {
        self.write_sizes.lock().expect("snapshot mutex poisoned").clone()
    }

    /// Total bytes handed to `write` across this store's life.
    pub fn bytes_written(&self) -> usize {
        self.write_sizes
            .lock()
            .expect("snapshot mutex poisoned")
            .iter()
            .sum()
    }
}

#[cfg(test)]
impl Sealed for InstrumentedSnapshotStore {
    type Error = SimulatedWriteFailure;

    async fn write(&self, bytes: Vec<u8>) -> Result<(), SimulatedWriteFailure> {
        self.write_sizes
            .lock()
            .expect("snapshot mutex poisoned")
            .push(bytes.len());
        if *self.failing.lock().expect("snapshot mutex poisoned") {
            return Err(SimulatedWriteFailure);
        }
        *self.inner.lock().expect("snapshot mutex poisoned") = Some(bytes);
        Ok(())
    }

    async fn read(&self) -> Result<Option<Vec<u8>>, SimulatedWriteFailure> {
        Ok(self.inner.lock().expect("snapshot mutex poisoned").clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn read_before_any_write_is_none() {
        let store = MemorySnapshotStore::default();
        assert_eq!(store.read().await.unwrap(), None);
    }

    #[tokio::test]
    async fn write_then_read_round_trips() {
        let store = MemorySnapshotStore::default();
        store.write(b"hello".to_vec()).await.unwrap();
        assert_eq!(store.read().await.unwrap(), Some(b"hello".to_vec()));
    }

    #[tokio::test]
    async fn write_fully_replaces_the_previous_snapshot() {
        let store = MemorySnapshotStore::default();
        store.write(b"first".to_vec()).await.unwrap();
        store.write(b"second".to_vec()).await.unwrap();
        assert_eq!(store.read().await.unwrap(), Some(b"second".to_vec()));
    }

    /// A failing write must leave the previously published snapshot intact
    /// — the same atomicity every real store promises, so a test using this
    /// fake is exercising the real failure shape, not a weaker one.
    #[tokio::test]
    async fn an_instrumented_failure_leaves_the_previous_snapshot_intact() {
        let store = InstrumentedSnapshotStore::new();
        store.write(b"first".to_vec()).await.unwrap();

        store.set_failing(true);
        assert!(store.write(b"second".to_vec()).await.is_err());

        assert_eq!(store.read().await.unwrap(), Some(b"first".to_vec()));
    }

    /// The counters record every attempt and its size, failed writes
    /// included — an attempted re-serialisation costs the same either way.
    #[tokio::test]
    async fn the_instrumented_store_counts_every_write_and_its_bytes() {
        let store = InstrumentedSnapshotStore::new();
        store.write(b"abc".to_vec()).await.unwrap();
        store.set_failing(true);
        let _ = store.write(b"defg".to_vec()).await;

        assert_eq!(store.write_count(), 2);
        assert_eq!(store.write_sizes(), vec![3, 4]);
        assert_eq!(store.bytes_written(), 7);
    }
}
