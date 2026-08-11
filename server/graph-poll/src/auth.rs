//! App-only Microsoft Graph client-credentials token acquisition,
//! certificate-based (ADR-0011's M365 credential: "certificate preferred
//! over a client secret, since secrets are portal-capped at 24 months").
//!
//! Unlike `authority/src/fcm.rs`'s split — the assertion's bytes decided in
//! the lib, the RS256 signature itself performed in
//! `worker/src/fcm.rs`'s WebCrypto, because `hummingbird-authority-worker`
//! is wasm32 and has no WebCrypto equivalent in a native Rust crate — this
//! poller is an ordinary native process. There is no wasm constraint
//! forcing the signature out of the testable half, so the whole client
//! assertion, RS256 signature included, is pure and natively tested here
//! against a fixture keypair (`tests/fixtures/test_key_pkcs8.pem`, a
//! throwaway 2048-bit key generated for this crate's tests only — never a
//! real credential). `main.rs` (both binaries) holds only the HTTP POST to
//! the token endpoint and `parse_access_token`'s caller.
//!
//! **The credential this reads from the environment does not follow
//! ADR-0011's original "Worker secret" table any more than #135/#136's
//! did** — see this crate's `lib.rs` module doc for the full posture note
//! and the open operator question it cross-references on issue #137.

use jsonwebtoken::{Algorithm, EncodingKey, Header};
use serde::{Deserialize, Serialize};

/// The one scope an app-only Graph client-credentials grant ever asks for
/// — `.default` requests exactly the application permissions already
/// admin-consented on the app registration (`Mail.Read`, `Calendars.Read`;
/// ADR-0011's table), never a broader ad-hoc set.
pub const GRAPH_SCOPE: &str = "https://graph.microsoft.com/.default";

/// The Azure AD v2 client-assertion grant type — the certificate-based
/// counterpart to a client-secret `POST /token`.
pub const CLIENT_ASSERTION_TYPE: &str = "urn:ietf:params:oauth:client-assertion-type:jwt-bearer";

/// How long the assertion claims validity for. Ten minutes is Microsoft's
/// own documented ceiling for a certificate-based client assertion; the
/// access token it buys back carries its own (typically ~1h) expiry.
const ASSERTION_TTL_SECS: i64 = 600;

/// Slack subtracted from an access token's stated lifetime before the
/// caller considers it stale — `authority/src/fcm.rs::TOKEN_EXPIRY_SLACK_SECS`'s
/// own reasoning, so a token can never expire mid-flight between the
/// staleness check and the request it is used for. This poller is
/// one-shot per run, so in practice a fresh token is minted every
/// invocation regardless; the slack exists for parity with the pattern and
/// in case a future revision caches a token across a run's several HTTP
/// calls.
pub const TOKEN_EXPIRY_SLACK_SECS: i64 = 60;

/// The tenant-scoped v2 token endpoint — the assertion's own `aud`, and
/// where the caller POSTs it.
pub fn token_url(tenant_id: &str) -> String {
    format!("https://login.microsoftonline.com/{tenant_id}/oauth2/v2.0/token")
}

/// The three facts a certificate-based app-only Graph credential needs,
/// read from the environment by `main.rs` (never parsed or validated
/// beyond non-emptiness there — this module is where the shape is used).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CertCredential {
    pub tenant_id: String,
    pub client_id: String,
    /// The registered certificate's SHA-1 thumbprint, base64url-encoded —
    /// the JWT header's `x5t` claim, which is how Azure AD finds the
    /// matching public key on the app registration. Computed by the
    /// operator from the certificate file (e.g. `openssl x509 -in cert.pem
    /// -noout -fingerprint -sha1` re-encoded base64url) and held as an
    /// Actions variable alongside the private key secret — the thumbprint
    /// identifies a public certificate and is not itself sensitive.
    pub thumbprint_b64url: String,
    /// The private key matching the registered certificate, as a PEM
    /// block (PKCS#1 or PKCS#8 — [`jsonwebtoken::EncodingKey::from_rsa_pem`]
    /// accepts either).
    pub private_key_pem: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthError {
    /// The private key PEM could not be parsed, or a jsonwebtoken encoding
    /// failure — the caller's env-supplied credential is malformed.
    BadPrivateKey(String),
}

impl std::fmt::Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthError::BadPrivateKey(reason) => write!(f, "client credential: {reason}"),
        }
    }
}

#[derive(Serialize)]
struct Claims<'a> {
    aud: &'a str,
    iss: &'a str,
    sub: &'a str,
    jti: &'a str,
    nbf: i64,
    exp: i64,
    iat: i64,
}

/// Builds and RS256-signs the JWT client assertion for one token request.
/// `now_secs`/`jti` are caller-injected (no clock, no RNG here) — the same
/// "caller injects the clock" discipline `client/core/src/rank.rs`
/// documents — so the whole function is a pure, fixture-testable fold:
/// given the same credential, clock reading and `jti`, it produces a
/// byte-identical assertion every time. `jti` needs no cryptographic
/// randomness of its own (Azure AD uses it only for replay dedup within
/// the assertion's own ten-minute TTL); `main.rs` mints one from
/// `std::time`/`std::process::id()`.
pub fn client_assertion(cred: &CertCredential, now_secs: i64, jti: &str) -> Result<String, AuthError> {
    let aud = token_url(&cred.tenant_id);
    let claims = Claims {
        aud: &aud,
        iss: &cred.client_id,
        sub: &cred.client_id,
        jti,
        nbf: now_secs,
        exp: now_secs + ASSERTION_TTL_SECS,
        iat: now_secs,
    };
    let mut header = Header::new(Algorithm::RS256);
    header.x5t = Some(cred.thumbprint_b64url.clone());
    let key = EncodingKey::from_rsa_pem(cred.private_key_pem.as_bytes())
        .map_err(|e| AuthError::BadPrivateKey(e.to_string()))?;
    jsonwebtoken::encode(&header, &claims, &key).map_err(|e| AuthError::BadPrivateKey(e.to_string()))
}

/// An access token and the wall-clock millisecond after which the caller
/// must mint a new one — `authority/src/fcm.rs::AccessToken`'s own shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessToken {
    pub token: String,
    pub expires_at_ms: i64,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: i64,
}

/// Parses Azure AD's token response, converting its relative `expires_in`
/// into an absolute deadline against the caller's clock, minus
/// [`TOKEN_EXPIRY_SLACK_SECS`] — `authority/src/fcm.rs::parse_access_token`'s
/// own logic, verbatim.
pub fn parse_access_token(body: &str, now_ms: i64) -> Option<AccessToken> {
    let parsed: TokenResponse = serde_json::from_str(body).ok()?;
    if parsed.access_token.is_empty() {
        return None;
    }
    let lifetime = (parsed.expires_in - TOKEN_EXPIRY_SLACK_SECS).max(0);
    Some(AccessToken { token: parsed.access_token, expires_at_ms: now_ms + lifetime * 1000 })
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_KEY_PKCS8: &str = include_str!("../tests/fixtures/test_key_pkcs8.pem");

    fn cred() -> CertCredential {
        CertCredential {
            tenant_id: "tenant-1".into(),
            client_id: "client-1".into(),
            thumbprint_b64url: "dGVzdC10aHVtYnByaW50".into(),
            private_key_pem: TEST_KEY_PKCS8.to_string(),
        }
    }

    #[test]
    fn the_token_url_is_tenant_scoped() {
        assert_eq!(token_url("tenant-1"), "https://login.microsoftonline.com/tenant-1/oauth2/v2.0/token");
    }

    #[test]
    fn signs_a_well_formed_three_part_jwt() {
        let jwt = client_assertion(&cred(), 1_000, "jti-1").expect("signs");
        assert_eq!(jwt.split('.').count(), 3);
    }

    /// Pure and deterministic: the same inputs produce a byte-identical
    /// assertion — RS256/PKCS1v15 padding takes no randomness, so this is
    /// provable rather than merely "usually true."
    #[test]
    fn the_same_inputs_produce_a_byte_identical_assertion() {
        let a = client_assertion(&cred(), 1_000, "jti-1").unwrap();
        let b = client_assertion(&cred(), 1_000, "jti-1").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn a_different_jti_changes_the_assertion() {
        let a = client_assertion(&cred(), 1_000, "jti-1").unwrap();
        let b = client_assertion(&cred(), 1_000, "jti-2").unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn a_malformed_private_key_is_a_named_error() {
        let mut c = cred();
        c.private_key_pem = "not a key".to_string();
        assert!(matches!(client_assertion(&c, 1_000, "jti-1"), Err(AuthError::BadPrivateKey(_))));
    }

    #[test]
    fn the_header_carries_the_x5t_thumbprint() {
        let jwt = client_assertion(&cred(), 1_000, "jti-1").unwrap();
        let header_b64 = jwt.split('.').next().unwrap();
        let header_bytes = base64::Engine::decode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, header_b64)
            .expect("header is valid base64url");
        let header_json: serde_json::Value = serde_json::from_slice(&header_bytes).expect("header is JSON");
        assert_eq!(header_json["x5t"], "dGVzdC10aHVtYnByaW50");
        assert_eq!(header_json["alg"], "RS256");
    }

    #[test]
    fn parses_a_well_formed_token_response() {
        let body = r#"{"token_type": "Bearer", "expires_in": 3600, "access_token": "tok-abc"}"#;
        let token = parse_access_token(body, 0).expect("parses");
        assert_eq!(token.token, "tok-abc");
        assert_eq!(token.expires_at_ms, 3540 * 1000);
    }

    #[test]
    fn a_response_with_no_access_token_is_none() {
        assert!(parse_access_token(r#"{"error": "invalid_client"}"#, 0).is_none());
    }

    #[test]
    fn not_json_is_none() {
        assert!(parse_access_token("not json", 0).is_none());
    }
}
