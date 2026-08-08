//! In-memory, per-provider credential holder (ADR-0005). The host pushes a
//! token in; this type never serializes and is never reachable from
//! [`crate::storage::save_snapshot`] — there is no `Persistable` impl here,
//! and there never will be.

/// One provider's in-memory token plus whether polling is currently held on
/// a rejected credential.
#[derive(Debug, Clone, Default)]
pub(super) struct CredentialState {
    token: Option<String>,
    held: bool,
}

impl CredentialState {
    /// Records a freshly pushed token and unconditionally resumes polling —
    /// a fresh push is the only way out of a held state.
    pub(super) fn push(&mut self, token: String) {
        self.token = Some(token);
        self.held = false;
    }

    /// The current token, or `None` if either no token has been pushed yet
    /// or polling is held.
    pub(super) fn token(&self) -> Option<&str> {
        if self.held {
            None
        } else {
            self.token.as_deref()
        }
    }

    /// Marks this provider held: an expiry/401 was just observed and no
    /// further poll should be attempted until a fresh token arrives.
    pub(super) fn hold(&mut self) {
        self.held = true;
    }

    pub(super) fn is_held(&self) -> bool {
        self.held
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_token_pushed_yields_no_token() {
        let state = CredentialState::default();
        assert_eq!(state.token(), None);
        assert!(!state.is_held());
    }

    #[test]
    fn a_pushed_token_is_readable() {
        let mut state = CredentialState::default();
        state.push("t-1".to_string());
        assert_eq!(state.token(), Some("t-1"));
    }

    #[test]
    fn holding_hides_the_token_until_a_fresh_push() {
        let mut state = CredentialState::default();
        state.push("t-1".to_string());
        state.hold();
        assert_eq!(state.token(), None);
        assert!(state.is_held());

        state.push("t-2".to_string());
        assert_eq!(state.token(), Some("t-2"));
        assert!(!state.is_held());
    }
}
