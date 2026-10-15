//! The decidable half of verifying a Google ID token (ADR-0040 decisions
//! 2, 4 and 5): splitting and decoding the JWT, every claim check and the
//! one-subject allowlist, parsing Google's JWKS and its `max-age`,
//! converting a JWK into the SPKI DER WebCrypto imports, and naming the
//! device. `handlers/auth_google.rs` strings these into the route.
//!
//! **What is not here, by design:** the two steps that need a runtime —
//! fetching the JWKS and `crypto.subtle.verify` — live in
//! `hummingbird-authority-worker`'s `signin` module, which has no test
//! harness (CLAUDE.md, "The wasm32 worker build stays thin"). Everything
//! that can be decided without them is decided here, natively tested, and
//! the shim is handed bytes: the SPKI to import, the signing input, the
//! signature. JSON is read with `serde_json`, which the crate already
//! depends on; base64 with [`crate::base64`]. No new crate.
//!
//! **Claims are checked before the signature,** and that order is safe:
//! nothing mints until both have passed (`auth_google`'s
//! `PendingSignin::signature_checked` is the only way to the mint), and
//! checking the cheap, pure half first means a junk token never costs a
//! JWKS fetch. Two consequences follow. A rejected `sub` in the log may
//! come from a token whose signature was never checked — it is a hint for
//! the operator setting `GOOGLE_SIGNIN_ALLOWED_SUB`, never an identity, and
//! `auth_google` logs it escaped and capped. And anyone who knows the
//! allowed `sub` can pass the claims with an unsigned token naming a
//! made-up `kid`, so the re-fetch an unknown `kid` causes is rate-limited
//! here, in [`JwksCache::lookup`], rather than in the shim.

use serde::Deserialize;

use crate::base64;
use hummingbird_domain::DevicePlatform;

/// Google's JWKS for the keys that sign its ID tokens.
pub const GOOGLE_JWKS_URL: &str = "https://www.googleapis.com/oauth2/v3/certs";

/// Both spellings Google documents for an ID token's `iss`.
pub const GOOGLE_ISSUERS: [&str; 2] = ["accounts.google.com", "https://accounts.google.com"];

/// How old `iat` may be (ADR-0040: "`iat` is under ten minutes old") —
/// the window that bounds a replay.
pub const IAT_MAX_AGE_SECS: i64 = 600;

/// How far into the future `iat` may sit: clock skew between Google and
/// the Worker, never a token issued ahead of time (#866).
pub const IAT_MAX_SKEW_SECS: i64 = 60;

/// The least time after one JWKS fetch before an unknown `kid` may cause
/// another (#866). Google publishes a new key well before signing with it,
/// so a real rotation is caught by the next fetch at most this late.
pub const UNKNOWN_KID_REFETCH_MS: i64 = 60_000;

/// The longest device name the route accepts, in characters. A label for a
/// Settings row, not prose; anything longer is a bad body.
pub const MAX_NAME_CHARS: usize = 64;

/// How much of the name survives into the token id's slug.
const MAX_SLUG_CHARS: usize = 32;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// The two values the route needs, read from the Worker's environment:
/// `GOOGLE_SIGNIN_CLIENT_IDS` (a wrangler **var** — client ids are public
/// by design) and `GOOGLE_SIGNIN_ALLOWED_SUB` (a Worker **secret**).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SigninConfig {
    pub client_ids: Vec<String>,
    pub allowed_sub: String,
}

impl SigninConfig {
    /// `None` when either value is unset or blank — the route then answers
    /// 503, failing closed exactly as an unset `ADMIN_SECRET` does. The
    /// client ids are a comma- or whitespace-separated list.
    pub fn from_env(client_ids: Option<&str>, allowed_sub: Option<&str>) -> Option<SigninConfig> {
        let client_ids: Vec<String> = client_ids?
            .split(|c: char| c == ',' || c.is_whitespace())
            .filter(|id| !id.is_empty())
            .map(str::to_string)
            .collect();
        let allowed_sub = allowed_sub?.trim();
        if client_ids.is_empty() || allowed_sub.is_empty() {
            return None;
        }
        Some(SigninConfig {
            client_ids,
            allowed_sub: allowed_sub.to_string(),
        })
    }
}

// ---------------------------------------------------------------------------
// The JWT
// ---------------------------------------------------------------------------

/// Why a token was refused. Every variant answers the same empty 403 (never
/// 401 — ADR-0004 keeps that for a held credential the authority refuses);
/// the variant exists for the log line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rejection {
    /// Not three base64url segments of JSON, or a claim of the wrong type.
    Malformed,
    WrongAlg,
    MissingKid,
    WrongIssuer,
    WrongAudience,
    Expired,
    StaleIat,
    EmailUnverified,
    OtherSub,
    /// The `kid` is not in Google's JWKS, even after a re-fetch.
    UnknownKid,
    BadSignature,
}

impl Rejection {
    pub fn reason(self) -> &'static str {
        match self {
            Rejection::Malformed => "malformed token",
            Rejection::WrongAlg => "alg is not RS256",
            Rejection::MissingKid => "no kid in the header",
            Rejection::WrongIssuer => "iss is not Google",
            Rejection::WrongAudience => "aud is not a configured client id",
            Rejection::Expired => "exp has passed",
            Rejection::StaleIat => "iat is over ten minutes old or over a minute ahead",
            Rejection::EmailUnverified => "email_verified is not true",
            Rejection::OtherSub => "sub is not the allowed subject",
            Rejection::UnknownKid => "kid is not in Google's JWKS",
            Rejection::BadSignature => "signature does not verify",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct JwtHeader {
    pub alg: String,
    #[serde(default)]
    pub kid: Option<String>,
}

/// The claims the verifier reads. Each is optional so that an absent one is
/// a specific rejection rather than a parse failure; a present one of the
/// wrong JSON type is [`Rejection::Malformed`].
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct IdClaims {
    #[serde(default)]
    pub iss: Option<String>,
    /// A string from Google; the JWT spec also allows an array, which this
    /// verifier refuses rather than reasoning about `azp`.
    #[serde(default)]
    pub aud: Option<serde_json::Value>,
    #[serde(default)]
    pub exp: Option<i64>,
    #[serde(default)]
    pub iat: Option<i64>,
    #[serde(default)]
    pub email_verified: Option<serde_json::Value>,
    #[serde(default)]
    pub sub: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
}

/// A JWT taken apart: the decoded header and claims, and the two things the
/// signature check needs — the exact bytes that were signed and the
/// signature itself.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedJwt {
    pub header: JwtHeader,
    pub claims: IdClaims,
    pub signing_input: String,
    pub signature: Vec<u8>,
}

pub fn parse_jwt(token: &str) -> Result<ParsedJwt, Rejection> {
    let mut parts = token.split('.');
    let (Some(header), Some(claims), Some(signature), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(Rejection::Malformed);
    };
    let decode_json = |segment: &str| base64::decode_url(segment).ok_or(Rejection::Malformed);
    let header: JwtHeader =
        serde_json::from_slice(&decode_json(header)?).map_err(|_| Rejection::Malformed)?;
    let claims_json = base64::decode_url(claims).ok_or(Rejection::Malformed)?;
    let claims: IdClaims =
        serde_json::from_slice(&claims_json).map_err(|_| Rejection::Malformed)?;
    let signature = base64::decode_url(signature).ok_or(Rejection::Malformed)?;
    if signature.is_empty() {
        return Err(Rejection::Malformed);
    }
    let signing_input = token[..token.rfind('.').expect("three segments")].to_string();
    Ok(ParsedJwt {
        header,
        claims,
        signing_input,
        signature,
    })
}

/// Whom a token names, once every claim has passed. The signature is still
/// to be checked when this exists — see the module doc.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedIdentity {
    pub sub: String,
    pub email: Option<String>,
}

/// Every check ADR-0040 decision 2 lists except the two that need Google's
/// keys (the `kid` being in the JWKS, and the signature). Checked in a
/// fixed order, so a token wrong in two ways always reports the same one.
pub fn verify_claims(
    header: &JwtHeader,
    claims: &IdClaims,
    now_ms: i64,
    config: &SigninConfig,
) -> Result<VerifiedIdentity, Rejection> {
    if header.alg != "RS256" {
        return Err(Rejection::WrongAlg);
    }
    if header.kid.as_deref().is_none_or(str::is_empty) {
        return Err(Rejection::MissingKid);
    }
    if !claims
        .iss
        .as_deref()
        .is_some_and(|iss| GOOGLE_ISSUERS.contains(&iss))
    {
        return Err(Rejection::WrongIssuer);
    }
    let aud = match &claims.aud {
        Some(serde_json::Value::String(aud)) => aud,
        _ => return Err(Rejection::WrongAudience),
    };
    if !config.client_ids.iter().any(|id| id == aud) {
        return Err(Rejection::WrongAudience);
    }
    let now_secs = now_ms.div_euclid(1000);
    if !claims.exp.is_some_and(|exp| now_secs < exp) {
        return Err(Rejection::Expired);
    }
    if !claims.iat.is_some_and(|iat| {
        now_secs - iat <= IAT_MAX_AGE_SECS && iat - now_secs <= IAT_MAX_SKEW_SECS
    }) {
        return Err(Rejection::StaleIat);
    }
    if claims.email_verified != Some(serde_json::Value::Bool(true)) {
        return Err(Rejection::EmailUnverified);
    }
    let sub = claims.sub.as_deref().unwrap_or_default();
    if sub.is_empty() || sub != config.allowed_sub {
        return Err(Rejection::OtherSub);
    }
    Ok(VerifiedIdentity {
        sub: sub.to_string(),
        email: claims.email.clone(),
    })
}

// ---------------------------------------------------------------------------
// Google's keys
// ---------------------------------------------------------------------------

/// Google's signing keys, as SPKI DER ready for WebCrypto, with the moment
/// they were fetched and the moment the JWKS response said to stop trusting
/// them. The shim keeps one of these on the Durable Object instance; it is
/// never persisted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JwksCache {
    keys: Vec<(String, Vec<u8>)>,
    fetched_at_ms: i64,
    expires_at_ms: i64,
}

/// What the cache says about a `kid` before anything is fetched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyLookup<'a> {
    /// The set is fresh and holds the key.
    Cached(&'a [u8]),
    /// Fetch the set again: it has expired, or the `kid` is unknown and
    /// the last fetch is at least [`UNKNOWN_KID_REFETCH_MS`] old.
    Fetch,
    /// The `kid` is unknown and the set was fetched too recently to fetch
    /// again: refused like a `kid` missing from a fresh set.
    Unknown,
}

#[derive(Deserialize)]
struct JwksDocument {
    keys: Vec<Jwk>,
}

#[derive(Deserialize)]
struct Jwk {
    kty: String,
    #[serde(default)]
    kid: Option<String>,
    #[serde(default)]
    n: Option<String>,
    #[serde(default)]
    e: Option<String>,
}

impl JwksCache {
    /// Parses a JWKS response body. Keys that are not RSA, carry no `kid`,
    /// or whose `n`/`e` do not decode are skipped rather than failing the
    /// whole set; `None` means the body was not a JWKS at all. The expiry
    /// comes from the response's `Cache-Control: max-age`, and no
    /// `max-age` means the set is good for this request only.
    pub fn parse(body: &str, cache_control: Option<&str>, now_ms: i64) -> Option<JwksCache> {
        let document: JwksDocument = serde_json::from_str(body).ok()?;
        let keys = document
            .keys
            .into_iter()
            .filter(|jwk| jwk.kty == "RSA")
            .filter_map(|jwk| {
                let kid = jwk.kid.filter(|kid| !kid.is_empty())?;
                let spki = jwk_to_spki(jwk.n.as_deref()?, jwk.e.as_deref()?)?;
                Some((kid, spki))
            })
            .collect();
        let max_age_secs = cache_control.and_then(max_age).unwrap_or(0);
        Some(JwksCache {
            keys,
            fetched_at_ms: now_ms,
            expires_at_ms: now_ms.saturating_add(max_age_secs.saturating_mul(1000)),
        })
    }

    /// The SPKI DER for `kid`, whether or not the set is still fresh — the
    /// shim reads a freshly fetched set through this.
    pub fn key(&self, kid: &str) -> Option<&[u8]> {
        self.keys
            .iter()
            .find(|(id, _)| id == kid)
            .map(|(_, spki)| spki.as_slice())
    }

    /// Whether the shim may answer `kid` from this set, must fetch, or
    /// must refuse. A known `kid` is answered while the set is fresh and
    /// fetched for once it has expired. An unknown one is fetched for too
    /// (Google rotates keys, and a token signed with a new one can arrive
    /// before the old set's `max-age` runs out) — but at most once per
    /// [`UNKNOWN_KID_REFETCH_MS`], fresh or not, so a stream of made-up
    /// `kid`s costs one fetch a minute, not one a request.
    pub fn lookup(&self, kid: &str, now_ms: i64) -> KeyLookup<'_> {
        match self.key(kid) {
            Some(spki) if now_ms < self.expires_at_ms => KeyLookup::Cached(spki),
            None if now_ms - self.fetched_at_ms < UNKNOWN_KID_REFETCH_MS => KeyLookup::Unknown,
            _ => KeyLookup::Fetch,
        }
    }
}

/// The `max-age` directive of a `Cache-Control` value, in seconds.
pub fn max_age(cache_control: &str) -> Option<i64> {
    cache_control.split(',').find_map(|directive| {
        let (name, value) = directive.trim().split_once('=')?;
        if !name.trim().eq_ignore_ascii_case("max-age") {
            return None;
        }
        value
            .trim()
            .trim_matches('"')
            .parse::<i64>()
            .ok()
            .filter(|secs| *secs >= 0)
    })
}

/// An RSA JWK's base64url `n` and `e` as a DER `SubjectPublicKeyInfo` —
/// the form `crypto.subtle.importKey("spki", …)` takes:
///
/// ```text
/// SEQUENCE {
///   SEQUENCE { OID rsaEncryption, NULL }
///   BIT STRING { 0 unused bits, RSAPublicKey ::= SEQUENCE { INTEGER n, INTEGER e } }
/// }
/// ```
///
/// `None` when either member is not base64url or is empty.
pub fn jwk_to_spki(n: &str, e: &str) -> Option<Vec<u8>> {
    let n = base64::decode_url(n).filter(|n| !n.is_empty())?;
    let e = base64::decode_url(e).filter(|e| !e.is_empty())?;
    let rsa_public_key = der(0x30, &[der_uint(&n), der_uint(&e)].concat());
    // OID 1.2.840.113549.1.1.1 (rsaEncryption), then its NULL parameters.
    let algorithm = der(
        0x30,
        &[
            der(
                0x06,
                &[0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x01],
            ),
            der(0x05, &[]),
        ]
        .concat(),
    );
    let bit_string = der(0x03, &[&[0u8][..], &rsa_public_key].concat());
    Some(der(0x30, &[algorithm, bit_string].concat()))
}

/// One DER TLV with a definite length.
fn der(tag: u8, content: &[u8]) -> Vec<u8> {
    let mut out = vec![tag];
    let len = content.len();
    if len < 0x80 {
        out.push(len as u8);
    } else {
        let bytes: Vec<u8> = len
            .to_be_bytes()
            .into_iter()
            .skip_while(|b| *b == 0)
            .collect();
        out.push(0x80 | bytes.len() as u8);
        out.extend(bytes);
    }
    out.extend_from_slice(content);
    out
}

/// A DER INTEGER for an unsigned big-endian magnitude: minimal (leading
/// zeros dropped), with one zero byte restored when the high bit is set so
/// it is not read as negative.
fn der_uint(magnitude: &[u8]) -> Vec<u8> {
    let first = magnitude
        .iter()
        .position(|b| *b != 0)
        .unwrap_or(magnitude.len() - 1);
    let trimmed = &magnitude[first..];
    let mut content = Vec::with_capacity(trimmed.len() + 1);
    if trimmed[0] & 0x80 != 0 {
        content.push(0);
    }
    content.extend_from_slice(trimmed);
    der(0x02, &content)
}

// ---------------------------------------------------------------------------
// Naming the device
// ---------------------------------------------------------------------------

/// The device's label as it will be stored: trimmed, non-empty, at most
/// [`MAX_NAME_CHARS`] characters. `Err` carries the 400's message.
pub fn validate_name(name: &str) -> Result<&str, &'static str> {
    let name = name.trim();
    if name.is_empty() {
        return Err("name must be non-empty");
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err("name must be at most 64 characters");
    }
    Ok(name)
}

/// `device-<platform>-<slug of the name>` (ADR-0040 decision 5), before any
/// collision suffix. The slug keeps ASCII letters and digits, lowercased,
/// turns every other run into one `-`, and is cut to 32 characters. A name
/// with nothing sluggable in it (all punctuation, or a script with no ASCII
/// letters) names the device by platform alone — `device-<platform>` — so
/// such a name is still accepted rather than answered 400.
pub fn device_id_base(platform: DevicePlatform, name: &str) -> String {
    let mut slug = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    slug.truncate(MAX_SLUG_CHARS);
    let slug = slug.trim_end_matches('-');
    if slug.is_empty() {
        format!("device-{}", platform.as_str())
    } else {
        format!("device-{}-{slug}", platform.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../fixtures/google-signin-rs256.json");

    fn fixture() -> serde_json::Value {
        serde_json::from_str(FIXTURE).unwrap()
    }

    fn config() -> SigninConfig {
        SigninConfig {
            client_ids: vec![
                "other-client".into(),
                "fixture-client.apps.googleusercontent.com".into(),
            ],
            allowed_sub: "110000000000000000001".into(),
        }
    }

    /// 2025-10-09T08:53:20Z — the fixture token's `iat`, one minute later.
    const NOW_MS: i64 = 1_760_000_060_000;

    fn good_header() -> JwtHeader {
        JwtHeader {
            alg: "RS256".into(),
            kid: Some("fixture-kid-1".into()),
        }
    }

    fn good_claims() -> IdClaims {
        IdClaims {
            iss: Some("https://accounts.google.com".into()),
            aud: Some("fixture-client.apps.googleusercontent.com".into()),
            exp: Some(1_760_003_600),
            iat: Some(1_760_000_000),
            email_verified: Some(serde_json::Value::Bool(true)),
            sub: Some("110000000000000000001".into()),
            email: Some("operator@example.com".into()),
        }
    }

    #[test]
    fn config_needs_both_values_and_splits_the_client_ids() {
        assert_eq!(SigninConfig::from_env(None, Some("sub")), None);
        assert_eq!(SigninConfig::from_env(Some("a"), None), None);
        assert_eq!(SigninConfig::from_env(Some(" , "), Some("sub")), None);
        assert_eq!(SigninConfig::from_env(Some("a"), Some("  ")), None);
        let config = SigninConfig::from_env(Some("a.apps, b.apps\nc.apps"), Some(" 123 ")).unwrap();
        assert_eq!(config.client_ids, ["a.apps", "b.apps", "c.apps"]);
        assert_eq!(config.allowed_sub, "123");
    }

    #[test]
    fn the_fixture_token_parses_into_its_signing_input_and_signature() {
        let token = fixture()["id_token"].as_str().unwrap().to_string();
        let parsed = parse_jwt(&token).expect("a real RS256 JWT parses");
        assert_eq!(parsed.header, good_header());
        // The fixture also carries `azp`, which the verifier never reads.
        assert_eq!(parsed.claims, good_claims());
        assert_eq!(parsed.signature.len(), 256, "an RSA-2048 signature");
        assert_eq!(
            format!(
                "{}.{}",
                parsed.signing_input,
                base64::encode_url(&parsed.signature)
            ),
            token,
        );
    }

    #[test]
    fn a_token_that_is_not_three_json_segments_is_malformed() {
        for token in [
            "",
            "a.b",
            "a.b.c.d",
            "!!.e30.c2ln",
            // `{}` header (no alg) is a missing field, so malformed too.
            "e30.e30.c2ln",
            // A good header and claims with no signature at all.
            "eyJhbGciOiJSUzI1NiJ9.e30.",
            // `exp` as a string is a claim of the wrong type.
            "eyJhbGciOiJSUzI1NiJ9.eyJleHAiOiJzb29uIn0.c2ln",
        ] {
            assert_eq!(
                parse_jwt(token).err(),
                Some(Rejection::Malformed),
                "{token:?}"
            );
        }
    }

    #[test]
    fn a_good_identity_passes_and_names_its_subject() {
        let identity = verify_claims(&good_header(), &good_claims(), NOW_MS, &config()).unwrap();
        assert_eq!(identity.sub, "110000000000000000001");
        assert_eq!(identity.email.as_deref(), Some("operator@example.com"));
        // The other spelling of the issuer passes too.
        let mut claims = good_claims();
        claims.iss = Some("accounts.google.com".into());
        assert!(verify_claims(&good_header(), &claims, NOW_MS, &config()).is_ok());
    }

    /// One acceptance box, claim by claim: each wrong claim is its own
    /// rejection.
    #[test]
    fn each_claim_is_checked() {
        type Mutation = fn(&mut JwtHeader, &mut IdClaims);
        let cases: [(Mutation, Rejection); 14] = [
            (|h, _| h.alg = "HS256".into(), Rejection::WrongAlg),
            (|h, _| h.alg = "none".into(), Rejection::WrongAlg),
            (|h, _| h.kid = None, Rejection::MissingKid),
            (|h, _| h.kid = Some(String::new()), Rejection::MissingKid),
            (
                |_, c| c.iss = Some("https://evil.example".into()),
                Rejection::WrongIssuer,
            ),
            (|_, c| c.iss = None, Rejection::WrongIssuer),
            (
                |_, c| c.aud = Some("someone-else".into()),
                Rejection::WrongAudience,
            ),
            (
                |_, c| {
                    c.aud = Some(serde_json::json!([
                        "fixture-client.apps.googleusercontent.com"
                    ]))
                },
                Rejection::WrongAudience,
            ),
            (|_, c| c.exp = Some(1_760_000_060), Rejection::Expired),
            (
                |_, c| c.iat = Some(1_760_000_060 - IAT_MAX_AGE_SECS - 1),
                Rejection::StaleIat,
            ),
            (
                |_, c| c.iat = Some(1_760_000_060 + IAT_MAX_SKEW_SECS + 1),
                Rejection::StaleIat,
            ),
            (
                |_, c| c.email_verified = Some("true".into()),
                Rejection::EmailUnverified,
            ),
            (|_, c| c.email_verified = None, Rejection::EmailUnverified),
            (
                |_, c| c.sub = Some("110000000000000000002".into()),
                Rejection::OtherSub,
            ),
        ];
        for (mutate, expected) in cases {
            let (mut header, mut claims) = (good_header(), good_claims());
            mutate(&mut header, &mut claims);
            assert_eq!(
                verify_claims(&header, &claims, NOW_MS, &config()).err(),
                Some(expected),
                "{header:?} {claims:?}",
            );
        }
    }

    /// Acceptance: ten minutes into the past and one minute into the
    /// future are both inclusive; one second past either is refused.
    #[test]
    fn iat_may_be_ten_minutes_old_or_a_minute_ahead() {
        let now_secs = NOW_MS / 1000;
        for (iat, accepted) in [
            (now_secs - 600, true),
            (now_secs - 601, false),
            (now_secs + 60, true),
            (now_secs + 61, false),
        ] {
            let mut claims = good_claims();
            claims.iat = Some(iat);
            assert_eq!(
                verify_claims(&good_header(), &claims, NOW_MS, &config()).err(),
                (!accepted).then_some(Rejection::StaleIat),
                "iat = now {:+}",
                iat - now_secs,
            );
        }
    }

    #[test]
    fn an_empty_allowed_sub_never_matches_a_missing_sub() {
        let mut claims = good_claims();
        claims.sub = None;
        let mut config = config();
        config.allowed_sub = String::new();
        assert_eq!(
            verify_claims(&good_header(), &claims, NOW_MS, &config).err(),
            Some(Rejection::OtherSub),
        );
    }

    /// The known-good vector: `openssl pkey -pubout -outform DER` of the
    /// same key the fixture JWK describes.
    #[test]
    fn jwk_to_spki_matches_openssl_for_the_fixture_key() {
        let fixture = fixture();
        let jwk = &fixture["jwks"]["keys"][0];
        let spki = jwk_to_spki(jwk["n"].as_str().unwrap(), jwk["e"].as_str().unwrap()).unwrap();
        let expected = base64::decode(fixture["spki_der_base64"].as_str().unwrap()).unwrap();
        assert_eq!(spki, expected);
    }

    /// The DER corners the 2048-bit vector does not reach: a short-form
    /// length, a magnitude without its high bit set, and leading zeros.
    #[test]
    fn der_integers_are_minimal_and_never_negative() {
        assert_eq!(
            der_uint(&[0x01, 0x00, 0x01]),
            [0x02, 0x03, 0x01, 0x00, 0x01]
        );
        assert_eq!(der_uint(&[0x80]), [0x02, 0x02, 0x00, 0x80]);
        assert_eq!(der_uint(&[0x00, 0x00, 0x7f]), [0x02, 0x01, 0x7f]);
        assert_eq!(der_uint(&[0x00]), [0x02, 0x01, 0x00]);
        assert_eq!(der(0x04, &[0u8; 0x80])[..3], [0x04, 0x81, 0x80]);
        assert_eq!(der(0x04, &[0u8; 0x100])[..4], [0x04, 0x82, 0x01, 0x00]);
    }

    #[test]
    fn a_jwk_with_an_empty_or_undecodable_member_has_no_spki() {
        assert_eq!(jwk_to_spki("", "AQAB"), None);
        assert_eq!(jwk_to_spki("AQAB", ""), None);
        assert_eq!(jwk_to_spki("AQ+B", "AQAB"), None);
    }

    #[test]
    fn the_jwks_cache_holds_rsa_keys_until_max_age() {
        let fixture = fixture();
        let mut body = fixture["jwks"].clone();
        body["keys"].as_array_mut().unwrap().extend([
            serde_json::json!({"kty": "EC", "kid": "ec-key", "crv": "P-256", "x": "AA", "y": "AA"}),
            serde_json::json!({"kty": "RSA", "kid": "broken", "n": "!!", "e": "AQAB"}),
            serde_json::json!({"kty": "RSA", "n": "AQAB", "e": "AQAB"}),
        ]);
        let cache = JwksCache::parse(
            &body.to_string(),
            Some("public, max-age=21600, must-revalidate, no-transform"),
            1_000,
        )
        .unwrap();
        assert!(cache.key("fixture-kid-1").is_some());
        assert_eq!(cache.key("ec-key"), None, "only RSA keys");
        assert_eq!(cache.key("broken"), None, "an undecodable key is skipped");
        assert_eq!(
            cache.lookup("fixture-kid-1", 1_000 + 21_600_000 - 1),
            KeyLookup::Cached(cache.key("fixture-kid-1").unwrap()),
        );
        assert_eq!(
            cache.lookup("fixture-kid-1", 1_000 + 21_600_000),
            KeyLookup::Fetch,
            "expired: re-fetch"
        );
    }

    /// Acceptance: an unknown `kid` re-fetches at most once per
    /// [`UNKNOWN_KID_REFETCH_MS`]. The cache stands for the set the first
    /// such sign-in fetched; a second one inside the interval is refused
    /// without a fetch, and one after it fetches. A known `kid` never does
    /// while the set is fresh.
    #[test]
    fn an_unknown_kid_refetches_at_most_once_per_interval() {
        let fetched_at = 1_000;
        let jwks = fixture()["jwks"].to_string();
        let cache = JwksCache::parse(&jwks, Some("max-age=21600"), fetched_at).unwrap();
        for now in [fetched_at, fetched_at + UNKNOWN_KID_REFETCH_MS - 1] {
            assert_eq!(cache.lookup("made-up", now), KeyLookup::Unknown, "{now}");
            assert!(matches!(
                cache.lookup("fixture-kid-1", now),
                KeyLookup::Cached(_)
            ));
        }
        let later = fetched_at + UNKNOWN_KID_REFETCH_MS;
        assert_eq!(cache.lookup("made-up", later), KeyLookup::Fetch);
        assert!(matches!(
            cache.lookup("fixture-kid-1", later),
            KeyLookup::Cached(_)
        ));

        // The interval holds even when the set has no `max-age` to keep it:
        // an expired set still names the keys Google had a moment ago.
        let unkept = JwksCache::parse(&jwks, None, fetched_at).unwrap();
        assert_eq!(unkept.lookup("made-up", fetched_at + 1), KeyLookup::Unknown);
        assert_eq!(
            unkept.lookup("fixture-kid-1", fetched_at + 1),
            KeyLookup::Fetch
        );
    }

    #[test]
    fn a_jwks_without_max_age_is_good_for_this_request_only() {
        let cache = JwksCache::parse(&fixture()["jwks"].to_string(), None, 5_000).unwrap();
        assert_eq!(cache.lookup("fixture-kid-1", 5_000), KeyLookup::Fetch);
        assert!(cache.key("fixture-kid-1").is_some());
        assert_eq!(JwksCache::parse("<html>", None, 0), None);
    }

    #[test]
    fn max_age_reads_the_directive_and_nothing_else() {
        assert_eq!(
            max_age("public, max-age=19998, must-revalidate"),
            Some(19_998)
        );
        assert_eq!(max_age("MAX-AGE = \"60\""), Some(60));
        assert_eq!(max_age("s-maxage=60, no-cache"), None);
        assert_eq!(max_age("max-age=-1"), None);
        assert_eq!(max_age(""), None);
    }

    #[test]
    fn names_are_trimmed_and_bounded() {
        assert_eq!(validate_name("  Pixel 9  "), Ok("Pixel 9"));
        assert!(validate_name("   ").is_err());
        assert_eq!(
            validate_name(&"é".repeat(64)).map(|n| n.chars().count()),
            Ok(64)
        );
        assert!(validate_name(&"x".repeat(65)).is_err());
    }

    #[test]
    fn the_id_is_platform_then_slug() {
        use DevicePlatform::*;
        assert_eq!(
            device_id_base(Android, "Pixel 9 Pro Fold"),
            "device-android-pixel-9-pro-fold"
        );
        assert_eq!(
            device_id_base(Web, "  John's MacBook (work) "),
            "device-web-john-s-macbook-work"
        );
        assert_eq!(device_id_base(Chrome, "Café—Mac"), "device-chrome-caf-mac");
        assert_eq!(device_id_base(Web, "我的电脑"), "device-web");
        assert_eq!(device_id_base(Web, "!!!"), "device-web");
        let long = device_id_base(Web, &"ab-".repeat(30));
        assert_eq!(long, "device-web-ab-ab-ab-ab-ab-ab-ab-ab-ab-ab-ab");
        assert!(hummingbird_domain::is_url_safe_id(&long));
    }
}
