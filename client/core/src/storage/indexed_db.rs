//! IndexedDB [`SnapshotStore`] (ADR-0003, `wasm32` target only).
//!
//! One fixed object store holding one fixed key: this store only ever needs
//! to hold a single envelope. The JSON envelope bytes are UTF-8, so they are
//! stored as a JS string rather than a byte array — simplest primitive path
//! this crate offers, and it keeps the on-disk/in-browser representation
//! human-readable, same as the `std::fs` leg.

use super::SnapshotStore;
use indexed_db_futures::database::Database;
use indexed_db_futures::prelude::*;
use indexed_db_futures::transaction::TransactionMode;

const OBJECT_STORE: &str = "snapshot";
const KEY: &str = "envelope";

/// Errors from the IndexedDB medium: opening/upgrading the database, or a
/// transaction/request failure.
#[derive(Debug)]
pub enum IndexedDbError {
    /// Failed to open or upgrade the database.
    Open(indexed_db_futures::error::OpenDbError),
    /// A transaction or request failed.
    Op(indexed_db_futures::error::Error),
    /// The stored bytes were not valid UTF-8 (should be unreachable: this
    /// store only ever writes `serde_json` output).
    NotUtf8(std::string::FromUtf8Error),
}

impl From<indexed_db_futures::error::OpenDbError> for IndexedDbError {
    fn from(err: indexed_db_futures::error::OpenDbError) -> Self {
        Self::Open(err)
    }
}

impl From<indexed_db_futures::error::Error> for IndexedDbError {
    fn from(err: indexed_db_futures::error::Error) -> Self {
        Self::Op(err)
    }
}

impl From<std::string::FromUtf8Error> for IndexedDbError {
    fn from(err: std::string::FromUtf8Error) -> Self {
        Self::NotUtf8(err)
    }
}

/// Stores one snapshot per IndexedDB database. The host-supplied namespace
/// (ADR-0003: the host contributes exactly one thing at init — a storage
/// path/namespace) becomes the IndexedDB database name.
#[derive(Debug, Clone)]
pub struct IndexedDbSnapshotStore {
    db_name: String,
}

impl IndexedDbSnapshotStore {
    /// Stores snapshots in the IndexedDB database named `namespace`.
    pub fn new(namespace: impl Into<String>) -> Self {
        Self {
            db_name: namespace.into(),
        }
    }

    async fn open(&self) -> Result<Database, IndexedDbError> {
        let db = Database::open(self.db_name.clone())
            .with_version(1u8)
            .with_on_upgrade_needed(|_event, db| {
                if !db.object_store_names().any(|name| name == OBJECT_STORE) {
                    db.create_object_store(OBJECT_STORE).build()?;
                }
                Ok(())
            })
            .await?;
        Ok(db)
    }
}

impl SnapshotStore for IndexedDbSnapshotStore {
    type Error = IndexedDbError;

    async fn write(&self, bytes: Vec<u8>) -> Result<(), IndexedDbError> {
        let text = String::from_utf8(bytes)?;
        let db = self.open().await?;
        let tx = db
            .transaction(OBJECT_STORE)
            .with_mode(TransactionMode::Readwrite)
            .build()?;
        let store = tx.object_store(OBJECT_STORE)?;
        store.put(text).with_key(KEY).primitive()?.await?;
        tx.commit().await?;
        Ok(())
    }

    async fn read(&self) -> Result<Option<Vec<u8>>, IndexedDbError> {
        let db = self.open().await?;
        let tx = db.transaction(OBJECT_STORE).build()?;
        let store = tx.object_store(OBJECT_STORE)?;
        let value: Option<String> = store.get(KEY).primitive()?.await?;
        Ok(value.map(String::into_bytes))
    }
}
