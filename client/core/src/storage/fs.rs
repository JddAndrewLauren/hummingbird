//! `std::fs` [`SnapshotStore`]: write-temp-then-rename for atomicity
//! (ADR-0003). Never runs as the persistence leg on Windows — a
//! dev-environment note in ADR-0003 — so the in-memory store stays the
//! cross-platform test default; this impl is exercised on macOS/Linux hosts.

use super::SnapshotStore;
use std::io;
use std::path::PathBuf;

/// Stores one snapshot as a file at `path`, publishing writes atomically:
/// the new snapshot is written to a sibling temp file first, then published
/// with a single `rename`. A reader never observes a partially written
/// snapshot, and a crash between the two steps leaves the previous snapshot
/// exactly as it was.
#[derive(Debug, Clone)]
pub struct FsSnapshotStore {
    path: PathBuf,
}

impl FsSnapshotStore {
    /// Stores snapshots at `path`. The host contributes this path at init
    /// (ADR-0003); `core` never invents storage locations on its own.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// The sibling temp file writes land in before being published.
    fn tmp_path(&self) -> PathBuf {
        self.path.with_extension("tmp")
    }
}

impl SnapshotStore for FsSnapshotStore {
    type Error = io::Error;

    async fn write(&self, bytes: Vec<u8>) -> Result<(), io::Error> {
        let tmp = self.tmp_path();
        std::fs::write(&tmp, &bytes)?;
        std::fs::rename(&tmp, &self.path)?;
        Ok(())
    }

    async fn read(&self) -> Result<Option<Vec<u8>>, io::Error> {
        match std::fs::read(&self.path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(err),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store_in(dir: &tempfile::TempDir) -> FsSnapshotStore {
        FsSnapshotStore::new(dir.path().join("snapshot.json"))
    }

    #[tokio::test]
    async fn read_before_any_write_is_none() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_in(&dir);
        assert_eq!(store.read().await.unwrap(), None);
    }

    #[tokio::test]
    async fn write_then_read_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_in(&dir);
        store.write(b"hello".to_vec()).await.unwrap();
        assert_eq!(store.read().await.unwrap(), Some(b"hello".to_vec()));
    }

    #[tokio::test]
    async fn write_fully_replaces_the_previous_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_in(&dir);
        store.write(b"first".to_vec()).await.unwrap();
        store.write(b"second".to_vec()).await.unwrap();
        assert_eq!(store.read().await.unwrap(), Some(b"second".to_vec()));
    }

    /// Fault injection: a crash between "write the temp file" and "rename it
    /// into place" must leave the previous, fully-written snapshot intact.
    #[tokio::test]
    async fn previous_snapshot_survives_a_torn_write() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_in(&dir);

        store.write(b"snapshot A".to_vec()).await.unwrap();

        // Simulate a crash mid-write: bytes land in the temp file (as a
        // real write does before its `rename`), but the rename that
        // publishes them never happens.
        std::fs::write(store.tmp_path(), b"torn garbage, never published").unwrap();

        assert_eq!(
            store.read().await.unwrap(),
            Some(b"snapshot A".to_vec()),
            "a torn write to the temp file must not corrupt the published snapshot"
        );

        // A subsequent real write still succeeds and publishes cleanly,
        // proving the store recovers rather than getting stuck.
        store.write(b"snapshot B".to_vec()).await.unwrap();
        assert_eq!(store.read().await.unwrap(), Some(b"snapshot B".to_vec()));
    }
}
