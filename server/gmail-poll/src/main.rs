//! The poller's shell: `std::env`, the Google OAuth token exchange, and the
//! Gmail + authority HTTP calls. Everything it *decides* lives in the lib
//! and is natively tested — this file is the untestable edge, kept as
//! small as `server/city-waste/src/main.rs` is for the same reason.
//!
//! ```text
//! POST https://oauth2.googleapis.com/token   → an access token
//! GET  /api/rules                            → the live rule set
//! GET  /api/snapshots?source=gmail/v1&key=cursor → the stored historyId, if any
//! GET  gmail history.list (or, on a lost cursor, messages.list + getProfile)
//! GET  gmail messages.get, once per new message id
//! POST /api/alerts                           → once per matching event
//! POST /api/snapshots                        → the advanced cursor, last
//! ```
//!
//! Exit codes: `0` success, `1` anything else. The cursor is written last
//! (see the module doc's "a lost cursor" section) — a crash before it lands
//! makes the *next* run re-fetch and re-evaluate the same batch, which
//! `alert::plan`'s never-sent `raised_at` plus the ingest upsert's
//! no-op-on-identical-payload rule make a safe re-fetch, never a duplicate
//! alert.

use std::process::ExitCode;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use hummingbird_domain::{now_as_deadline, Rule};
use hummingbird_gmail_poll::{
    cursor_envelope, evaluate_events, fold_messages, parse_cursor, parse_history_list,
    parse_messages_list, parse_profile, plan_alert, resume, HistoryOutcome, Plan, CURSOR_KEY,
    SOURCE,
};

const HTTP_TIMEOUT: Duration = Duration::from_secs(30);
const GMAIL_API: &str = "https://gmail.googleapis.com/gmail/v1/users/me";
const OAUTH_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";

/// The bounded re-sync's own window, for a lost or first-ever cursor
/// (ADR-0011: "losing a cursor degrades to re-fetch-and-upsert"). Bounded
/// on both axes — a time window and a result cap — so a re-sync can never
/// balloon into "fetch the whole mailbox."
const RESYNC_QUERY: &str = "newer_than:2d";
const RESYNC_MAX_RESULTS: u32 = 200;

fn main() -> ExitCode {
    match run() {
        Ok(summary) => {
            println!(
                "gmail-poll: {} event(s) matched, {} alert(s) posted, cursor -> {}",
                summary.events_fetched, summary.alerts_posted, summary.new_history_id
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("gmail-poll: {e}");
            ExitCode::FAILURE
        }
    }
}

struct Summary {
    events_fetched: usize,
    alerts_posted: usize,
    new_history_id: String,
}

fn run() -> Result<Summary, String> {
    let base_url = env("HB_BASE_URL")?;
    let base_url = base_url.trim_end_matches('/');
    let hb_token = env("HB_INGEST_TOKEN")?;

    let access_token = oauth_access_token(
        &env("GOOGLE_CLIENT_ID")?,
        &env("GOOGLE_CLIENT_SECRET")?,
        &env("GOOGLE_REFRESH_TOKEN")?,
    )?;

    let rules = get_rules(base_url, &hb_token)?;
    let stored_cursor = get_cursor(base_url, &hb_token)?;

    // `main.rs`'s one job in the cursor-loss decision: read the real HTTP
    // status and map it onto `HistoryOutcome` — everything downstream of
    // that mapping is `resume`'s pure decision (`hummingbird_gmail_poll`'s
    // module doc; #264 review item 5). A transport error or a non-404
    // status is not a cursor-loss case at all and aborts the run here,
    // before anything is fetched or written.
    let plan = match &stored_cursor {
        // No cursor ever written — the same bounded start a lost cursor
        // gets (`resume`'s own doc); there is no `history.list` call to
        // make at all, since there is no `historyId` to start it from.
        None => Plan::Resync,
        Some(history_id) => match gmail_history_list(&access_token, history_id) {
            Ok(page) => resume(Some(history_id), HistoryOutcome::Page(page)),
            Err(GmailError::Status(404)) => resume(Some(history_id), HistoryOutcome::Expired),
            Err(e) => return Err(e.to_string()),
        },
    };

    let (message_ids, new_history_id) = match plan {
        Plan::Advance { message_ids, new_history_id } => (message_ids, new_history_id),
        Plan::Resync => bounded_resync(&access_token)?,
    };

    // The per-message fetch/parse fold (#264 review item 4): a transient
    // fetch failure aborts the whole run via `?`, before `post_cursor` is
    // ever reached, so that message's id stays inside the *next* poll's
    // `history.list` window rather than being lost the moment the cursor
    // advances past it. A permanently unparseable message is skipped
    // loudly (logged below) but does not abort the batch.
    let batch = fold_messages(&message_ids, |id| gmail_get_message(&access_token, id))?;
    for (id, reason) in &batch.unparseable {
        eprintln!("gmail-poll: skipping {id}, unparseable: {reason}");
    }
    let events = batch.events;

    let now = now_as_deadline(now_ms()?);
    let matches = evaluate_events(&rules, &events, &now);
    for m in &matches {
        post_alert(base_url, &hb_token, &plan_alert(m))?;
    }

    // Written last, deliberately — see the module doc.
    post_cursor(base_url, &hb_token, &new_history_id)?;

    Ok(Summary {
        events_fetched: events.len(),
        alerts_posted: matches.len(),
        new_history_id,
    })
}

fn bounded_resync(access_token: &str) -> Result<(Vec<String>, String), String> {
    let ids = gmail_messages_list(access_token, RESYNC_QUERY, RESYNC_MAX_RESULTS)?;
    let history_id = gmail_profile(access_token)?;
    Ok((ids, history_id))
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

// ------------------------------------------------------------------ HTTP

#[derive(Debug)]
enum GmailError {
    Status(u16),
    Transport(String),
    Parse(String),
}

impl std::fmt::Display for GmailError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GmailError::Status(s) => write!(f, "gmail api: HTTP {s}"),
            GmailError::Transport(e) => write!(f, "gmail api: {e}"),
            GmailError::Parse(e) => write!(f, "gmail api: {e}"),
        }
    }
}

fn oauth_access_token(client_id: &str, client_secret: &str, refresh_token: &str) -> Result<String, String> {
    let response = ureq::post(OAUTH_TOKEN_URL)
        .config()
        .timeout_global(Some(HTTP_TIMEOUT))
        .build()
        .send_form([
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("refresh_token", refresh_token),
            ("grant_type", "refresh_token"),
        ])
        .map_err(|e| format!("oauth token refresh: {e}"))?;
    let text = response
        .into_body()
        .read_to_string()
        .map_err(|e| format!("oauth token refresh: reading response: {e}"))?;
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("oauth token refresh: {e}"))?;
    value
        .get("access_token")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .ok_or_else(|| "oauth token refresh: no access_token in response".to_string())
}

fn gmail_get(access_token: &str, url: &str, query: &[(&str, &str)]) -> Result<String, GmailError> {
    let mut req = ureq::get(url).header("Authorization", &format!("Bearer {access_token}"));
    for (k, v) in query {
        req = req.query(*k, *v);
    }
    let response = req.config().timeout_global(Some(HTTP_TIMEOUT)).build().call();
    match response {
        Ok(mut r) => r.body_mut().read_to_string().map_err(|e| GmailError::Parse(e.to_string())),
        Err(ureq::Error::StatusCode(status)) => Err(GmailError::Status(status)),
        Err(e) => Err(GmailError::Transport(e.to_string())),
    }
}

fn gmail_history_list(access_token: &str, start_history_id: &str) -> Result<hummingbird_gmail_poll::HistoryPage, GmailError> {
    let url = format!("{GMAIL_API}/history");
    let mut page = hummingbird_gmail_poll::HistoryPage::default();
    let mut page_token: Option<String> = None;
    loop {
        let mut query: Vec<(&str, &str)> = vec![
            ("startHistoryId", start_history_id),
            ("historyTypes", "messageAdded"),
        ];
        if let Some(token) = &page_token {
            query.push(("pageToken", token));
        }
        let body = gmail_get(access_token, &url, &query)?;
        let this_page = parse_history_list(&body).map_err(|e| GmailError::Parse(e.to_string()))?;
        page.message_ids.extend(this_page.message_ids);
        if this_page.history_id.is_some() {
            page.history_id = this_page.history_id;
        }
        match this_page.next_page_token {
            Some(token) => page_token = Some(token),
            None => break,
        }
    }
    Ok(page)
}

fn gmail_messages_list(access_token: &str, query: &str, max_results: u32) -> Result<Vec<String>, String> {
    let url = format!("{GMAIL_API}/messages");
    let body = gmail_get(
        access_token,
        &url,
        &[("q", query), ("maxResults", &max_results.to_string())],
    )
    .map_err(|e| e.to_string())?;
    parse_messages_list(&body).map_err(|e| e.to_string())
}

fn gmail_profile(access_token: &str) -> Result<String, String> {
    let url = format!("{GMAIL_API}/profile");
    let body = gmail_get(access_token, &url, &[]).map_err(|e| e.to_string())?;
    parse_profile(&body).map_err(|e| e.to_string())
}

fn gmail_get_message(access_token: &str, id: &str) -> Result<String, String> {
    let url = format!("{GMAIL_API}/messages/{id}");
    gmail_get(access_token, &url, &[("format", "full")]).map_err(|e| e.to_string())
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
    let text = hb_get(base_url, token, "/api/rules", None)?
        .ok_or_else(|| "GET /api/rules: unexpectedly absent".to_string())?;
    serde_json::from_str(&text).map_err(|e| format!("GET /api/rules: {e}"))
}

/// `Ok(None)` is the legitimate "no cursor written yet" state — the
/// authority answers 404 for it precisely so the two are distinguishable.
fn get_cursor(base_url: &str, token: &str) -> Result<Option<String>, String> {
    let query = format!("source={SOURCE}&key={CURSOR_KEY}");
    match hb_get(base_url, token, "/api/snapshots", Some(&query))? {
        None => Ok(None),
        Some(text) => {
            let row: serde_json::Value =
                serde_json::from_str(&text).map_err(|e| format!("reading cursor: {e}"))?;
            let payload = row
                .get("payload")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "reading cursor: no `payload` in response".to_string())?;
            parse_cursor(payload).map(Some).map_err(|e| e.to_string())
        }
    }
}

fn post_cursor(base_url: &str, token: &str, history_id: &str) -> Result<(), String> {
    hb_post(
        base_url,
        token,
        "/api/snapshots",
        &serde_json::json!({
            "source": SOURCE,
            "key": CURSOR_KEY,
            "payload": cursor_envelope(history_id),
            "fetched_at": now_ms()?,
        }),
    )
}

fn post_alert(base_url: &str, token: &str, ingest: &hummingbird_domain::AlertIngest) -> Result<(), String> {
    hb_post(base_url, token, "/api/alerts", &serde_json::to_value(ingest).map_err(|e| e.to_string())?)
}
