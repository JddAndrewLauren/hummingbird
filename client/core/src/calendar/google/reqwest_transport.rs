//! [`ReqwestEventsTransport`]: the live [`EventsTransport`] implementation
//! over `calendars/{calendarId}/events`, built on the `reqwest::Client` the
//! core already owns HTTP through (ADR-0003) — the same client works
//! unmodified on `wasm32` (browser Fetch) and on native targets, per
//! [`crate::fetch_status`].
//!
//! No test here ever performs a live network call (#46's acceptance,
//! mirrored from the adapter fixture tests): [`build_events_url`] — the only
//! logic with a branch worth pinning — is a pure function tested in
//! isolation; the request-sending path itself is exercised end-to-end by
//! #71/#73's higher-level tests against scripted/fixture transports.

use super::transport::{EventsTransport, TransportError};

const EVENTS_BASE_URL: &str = "https://www.googleapis.com/calendar/v3/calendars";

/// Builds the `calendars/{calendarId}/events` URL with `singleEvents=true`,
/// `showDeleted=true` and the adapter's window/pagination query params.
///
/// `showDeleted` is explicit because Google defaults it to false: without it
/// a cancelled instance is simply absent from the response, and #71's
/// requirement that a cancellation reach the mapper (and therefore the
/// snapshot, so a previously-fetched instance can be superseded rather than
/// linger) would be satisfied only by the fixture tests, never live.
fn build_events_url(
    calendar_id: &str,
    time_min: &str,
    time_max: &str,
    page_token: Option<&str>,
) -> String {
    let encoded_calendar_id = urlencode(calendar_id);
    let mut url = format!(
        "{EVENTS_BASE_URL}/{encoded_calendar_id}/events\
         ?singleEvents=true&showDeleted=true&timeMin={}&timeMax={}",
        urlencode(time_min),
        urlencode(time_max),
    );
    if let Some(token) = page_token {
        url.push_str("&pageToken=");
        url.push_str(&urlencode(token));
    }
    url
}

/// A minimal, dependency-free percent-encoder sufficient for the fixed
/// alphabet these query values are drawn from (RFC 3339 timestamps,
/// provider-issued calendar ids and page tokens) — this module reaches for
/// nothing beyond what `reqwest`/`chrono` already pull in.
fn urlencode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[derive(Debug)]
pub struct ReqwestEventsTransport {
    client: reqwest::Client,
}

impl Default for ReqwestEventsTransport {
    fn default() -> Self {
        Self::new(reqwest::Client::new())
    }
}

impl ReqwestEventsTransport {
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
impl EventsTransport for ReqwestEventsTransport {
    async fn fetch_page(
        &self,
        calendar_id: &str,
        access_token: &str,
        time_min: &str,
        time_max: &str,
        page_token: Option<&str>,
    ) -> Result<String, TransportError> {
        let url = build_events_url(calendar_id, time_min, time_max, page_token);
        let response = self
            .client
            .get(url)
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(|source| TransportError::new(source.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            return Err(TransportError::http(
                status.as_u16(),
                format!("Google returned HTTP {status}"),
            ));
        }

        response
            .text()
            .await
            .map_err(|source| TransportError::new(source.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_url_carries_the_window_and_single_events_flag() {
        let url = build_events_url(
            "primary",
            "2024-06-08T12:00:00Z",
            "2024-09-13T12:00:00Z",
            None,
        );
        assert_eq!(
            url,
            "https://www.googleapis.com/calendar/v3/calendars/primary/events\
             ?singleEvents=true&showDeleted=true\
             &timeMin=2024-06-08T12%3A00%3A00Z&timeMax=2024-09-13T12%3A00%3A00Z"
        );
    }

    #[test]
    fn events_url_asks_for_deleted_events_because_google_omits_them_by_default() {
        // The mapper handles cancellations and the fixture tests cover them,
        // but Google's `showDeleted` defaults to false — without this
        // parameter no live response would ever contain one to map.
        let url = build_events_url("primary", "min", "max", None);
        assert!(url.contains("showDeleted=true"));
    }

    #[test]
    fn events_url_appends_the_page_token_when_present() {
        let url = build_events_url("primary", "min", "max", Some("page-2"));
        assert!(url.ends_with("&pageToken=page-2"));
    }

    #[test]
    fn calendar_ids_containing_reserved_characters_are_percent_encoded() {
        let url = build_events_url("team@example.com", "min", "max", None);
        assert!(url.contains("/calendars/team%40example.com/events"));
    }
}
