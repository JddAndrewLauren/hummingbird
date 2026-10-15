//! The runtime half of `POST /api/auth/google` (ADR-0040 decision 4): the
//! two steps that cannot be decided in a pure crate — fetching Google's
//! JWKS, and `crypto.subtle.verify` over the token's signature.
//!
//! Everything else — the claims, the allowlist, the JWKS parse and its
//! `max-age`, the JWK → SPKI conversion, the name, the id and the mint cap
//! — is `hummingbird_authority::auth_google` and `google_signin`, natively
//! tested. This file holds no policy, only the order of those pure steps
//! with the two awaits between them; `server/worker` has no test harness,
//! so anything expressed here is untested by construction. The import path
//! below (`importKey("spki", …)` then `verify`) is mirrored, with the same
//! algorithm parameters, by `server/worker/tests/rs256-import.test.mjs`
//! against the fixture key.
//!
//! # Configuration
//!
//! `GOOGLE_SIGNIN_CLIENT_IDS` is a wrangler **var** (client ids are public
//! by design); `GOOGLE_SIGNIN_ALLOWED_SUB` is a Worker **secret**. Either
//! unset, the route answers 503. Locally both come from `.dev.vars`.

use std::cell::RefCell;

use hummingbird_authority::auth_google::{self, Refusal};
use hummingbird_authority::{
    ApiResponse, HandleContext, JwksCache, SigninConfig, Sql, GOOGLE_JWKS_URL,
};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use worker::*;

use crate::fcm::{js_err, rs256_import_algorithm, subtle_crypto, RSASSA_PKCS1_V1_5};
use crate::http::get;

pub struct GoogleSignin {
    config: Option<SigninConfig>,
    /// Google's keys, kept for as long as the JWKS response's `max-age`
    /// says and re-fetched early on an unknown `kid`. Not persisted: an
    /// evicted instance fetches again on its next sign-in.
    jwks: RefCell<Option<JwksCache>>,
}

impl GoogleSignin {
    pub fn from_env(env: &Env) -> GoogleSignin {
        let client_ids = env
            .var("GOOGLE_SIGNIN_CLIENT_IDS")
            .ok()
            .map(|v| v.to_string());
        let allowed_sub = env
            .secret("GOOGLE_SIGNIN_ALLOWED_SUB")
            .ok()
            .map(|v| v.to_string());
        GoogleSignin {
            config: SigninConfig::from_env(client_ids.as_deref(), allowed_sub.as_deref()),
            jwks: RefCell::new(None),
        }
    }

    /// The whole route. Never fails: every outcome, a storage error
    /// included, is a response.
    pub async fn respond(
        &self,
        method: &str,
        body: Option<&str>,
        ctx: &HandleContext<'_>,
        sql: &dyn Sql,
    ) -> ApiResponse {
        let pending = match auth_google::begin(method, body, ctx.now_ms, self.config.as_ref()) {
            Ok(pending) => pending,
            Err(refusal) => return logged(refusal),
        };
        let spki = match self.spki_for(&pending, ctx.now_ms).await {
            Ok(spki) => spki,
            Err(refusal) => return logged(refusal),
        };
        let verified = match verify_rs256(&spki, &pending.signing_input, &pending.signature).await {
            Ok(verified) => verified,
            Err(e) => {
                // A key WebCrypto will not import, or a verify that throws,
                // verifies nothing: the token is refused like a bad signature.
                console_error!("google sign-in: signature check failed to run: {e}");
                false
            }
        };
        match pending.signature_checked(verified) {
            Ok(signed_in) => auth_google::complete(signed_in, ctx, sql),
            Err(refusal) => logged(refusal),
        }
    }

    /// The SPKI DER for the token's `kid`: from the cache while it is fresh
    /// and holds the `kid`, otherwise from a fresh fetch, which replaces the
    /// cache.
    async fn spki_for(
        &self,
        pending: &auth_google::PendingSignin,
        now_ms: i64,
    ) -> std::result::Result<Vec<u8>, Refusal> {
        // Copied out of the RefCell rather than borrowed across the await
        // below, as `FcmSender::access_token` does.
        let cached = self
            .jwks
            .borrow()
            .as_ref()
            .and_then(|jwks| jwks.fresh_key(&pending.kid, now_ms))
            .map(<[u8]>::to_vec);
        if let Some(spki) = cached {
            return Ok(spki);
        }
        let (status, cache_control, body) = get(GOOGLE_JWKS_URL)
            .await
            .map_err(|e| auth_google::jwks_unavailable(&e.to_string()))?;
        let fresh =
            auth_google::jwks_from_response(status, cache_control.as_deref(), &body, now_ms)?;
        let spki = pending.key(&fresh).map(<[u8]>::to_vec);
        *self.jwks.borrow_mut() = Some(fresh);
        spki
    }
}

fn logged(refusal: Refusal) -> ApiResponse {
    if let Some(line) = &refusal.log {
        console_warn!("{line}");
    }
    refusal.response
}

/// Verifies an RS256 signature with an SPKI DER public key, via the
/// runtime's `crypto.subtle`. The key is imported non-extractable with
/// `verify` as its only usage.
async fn verify_rs256(spki_der: &[u8], signing_input: &str, signature: &[u8]) -> Result<bool> {
    let subtle = subtle_crypto()?;
    let algorithm = rs256_import_algorithm()?;
    // `Uint8Array::from` copies into the JS heap, as `fcm::sign_rs256` does.
    let key_data = js_sys::Uint8Array::from(spki_der);
    let usages = js_sys::Array::of1(&"verify".into());
    let key = JsFuture::from(
        subtle
            .import_key_with_object("spki", &key_data, &algorithm, false, &usages)
            .map_err(js_err)?,
    )
    .await
    .map_err(js_err)?
    .dyn_into::<web_sys::CryptoKey>()
    .map_err(|_| Error::RustError("importKey did not yield a CryptoKey".into()))?;

    let signature = js_sys::Uint8Array::from(signature);
    let data = js_sys::Uint8Array::from(signing_input.as_bytes());
    let verified = JsFuture::from(
        subtle
            .verify_with_str_and_js_u8_array_and_js_u8_array(
                RSASSA_PKCS1_V1_5,
                &key,
                &signature,
                &data,
            )
            .map_err(js_err)?,
    )
    .await
    .map_err(js_err)?;
    Ok(verified.as_bool() == Some(true))
}
