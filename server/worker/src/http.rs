//! The raw HTTP calls every `wasm32` shim consumer shares: the POST behind
//! `fcm.rs`'s OAuth and send legs and `calendar.rs`'s token exchange, and
//! the GET behind `signin.rs`'s JWKS fetch. Holds no policy of its own — a non-2xx is a normal return, not an error, because
//! what it *means* is always a pure-crate decision.

use wasm_bindgen::JsValue;
use worker::*;

/// One POST, returning the status and body together — the pair every
/// classifier in the pure crate takes.
pub async fn post(
    url: &str,
    body: &str,
    bearer: Option<&str>,
    content_type: &str,
) -> Result<(u16, String)> {
    let headers = Headers::new();
    headers.set("content-type", content_type)?;
    if let Some(token) = bearer {
        headers.set("authorization", &format!("Bearer {token}"))?;
    }

    let mut init = RequestInit::new();
    init.with_method(Method::Post)
        .with_headers(headers)
        .with_body(Some(JsValue::from_str(body)));

    let mut response = Fetch::Request(Request::new_with_init(url, &init)?).send().await?;
    let status = response.status_code();
    Ok((status, response.text().await?))
}

/// One GET, returning the status, the `Cache-Control` header and the body —
/// the triple `auth_google::jwks_from_response` takes.
pub async fn get(url: &str) -> Result<(u16, Option<String>, String)> {
    let mut response = Fetch::Url(Url::parse(url)?).send().await?;
    let status = response.status_code();
    let cache_control = response.headers().get("cache-control")?;
    Ok((status, cache_control, response.text().await?))
}
