//! [`create`] / [`patch_with_rebase`]: the owned-API write adapter (#101).
//!
//! Generic over any of `hummingbird_domain`'s create/patch DTOs and entity
//! types — there is no per-entity copy of the CAS contract, because the
//! contract itself (absolute sets, `expected_version`, 409-carries-current)
//! is identical across items, steps, blocked_by, projects, routes, fog, and
//! settings.

use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;

use super::rebase::{self, RebaseDecision};
use super::taxonomy::{classify_status, WriteError};
use super::transport::{HttpMethod, MutationRequest, MutationTransport};

/// A create's outcome: both branches are success (ADR-0008: a duplicate-id
/// create is idempotent, never a failure), but which happened is worth
/// keeping — `Created` bumped the workspace version, `AlreadyExisted`
/// (the server's 200) did not.
#[derive(Debug, Clone, PartialEq)]
pub enum CreateOutcome<T> {
    Created(T),
    AlreadyExisted(T),
}

impl<T> CreateOutcome<T> {
    pub fn into_inner(self) -> T {
        match self {
            CreateOutcome::Created(v) | CreateOutcome::AlreadyExisted(v) => v,
        }
    }
}

/// What one round trip resolved to, before the caller (create vs.
/// patch-with-rebase) decides what it means.
enum Sent {
    Success(u16, String),
    Conflict(Value),
    Failed(WriteError),
}

async fn send(
    transport: &impl MutationTransport,
    access_token: &str,
    method: HttpMethod,
    path: &str,
    body: String,
) -> Sent {
    match transport
        .send(
            access_token,
            MutationRequest {
                method,
                path: path.to_string(),
                body,
            },
        )
        .await
    {
        // A well-behaved `MutationTransport` never carries a status on this
        // branch (write::transport's own doc: status lives on `RawResponse`,
        // this is connection-level only) — but the outbound queue (#102)
        // holds the *entire* queue on `WriteError::Unauthorized` and only
        // blocks the current entry on `Retryable`, so a transport that ever
        // did tag a dead-credential failure this way must not be silently
        // downgraded to "retry forever" instead of "ask for a fresh token".
        Err(source) if source.is_unauthorized() => Sent::Failed(WriteError::Unauthorized),
        Err(source) => Sent::Failed(WriteError::Retryable(source.to_string())),
        Ok(response) if (200..300).contains(&response.status) => {
            Sent::Success(response.status, response.body)
        }
        Ok(response) if response.status == 409 => {
            match serde_json::from_str::<Value>(&response.body) {
                Ok(body) => Sent::Conflict(body.get("current").cloned().unwrap_or(Value::Null)),
                Err(source) => Sent::Failed(WriteError::InvalidResponse(source.to_string())),
            }
        }
        Ok(response) => Sent::Failed(classify_status(response.status)),
    }
}

fn parse<T: DeserializeOwned>(body: &str) -> Result<T, WriteError> {
    serde_json::from_str(body).map_err(|source| WriteError::InvalidResponse(source.to_string()))
}

/// `POST` a create DTO. Idempotent by the DTO's client-supplied id: a
/// duplicate lands on the 200 branch, which is success, not an error.
pub async fn create<C, T>(
    transport: &impl MutationTransport,
    access_token: &str,
    path: &str,
    create: &C,
) -> Result<CreateOutcome<T>, WriteError>
where
    C: Serialize,
    T: DeserializeOwned,
{
    let body = serde_json::to_string(create)
        .map_err(|source| WriteError::InvalidResponse(source.to_string()))?;
    match send(transport, access_token, HttpMethod::Post, path, body).await {
        Sent::Success(201, body) => Ok(CreateOutcome::Created(parse(&body)?)),
        Sent::Success(_, body) => Ok(CreateOutcome::AlreadyExisted(parse(&body)?)),
        // Creates carry no `expected_version`, so a 409 here is not this
        // adapter's contract — surface it as a conflict anyway (with no
        // named fields) rather than lose it, but this path is not expected
        // to be exercised against the real authority.
        Sent::Conflict(current) => Err(WriteError::Conflict {
            fields: Vec::new(),
            current,
        }),
        Sent::Failed(error) => Err(error),
    }
}

/// `PATCH`/`PUT` a CAS write, resolving a 409 automatically when it is safe
/// (see [`rebase::decide`]) by reissuing the identical touched-field intent
/// at the current version — exactly once. A second 409 on that retry, or a
/// same-field collision on the first, is reported as a
/// [`WriteError::Conflict`] naming every colliding field.
///
/// `base` is the entity this client last knew, as JSON (`serde_json::to_
/// value` of whatever `hummingbird_domain` entity the caller holds).
/// `build_patch` receives the `expected_version` to use — called once for
/// the original attempt, and again (with the version the 409 carried) only
/// if the rebase is safe.
pub async fn patch_with_rebase<P, T>(
    transport: &impl MutationTransport,
    access_token: &str,
    method: HttpMethod,
    path: &str,
    base: &Value,
    mut build_patch: impl FnMut(i64) -> P,
) -> Result<T, WriteError>
where
    P: Serialize,
    T: DeserializeOwned,
{
    let Some(base_version) = base.get("version").and_then(Value::as_i64) else {
        // #103 forwarded-review fix: a `base` with no numeric `version` is a
        // malformed local record, not a legitimate "start from zero" — this
        // client's own captured base always carries a version. Silently
        // defaulting to `0` would send a CAS write against a fabricated
        // expected_version, hiding whatever produced the malformed base
        // behind a spurious 409 instead of surfacing it here.
        return Err(WriteError::InvalidResponse(
            "base has no numeric \"version\" field".to_string(),
        ));
    };

    let patch = build_patch(base_version);
    let patch_value = serde_json::to_value(&patch)
        .map_err(|source| WriteError::InvalidResponse(source.to_string()))?;
    let body = serde_json::to_string(&patch)
        .map_err(|source| WriteError::InvalidResponse(source.to_string()))?;

    match send(transport, access_token, method, path, body).await {
        Sent::Success(_, body) => parse(&body),
        Sent::Failed(error) => Err(error),
        Sent::Conflict(current) => match rebase::decide(&patch_value, base, &current) {
            RebaseDecision::Collision(fields) => Err(WriteError::Conflict { fields, current }),
            RebaseDecision::Safe => {
                // #103 forwarded-review fix: a `current` with no numeric
                // `version` is a malformed 409 body — the server's own
                // contract guarantees `current.version` on every conflict.
                // Silently falling back to `base_version` would resend the
                // exact version this client already knows is stale, hiding
                // the malformed body behind a second, spurious 409 instead
                // of surfacing it here.
                let Some(current_version) = current.get("version").and_then(Value::as_i64) else {
                    return Err(WriteError::InvalidResponse(
                        "409 conflict body has no numeric \"version\" field".to_string(),
                    ));
                };
                let retry_patch = build_patch(current_version);
                let retry_value = serde_json::to_value(&retry_patch)
                    .map_err(|source| WriteError::InvalidResponse(source.to_string()))?;
                let retry_body = serde_json::to_string(&retry_patch)
                    .map_err(|source| WriteError::InvalidResponse(source.to_string()))?;

                match send(transport, access_token, method, path, retry_body).await {
                    Sent::Success(_, body) => parse(&body),
                    Sent::Failed(error) => Err(error),
                    Sent::Conflict(current2) => {
                        // #103 forwarded-review fix: diff against `current`
                        // — the point this retry was actually rebased onto
                        // — never the original `base` two versions back. See
                        // `a_second_conflict_is_diffed_against_the_rebased_onto_current_not_the_original_base`.
                        let fields = match rebase::decide(&retry_value, &current, &current2) {
                            RebaseDecision::Collision(fields) => fields,
                            RebaseDecision::Safe => Vec::new(),
                        };
                        Err(WriteError::Conflict {
                            fields,
                            current: current2,
                        })
                    }
                }
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::write::transport::{RawResponse, TransportError};
    use serde_json::json;
    use std::sync::Mutex;

    /// A transport scripted to return one response per call, in order —
    /// so a fixture test never touches the network or a live credential.
    /// Every fixture payload below is a committed literal, one per
    /// taxonomy branch.
    struct ScriptedTransport {
        responses: Mutex<Vec<Result<RawResponse, TransportError>>>,
        calls: Mutex<Vec<MutationRequest>>,
    }

    impl ScriptedTransport {
        fn new(responses: Vec<Result<RawResponse, TransportError>>) -> Self {
            Self {
                responses: Mutex::new(responses),
                calls: Mutex::new(Vec::new()),
            }
        }

        fn call_count(&self) -> usize {
            self.calls.lock().unwrap().len()
        }
    }

    fn ok(status: u16, body: impl Into<String>) -> Result<RawResponse, TransportError> {
        Ok(RawResponse {
            status,
            body: body.into(),
        })
    }

    #[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
    #[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
    impl MutationTransport for ScriptedTransport {
        async fn send(
            &self,
            _access_token: &str,
            request: MutationRequest,
        ) -> Result<RawResponse, TransportError> {
            self.calls.lock().unwrap().push(request);
            self.responses
                .lock()
                .unwrap()
                .pop()
                .unwrap_or_else(|| panic!("no more scripted responses"))
        }
    }

    #[derive(Debug, serde::Serialize, serde::Deserialize, PartialEq)]
    struct FakeItem {
        id: String,
        title: String,
        version: i64,
    }

    #[derive(Debug, serde::Serialize)]
    struct FakeCreate {
        id: String,
        title: String,
    }

    // ---------------------------------------------------------- create()

    /// Fixture: the authority's `201 Created` body for a brand-new item.
    const CREATED_ITEM: &str = r#"{"id":"a-1","title":"buy milk","version":1}"#;
    /// Fixture: the authority's `200 OK` body for a duplicate-id create —
    /// the stored row, unchanged.
    const EXISTING_ITEM: &str = r#"{"id":"a-1","title":"buy milk","version":1}"#;

    #[tokio::test]
    async fn a_201_is_a_created_outcome() {
        let transport = ScriptedTransport::new(vec![ok(201, CREATED_ITEM)]);
        let create_dto = FakeCreate {
            id: "a-1".into(),
            title: "buy milk".into(),
        };

        let outcome: CreateOutcome<FakeItem> =
            create(&transport, "token", "/api/items", &create_dto)
                .await
                .unwrap();

        assert!(matches!(outcome, CreateOutcome::Created(_)));
    }

    /// #101 acceptance: "a duplicate-id create is classified as success."
    #[tokio::test]
    async fn a_200_on_a_duplicate_id_is_success_not_failure() {
        let transport = ScriptedTransport::new(vec![ok(200, EXISTING_ITEM)]);
        let create_dto = FakeCreate {
            id: "a-1".into(),
            title: "buy milk".into(),
        };

        let outcome: CreateOutcome<FakeItem> =
            create(&transport, "token", "/api/items", &create_dto)
                .await
                .unwrap();

        assert!(matches!(outcome, CreateOutcome::AlreadyExisted(_)));
    }

    /// #101 acceptance: "replaying any mutation twice leaves the same end
    /// state" — for a create, two identical requests both succeed and both
    /// carry the identical stored row.
    #[tokio::test]
    async fn replaying_a_create_twice_leaves_the_same_row() {
        let transport = ScriptedTransport::new(vec![ok(200, EXISTING_ITEM), ok(201, CREATED_ITEM)]);
        let create_dto = FakeCreate {
            id: "a-1".into(),
            title: "buy milk".into(),
        };

        let first: CreateOutcome<FakeItem> = create(&transport, "token", "/api/items", &create_dto)
            .await
            .unwrap();
        let second: CreateOutcome<FakeItem> =
            create(&transport, "token", "/api/items", &create_dto)
                .await
                .unwrap();

        assert_eq!(first.into_inner(), second.into_inner());
    }

    #[tokio::test]
    async fn a_401_on_create_is_unauthorized() {
        let transport = ScriptedTransport::new(vec![ok(401, "")]);
        let create_dto = FakeCreate {
            id: "a-1".into(),
            title: "buy milk".into(),
        };

        let err = create::<_, FakeItem>(&transport, "token", "/api/items", &create_dto)
            .await
            .unwrap_err();

        assert_eq!(err, WriteError::Unauthorized);
    }

    /// Fixture: a validation failure — the authority's real `ApiError`
    /// shape.
    const VALIDATION_ERROR: &str = r#"{"error":"validation","message":"title must be non-empty"}"#;

    #[tokio::test]
    async fn a_400_on_create_is_permanent() {
        let transport = ScriptedTransport::new(vec![ok(400, VALIDATION_ERROR)]);
        let create_dto = FakeCreate {
            id: "a-1".into(),
            title: "".into(),
        };

        let err = create::<_, FakeItem>(&transport, "token", "/api/items", &create_dto)
            .await
            .unwrap_err();

        assert!(matches!(err, WriteError::Permanent(_)));
    }

    #[tokio::test]
    async fn a_503_on_create_is_retryable() {
        let transport = ScriptedTransport::new(vec![ok(503, "")]);
        let create_dto = FakeCreate {
            id: "a-1".into(),
            title: "buy milk".into(),
        };

        let err = create::<_, FakeItem>(&transport, "token", "/api/items", &create_dto)
            .await
            .unwrap_err();

        assert!(matches!(err, WriteError::Retryable(_)));
    }

    #[tokio::test]
    async fn a_network_failure_on_create_is_retryable() {
        let transport = ScriptedTransport::new(vec![Err(TransportError::new("connection reset"))]);
        let create_dto = FakeCreate {
            id: "a-1".into(),
            title: "buy milk".into(),
        };

        let err = create::<_, FakeItem>(&transport, "token", "/api/items", &create_dto)
            .await
            .unwrap_err();

        assert!(matches!(err, WriteError::Retryable(_)));
    }

    /// A connection-level failure is not always status-free: a transport
    /// that tags a dead credential this way (rather than the documented
    /// `RawResponse{status: 401, ..}` path) must still surface
    /// `Unauthorized`, not `Retryable` — #102's outbound queue holds the
    /// whole queue on the former and only blocks the current entry on the
    /// latter, so confusing the two would silently turn "ask for a fresh
    /// token" into "retry forever".
    #[tokio::test]
    async fn a_connection_failure_carrying_a_401_status_is_unauthorized_not_retryable() {
        let transport = ScriptedTransport::new(vec![Err(TransportError::http(401, "expired"))]);
        let create_dto = FakeCreate {
            id: "a-1".into(),
            title: "buy milk".into(),
        };

        let err = create::<_, FakeItem>(&transport, "token", "/api/items", &create_dto)
            .await
            .unwrap_err();

        assert_eq!(err, WriteError::Unauthorized);
    }

    #[tokio::test]
    async fn a_429_on_create_is_retryable() {
        let transport = ScriptedTransport::new(vec![ok(429, "")]);
        let create_dto = FakeCreate {
            id: "a-1".into(),
            title: "buy milk".into(),
        };

        let err = create::<_, FakeItem>(&transport, "token", "/api/items", &create_dto)
            .await
            .unwrap_err();

        assert!(matches!(err, WriteError::Retryable(_)));
    }

    // ------------------------------------------------ patch_with_rebase()

    /// Fixture: the direct success case — the patch applies at the expected
    /// version, no conflict at all.
    const PATCHED_ITEM: &str = r#"{"id":"a-1","title":"buy milk","version":2}"#;

    #[tokio::test]
    async fn a_clean_patch_applies_with_no_conflict() {
        let transport = ScriptedTransport::new(vec![ok(200, PATCHED_ITEM)]);
        let base = json!({"id": "a-1", "title": "buy milk", "version": 1});

        let result: FakeItem = patch_with_rebase(
            &transport,
            "token",
            HttpMethod::Patch,
            "/api/items/a-1",
            &base,
            |v| json!({"expected_version": v, "title": "buy milk"}),
        )
        .await
        .unwrap();

        assert_eq!(result.version, 2);
        assert_eq!(transport.call_count(), 1);
    }

    /// #101 acceptance: "a 409 on disjoint fields rebases onto the returned
    /// entity and succeeds on one retry." Fixtures: the 409's conflict body
    /// (someone else bumped `context`, a field this patch never touches),
    /// then the retry's success body.
    #[tokio::test]
    async fn a_409_on_a_disjoint_field_rebases_and_succeeds() {
        let conflict_body = r#"{"error":"version_conflict","current":{"id":"a-1","title":"buy milk","context":"@computer","version":2}}"#;
        let retry_success =
            r#"{"id":"a-1","title":"buy oat milk","context":"@computer","version":3}"#;
        // Scripts pop from the back, so list them last-first.
        let transport =
            ScriptedTransport::new(vec![ok(200, retry_success), ok(409, conflict_body)]);
        let base = json!({"id": "a-1", "title": "buy milk", "context": "@calls", "version": 1});

        let result: FakeItem = patch_with_rebase(
            &transport,
            "token",
            HttpMethod::Patch,
            "/api/items/a-1",
            &base,
            |v| json!({"expected_version": v, "title": "buy oat milk"}),
        )
        .await
        .unwrap();

        assert_eq!(result.title, "buy oat milk");
        assert_eq!(result.version, 3);
        assert_eq!(
            transport.call_count(),
            2,
            "one original attempt plus one rebased retry"
        );
    }

    /// #101 acceptance: "a 409 on the same field is reported as a conflict
    /// with the colliding fields named." Fixture: the intervening write set
    /// `title` to something else entirely.
    #[tokio::test]
    async fn a_409_on_the_same_field_names_the_collision() {
        let conflict_body = r#"{"error":"version_conflict","current":{"id":"a-1","title":"someone else's title","version":2}}"#;
        let transport = ScriptedTransport::new(vec![ok(409, conflict_body)]);
        let base = json!({"id": "a-1", "title": "buy milk", "version": 1});

        let err = patch_with_rebase::<_, FakeItem>(
            &transport,
            "token",
            HttpMethod::Patch,
            "/api/items/a-1",
            &base,
            |v| json!({"expected_version": v, "title": "buy oat milk"}),
        )
        .await
        .unwrap_err();

        match err {
            WriteError::Conflict { fields, .. } => assert_eq!(fields, vec!["title".to_string()]),
            other => panic!("expected a named collision, got {other:?}"),
        }
        assert_eq!(
            transport.call_count(),
            1,
            "a same-field collision never retries"
        );
    }

    /// #101 acceptance: "replaying any mutation twice leaves the same end
    /// state" — a crash swallows the ack, the client replays the identical
    /// patch against the identical (now stale) base. The retry lands on
    /// the value it already set.
    #[tokio::test]
    async fn replaying_a_patch_after_a_swallowed_ack_converges() {
        let conflict_body = r#"{"error":"version_conflict","current":{"id":"a-1","title":"buy oat milk","version":2}}"#;
        let retry_success = r#"{"id":"a-1","title":"buy oat milk","version":3}"#;
        let transport =
            ScriptedTransport::new(vec![ok(200, retry_success), ok(409, conflict_body)]);
        let base = json!({"id": "a-1", "title": "buy milk", "version": 1});

        let result: FakeItem = patch_with_rebase(
            &transport,
            "token",
            HttpMethod::Patch,
            "/api/items/a-1",
            &base,
            |v| json!({"expected_version": v, "title": "buy oat milk"}),
        )
        .await
        .unwrap();

        assert_eq!(result.title, "buy oat milk");
    }

    /// A second 409 on the rebased retry is reported as a conflict too,
    /// rather than rebasing forever — "succeeds on one retry" is a cap, not
    /// a loop.
    #[tokio::test]
    async fn a_second_conflict_on_the_retry_is_reported_not_retried_again() {
        let second_conflict = r#"{"error":"version_conflict","current":{"id":"a-1","title":"yet another title","version":3}}"#;
        let first_conflict = r#"{"error":"version_conflict","current":{"id":"a-1","title":"buy milk","context":"@computer","version":2}}"#;
        let transport =
            ScriptedTransport::new(vec![ok(409, second_conflict), ok(409, first_conflict)]);
        let base = json!({"id": "a-1", "title": "buy milk", "version": 1});

        let err = patch_with_rebase::<_, FakeItem>(
            &transport,
            "token",
            HttpMethod::Patch,
            "/api/items/a-1",
            &base,
            |v| json!({"expected_version": v, "title": "buy oat milk"}),
        )
        .await
        .unwrap_err();

        assert!(matches!(err, WriteError::Conflict { .. }));
        assert_eq!(transport.call_count(), 2, "capped at one retry");
    }

    /// #103 forwarded-review fix: the second conflict's rebase diff must
    /// compare against the *first* 409's `current` (the point this retry was
    /// actually rebased onto), never the original `base` two versions back.
    /// Fixture: the field is achieved-then-churned — `title` is "oat milk"
    /// (already achieved, hence `Safe`) at the first 409, then a second
    /// intervening write moves it back to the *original* `base`'s value
    /// ("buy milk") by the second 409. Diffing against the stale `base`
    /// would see `current2.title == base.title` and wrongly call that
    /// "untouched since we last rebased" (`Safe`, `fields: []`) — a real
    /// collision (someone reverted the field between the two attempts)
    /// reported with nothing the user can act on. Diffing against the first
    /// `current` catches it: that field moved away from what this retry was
    /// rebased onto.
    #[tokio::test]
    async fn a_second_conflict_is_diffed_against_the_rebased_onto_current_not_the_original_base()
    {
        let first_conflict = r#"{"error":"version_conflict","current":{"id":"a-1","title":"buy oat milk","version":2}}"#;
        let second_conflict = r#"{"error":"version_conflict","current":{"id":"a-1","title":"buy milk","version":3}}"#;
        let transport =
            ScriptedTransport::new(vec![ok(409, second_conflict), ok(409, first_conflict)]);
        let base = json!({"id": "a-1", "title": "buy milk", "version": 1});

        let err = patch_with_rebase::<_, FakeItem>(
            &transport,
            "token",
            HttpMethod::Patch,
            "/api/items/a-1",
            &base,
            |v| json!({"expected_version": v, "title": "buy oat milk"}),
        )
        .await
        .unwrap_err();

        match err {
            WriteError::Conflict { fields, .. } => assert_eq!(
                fields,
                vec!["title".to_string()],
                "the field must be named as a collision, not silently reported empty"
            ),
            other => panic!("expected a named collision, got {other:?}"),
        }
    }

    /// #103 forwarded-review fix: a `base` with no numeric `version` field
    /// is a malformed local record — never a legitimate "start from zero".
    /// This must surface as `InvalidResponse` before any request is sent,
    /// not silently default `expected_version` to `0`.
    #[tokio::test]
    async fn a_base_with_no_numeric_version_is_an_invalid_response_not_a_silent_zero() {
        let transport = ScriptedTransport::new(vec![]); // must never be called
        let base = json!({"id": "a-1", "title": "buy milk"}); // no "version" at all

        let err = patch_with_rebase::<_, FakeItem>(
            &transport,
            "token",
            HttpMethod::Patch,
            "/api/items/a-1",
            &base,
            |v| json!({"expected_version": v, "title": "buy oat milk"}),
        )
        .await
        .unwrap_err();

        assert!(matches!(err, WriteError::InvalidResponse(_)));
        assert_eq!(
            transport.call_count(),
            0,
            "a malformed base must never reach the transport at all"
        );
    }

    /// #103 forwarded-review fix: a safe-rebase 409 whose `current` body has
    /// no numeric `version` field is a malformed conflict response from the
    /// server — never a legitimate cue to silently retry at `base_version`
    /// (which is guaranteed stale, since the whole point of rebasing is to
    /// retry at the *new* version).
    #[tokio::test]
    async fn a_conflict_current_with_no_numeric_version_is_an_invalid_response_not_a_silent_retry_at_base_version(
    ) {
        // Disjoint field (`context`), so `rebase::decide` calls this `Safe`
        // and the adapter proceeds to build a retry — which is exactly the
        // path that must be caught before it fires.
        let conflict_body =
            r#"{"error":"version_conflict","current":{"id":"a-1","title":"buy milk","context":"@computer"}}"#;
        let transport = ScriptedTransport::new(vec![ok(409, conflict_body)]);
        let base = json!({"id": "a-1", "title": "buy milk", "context": "@calls", "version": 1});

        let err = patch_with_rebase::<_, FakeItem>(
            &transport,
            "token",
            HttpMethod::Patch,
            "/api/items/a-1",
            &base,
            |v| json!({"expected_version": v, "title": "buy oat milk"}),
        )
        .await
        .unwrap_err();

        assert!(matches!(err, WriteError::InvalidResponse(_)));
        assert_eq!(
            transport.call_count(),
            1,
            "the malformed conflict body must be caught before any retry is sent"
        );
    }

    #[tokio::test]
    async fn a_401_on_patch_is_unauthorized() {
        let transport = ScriptedTransport::new(vec![ok(401, "")]);
        let base = json!({"id": "a-1", "title": "buy milk", "version": 1});

        let err = patch_with_rebase::<_, FakeItem>(
            &transport,
            "token",
            HttpMethod::Patch,
            "/api/items/a-1",
            &base,
            |v| json!({"expected_version": v, "title": "buy oat milk"}),
        )
        .await
        .unwrap_err();

        assert_eq!(err, WriteError::Unauthorized);
    }

    // ----------------------------------------- the real Step/StepPatch DTOs

    /// #101 acceptance: "ticking a Step is a scalar write — no body or
    /// description text is ever composed or parsed." Exercises the generic
    /// engine against `hummingbird_domain`'s real `Step`/`StepPatch` types
    /// (not a fake), and pins the wire body itself: setting `done` serializes
    /// to exactly `{"expected_version":N,"done":true}` — no `body` field is
    /// ever present, because ticking never composes or parses Step text.
    #[tokio::test]
    async fn ticking_a_step_is_a_scalar_write_with_no_body_text_on_the_wire() {
        use hummingbird_domain::{Step, StepPatch};

        let base_step = Step {
            id: "s-1".into(),
            item_id: "a-1".into(),
            body: "buy milk".into(),
            done: false,
            position: 1,
            deleted_at: None,
            version: 1,
        };
        let base = serde_json::to_value(&base_step).unwrap();
        let ticked = r#"{"id":"s-1","item_id":"a-1","body":"buy milk","done":true,"position":1,"deleted_at":null,"version":2}"#;
        let transport = ScriptedTransport::new(vec![ok(200, ticked)]);

        let result: Step = patch_with_rebase(
            &transport,
            "token",
            HttpMethod::Patch,
            "/api/steps/s-1",
            &base,
            |expected_version| StepPatch {
                expected_version,
                done: Some(true),
                ..Default::default()
            },
        )
        .await
        .unwrap();

        assert!(result.done);
        assert_eq!(result.body, "buy milk", "the step's text is untouched");

        let sent_body = &transport.calls.lock().unwrap()[0].body;
        assert_eq!(sent_body, r#"{"expected_version":1,"done":true}"#);
    }
}
