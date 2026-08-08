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
}
