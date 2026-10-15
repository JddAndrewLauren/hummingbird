//! The device lane's calls (#839, ADR-0040 decisions 1 and 6): trading a
//! Google ID token for a `device` token, listing the operator's devices,
//! and revoking one. [`crate::Core::sign_in_with_google`],
//! [`crate::Core::devices`] and [`crate::Core::revoke_device`] are the
//! doors; this module is what they send and how each answer is read.
//!
//! **None of these is a queued write.** A sign-in has no credential to
//! queue under, a list is a read, and a revoke the operator watches for
//! must answer now — so each is one [`DirectTransport`] call, and a
//! connection failure is an outcome, never a retry.
//!
//! **The core never persists a credential, and a sign-in does not even
//! hold one.** [`SignInOutcome::SignedIn`] hands the token back to the
//! host, which stores it where it always has and pushes it through
//! [`crate::Core::push_api_key`] exactly as a pasted token flows (ADR-0040
//! decision 1) — so nothing after that moment knows how it was minted.
//!
//! How the rows are ordered and worded is a decision, and lives in
//! [`crate::decisions::devices`].

use hummingbird_domain::{
    is_url_safe_id, ApiError, DeviceInfo, DevicePlatform, GoogleSignIn, SignedInDevice,
};

use crate::sync::write::{DirectMethod, DirectRequest, RawResponse};
use crate::sync::TransportError;

/// `POST /api/auth/google` — unauthenticated; the ID token is the
/// credential.
pub const SIGN_IN_PATH: &str = "/api/auth/google";

/// `GET /api/devices`.
pub const DEVICES_PATH: &str = "/api/devices";

/// `DELETE /api/devices/:id`.
pub fn device_path(id: &str) -> String {
    format!("{DEVICES_PATH}/{id}")
}

/// What one sign-in exchange came to. Each refusal the route can give is
/// its own arm, so a client can say "not your account", "too many
/// sign-ins, try later" and "sign-in isn't set up" without reading a
/// status code itself.
#[derive(Debug, Clone, PartialEq)]
pub enum SignInOutcome {
    /// 201: the authority minted this device's token. The host stores
    /// `token` and then pushes it through [`crate::Core::push_api_key`];
    /// the core has not kept it.
    SignedIn(SignedInDevice),
    /// 403: the Google account is not the one allowed to sign in, or its
    /// ID token was refused (expired, wrong audience, bad signature).
    NotAllowed,
    /// 429: more than the route's hourly mint cap.
    RateLimited,
    /// 503 `signin_unconfigured`: the server has no sign-in client ids or
    /// allowed subject configured.
    Unconfigured,
    /// Any other 503 — `signin_unavailable` (the server could not fetch
    /// Google's signing keys), or one whose body names no code this lane
    /// knows. Nothing was refused; try again. Only the one explicit code
    /// reads as "not set up", so an outage never does (#839's ruling).
    Unavailable,
    /// 400: the server refused the body — in practice the device name
    /// (empty, or longer than the server allows). `message` is the
    /// server's.
    Invalid { message: String },
    /// No response arrived, or one this lane does not expect (any other
    /// status, or a 201 that is not a token).
    Failed { message: String },
}

impl SignInOutcome {
    /// The arm's wire name, the same on both seams.
    pub fn kind(&self) -> &'static str {
        match self {
            SignInOutcome::SignedIn(_) => "signed_in",
            SignInOutcome::NotAllowed => "not_allowed",
            SignInOutcome::RateLimited => "rate_limited",
            SignInOutcome::Unconfigured => "unconfigured",
            SignInOutcome::Unavailable => "unavailable",
            SignInOutcome::Invalid { .. } => "invalid",
            SignInOutcome::Failed { .. } => "failed",
        }
    }
}

/// What one device list came to.
#[derive(Debug, Clone, PartialEq)]
pub enum DevicesOutcome {
    /// Every `device`-scope row the authority holds, in its own order —
    /// [`crate::decisions::devices::device_rows`] decides the shown one.
    Listed(Vec<DeviceInfo>),
    /// No credential to send: none was ever supplied, or the held one was
    /// already refused. Nothing was sent.
    NoCredential,
    /// 401: the authority refused this device's token. The core now holds
    /// it, exactly as a 401 in a sync cycle does.
    CredentialRejected,
    /// No response arrived, or an unexpected one.
    Failed { message: String },
}

impl DevicesOutcome {
    pub fn kind(&self) -> &'static str {
        match self {
            DevicesOutcome::Listed(_) => "listed",
            DevicesOutcome::NoCredential => "no_credential",
            DevicesOutcome::CredentialRejected => "credential_rejected",
            DevicesOutcome::Failed { .. } => "failed",
        }
    }
}

/// What one revoke came to.
#[derive(Debug, Clone, PartialEq)]
pub enum RevokeOutcome {
    /// Another device is signed out; the row as it now stands.
    Revoked(DeviceInfo),
    /// This device is signed out: the core no longer holds a credential,
    /// and every queued write is still queued (ADR-0004).
    SignedOut(DeviceInfo),
    /// 403: one of the holders a device may not revoke (`runner`,
    /// `openclaw-agent`).
    NotRevocable,
    /// 404: no such device — or an id that names no `device` row.
    NotFound,
    /// As [`DevicesOutcome::NoCredential`].
    NoCredential,
    /// As [`DevicesOutcome::CredentialRejected`].
    CredentialRejected,
    /// No response arrived, or an unexpected one.
    Failed { message: String },
}

impl RevokeOutcome {
    pub fn kind(&self) -> &'static str {
        match self {
            RevokeOutcome::Revoked(_) => "revoked",
            RevokeOutcome::SignedOut(_) => "signed_out",
            RevokeOutcome::NotRevocable => "not_revocable",
            RevokeOutcome::NotFound => "not_found",
            RevokeOutcome::NoCredential => "no_credential",
            RevokeOutcome::CredentialRejected => "credential_rejected",
            RevokeOutcome::Failed { .. } => "failed",
        }
    }
}

pub(crate) fn sign_in_request(id_token: &str, name: &str, platform: DevicePlatform) -> DirectRequest {
    let body = GoogleSignIn {
        id_token: id_token.to_string(),
        name: name.to_string(),
        platform,
    };
    DirectRequest {
        method: DirectMethod::Post,
        path: SIGN_IN_PATH.to_string(),
        body: Some(serde_json::to_string(&body).expect("GoogleSignIn serializes")),
    }
}

pub(crate) fn devices_request() -> DirectRequest {
    DirectRequest {
        method: DirectMethod::Get,
        path: DEVICES_PATH.to_string(),
        body: None,
    }
}

/// `None` for an id no route could name — every token id the authority
/// mints is URL-safe (`admin_tokens::mint`, `auth_google`'s slug), so such
/// an id is no device, and nothing is sent for it.
pub(crate) fn revoke_request(id: &str) -> Option<DirectRequest> {
    is_url_safe_id(id).then(|| DirectRequest {
        method: DirectMethod::Delete,
        path: device_path(id),
        body: None,
    })
}

fn unexpected(response: &RawResponse) -> String {
    format!("unexpected {} from the authority: {}", response.status, response.body)
}

/// The `error` code of a JSON error body, if it is one.
fn error_code(body: &str) -> Option<String> {
    serde_json::from_str::<ApiError>(body).ok().map(|e| e.error)
}

pub(crate) fn sign_in_outcome(response: Result<RawResponse, TransportError>) -> SignInOutcome {
    let response = match response {
        Ok(response) => response,
        Err(error) => return SignInOutcome::Failed { message: error.message },
    };
    match response.status {
        201 => match serde_json::from_str::<SignedInDevice>(&response.body) {
            Ok(device) if !device.token.is_empty() => SignInOutcome::SignedIn(device),
            _ => SignInOutcome::Failed {
                message: "the authority's 201 carried no token".to_string(),
            },
        },
        400 => SignInOutcome::Invalid {
            message: serde_json::from_str::<ApiError>(&response.body)
                .map(|e| e.message)
                .unwrap_or(response.body),
        },
        403 => SignInOutcome::NotAllowed,
        429 => SignInOutcome::RateLimited,
        503 if error_code(&response.body).as_deref() == Some("signin_unconfigured") => {
            SignInOutcome::Unconfigured
        }
        503 => SignInOutcome::Unavailable,
        _ => SignInOutcome::Failed {
            message: unexpected(&response),
        },
    }
}

pub(crate) fn devices_outcome(response: Result<RawResponse, TransportError>) -> DevicesOutcome {
    let response = match response {
        Ok(response) => response,
        Err(error) => return DevicesOutcome::Failed { message: error.message },
    };
    match response.status {
        200 => match serde_json::from_str::<Vec<DeviceInfo>>(&response.body) {
            Ok(devices) => DevicesOutcome::Listed(devices),
            Err(error) => DevicesOutcome::Failed {
                message: format!("unreadable device list: {error}"),
            },
        },
        401 => DevicesOutcome::CredentialRejected,
        _ => DevicesOutcome::Failed {
            message: unexpected(&response),
        },
    }
}

pub(crate) fn revoke_outcome(response: Result<RawResponse, TransportError>) -> RevokeOutcome {
    let response = match response {
        Ok(response) => response,
        Err(error) => return RevokeOutcome::Failed { message: error.message },
    };
    match response.status {
        200 => match serde_json::from_str::<DeviceInfo>(&response.body) {
            Ok(device) if device.is_current => RevokeOutcome::SignedOut(device),
            Ok(device) => RevokeOutcome::Revoked(device),
            Err(error) => RevokeOutcome::Failed {
                message: format!("unreadable revoked device: {error}"),
            },
        },
        401 => RevokeOutcome::CredentialRejected,
        403 => RevokeOutcome::NotRevocable,
        404 => RevokeOutcome::NotFound,
        _ => RevokeOutcome::Failed {
            message: unexpected(&response),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response(status: u16, body: &str) -> Result<RawResponse, TransportError> {
        Ok(RawResponse {
            status,
            body: body.to_string(),
        })
    }

    #[test]
    fn the_sign_in_body_is_the_routes_own_dto() {
        let request = sign_in_request("eyJ.id.token", "Laptop", DevicePlatform::Web);
        assert_eq!(request.method, DirectMethod::Post);
        assert_eq!(request.path, "/api/auth/google");
        let body: GoogleSignIn = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
        assert_eq!(
            body,
            GoogleSignIn {
                id_token: "eyJ.id.token".to_string(),
                name: "Laptop".to_string(),
                platform: DevicePlatform::Web,
            }
        );
    }

    #[test]
    fn each_sign_in_status_is_its_own_outcome() {
        assert_eq!(
            sign_in_outcome(response(201, r#"{"id":"device-web-laptop","token":"hb_abc"}"#)),
            SignInOutcome::SignedIn(SignedInDevice {
                id: "device-web-laptop".to_string(),
                token: "hb_abc".to_string(),
            })
        );
        assert_eq!(sign_in_outcome(response(403, "")), SignInOutcome::NotAllowed);
        assert_eq!(
            sign_in_outcome(response(429, r#"{"error":"rate_limited","message":"x"}"#)),
            SignInOutcome::RateLimited
        );
        assert_eq!(
            sign_in_outcome(response(503, r#"{"error":"signin_unconfigured","message":"x"}"#)),
            SignInOutcome::Unconfigured
        );
        assert_eq!(
            sign_in_outcome(response(503, r#"{"error":"signin_unavailable","message":"x"}"#)),
            SignInOutcome::Unavailable
        );
        for unrecognised in ["", "Service Unavailable", r#"{"error":"overloaded","message":"x"}"#] {
            assert_eq!(
                sign_in_outcome(response(503, unrecognised)),
                SignInOutcome::Unavailable,
                "an unrecognised 503 never reads as not set up: {unrecognised:?}"
            );
        }
        assert_eq!(
            sign_in_outcome(response(
                400,
                r#"{"error":"validation","message":"name must be non-empty"}"#
            )),
            SignInOutcome::Invalid {
                message: "name must be non-empty".to_string()
            }
        );
    }

    #[test]
    fn a_sign_in_that_cannot_be_read_as_a_token_fails() {
        for outcome in [
            sign_in_outcome(response(201, "not json")),
            sign_in_outcome(response(201, r#"{"id":"device-web","token":""}"#)),
            sign_in_outcome(response(500, "boom")),
            sign_in_outcome(response(401, "")),
            sign_in_outcome(Err(TransportError::new("offline"))),
        ] {
            assert_eq!(outcome.kind(), "failed", "{outcome:?}");
        }
    }

    #[test]
    fn a_revoke_request_names_only_a_url_safe_id() {
        let request = revoke_request("device-web-laptop").unwrap();
        assert_eq!(request.method, DirectMethod::Delete);
        assert_eq!(request.path, "/api/devices/device-web-laptop");
        assert!(request.body.is_none());
        assert_eq!(revoke_request("../tokens"), None);
        assert_eq!(revoke_request(""), None);
    }

    #[test]
    fn a_revoke_answer_tells_this_device_from_another() {
        let row = |is_current: bool| {
            format!(
                r#"{{"id":"device-web-laptop","name":"Laptop","platform":"web","created_at":1,
                "last_seen":2,"revoked_at":3,"is_current":{is_current},"revocable":true}}"#
            )
        };
        assert_eq!(revoke_outcome(response(200, &row(true))).kind(), "signed_out");
        assert_eq!(revoke_outcome(response(200, &row(false))).kind(), "revoked");
        assert_eq!(revoke_outcome(response(401, "")), RevokeOutcome::CredentialRejected);
        assert_eq!(revoke_outcome(response(403, "")), RevokeOutcome::NotRevocable);
        assert_eq!(
            revoke_outcome(response(404, r#"{"error":"not_found","message":"no such device"}"#)),
            RevokeOutcome::NotFound
        );
        assert_eq!(revoke_outcome(response(500, "")).kind(), "failed");
    }

    #[test]
    fn a_device_list_reads_the_wire_rows_or_fails() {
        let body = r#"[{"id":"device-web","name":"Web","platform":"web","created_at":1,
            "last_seen":null,"revoked_at":null,"is_current":true,"revocable":true}]"#;
        match devices_outcome(response(200, body)) {
            DevicesOutcome::Listed(devices) => {
                assert_eq!(devices.len(), 1);
                assert!(devices[0].is_current);
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(devices_outcome(response(401, "")), DevicesOutcome::CredentialRejected);
        assert_eq!(devices_outcome(response(200, "{}")).kind(), "failed");
        assert_eq!(devices_outcome(Err(TransportError::new("offline"))).kind(), "failed");
    }
}
