//! The runtime half of the FCM send leg (#219): the two steps that cannot
//! be decided in a pure crate — signing the OAuth assertion with WebCrypto,
//! and making the HTTP calls.
//!
//! Everything else lives in [`hummingbird_authority::fcm`]'s pure surface
//! (the assertion's bytes, the message body, what a response *means*, the
//! revocation write), where a native test can execute it. This file is
//! deliberately as thin as the rest of the shim: it holds no policy, only
//! `crypto.subtle` and `fetch`. That matters more here than elsewhere —
//! `server/worker` has no test harness of any kind, so anything expressed
//! in this file is untested by construction.
//!
//! # Credential
//!
//! `FCM_SERVICE_ACCOUNT` — the Google service-account JSON key, as a
//! **Worker secret** (`wrangler secret put FCM_SERVICE_ACCOUNT`), never in
//! `wrangler.toml` and never in the repo. Unset or malformed, the lane
//! fails closed exactly as `ADMIN_SECRET` does: [`FcmSender::from_env`]
//! returns `None`, and `alarm()` logs each transition it could not send.
//!
//! # No retry
//!
//! `deliver` commits the delivery row before the send is attempted, so a
//! retry that re-sent would double-ring. A failed send is logged and
//! dropped; only `UNREGISTERED` writes, revoking the dead target. See
//! `hummingbird_authority::fcm`'s module doc for the full argument.

use std::cell::RefCell;

use hummingbird_authority::{
    assemble_assertion, assertion_signing_input, classify_response, message_json,
    parse_access_token, pkcs8_der_from_pem, send_url, token_request_body, AccessToken, PushNotification,
    SendVerdict, ServiceAccount, OAUTH_TOKEN_URL,
};
use hummingbird_domain::PushTarget;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use worker::*;

use crate::http::post;

/// The RS256 key/signature algorithm, spelled as WebCrypto names it.
const RSASSA_PKCS1_V1_5: &str = "RSASSA-PKCS1-v1_5";

/// Holds the operator credential and caches the access token it buys.
pub struct FcmSender {
    account: ServiceAccount,
    /// Google's access tokens last about an hour, and a Durable Object
    /// instance normally outlives many 15-minute alarm ticks, so caching
    /// here turns four OAuth round trips an hour into one. Not persisted:
    /// an evicted instance simply mints a fresh token on its next tick.
    cached: RefCell<Option<AccessToken>>,
}

impl FcmSender {
    /// Reads and parses the `FCM_SERVICE_ACCOUNT` secret. `None` — unset,
    /// or not a usable key file — means the send leg is not configured;
    /// the caller logs rather than failing the tick, because a tick that
    /// died here would also stop rescheduling the alarm.
    pub fn from_env(env: &Env) -> Option<FcmSender> {
        let secret = env.secret("FCM_SERVICE_ACCOUNT").ok()?.to_string();
        match ServiceAccount::parse(&secret) {
            Ok(account) => Some(FcmSender { account, cached: RefCell::new(None) }),
            Err(e) => {
                console_error!("FCM_SERVICE_ACCOUNT is set but unusable ({e:?}); no push will send");
                None
            }
        }
    }

    /// Sends one notification to one device, and reports what FCM said.
    /// Never retries (see the module doc); a transport-level failure is
    /// reported as [`SendVerdict::Failed`], indistinguishable to the caller
    /// from an FCM rejection, because the response to both is the same.
    pub async fn send(
        &self,
        notification: &PushNotification,
        target: &PushTarget,
        now_ms: i64,
    ) -> SendVerdict {
        let access_token = match self.access_token(now_ms).await {
            Ok(token) => token,
            Err(e) => return SendVerdict::Failed { detail: format!("no access token: {e}") },
        };
        let body = message_json(notification, &target.fcm_token);
        match post(&send_url(&self.account.project_id), &body, Some(&access_token), "application/json")
            .await
        {
            Ok((status, response_body)) => classify_response(status, &response_body),
            Err(e) => SendVerdict::Failed { detail: format!("fcm request failed: {e}") },
        }
    }

    /// The cached access token, minting a new one when there is none or the
    /// stored one is within its expiry slack.
    async fn access_token(&self, now_ms: i64) -> Result<String> {
        // Cloned out of the RefCell rather than borrowed across the await
        // below — a borrow held over a suspension point would panic the
        // moment two sends overlapped.
        let cached = self.cached.borrow().clone();
        if let Some(token) = cached {
            if now_ms < token.expires_at_ms {
                return Ok(token.token);
            }
        }

        let minted = self.mint_access_token(now_ms).await?;
        let token = minted.token.clone();
        *self.cached.borrow_mut() = Some(minted);
        Ok(token)
    }

    async fn mint_access_token(&self, now_ms: i64) -> Result<AccessToken> {
        let der = pkcs8_der_from_pem(&self.account.private_key)
            .map_err(|e| Error::RustError(format!("FCM private key is unusable: {e:?}")))?;
        let signing_input = assertion_signing_input(&self.account.client_email, now_ms / 1000);
        let signature = sign_rs256(&der, &signing_input).await?;
        let assertion = assemble_assertion(&signing_input, &signature);

        let (status, body) = post(
            OAUTH_TOKEN_URL,
            &token_request_body(&assertion),
            None,
            "application/x-www-form-urlencoded",
        )
        .await?;

        // The body is deliberately not logged: on success it *is* the
        // bearer token.
        parse_access_token(&body, now_ms)
            .ok_or_else(|| Error::RustError(format!("OAuth token request returned HTTP {status}")))
    }
}

/// RS256-signs `signing_input` with a PKCS#8 DER private key, via the
/// runtime's `crypto.subtle`. The key is imported non-extractable and with
/// `sign` as its only usage, so nothing downstream of here can read it back
/// out or repurpose it.
async fn sign_rs256(pkcs8_der: &[u8], signing_input: &str) -> Result<Vec<u8>> {
    let subtle = subtle_crypto()?;

    let hash = js_sys::Object::new();
    js_sys::Reflect::set(&hash, &"name".into(), &"SHA-256".into()).map_err(js_err)?;
    let algorithm = js_sys::Object::new();
    js_sys::Reflect::set(&algorithm, &"name".into(), &RSASSA_PKCS1_V1_5.into()).map_err(js_err)?;
    js_sys::Reflect::set(&algorithm, &"hash".into(), &hash).map_err(js_err)?;

    // `Uint8Array::from` copies into the JS heap. A wasm-memory *view*
    // would be the alternative, and would dangle if the allocator moved
    // linear memory while `sign` was still awaiting it.
    let key_data = js_sys::Uint8Array::from(pkcs8_der);
    let usages = js_sys::Array::of1(&JsValue::from_str("sign"));
    let key = JsFuture::from(
        subtle
            .import_key_with_object("pkcs8", &key_data, &algorithm, false, &usages)
            .map_err(js_err)?,
    )
    .await
    .map_err(js_err)?
    .dyn_into::<web_sys::CryptoKey>()
    .map_err(|_| Error::RustError("importKey did not yield a CryptoKey".into()))?;

    let data = js_sys::Uint8Array::from(signing_input.as_bytes());
    let signature = JsFuture::from(
        subtle
            .sign_with_str_and_js_u8_array(RSASSA_PKCS1_V1_5, &key, &data)
            .map_err(js_err)?,
    )
    .await
    .map_err(js_err)?;

    Ok(js_sys::Uint8Array::new(&signature).to_vec())
}

/// `crypto.subtle` off the worker global. Reached through `Reflect` rather
/// than `web_sys::window()`: a Worker has no `window`, and no `document`.
fn subtle_crypto() -> Result<web_sys::SubtleCrypto> {
    let crypto = js_sys::Reflect::get(&js_sys::global(), &"crypto".into())
        .map_err(js_err)?
        .dyn_into::<web_sys::Crypto>()
        .map_err(|_| Error::RustError("the runtime exposes no `crypto` global".into()))?;
    Ok(crypto.subtle())
}

fn js_err(value: JsValue) -> Error {
    Error::RustError(format!("{value:?}"))
}
