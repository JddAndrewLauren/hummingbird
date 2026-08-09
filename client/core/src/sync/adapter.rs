//! [`fetch_delta`] / [`fetch_sweep`]: the owned-API read adapter (#100).
//!
//! Deserialisation plus validation over `hummingbird_domain::ChangesResponse`
//! — there is no mapping table, because there is no foreign shape (the
//! domain types are the shared crate both sides compile). Complete-or-
//! nothing: a transport failure or a malformed body yields an error, never
//! a partial [`ChangesResponse`] for a caller to apply.

use std::fmt;

use hummingbird_domain::ChangesResponse;

use super::transport::{ChangesTransport, TransportError};

#[derive(Debug)]
pub enum AdapterError {
    Transport(TransportError),
    /// The body was not valid JSON, or not a `ChangesResponse` — a
    /// malformed response is never partially applied.
    InvalidResponse(String),
}

impl fmt::Display for AdapterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AdapterError::Transport(source) => write!(f, "transport error: {source}"),
            AdapterError::InvalidResponse(message) => write!(f, "invalid response: {message}"),
        }
    }
}

impl std::error::Error for AdapterError {}

impl AdapterError {
    /// Whether the authority rejected the credential (HTTP 401) rather than
    /// anything transient. A malformed body is never unauthorized, however
    /// it was provoked.
    pub fn is_unauthorized(&self) -> bool {
        match self {
            AdapterError::Transport(source) => source.is_unauthorized(),
            AdapterError::InvalidResponse(_) => false,
        }
    }
}

fn parse(body: &str) -> Result<ChangesResponse, AdapterError> {
    serde_json::from_str(body).map_err(|source| AdapterError::InvalidResponse(source.to_string()))
}

/// `GET /api/changes?since={since}` — the steady-state delta pull.
pub async fn fetch_delta(
    transport: &impl ChangesTransport,
    access_token: &str,
    since: i64,
) -> Result<ChangesResponse, AdapterError> {
    let body = transport
        .fetch_changes(access_token, since)
        .await
        .map_err(AdapterError::Transport)?;
    parse(&body)
}

/// `GET /api/sweep` — the correctness backstop: the entire workspace.
pub async fn fetch_sweep(
    transport: &impl ChangesTransport,
    access_token: &str,
) -> Result<ChangesResponse, AdapterError> {
    let body = transport
        .fetch_sweep(access_token)
        .await
        .map_err(AdapterError::Transport)?;
    parse(&body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// A transport scripted to return one fixed result per call, so a
    /// fixture test never touches the network or a live credential.
    struct ScriptedTransport {
        changes: Mutex<Option<Result<String, TransportError>>>,
        sweep: Mutex<Option<Result<String, TransportError>>>,
    }

    impl ScriptedTransport {
        fn changes_only(result: Result<String, TransportError>) -> Self {
            Self {
                changes: Mutex::new(Some(result)),
                sweep: Mutex::new(None),
            }
        }

        fn sweep_only(result: Result<String, TransportError>) -> Self {
            Self {
                changes: Mutex::new(None),
                sweep: Mutex::new(Some(result)),
            }
        }
    }

    #[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
    #[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
    impl ChangesTransport for ScriptedTransport {
        async fn fetch_changes(
            &self,
            _access_token: &str,
            _since: i64,
        ) -> Result<String, TransportError> {
            self.changes
                .lock()
                .unwrap()
                .take()
                .expect("fetch_changes called but no script was set")
        }

        async fn fetch_sweep(&self, _access_token: &str) -> Result<String, TransportError> {
            self.sweep
                .lock()
                .unwrap()
                .take()
                .expect("fetch_sweep called but no script was set")
        }
    }

    fn empty_body(version: i64) -> String {
        serde_json::to_string(&ChangesResponse::empty(version)).unwrap()
    }

    #[tokio::test]
    async fn a_valid_body_deserialises_into_a_changes_response() {
        let transport = ScriptedTransport::changes_only(Ok(empty_body(7)));

        let resp = fetch_delta(&transport, "token", 0).await.unwrap();

        assert_eq!(resp.version, 7);
        assert!(resp.items.is_empty());
    }

    #[tokio::test]
    async fn a_transport_failure_yields_no_response_at_all() {
        let transport =
            ScriptedTransport::changes_only(Err(TransportError::new("502 from the authority")));

        let result = fetch_delta(&transport, "token", 0).await;

        assert!(matches!(result, Err(AdapterError::Transport(_))));
    }

    #[tokio::test]
    async fn a_401_is_classified_as_unauthorized_not_transient() {
        let transport =
            ScriptedTransport::changes_only(Err(TransportError::http(401, "bad token")));

        let err = fetch_delta(&transport, "token", 0).await.unwrap_err();

        assert!(err.is_unauthorized());
    }

    #[tokio::test]
    async fn a_500_is_not_classified_as_unauthorized() {
        let transport = ScriptedTransport::changes_only(Err(TransportError::http(500, "oops")));

        let err = fetch_delta(&transport, "token", 0).await.unwrap_err();

        assert!(!err.is_unauthorized());
    }

    #[tokio::test]
    async fn a_malformed_body_is_an_invalid_response_not_a_panic() {
        let transport = ScriptedTransport::changes_only(Ok("not json".to_string()));

        let result = fetch_delta(&transport, "token", 0).await;

        assert!(matches!(result, Err(AdapterError::InvalidResponse(_))));
    }

    #[tokio::test]
    async fn fetch_sweep_goes_through_the_sweep_endpoint_not_changes() {
        let transport = ScriptedTransport::sweep_only(Ok(empty_body(3)));

        let resp = fetch_sweep(&transport, "token").await.unwrap();

        assert_eq!(resp.version, 3);
    }

    /// #100 acceptance: "an entity carrying an unmodelled field survives
    /// deserialisation" — the domain entity types have no
    /// `deny_unknown_fields`, so a server that has grown a column the
    /// client doesn't know about yet must not fail the whole pull over it.
    #[tokio::test]
    async fn an_item_with_an_unmodelled_field_still_deserialises() {
        let body = r#"{
            "version": 1,
            "projects": [], "routes": [], "fog": [],
            "items": [{
                "id": "a-1", "seq": 1, "title": "hi", "description": null,
                "stage": "triage", "size": null, "energy": null, "context": null,
                "priority": 0, "project_id": null, "project_pos": null,
                "due_date": null, "scheduled_date": null, "source": null,
                "source_key": null, "source_url": null, "archived_at": null,
                "created_at": 1, "updated_at": 1, "version": 1,
                "some_future_field": "the client has never heard of this"
            }],
            "steps": [], "blocked_by": [], "alerts": [],
            "context_snapshots": [], "settings": []
        }"#;
        let transport = ScriptedTransport::changes_only(Ok(body.to_string()));

        let resp = fetch_delta(&transport, "token", 0).await.unwrap();

        assert_eq!(resp.items.len(), 1);
        assert_eq!(resp.items[0].id, "a-1");
    }
}
