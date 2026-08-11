//! `graph-calendar-poll`'s shell: `std::env`, the app-only Graph
//! client-credentials token acquisition, and the Graph + authority HTTP
//! calls. Everything it *decides* lives in `hummingbird_graph_poll` and is
//! natively tested — `graph_mail_poll`'s own discipline.
//!
//! ```text
//! POST https://login.microsoftonline.com/{tenant}/oauth2/v2.0/token → an access token
//! GET  /api/rules                                        → the live rule set
//! GET  /api/snapshots?source=m365-calendar/v1&key=cursor  → the stored deltaLink, if any
//! GET  the stored deltaLink (or, on a lost/first cursor, a bounded calendarView/delta resync)
//! POST /api/alerts                                       → once per matching event
//! POST /api/snapshots                                    → the advanced deltaLink, last
//! ```
//!
//! **Every Graph calendar request carries `Prefer: outlook.timezone="UTC"`**
//! — `calendar_item.rs`'s own module doc explains why this header is
//! load-bearing, not cosmetic.
//!
//! **The bounded resync differs from the mail lane's**: Graph's
//! `calendarView/delta` (unlike `messages/delta`) both accepts and
//! *requires* `startDateTime`/`endDateTime` on its initial request — the
//! documented, standard way to bound a calendar delta resync, so (unlike
//! mail) one call supplies both the catch-up items and the fresh
//! `deltaLink`, `calendar_poll::main::bounded_resync`'s own shape. The
//! window is bounded on both sides: [`RESYNC_LOOKBACK_MS`] catches an
//! event already in progress, [`RESYNC_LOOKAHEAD_MS`] catches near-term
//! future events without pulling the operator's entire calendar history.
//!
//! There is no separate `busy_now` snapshot here — #137's brief, unlike
//! #136's, names only the evaluated stream for M365 calendar.
//!
//! Exit codes: `0` success, `1` anything else. The cursor is written last,
//! `graph_mail_poll`'s own discipline.

use std::process::ExitCode;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use hummingbird_domain::{now_as_deadline, Rule};
use hummingbird_graph_poll::{
    calendar_cursor_envelope, client_assertion, evaluate_events, fold_calendar_items, parse_access_token,
    parse_calendar_cursor, parse_delta_page, plan_alert, resume, token_url, CertCredential, DeltaOutcome, Plan,
    CALENDAR_CURSOR_KEY, CALENDAR_SOURCE, CLIENT_ASSERTION_TYPE, GRAPH_SCOPE,
};

const HTTP_TIMEOUT: Duration = Duration::from_secs(30);
const GRAPH_API: &str = "https://graph.microsoft.com/v1.0";
const OUTLOOK_TIMEZONE_UTC: (&str, &str) = ("Prefer", r#"outlook.timezone="UTC""#);

const RESYNC_LOOKBACK_MS: i64 = 24 * 60 * 60 * 1000;
const RESYNC_LOOKAHEAD_MS: i64 = 90 * 24 * 60 * 60 * 1000;

fn main() -> ExitCode {
    match run() {
        Ok(summary) => {
            println!(
                "graph-calendar-poll: {} event(s) matched, {} alert(s) posted, cursor -> {}",
                summary.events_fetched, summary.alerts_posted, summary.new_delta_link
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("graph-calendar-poll: {e}");
            ExitCode::FAILURE
        }
    }
}

struct Summary {
    events_fetched: usize,
    alerts_posted: usize,
    new_delta_link: String,
}

fn run() -> Result<Summary, String> {
    let base_url = env("HB_BASE_URL")?;
    let base_url = base_url.trim_end_matches('/');
    let hb_token = env("HB_INGEST_TOKEN")?;
    let upn = env("GRAPH_MAILBOX_UPN")?;

    let access_token = graph_access_token()?;

    let rules = get_rules(base_url, &hb_token)?;
    let stored_cursor = get_cursor(base_url, &hb_token)?;

    let plan = match &stored_cursor {
        None => Plan::Resync,
        Some(delta_link) => match graph_delta_sync(&access_token, delta_link) {
            Ok(page) => resume(Some(delta_link), DeltaOutcome::Page(page)),
            Err(GraphError::Status(410)) => resume(Some(delta_link), DeltaOutcome::Expired),
            Err(e) => return Err(e.to_string()),
        },
    };

    let (raw_items, new_delta_link) = match plan {
        Plan::Advance { raw_items, new_delta_link } => (raw_items, new_delta_link),
        Plan::Resync => bounded_resync(&access_token, &upn)?,
    };

    let batch = fold_calendar_items(&raw_items);
    for (id, reason) in &batch.skipped {
        eprintln!("graph-calendar-poll: skipping item {id}: {reason}");
    }

    let now = now_as_deadline(now_ms()?);
    let matches = evaluate_events(&rules, &batch.candidates, &now);
    for m in &matches {
        post_alert(base_url, &hb_token, &plan_alert(m))?;
    }

    // Written last, deliberately — see the module doc.
    post_cursor(base_url, &hb_token, &new_delta_link)?;

    Ok(Summary { events_fetched: batch.candidates.len(), alerts_posted: matches.len(), new_delta_link })
}

/// The bounded resync: `calendarView/delta`'s own initial request, bounded
/// by `startDateTime`/`endDateTime` — this file's own module doc.
fn bounded_resync(access_token: &str, upn: &str) -> Result<(Vec<String>, String), String> {
    let now = now_ms()?;
    let start = rfc3339_ms(now - RESYNC_LOOKBACK_MS)?;
    let end = rfc3339_ms(now + RESYNC_LOOKAHEAD_MS)?;
    let start_url = format!("{GRAPH_API}/users/{upn}/calendarView/delta");
    let query = vec![("startDateTime".to_string(), start), ("endDateTime".to_string(), end)];

    let mut raw_items = Vec::new();
    let mut url = start_url;
    let mut first = true;
    let mut delta_link = None;
    loop {
        let body = if first {
            graph_get(access_token, &url, &query, &[OUTLOOK_TIMEZONE_UTC]).map_err(|e| e.to_string())?
        } else {
            graph_get(access_token, &url, &[], &[OUTLOOK_TIMEZONE_UTC]).map_err(|e| e.to_string())?
        };
        first = false;
        let page = parse_delta_page(&body).map_err(|e| e.to_string())?;
        raw_items.extend(page.raw_items);
        if page.delta_link.is_some() {
            delta_link = page.delta_link;
        }
        match page.next_link {
            Some(next) => url = next,
            None => break,
        }
    }
    let delta_link = delta_link.ok_or_else(|| "graph calendar: bounded resync's final page carried no @odata.deltaLink".to_string())?;
    Ok((raw_items, delta_link))
}

// ------------------------------------------------------------- env/clock

fn env(name: &str) -> Result<String, String> {
    match std::env::var(name) {
        Ok(value) if !value.trim().is_empty() => Ok(value),
        _ => Err(format!("{name} is not set")),
    }
}

fn now_ms() -> Result<i64, String> {
    let since = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e| e.to_string())?;
    i64::try_from(since.as_millis()).map_err(|e| e.to_string())
}

fn rfc3339_ms(ms: i64) -> Result<String, String> {
    jiff::Timestamp::from_millisecond(ms).map(|t| t.to_string()).map_err(|e| e.to_string())
}

// ------------------------------------------------------------------ HTTP

#[derive(Debug)]
enum GraphError {
    Status(u16),
    Transport(String),
    Parse(String),
}

impl std::fmt::Display for GraphError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GraphError::Status(s) => write!(f, "graph api: HTTP {s}"),
            GraphError::Transport(e) => write!(f, "graph api: {e}"),
            GraphError::Parse(e) => write!(f, "graph api: {e}"),
        }
    }
}

fn graph_access_token() -> Result<String, String> {
    let cred = CertCredential {
        tenant_id: env("GRAPH_TENANT_ID")?,
        client_id: env("GRAPH_CLIENT_ID")?,
        thumbprint_b64url: env("GRAPH_CERT_THUMBPRINT_B64URL")?,
        private_key_pem: env("GRAPH_CLIENT_PRIVATE_KEY")?,
    };
    let now_secs = now_ms()? / 1000;
    let jti = format!("{now_secs}-{}", std::process::id());
    let assertion = client_assertion(&cred, now_secs, &jti).map_err(|e| e.to_string())?;

    let response = ureq::post(token_url(&cred.tenant_id))
        .config()
        .timeout_global(Some(HTTP_TIMEOUT))
        .build()
        .send_form([
            ("client_id", cred.client_id.as_str()),
            ("scope", GRAPH_SCOPE),
            ("client_assertion_type", CLIENT_ASSERTION_TYPE),
            ("client_assertion", assertion.as_str()),
            ("grant_type", "client_credentials"),
        ])
        .map_err(|e| format!("graph oauth token request: {e}"))?;
    let text = response
        .into_body()
        .read_to_string()
        .map_err(|e| format!("graph oauth token request: reading response: {e}"))?;
    parse_access_token(&text, now_ms()?)
        .map(|t| t.token)
        .ok_or_else(|| "graph oauth token request: no access_token in response".to_string())
}

fn graph_get(access_token: &str, url: &str, query: &[(String, String)], extra_headers: &[(&str, &str)]) -> Result<String, GraphError> {
    let mut req = ureq::get(url).header("Authorization", &format!("Bearer {access_token}"));
    for (k, v) in extra_headers {
        req = req.header(*k, *v);
    }
    for (k, v) in query {
        req = req.query(k.as_str(), v.as_str());
    }
    let response = req.config().timeout_global(Some(HTTP_TIMEOUT)).build().call();
    match response {
        Ok(mut r) => r.body_mut().read_to_string().map_err(|e| GraphError::Parse(e.to_string())),
        Err(ureq::Error::StatusCode(status)) => Err(GraphError::Status(status)),
        Err(e) => Err(GraphError::Transport(e.to_string())),
    }
}

/// Replays a stored delta URL to completion — `graph_mail_poll`'s own
/// pagination shape, with the UTC timezone preference on every request.
fn graph_delta_sync(access_token: &str, start_url: &str) -> Result<hummingbird_graph_poll::DeltaPage, GraphError> {
    let mut page = hummingbird_graph_poll::DeltaPage::default();
    let mut url = start_url.to_string();
    loop {
        let body = graph_get(access_token, &url, &[], &[OUTLOOK_TIMEZONE_UTC])?;
        let this_page = parse_delta_page(&body).map_err(|e| GraphError::Parse(e.to_string()))?;
        page.raw_items.extend(this_page.raw_items);
        if this_page.delta_link.is_some() {
            page.delta_link = this_page.delta_link;
        }
        match this_page.next_link {
            Some(next) => url = next,
            None => break,
        }
    }
    Ok(page)
}

// --------------------------------------------------------- authority API

fn hb_get(base_url: &str, token: &str, path: &str, query: Option<&str>) -> Result<Option<String>, String> {
    let url = match query {
        Some(q) => format!("{base_url}{path}?{q}"),
        None => format!("{base_url}{path}"),
    };
    let response = ureq::get(&url)
        .header("Authorization", &format!("Bearer {token}"))
        .config()
        .timeout_global(Some(HTTP_TIMEOUT))
        .build()
        .call();
    match response {
        Ok(mut r) => r.body_mut().read_to_string().map(Some).map_err(|e| e.to_string()),
        Err(ureq::Error::StatusCode(404)) => Ok(None),
        Err(e) => Err(format!("GET {path}: {e}")),
    }
}

fn hb_post(base_url: &str, token: &str, path: &str, body: &serde_json::Value) -> Result<(), String> {
    ureq::post(&format!("{base_url}{path}"))
        .header("Authorization", &format!("Bearer {token}"))
        .config()
        .timeout_global(Some(HTTP_TIMEOUT))
        .build()
        .send_json(body)
        .map_err(|e| format!("POST {path}: {e}"))?;
    Ok(())
}

fn get_rules(base_url: &str, token: &str) -> Result<Vec<Rule>, String> {
    let text = hb_get(base_url, token, "/api/rules", None)?.ok_or_else(|| "GET /api/rules: unexpectedly absent".to_string())?;
    serde_json::from_str(&text).map_err(|e| format!("GET /api/rules: {e}"))
}

fn get_cursor(base_url: &str, token: &str) -> Result<Option<String>, String> {
    let query = format!("source={CALENDAR_SOURCE}&key={CALENDAR_CURSOR_KEY}");
    match hb_get(base_url, token, "/api/snapshots", Some(&query))? {
        None => Ok(None),
        Some(text) => {
            let row: serde_json::Value = serde_json::from_str(&text).map_err(|e| format!("reading cursor: {e}"))?;
            let payload = row.get("payload").and_then(|v| v.as_str()).ok_or_else(|| "reading cursor: no `payload` in response".to_string())?;
            parse_calendar_cursor(payload).map(Some).map_err(|e| e.to_string())
        }
    }
}

fn post_cursor(base_url: &str, token: &str, delta_link: &str) -> Result<(), String> {
    hb_post(
        base_url,
        token,
        "/api/snapshots",
        &serde_json::json!({
            "source": CALENDAR_SOURCE,
            "key": CALENDAR_CURSOR_KEY,
            "payload": calendar_cursor_envelope(delta_link),
            "fetched_at": now_ms()?,
        }),
    )
}

fn post_alert(base_url: &str, token: &str, ingest: &hummingbird_domain::AlertIngest) -> Result<(), String> {
    hb_post(base_url, token, "/api/alerts", &serde_json::to_value(ingest).map_err(|e| e.to_string())?)
}
