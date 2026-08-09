//! Deterministic client-supplied ids for locally-created entities (#101).
//!
//! Ports `sweep.py`'s `deterministic_v4`: `sha256(namespace + seed)`, taking
//! the first 16 bytes and forcing the version/variant nibbles into UUID v4
//! shape. The frozen [`NAMESPACE`] is this client's own — distinct from
//! `sweep.py`'s `NAMESPACE` (`hummingbird-sweeper/google-tasks/v1`) and
//! `GMAIL_NAMESPACE` (`hummingbird-sweeper/gmail/v1`) — so the two id spaces
//! stay disjoint by the same hash-domain-separation argument `docs/
//! sweeper.md` already relies on: a fixed namespace prefix on the hash
//! input means no client id can ever equal the sweeper's id for the same
//! seed, and changing this namespace re-mints every id this client has ever
//! minted (frozen by the test vector below, the way the sweeper's is).
use sha2::{Digest, Sha256};

/// Frozen. Never change — every locally-created entity's id derives from
/// this string. Changing it re-mints every id this client has ever minted,
/// duplicating every not-yet-synced local entity against the authority.
const NAMESPACE: &[u8] = b"hummingbird-client/write/v1";

/// A UUID-v4-shaped id derived deterministically from `seed`, so retrying a
/// create after a crash (before the ack) reuses the identical id and lands
/// on the idempotent "already exists" path rather than minting a duplicate.
pub fn deterministic_id(seed: &str) -> String {
    derive(NAMESPACE, seed)
}

fn derive(namespace: &[u8], seed: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(namespace);
    hasher.update(seed.as_bytes());
    let digest = hasher.finalize();

    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0F) | 0x40; // version 4
    bytes[8] = (bytes[8] & 0x3F) | 0x80; // variant 10xx

    let hex = bytes.iter().fold(String::with_capacity(32), |mut out, b| {
        out.push_str(&format!("{b:02x}"));
        out
    });
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The sweeper's two frozen namespaces (`sweep.py`), ported verbatim
    /// only for this disjointness proof — this module never derives an id
    /// with them for real.
    const SWEEPER_TASKS_NAMESPACE: &[u8] = b"hummingbird-sweeper/google-tasks/v1";
    const SWEEPER_GMAIL_NAMESPACE: &[u8] = b"hummingbird-sweeper/gmail/v1";

    /// Frozen test vector: pins `deterministic_id`'s exact output for a
    /// known seed. A change to [`NAMESPACE`] — or to the derivation itself —
    /// breaks this loudly rather than silently re-minting ids.
    #[test]
    fn a_frozen_seed_derives_a_frozen_id() {
        assert_eq!(
            deterministic_id("item:capture-1"),
            derive(NAMESPACE, "item:capture-1"),
        );
        // The literal is the frozen vector: if this assertion ever needs
        // editing, the namespace changed and every existing client id with
        // it.
        assert_eq!(
            deterministic_id("item:capture-1"),
            "62020d07-c6b8-48ff-87ec-1266ad738964"
        );
    }

    #[test]
    fn the_same_seed_always_derives_the_same_id() {
        assert_eq!(
            deterministic_id("item:capture-1"),
            deterministic_id("item:capture-1")
        );
    }

    #[test]
    fn different_seeds_derive_different_ids() {
        assert_ne!(
            deterministic_id("item:capture-1"),
            deterministic_id("item:capture-2")
        );
    }

    #[test]
    fn every_derived_id_is_uuid_v4_shaped() {
        let id = deterministic_id("item:capture-1");
        let segments: Vec<&str> = id.split('-').collect();
        assert_eq!(
            segments.iter().map(|s| s.len()).collect::<Vec<_>>(),
            [8, 4, 4, 4, 12]
        );
        assert_eq!(&segments[2][0..1], "4", "version nibble must be 4");
        assert!(
            matches!(segments[3].chars().next(), Some('8' | '9' | 'a' | 'b')),
            "variant nibble must be 10xx"
        );
    }

    /// The client's namespace is provably disjoint from both of `sweep.py`'s
    /// frozen namespaces: for the identical seed, the three namespaces hash
    /// to three different ids, because the namespace prefix is part of the
    /// SHA-256 preimage. No client id can ever equal a sweeper id derived
    /// from the same source key.
    #[test]
    fn the_client_namespace_is_disjoint_from_both_sweeper_namespaces() {
        let seed = "shared-key";
        let client_id = derive(NAMESPACE, seed);
        let tasks_id = derive(SWEEPER_TASKS_NAMESPACE, seed);
        let gmail_id = derive(SWEEPER_GMAIL_NAMESPACE, seed);

        assert_ne!(client_id, tasks_id);
        assert_ne!(client_id, gmail_id);
        assert_ne!(
            tasks_id, gmail_id,
            "the two sweeper namespaces are themselves disjoint"
        );
    }
}
