//! `graph-mail-poll`'s shell: `std::env`, the app-only Graph client-credentials
//! token acquisition, and the Graph + authority HTTP calls. Everything it
//! *decides* lives in `hummingbird_graph_poll` and is natively tested —
//! this file is the untestable edge, `gmail_poll::main`'s own discipline.
//!
//! ```text
//! POST https://login.microsoftonline.com/{tenant}/oauth2/v2.0/token → an access token
//! GET  /api/rules                                    → the live rule set
//! GET  /api/snapshots?source=m365-mail/v1&key=cursor  → the stored deltaLink, if any
//! GET  the stored deltaLink (or, on a lost/first cursor, a bounded resync)
//! POST /api/alerts                                   → once per matching event
//! POST /api/snapshots                                → the advanced deltaLink, last
//! ```
//!
//! **The bounded resync is one bounded `delta` chain** — the shape
//! `graph_calendar_poll::bounded_resync` already uses: an initial delta
//! request bounded by `$filter=receivedDateTime ge …`, followed to the end
//! of its `@odata.nextLink` chain, whose last page carries the
//! `@odata.deltaLink` this run stores.
//!
//! It was two calls until #486 Phase B — a `$deltatoken=latest` anchor plus
//! an ordinary (non-delta) `$filter` listing — resting on a belief that
//! design stated but, in its own words, could not check against a live
//! tenant. Checked on 2026-08-15, both halves of it are false:
//!
//! - the mail delta query **does** accept `$filter=receivedDateTime ge …`,
//!   and the bounded chain terminates with a `deltaLink` — one page for a
//!   two-day window on a 14k-message inbox;
//! - `$deltatoken=latest` is **silently ignored** on this resource. It
//!   anchors nothing: the response is byte-identical to an unparameterized
//!   initial delta — the first page of a full enumeration of the folder,
//!   carrying items and a `$skiptoken`, no `deltaLink`. Following *that*
//!   chain to its end would page the whole mailbox (~1,400 requests here),
//!   which is why the fix was a different query, not a missing loop.
//!
//! One chain also dissolves the ordering hazard the two-call design had to
//! reason about: with no boundary between an anchor and a listing there is
//! no hole to convert into an overlap, and the cursor comes from the same
//! chain that produced the items. The overlap that remains (a message
//! evaluated here reappearing once the cursor advances) is harmless:
//! `alert::plan`'s never-sent `raised_at` plus the ingest upsert's
//! no-op-on-identical-payload rule make a re-evaluation safe, never a
//! duplicate alert — the same tolerance `gmail_poll`/`calendar_poll`
//! already rely on for their own resync overlap.
//!
//! Exit codes: `0` success, `1` anything else. **The cursor is written
//! last** — a crash before it lands makes the *next* run re-fetch and
//! re-evaluate the same batch, which the ingest upsert's own dedupe makes
//! a safe re-fetch, never a duplicate alert.

use std::process::ExitCode;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use hummingbird_domain::{now_as_deadline, Rule};
use hummingbird_graph_poll::{
    client_assertion, evaluate_events, fold_mail_messages, mail_cursor_envelope, parse_access_token,
    parse_delta_page, parse_mail_cursor, plan_alert, resume, token_url, CertCredential, DeltaOutcome, Plan,
    CLIENT_ASSERTION_TYPE, GRAPH_SCOPE, MAIL_CURSOR_KEY, MAIL_SOURCE,
};

const HTTP_TIMEOUT: Duration = Duration::from_secs(30);
const GRAPH_API: &str = "https://graph.microsoft.com/v1.0";

/// The bounded resync's own lookback window — `gmail_poll::RESYNC_QUERY`'s
/// own reasoning, bounded on both axes so a resync can never balloon into
/// "fetch the whole mailbox."
const RESYNC_LOOKBACK_MS: i64 = 2 * 24 * 60 * 60 * 1000;

/// Graph's mail delta pages hold ten items by default, and this lane's
/// window is routinely a few dozen messages — asking for a bigger page
/// turns a handful of round trips into one. It is a request, not a
/// guarantee: `bounded_resync` follows `@odata.nextLink` regardless.
const MAX_PAGE_SIZE: (&str, &str) = ("Prefer", "odata.maxpagesize=100");

fn main() -> ExitCode {
    match run() {
        Ok(summary) => {
            println!(
                "graph-mail-poll: {} event(s) matched, {} alert(s) posted, cursor -> {}",
                summary.events_fetched, summary.alerts_posted, summary.new_delta_link
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("graph-mail-poll: {e}");
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

    let batch = fold_mail_messages(&raw_items);
    for (id, reason) in &batch.skipped {
        eprintln!("graph-mail-poll: skipping item {id}: {reason}");
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

/// The bounded catch-up (this file's own module doc): one initial `delta`
/// request bounded by `$filter=receivedDateTime ge …`, followed to the end
/// of its `@odata.nextLink` chain. The items and the fresh cursor both come
/// out of that one chain.
///
/// `$filter` is stated on the **first** request only — every `nextLink`
/// Graph hands back already carries the window forward, exactly as
/// `graph_calendar_poll::bounded_resync` treats its `startDateTime`/
/// `endDateTime` bounds. A `deltaLink` is kept once seen rather than
/// re-set to `None` by a later page (that lane's own rule): Graph emits it
/// on the final page, anchored at the end of the sync.
fn bounded_resync(access_token: &str, upn: &str) -> Result<(Vec<String>, String), String> {
    let cutoff = rfc3339_ms(now_ms()? - RESYNC_LOOKBACK_MS)?;
    let query = vec![("$filter".to_string(), format!("receivedDateTime ge {cutoff}"))];

    let mut url = format!("{GRAPH_API}/users/{upn}/mailFolders/inbox/messages/delta");
    let mut first = true;
    let mut raw_items = Vec::new();
    let mut delta_link = None;
    loop {
        let body = if first {
            graph_get(access_token, &url, &query, &[MAX_PAGE_SIZE]).map_err(|e| e.to_string())?
        } else {
            graph_get(access_token, &url, &[], &[MAX_PAGE_SIZE]).map_err(|e| e.to_string())?
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

    let delta_link = delta_link.ok_or_else(|| "graph mail: bounded resync's final page carried no @odata.deltaLink".to_string())?;
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

/// Mints an access token via the certificate-based client-credentials
/// grant (`hummingbird_graph_poll::auth`'s own doc: the assertion, RS256
/// signature included, is built and tested in the lib; this function is
/// only the HTTP POST and the env read).
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

/// Replays a stored (or paginated) delta URL to completion, accumulating
/// every page's raw items — `gmail_poll::gmail_history_list`'s own
/// pagination shape, generalized over Graph's `@odata.nextLink`.
fn graph_delta_sync(access_token: &str, start_url: &str) -> Result<hummingbird_graph_poll::DeltaPage, GraphError> {
    let mut page = hummingbird_graph_poll::DeltaPage::default();
    let mut url = start_url.to_string();
    loop {
        let body = graph_get(access_token, &url, &[], &[])?;
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
    let query = format!("source={MAIL_SOURCE}&key={MAIL_CURSOR_KEY}");
    match hb_get(base_url, token, "/api/snapshots", Some(&query))? {
        None => Ok(None),
        Some(text) => {
            let row: serde_json::Value = serde_json::from_str(&text).map_err(|e| format!("reading cursor: {e}"))?;
            let payload = row.get("payload").and_then(|v| v.as_str()).ok_or_else(|| "reading cursor: no `payload` in response".to_string())?;
            parse_mail_cursor(payload).map(Some).map_err(|e| e.to_string())
        }
    }
}

fn post_cursor(base_url: &str, token: &str, delta_link: &str) -> Result<(), String> {
    hb_post(
        base_url,
        token,
        "/api/snapshots",
        &serde_json::json!({
            "source": MAIL_SOURCE,
            "key": MAIL_CURSOR_KEY,
            "payload": mail_cursor_envelope(delta_link),
            "fetched_at": now_ms()?,
        }),
    )
}

fn post_alert(base_url: &str, token: &str, ingest: &hummingbird_domain::AlertIngest) -> Result<(), String> {
    hb_post(base_url, token, "/api/alerts", &serde_json::to_value(ingest).map_err(|e| e.to_string())?)
}
