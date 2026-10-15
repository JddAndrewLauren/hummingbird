//! `POST /api/auth/google` — signing in with Google mints a `device` token,
//! and that is all it does (ADR-0040 decisions 1, 2 and 5). The route is
//! unauthenticated: the Google ID token in the body *is* the credential,
//! checked once, here, and never again — the token it buys is an ordinary
//! `device` token that knows nothing of how it was minted.
//!
//! **Why this is not an arm of `handle()`.** Verifying the token needs two
//! async steps — fetching Google's JWKS and `crypto.subtle.verify` — and
//! `handle()` is sync. So the route is three pure steps with the shim's
//! two runtime steps between them, the calendar-token mint's shape taken
//! one step further:
//!
//! 1. [`begin`] — method, config (503), body (400), the JWT and every claim
//!    (403). Yields a [`PendingSignin`] naming the `kid` to look up.
//! 2. *shim:* the JWKS key for that `kid` — [`PendingSignin::key`] decides
//!    an unknown one (403) — then `crypto.subtle.verify`.
//! 3. [`PendingSignin::signature_checked`] turns the shim's bare `bool`
//!    into a [`SignedIn`] (or a 403), and [`complete`] — the only function
//!    that writes — takes nothing else. Nothing can mint from a token whose
//!    signature was not reported good.
//!
//! **Status contract.** 201 `{id, token}` once; 400 a bad body; 403 —
//! empty, never 401 (ADR-0004 keeps 401 for a held credential the
//! authority refuses) — for every rejected token, with the reason and the
//! rejected `sub` in the log line the shim writes; 405 a wrong method; 429
//! past the mint cap; 503 when either config value is unset, or when
//! Google's JWKS cannot be fetched.

use hummingbird_domain::{DevicePlatform, GoogleSignIn, Scope, SignedInDevice};

use super::admin_tokens::{insert_token, select_token};
use super::{
    empty_status, error, json, method_not_allowed, parse_body, ApiResponse, HandleContext,
};
use crate::codec::RowReader;
use crate::google_signin::{
    device_id_base, parse_jwt, validate_name, verify_claims, JwksCache, Rejection, SigninConfig,
    VerifiedIdentity,
};
use crate::sql::{Sql, SqlError, SqlValue};

/// At most this many mints through this route per rolling hour.
pub const MINTS_PER_HOUR: i64 = 10;

const HOUR_MS: i64 = 3_600_000;

/// A response the route gives without minting, and the line the shim logs
/// for it — `Some` for every 403 (naming the rejected `sub`, so the
/// operator can find their own on the first try) and for the 503s.
#[derive(Debug)]
pub struct Refusal {
    pub response: ApiResponse,
    pub log: Option<String>,
}

impl Refusal {
    fn silent(response: ApiResponse) -> Refusal {
        Refusal {
            response,
            log: None,
        }
    }

    fn rejected(why: Rejection, sub: Option<&str>) -> Refusal {
        Refusal {
            response: empty_status(403),
            log: Some(format!(
                "google sign-in refused ({}): sub={}",
                why.reason(),
                sub.filter(|s| !s.is_empty()).unwrap_or("-"),
            )),
        }
    }
}

/// A sign-in whose claims all passed, waiting on the signature.
#[derive(Debug)]
pub struct PendingSignin {
    /// The JWKS key the shim must find.
    pub kid: String,
    /// The bytes that were signed — the JWT's first two segments.
    pub signing_input: String,
    pub signature: Vec<u8>,
    identity: VerifiedIdentity,
    name: String,
    platform: DevicePlatform,
}

/// A sign-in whose signature the shim verified. Constructed only by
/// [`PendingSignin::signature_checked`].
#[derive(Debug)]
pub struct SignedIn(PendingSignin);

/// Step 1: everything decidable before Google's keys are needed.
#[allow(clippy::result_large_err)] // `Refusal` wraps `ApiResponse`; see `mod.rs`'s `parse_body`.
pub fn begin(
    method: &str,
    body: Option<&str>,
    now_ms: i64,
    config: Option<&SigninConfig>,
) -> Result<PendingSignin, Refusal> {
    if method != "POST" {
        return Err(Refusal::silent(method_not_allowed()));
    }
    let Some(config) = config else {
        return Err(Refusal {
            response: error(
                503,
                "signin_unconfigured",
                "Google sign-in is not configured on this server.",
            ),
            log: Some(
                "google sign-in unavailable: GOOGLE_SIGNIN_CLIENT_IDS or \
                 GOOGLE_SIGNIN_ALLOWED_SUB is unset"
                    .into(),
            ),
        });
    };
    let request: GoogleSignIn = parse_body(body).map_err(Refusal::silent)?;
    let name = validate_name(&request.name)
        .map_err(|message| Refusal::silent(error(400, "validation", message)))?
        .to_string();

    let jwt = parse_jwt(&request.id_token).map_err(|why| Refusal::rejected(why, None))?;
    let identity = verify_claims(&jwt.header, &jwt.claims, now_ms, config)
        .map_err(|why| Refusal::rejected(why, jwt.claims.sub.as_deref()))?;
    Ok(PendingSignin {
        kid: jwt
            .header
            .kid
            .expect("verify_claims refuses a token without a kid"),
        signing_input: jwt.signing_input,
        signature: jwt.signature,
        identity,
        name,
        platform: request.platform,
    })
}

impl PendingSignin {
    /// Step 2's decision: the SPKI DER for this token's `kid` in `jwks` — a
    /// set the shim has just fetched, since a cached set that lacks the
    /// `kid` is re-fetched first. Absent → 403.
    #[allow(clippy::result_large_err)]
    pub fn key<'a>(&self, jwks: &'a JwksCache) -> Result<&'a [u8], Refusal> {
        jwks.key(&self.kid)
            .ok_or_else(|| Refusal::rejected(Rejection::UnknownKid, Some(&self.identity.sub)))
    }

    /// Step 3: the shim's `crypto.subtle.verify` answer, as a type. `false`
    /// is a 403 like any other rejected token.
    #[allow(clippy::result_large_err)]
    pub fn signature_checked(self, verified: bool) -> Result<SignedIn, Refusal> {
        if verified {
            Ok(SignedIn(self))
        } else {
            Err(Refusal::rejected(
                Rejection::BadSignature,
                Some(&self.identity.sub),
            ))
        }
    }
}

/// Google's JWKS could not be fetched or read: nothing can be verified, so
/// nothing is refused either — a 503, never a 403 that would read as "your
/// account is not allowed".
pub fn jwks_unavailable(detail: &str) -> Refusal {
    Refusal {
        response: error(
            503,
            "signin_unavailable",
            "Google's signing keys could not be fetched; try again.",
        ),
        log: Some(format!(
            "google sign-in unavailable: JWKS fetch failed ({detail})"
        )),
    }
}

/// Step 2's other decision: what Google's JWKS response is worth. A non-2xx
/// or a body that is not a JWKS is a 503 like an unreachable host.
#[allow(clippy::result_large_err)]
pub fn jwks_from_response(
    status: u16,
    cache_control: Option<&str>,
    body: &str,
    now_ms: i64,
) -> Result<JwksCache, Refusal> {
    if !(200..300).contains(&status) {
        return Err(jwks_unavailable(&format!("HTTP {status}")));
    }
    JwksCache::parse(body, cache_control, now_ms).ok_or_else(|| jwks_unavailable("not a JWKS"))
}

/// The last step, and the only one that writes: the mint cap, the id, the
/// row. A storage error is the route's 500, as it is for every handler.
pub fn complete(signed_in: SignedIn, ctx: &HandleContext, sql: &dyn Sql) -> ApiResponse {
    mint(signed_in.0, ctx, sql).unwrap_or_else(|e| error(500, "internal", &e.message))
}

fn mint(
    pending: PendingSignin,
    ctx: &HandleContext,
    sql: &dyn Sql,
) -> Result<ApiResponse, SqlError> {
    if mints_in_the_last_hour(ctx.now_ms, sql)? >= MINTS_PER_HOUR {
        return Ok(error(
            429,
            "rate_limited",
            "too many sign-ins in the last hour; try again later",
        ));
    }
    let base = device_id_base(pending.platform, &pending.name);
    let id = first_unused_id(&base, sql)?;
    let token = insert_token(sql, ctx, &id, &pending.name, Scope::Device, None)?;
    Ok(json(201, &SignedInDevice { id, token }))
}

/// ADR-0040 decision 5's cap, counted from the tokens table itself — no new
/// state. A row "came through this route" when it is a `device` row whose
/// id has the shape this route derives, `device-<platform>` or
/// `device-<platform>-…`. Revoked rows count: the mint still happened.
///
/// The count cannot tell this route's rows from an admin-lane mint that
/// happened to choose the same shape (ADR-0037's `device-chrome-<machine>`
/// is one), so such a mint inside the hour also counts against the cap.
/// That errs the safe way — fewer sign-ins, never more.
fn mints_in_the_last_hour(now_ms: i64, sql: &dyn Sql) -> Result<i64, SqlError> {
    let mut shapes = Vec::new();
    let mut params = vec![SqlValue::Integer(now_ms - HOUR_MS)];
    for platform in DevicePlatform::ALL {
        shapes.push("id = ? OR id LIKE ?");
        params.push(SqlValue::Text(format!("device-{}", platform.as_str())));
        params.push(SqlValue::Text(format!("device-{}-%", platform.as_str())));
    }
    let rows = sql.exec(
        &format!(
            "SELECT COUNT(*) AS n FROM tokens \
             WHERE scope = 'device' AND created_at > ? AND ({})",
            shapes.join(" OR "),
        ),
        &params,
    )?;
    match rows.first() {
        Some(row) => RowReader(row).int("n"),
        None => Ok(0),
    }
}

/// `base`, else `base-2`, `base-3`, … — the first id no row holds, revoked
/// rows included, because a revoked id is never reused (ADR-0008).
fn first_unused_id(base: &str, sql: &dyn Sql) -> Result<String, SqlError> {
    if select_token(sql, base)?.is_none() {
        return Ok(base.to_string());
    }
    let mut n = 2;
    loop {
        let candidate = format!("{base}-{n}");
        if select_token(sql, &candidate)?.is_none() {
            return Ok(candidate);
        }
        n += 1;
    }
}
