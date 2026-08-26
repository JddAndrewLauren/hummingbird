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
    operation_id: Option<&str>,
) -> Sent {
    match transport
        .send(
            access_token,
            MutationRequest {
                method,
                path: path.to_string(),
                body,
                operation_id: operation_id.map(str::to_string),
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
    operation_id: Option<&str>,
) -> Result<CreateOutcome<T>, WriteError>
where
    C: Serialize,
    T: DeserializeOwned,
{
    let body = serde_json::to_string(create)
        .map_err(|source| WriteError::InvalidResponse(source.to_string()))?;
    match send(transport, access_token, HttpMethod::Post, path, body, operation_id).await {
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

/// The total number of `send`s [`patch_with_rebase`] will ever make for one
/// logical write: the original attempt, the one unconditional retry a
/// `Safe` first 409 earns, and one further bounded retry for a `Safe`
/// *second* 409 (genuine contention, #163) — never a fourth.
const MAX_ATTEMPTS: u32 = 3;

/// `PATCH`/`PUT` a CAS write, resolving a 409 automatically per
/// [`rebase::decide`] (#163):
/// - `Achieved` — every touched field already holds its intended value (this
///   write already landed, or coincidentally matches). Returns the carried
///   entity as the outcome; issues no further request.
/// - `Safe` — at least one touched field needs reapplying and nothing
///   collides. Reissues the identical touched-field intent at the 409's
///   version.
/// - `Collision` — reported as [`WriteError::Conflict`], naming every
///   colliding field. Never retried.
///
/// One success is **not** taken at face value: a create-shaped attempt
/// (`expected_version: 0`) answered `200` rather than `201` is the
/// authority's create replay, and its body is the stored row rather than
/// what was just sent. If that row does not already carry this write's
/// intent ([`rebase::divergent_fields`]), it is reported as
/// [`WriteError::Conflict`] — the row belongs to another device and this
/// edit never landed. Every other success is parsed and returned as-is.
///
/// A `Safe` decision on the first 409 earns exactly one retry; a `Safe`
/// decision on that retry's own 409 is genuine contention and earns exactly
/// one further bounded retry (see [`MAX_ATTEMPTS`]) before dead-lettering as
/// [`WriteError::Contention`] rather than a conflict with an empty field
/// list. Each rebase diffs against the point this attempt was actually
/// rebased onto — never the original `base` — so a field that moved *back*
/// to `base`'s value between two attempts is still caught as a genuine
/// collision (#103 forwarded-review fix, preserved here).
///
/// `base` is the entity this client last knew, as JSON (`serde_json::to_
/// value` of whatever `hummingbird_domain` entity the caller holds).
/// `build_patch` receives the `expected_version` to use — called once per
/// attempt, at whichever version that attempt is rebased onto.
///
/// `rebase_view` is the same intent expressed in the **entity's** own
/// encoding, for the one case where that differs from the wire's: `PUT
/// /api/settings/:key` sends `value` as typed JSON, but the entity stores it
/// as that JSON's canonical *text* (`hummingbird_domain::Setting::value`),
/// so diffing the wire body against `base`/`current` would compare `"f1"`
/// against `"\"f1\""` and read this client's own already-landed write as a
/// collision with itself. `None` — every other entity in ADR-0009's write
/// vocabulary — means the wire body already IS the entity's encoding and is
/// diffed directly. It changes only what [`rebase::decide`] compares; the
/// bytes sent are always `build_patch`'s.
pub async fn patch_with_rebase<P, T>(
    transport: &impl MutationTransport,
    access_token: &str,
    method: HttpMethod,
    path: &str,
    base: &Value,
    rebase_view: Option<&Value>,
    mut build_patch: impl FnMut(i64) -> P,
    operation_id: Option<&str>,
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

    // What this attempt's `current` is diffed against, and the version to
    // send at. Starts at the caller's `base`; after each `Safe` 409, both
    // become the 409's own `current` — the point that attempt was rebased
    // onto (#103 forwarded-review fix).
    let mut compare_against = base.clone();
    let mut version = base_version;

    for attempt_number in 1..=MAX_ATTEMPTS {
        let patch = build_patch(version);
        let patch_value = serde_json::to_value(&patch)
            .map_err(|source| WriteError::InvalidResponse(source.to_string()))?;
        let body = serde_json::to_string(&patch)
            .map_err(|source| WriteError::InvalidResponse(source.to_string()))?;

        match send(transport, access_token, method, path, body, operation_id).await {
            Sent::Success(status, body) => {
                // A create-shaped PUT (`expected_version: 0`) that lands on
                // 200 rather than 201 is the authority's create *replay*:
                // the key already existed, and the body is the STORED row,
                // not what this attempt sent. Idempotent when it really is
                // a replay; silent data loss when this device had simply
                // never pulled the row (a binding edited before the first
                // sync), because the queue would drop the entry as a
                // success and the next pull would quietly restore the other
                // value. Diffed in the entity's own encoding, exactly as
                // the 409 branch below is, and surfaced as the conflict it
                // is — the journal gets local and server side by side.
                if version == 0 && status != 201 {
                    let entity: Value = parse(&body)?;
                    let fields =
                        rebase::divergent_fields(rebase_view.unwrap_or(&patch_value), &entity);
                    if !fields.is_empty() {
                        return Err(WriteError::Conflict {
                            fields,
                            current: entity,
                        });
                    }
                    return serde_json::from_value(entity)
                        .map_err(|source| WriteError::InvalidResponse(source.to_string()));
                }
                return parse(&body);
            }
            Sent::Failed(error) => return Err(error),
            Sent::Conflict(current) => {
                let diffed = rebase_view.unwrap_or(&patch_value);
                match rebase::decide(diffed, &compare_against, &current) {
                    RebaseDecision::Collision(fields) => {
                        return Err(WriteError::Conflict { fields, current })
                    }
                    RebaseDecision::Achieved => {
                        // The write already landed — return the carried
                        // entity as the outcome, no further request.
                        return serde_json::from_value(current)
                            .map_err(|source| WriteError::InvalidResponse(source.to_string()));
                    }
                    RebaseDecision::Safe => {
                        if attempt_number == MAX_ATTEMPTS {
                            // The bounded retry budget is spent and this is
                            // still genuinely disjoint — contention, not a
                            // fabricated conflict with no fields to show.
                            return Err(WriteError::Contention { current });
                        }
                        // #103 forwarded-review fix: a `current` with no
                        // numeric `version` is a malformed 409 body — the
                        // server's own contract guarantees `current.version`
                        // on every conflict. Silently falling back to the
                        // last known version would resend a version this
                        // client already knows is stale, hiding the
                        // malformed body behind a spurious further 409
                        // instead of surfacing it here.
                        let Some(current_version) =
                            current.get("version").and_then(Value::as_i64)
                        else {
                            return Err(WriteError::InvalidResponse(
                                "409 conflict body has no numeric \"version\" field".to_string(),
                            ));
                        };
                        version = current_version;
                        compare_against = current;
                    }
                }
            }
        }
    }

    unreachable!("the loop always returns by MAX_ATTEMPTS: Success, Failed, Collision, Achieved, or the last Safe becomes Contention")
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
            create(&transport, "token", "/api/items", &create_dto, None)
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
            create(&transport, "token", "/api/items", &create_dto, None)
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

        let first: CreateOutcome<FakeItem> = create(&transport, "token", "/api/items", &create_dto, None)
            .await
            .unwrap();
        let second: CreateOutcome<FakeItem> =
            create(&transport, "token", "/api/items", &create_dto, None)
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

        let err = create::<_, FakeItem>(&transport, "token", "/api/items", &create_dto, None)
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

        let err = create::<_, FakeItem>(&transport, "token", "/api/items", &create_dto, None)
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

        let err = create::<_, FakeItem>(&transport, "token", "/api/items", &create_dto, None)
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

        let err = create::<_, FakeItem>(&transport, "token", "/api/items", &create_dto, None)
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

        let err = create::<_, FakeItem>(&transport, "token", "/api/items", &create_dto, None)
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

        let err = create::<_, FakeItem>(&transport, "token", "/api/items", &create_dto, None)
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
            None,
            |v| json!({"expected_version": v, "title": "buy milk"}),
            None,
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
            None,
            |v| json!({"expected_version": v, "title": "buy oat milk"}),
            None,
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
            None,
            |v| json!({"expected_version": v, "title": "buy oat milk"}),
            None,
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

    /// #101/#163 acceptance: "replaying any mutation twice leaves the same
    /// end state" — a crash swallows the ack, the client replays the
    /// identical patch against the identical (now stale) base. The 409's
    /// `current` already holds exactly what this patch intends
    /// (`RebaseDecision::Achieved`), so the replay converges on that
    /// carried entity with **no resend at all** — the real authority would
    /// answer any actual retry with the *next* version, never the same one,
    /// which is exactly why there is nothing scripted here to (wrongly)
    /// rely on.
    #[tokio::test]
    async fn replaying_a_patch_after_a_swallowed_ack_converges() {
        let conflict_body = r#"{"error":"version_conflict","current":{"id":"a-1","title":"buy oat milk","version":2}}"#;
        let transport = ScriptedTransport::new(vec![ok(409, conflict_body)]);
        let base = json!({"id": "a-1", "title": "buy milk", "version": 1});

        let result: FakeItem = patch_with_rebase(
            &transport,
            "token",
            HttpMethod::Patch,
            "/api/items/a-1",
            &base,
            None,
            |v| json!({"expected_version": v, "title": "buy oat milk"}),
            None,
        )
        .await
        .unwrap();

        assert_eq!(result.title, "buy oat milk");
        assert_eq!(
            transport.call_count(),
            1,
            "an already-achieved 409 must never be resent"
        );
    }

    /// #163 acceptance: "A first 409 whose patch is fully achieved returns
    /// the carried entity and issues no second request."
    #[tokio::test]
    async fn a_fully_achieved_first_409_returns_the_carried_entity_with_no_second_request() {
        let conflict_body = r#"{"error":"version_conflict","current":{"id":"a-1","title":"buy oat milk","version":2}}"#;
        let transport = ScriptedTransport::new(vec![ok(409, conflict_body)]);
        let base = json!({"id": "a-1", "title": "buy milk", "version": 1});

        let result: FakeItem = patch_with_rebase(
            &transport,
            "token",
            HttpMethod::Patch,
            "/api/items/a-1",
            &base,
            None,
            |v| json!({"expected_version": v, "title": "buy oat milk"}),
            None,
        )
        .await
        .unwrap();

        assert_eq!(result.title, "buy oat milk");
        assert_eq!(result.version, 2);
        assert_eq!(transport.call_count(), 1, "no second request");
    }

    /// #163 acceptance: "A second 409 that is fully achieved resolves as
    /// success, not a dead-letter." The rebased retry's own 409 is exactly
    /// what this client already intended — the write landed while this
    /// client was rebasing, and that must not lose the edit.
    #[tokio::test]
    async fn a_fully_achieved_second_409_is_success_not_a_conflict() {
        let first_conflict = r#"{"error":"version_conflict","current":{"id":"a-1","title":"buy milk","version":2}}"#;
        let second_conflict = r#"{"error":"version_conflict","current":{"id":"a-1","title":"buy oat milk","version":3}}"#;
        let transport =
            ScriptedTransport::new(vec![ok(409, second_conflict), ok(409, first_conflict)]);
        let base = json!({"id": "a-1", "title": "buy milk", "version": 1});

        let result: FakeItem = patch_with_rebase(
            &transport,
            "token",
            HttpMethod::Patch,
            "/api/items/a-1",
            &base,
            None,
            |v| json!({"expected_version": v, "title": "buy oat milk"}),
            None,
        )
        .await
        .unwrap();

        assert_eq!(result.title, "buy oat milk");
        assert_eq!(result.version, 3);
        assert_eq!(
            transport.call_count(),
            2,
            "an achieved second 409 resolves without a third request"
        );
    }

    /// #163 acceptance: "A second 409 that is genuinely disjoint retries
    /// once more" — and if that further retry succeeds outright, the write
    /// lands normally.
    #[tokio::test]
    async fn a_disjoint_second_409_earns_one_further_bounded_retry_that_can_still_succeed() {
        let first_conflict = r#"{"error":"version_conflict","current":{"id":"a-1","priority":1,"title":"someone bumped this","version":2}}"#;
        let second_conflict = r#"{"error":"version_conflict","current":{"id":"a-1","priority":1,"title":"someone bumped this again","version":3}}"#;
        let third_success = r#"{"id":"a-1","priority":2,"title":"someone bumped this again","version":4}"#;
        let transport = ScriptedTransport::new(vec![
            ok(200, third_success),
            ok(409, second_conflict),
            ok(409, first_conflict),
        ]);
        let base = json!({"id": "a-1", "priority": 1, "title": "buy milk", "version": 1});

        let result: FakeItem = patch_with_rebase(
            &transport,
            "token",
            HttpMethod::Patch,
            "/api/items/a-1",
            &base,
            None,
            |v| json!({"expected_version": v, "priority": 2}),
            None,
        )
        .await
        .unwrap();

        assert_eq!(result.version, 4);
        assert_eq!(
            transport.call_count(),
            3,
            "original attempt, one retry, one further bounded retry"
        );
    }

    /// #163 acceptance: "if that also fails disjoint, dead-letter as
    /// contention, never as a conflict with an empty field list."
    #[tokio::test]
    async fn a_disjoint_third_attempt_dead_letters_as_contention_not_an_empty_conflict() {
        let first_conflict = r#"{"error":"version_conflict","current":{"id":"a-1","priority":1,"title":"someone bumped this","version":2}}"#;
        let second_conflict = r#"{"error":"version_conflict","current":{"id":"a-1","priority":1,"title":"someone bumped this again","version":3}}"#;
        let third_conflict = r#"{"error":"version_conflict","current":{"id":"a-1","priority":1,"title":"and again","version":4}}"#;
        let transport = ScriptedTransport::new(vec![
            ok(409, third_conflict),
            ok(409, second_conflict),
            ok(409, first_conflict),
        ]);
        let base = json!({"id": "a-1", "priority": 1, "title": "buy milk", "version": 1});

        let err = patch_with_rebase::<_, FakeItem>(
            &transport,
            "token",
            HttpMethod::Patch,
            "/api/items/a-1",
            &base,
            None,
            |v| json!({"expected_version": v, "priority": 2}),
            None,
        )
        .await
        .unwrap_err();

        match err {
            WriteError::Contention { current } => assert_eq!(
                current,
                json!({"id": "a-1", "priority": 1, "title": "and again", "version": 4}),
                "the current entity must reach the caller, not be dropped"
            ),
            other => panic!("expected contention, got {other:?}"),
        }
        assert_eq!(
            transport.call_count(),
            3,
            "original attempt, one retry, one bounded further retry — never a fourth"
        );
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
            None,
            |v| json!({"expected_version": v, "title": "buy oat milk"}),
            None,
        )
        .await
        .unwrap_err();

        assert!(matches!(err, WriteError::Conflict { .. }));
        assert_eq!(transport.call_count(), 2, "capped at one retry");
    }

    /// #103 forwarded-review fix (still true under #163's three-way rebase):
    /// the second conflict's rebase diff must compare against the *first*
    /// 409's `current` (the point this retry was actually rebased onto),
    /// never the original `base` two versions back.
    ///
    /// Fixture: a two-field patch (`title`, `context`) so the first 409 is
    /// `Safe`, not `Achieved` — `title` already matches what this patch
    /// intends, but `context` is still untouched since `base` and needs
    /// reapplying, so at least one field genuinely needs resending and the
    /// retry actually happens (an all-achieved first 409 would return
    /// immediately with no second request at all, per #163, and never reach
    /// this scenario). By the second 409 a further intervening write has
    /// moved `title` back to the *original* `base`'s value ("buy milk").
    /// Diffing against the stale `base` would see `current2.title ==
    /// base.title` and wrongly call that "untouched since we last rebased"
    /// — a real collision (someone reverted the field between the two
    /// attempts) reported with nothing the user can act on. Diffing against
    /// the first `current` catches it: that field moved away from what this
    /// retry was rebased onto.
    #[tokio::test]
    async fn a_second_conflict_is_diffed_against_the_rebased_onto_current_not_the_original_base()
    {
        let first_conflict = r#"{"error":"version_conflict","current":{"id":"a-1","title":"buy oat milk","context":"@calls","version":2}}"#;
        let second_conflict = r#"{"error":"version_conflict","current":{"id":"a-1","title":"buy milk","context":"@computer","version":3}}"#;
        let transport =
            ScriptedTransport::new(vec![ok(409, second_conflict), ok(409, first_conflict)]);
        let base = json!({"id": "a-1", "title": "buy milk", "context": "@calls", "version": 1});

        let err = patch_with_rebase::<_, FakeItem>(
            &transport,
            "token",
            HttpMethod::Patch,
            "/api/items/a-1",
            &base,
            None,
            |v| json!({"expected_version": v, "title": "buy oat milk", "context": "@computer"}),
            None,
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
        assert_eq!(transport.call_count(), 2, "capped at one retry per attempt");
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
            None,
            |v| json!({"expected_version": v, "title": "buy oat milk"}),
            None,
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
            None,
            |v| json!({"expected_version": v, "title": "buy oat milk"}),
            None,
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
            None,
            |v| json!({"expected_version": v, "title": "buy oat milk"}),
            None,
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
            None,
            |expected_version| StepPatch {
                expected_version,
                done: Some(true),
                ..Default::default()
            },
            None,
        )
        .await
        .unwrap();

        assert!(result.done);
        assert_eq!(result.body, "buy milk", "the step's text is untouched");

        let sent_body = &transport.calls.lock().unwrap()[0].body;
        assert_eq!(sent_body, r#"{"expected_version":1,"done":true}"#);
    }

    // -------------------------- the settings encoding split (#118)
    //
    // `PUT /api/settings/:key` is the one write whose wire encoding and
    // stored encoding differ: `PutSetting::value` is typed JSON, and
    // `Setting::value` is that JSON's canonical *text*. `rebase_view` is
    // what keeps the 409 diff in the stored encoding; these two tests are
    // the reason it exists, and the reason it may not be dropped as
    // redundant.

    #[tokio::test]
    async fn a_settings_replay_that_already_landed_is_achieved_not_a_collision() {
        // The classic replay: this client's own PUT landed, the ack was
        // lost, and the retry 409s carrying that very value back. Diffed in
        // the WIRE encoding this would read as `"motogp"` vs `"\"motogp\""`
        // — a collision with itself, dead-lettering a write that in fact
        // succeeded.
        let conflict_body = r#"{"error":"version_conflict","current":{"key":"race-series","value":"\"motogp\"","updated_at":3000,"version":4}}"#;
        let transport = ScriptedTransport::new(vec![ok(409, conflict_body)]);
        let base = json!({"key": "race-series", "value": "\"f1\"", "updated_at": 1, "version": 3});

        let result: hummingbird_domain::Setting = patch_with_rebase(
            &transport,
            "token",
            HttpMethod::Put,
            "/api/settings/race-series",
            &base,
            Some(&json!({"value": "\"motogp\""})),
            |v| json!({"expected_version": v, "value": "motogp"}),
            None,
        )
        .await
        .unwrap();

        assert_eq!(result.value, "\"motogp\"");
        assert_eq!(transport.call_count(), 1, "an achieved write is never resent");
    }

    #[tokio::test]
    async fn a_stale_settings_write_still_rebases_and_a_real_one_still_collides() {
        // Someone else bumped the workspace version without touching this
        // key: untouched since base, so this reapplies at the new version.
        let untouched = r#"{"error":"version_conflict","current":{"key":"race-series","value":"\"f1\"","updated_at":1,"version":5}}"#;
        let applied = r#"{"key":"race-series","value":"\"motogp\"","updated_at":3000,"version":6}"#;
        let transport = ScriptedTransport::new(vec![ok(200, applied), ok(409, untouched)]);
        let base = json!({"key": "race-series", "value": "\"f1\"", "updated_at": 1, "version": 3});

        let result: hummingbird_domain::Setting = patch_with_rebase(
            &transport,
            "token",
            HttpMethod::Put,
            "/api/settings/race-series",
            &base,
            Some(&json!({"value": "\"motogp\""})),
            |v| json!({"expected_version": v, "value": "motogp"}),
            None,
        )
        .await
        .unwrap();
        assert_eq!(result.version, 6);
        assert_eq!(transport.call_count(), 2, "one rebased retry");
        assert_eq!(
            transport.calls.lock().unwrap()[1].body,
            r#"{"expected_version":5,"value":"motogp"}"#,
            "the retry re-sends the WIRE encoding at the 409's version",
        );

        // And a genuine same-key disagreement is still a named collision.
        let collided = r#"{"error":"version_conflict","current":{"key":"race-series","value":"\"wrc\"","updated_at":2,"version":5}}"#;
        let transport = ScriptedTransport::new(vec![ok(409, collided)]);
        let err = patch_with_rebase::<_, hummingbird_domain::Setting>(
            &transport,
            "token",
            HttpMethod::Put,
            "/api/settings/race-series",
            &base,
            Some(&json!({"value": "\"motogp\""})),
            |v| json!({"expected_version": v, "value": "motogp"}),
            None,
        )
        .await
        .unwrap_err();
        assert!(matches!(
            err,
            WriteError::Conflict { ref fields, .. } if fields == &vec!["value".to_string()]
        ));
    }

    // ---------------- the create-replay 200 (post-#118 review finding)
    //
    // A `PUT` at `expected_version: 0` against a key that already exists
    // never 409s: the authority answers 200 with the STORED row. Success
    // for a true replay, silent loss for a device that had not pulled the
    // row yet — which is exactly the first-run bindings case.

    #[tokio::test]
    async fn a_version_zero_create_that_actually_created_is_a_plain_success() {
        let created = r#"{"key":"race-series","value":"\"motogp\"","updated_at":3000,"version":1}"#;
        let transport = ScriptedTransport::new(vec![ok(201, created)]);
        let base = json!({"key": "race-series", "value": "null", "updated_at": 0, "version": 0});

        let result: hummingbird_domain::Setting = patch_with_rebase(
            &transport,
            "token",
            HttpMethod::Put,
            "/api/settings/race-series",
            &base,
            Some(&json!({"value": "\"motogp\""})),
            |v| json!({"expected_version": v, "value": "motogp"}),
            None,
        )
        .await
        .unwrap();

        assert_eq!(result.value, "\"motogp\"");
        assert_eq!(transport.call_count(), 1);
    }

    #[tokio::test]
    async fn a_version_zero_replay_of_this_writes_own_value_is_still_a_success() {
        // The idempotent case the 200 exists for: the row already holds
        // exactly what this write intended, so there is nothing lost.
        let stored = r#"{"key":"race-series","value":"\"motogp\"","updated_at":3000,"version":1}"#;
        let transport = ScriptedTransport::new(vec![ok(200, stored)]);
        let base = json!({"key": "race-series", "value": "null", "updated_at": 0, "version": 0});

        let result: hummingbird_domain::Setting = patch_with_rebase(
            &transport,
            "token",
            HttpMethod::Put,
            "/api/settings/race-series",
            &base,
            Some(&json!({"value": "\"motogp\""})),
            |v| json!({"expected_version": v, "value": "motogp"}),
            None,
        )
        .await
        .unwrap();

        assert_eq!(result.value, "\"motogp\"");
    }

    #[tokio::test]
    async fn a_version_zero_write_over_someone_elses_row_is_a_named_conflict_not_a_success() {
        // The loss this check exists for: this device never pulled
        // `race-series`, so it wrote at version 0 — and the authority
        // answered 200 with the value another device had already set.
        // Reported as a conflict so the entry dead-letters with both sides
        // visible, rather than being dropped as a success whose edit the
        // next pull quietly reverts.
        let stored = r#"{"key":"race-series","value":"\"f1\"","updated_at":1000,"version":1}"#;
        let transport = ScriptedTransport::new(vec![ok(200, stored)]);
        let base = json!({"key": "race-series", "value": "null", "updated_at": 0, "version": 0});

        let err = patch_with_rebase::<_, hummingbird_domain::Setting>(
            &transport,
            "token",
            HttpMethod::Put,
            "/api/settings/race-series",
            &base,
            Some(&json!({"value": "\"motogp\""})),
            |v| json!({"expected_version": v, "value": "motogp"}),
            None,
        )
        .await
        .unwrap_err();

        match err {
            WriteError::Conflict { fields, current } => {
                assert_eq!(fields, vec!["value".to_string()]);
                assert_eq!(current["value"], "\"f1\"", "the journal keeps the server side");
            }
            other => panic!("expected a named conflict, got {other:?}"),
        }
        assert_eq!(transport.call_count(), 1, "a divergent replay is never resent");
    }
}
