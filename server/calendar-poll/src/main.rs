//! The poller's shell: `std::env`, the Google OAuth token exchange, and the
//! Calendar + authority HTTP calls. Everything it *decides* lives in the
//! lib and is natively tested — this file is the untestable edge, kept as
//! small as `gmail_poll::main` is for the same reason.
//!
//! ```text
//! POST https://oauth2.googleapis.com/token        → an access token
//! GET  /api/rules                                 → the live rule set
//! GET  /api/snapshots?source=google-calendar/v1&key=cursor → the stored syncToken, if any
//! GET  calendar events.list (syncToken, or on a lost cursor, a bounded full sync)
//! POST /api/alerts                                → once per matching event
//! GET  calendar events.list (timeMin/timeMax around "now", job 2)
//! POST /api/snapshots                             → the busy_now gauge
//! POST /api/snapshots                             → the advanced syncToken cursor, last
//! ```
//!
//! **None of the three `events.list` query builders below ever set a
//! `fields` projection.** The brief's own acceptance criterion is that the
//! request asks for `transparency` and the operator's own `responseStatus`
//! — both are part of the Calendar API's *default* full Event resource, so
//! the way to "ask for" them here is exactly the way to lose them: adding a
//! `fields=items(id,summary,start,end)`-shaped restriction (the usual
//! payload-trimming move, and the one #135's `messages.get(format=full)`
//! makes the opposite call on) would silently drop both, and with them the
//! brief's transparent/free and declined exclusions — `busy.rs` would then
//! read every event as opaque and every attendee as accepted, and
//! over-suppress nothing it should. `calendar_event.rs`'s parse already
//! reads `transparency`/`attendees[].responseStatus` from exactly this
//! unrestricted shape.
//!
//! Exit codes: `0` success, `1` anything else. **The sync-token cursor is
//! written last**, `gmail_poll::main`'s own discipline: a crash before it
//! lands makes the *next* run re-fetch and re-evaluate the same batch,
//! which `alert::plan`'s never-sent `raised_at` plus the ingest upsert's
//! no-op-on-identical-payload rule make a safe re-fetch, never a duplicate
//! alert. The busy snapshot is written before the cursor for the same
//! reason applied to job 2: if it fails, the cursor must not advance either
//! — the run is retried as a whole next poll rather than half-landing.
//!
//! **`SHARED_LIST_PARAMS` is the sync and full builders' one shared
//! identity** (`singleEvents=true`, `showDeleted=true`), built once and
//! used by both — Google binds a `syncToken` to the parameters of the full
//! sync that minted it, so the two must never drift. The bug this exists to
//! prevent (round 1 of PR #268's review: the incremental builder alone
//! omitted `singleEvents`) could not be seen by any fixture-shaped test,
//! since fixtures are hand-built already in instance shape — so
//! `sync_query`/`full_query` are pure and unit-tested directly on their
//! constructed parameters, in this file's own `#[cfg(test)]` module.

use std::process::ExitCode;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use hummingbird_domain::{now_as_deadline, Rule};
use hummingbird_calendar_poll::{
    busy_envelope, busy_window, cursor_envelope, evaluate_events, fold_events, live_calendar_events,
    parse_cursor, parse_events_list, plan_alert, resume, Plan, SyncOutcome, BUSY_KEY, CURSOR_KEY, SOURCE,
};

const HTTP_TIMEOUT: Duration = Duration::from_secs(30);
const CALENDAR_API: &str = "https://www.googleapis.com/calendar/v3/calendars/primary/events";
const OAUTH_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";

/// The bounded re-sync's own window, for a lost or first-ever cursor
/// (`gmail_poll::main`'s own `RESYNC_QUERY`/`RESYNC_MAX_RESULTS`, applied
/// here to Calendar's `updatedMin` rather than Gmail's search grammar).
/// Bounded on both axes so a re-sync can never balloon into "fetch the
/// whole calendar's history."
const RESYNC_LOOKBACK_MS: i64 = 2 * 24 * 60 * 60 * 1000;
const RESYNC_MAX_RESULTS: u32 = 250;

/// The busy job's own query window — generous enough to catch a
/// long-running or currently-in-progress event without needing exact
/// bounds, since [`hummingbird_calendar_poll::busy_window`] does the real
/// filtering in memory. 24 hours each direction comfortably covers any
/// ordinary meeting, including one that started yesterday evening and
/// crosses midnight.
const BUSY_WINDOW_MS: i64 = 24 * 60 * 60 * 1000;

fn main() -> ExitCode {
    match run() {
        Ok(summary) => {
            println!(
                "calendar-poll: {} event(s) matched, {} alert(s) posted, busy={}, cursor -> {}",
                summary.events_fetched, summary.alerts_posted, summary.busy, summary.new_sync_token
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("calendar-poll: {e}");
            ExitCode::FAILURE
        }
    }
}

struct Summary {
    events_fetched: usize,
    alerts_posted: usize,
    busy: bool,
    new_sync_token: String,
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
    // status and map it onto `SyncOutcome` — everything downstream of that
    // mapping is `resume`'s pure decision. A transport error or a non-410
    // status is not a cursor-loss case at all and aborts the run here,
    // before anything is fetched or written.
    let plan = match &stored_cursor {
        None => Plan::Resync,
        Some(sync_token) => match calendar_events_list_sync(&access_token, sync_token) {
            Ok(page) => resume(Some(sync_token), SyncOutcome::Page(page)),
            Err(CalendarError::Status(410)) => resume(Some(sync_token), SyncOutcome::Expired),
            Err(e) => return Err(e.to_string()),
        },
    };

    let (raw_events, new_sync_token) = match plan {
        Plan::Advance { raw_events, new_sync_token } => (raw_events, new_sync_token),
        Plan::Resync => bounded_resync(&access_token)?,
    };

    let batch = fold_events(&raw_events);
    for (id, reason) in &batch.skipped {
        eprintln!("calendar-poll: skipping {id}: {reason}");
    }

    let now_ms = now_ms()?;
    let now = now_as_deadline(now_ms);
    let matches = evaluate_events(&rules, &batch.candidates, &now);
    for m in &matches {
        post_alert(base_url, &hb_token, &plan_alert(m))?;
    }

    // Job 2: the busy_now snapshot — a fresh, always-run query around
    // `now`, independent of the sync-token cursor (which only ever answers
    // "what changed"; busy needs "what's true right now").
    let busy_raw = calendar_events_list_window(&access_token, now_ms)?;
    let window = busy_window(&live_calendar_events(&busy_raw), now_ms);
    post_busy(base_url, &hb_token, window, now_ms)?;

    // Written last, deliberately — see the module doc.
    post_cursor(base_url, &hb_token, &new_sync_token, now_ms)?;

    Ok(Summary {
        events_fetched: batch.candidates.len(),
        alerts_posted: matches.len(),
        busy: window.is_some(),
        new_sync_token,
    })
}

fn bounded_resync(access_token: &str) -> Result<(Vec<String>, String), String> {
    let updated_min = now_ms().map_err(|e| e.to_string())? - RESYNC_LOOKBACK_MS;
    calendar_events_list_full(access_token, updated_min).map_err(|e| e.to_string())
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
enum GmailLikeError {
    Status(u16),
    Transport(String),
    Parse(String),
}

type CalendarError = GmailLikeError;

impl std::fmt::Display for GmailLikeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GmailLikeError::Status(s) => write!(f, "calendar api: HTTP {s}"),
            GmailLikeError::Transport(e) => write!(f, "calendar api: {e}"),
            GmailLikeError::Parse(e) => write!(f, "calendar api: {e}"),
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

fn calendar_get(access_token: &str, query: &[(&str, &str)]) -> Result<String, CalendarError> {
    let mut req = ureq::get(CALENDAR_API).header("Authorization", &format!("Bearer {access_token}"));
    for (k, v) in query {
        req = req.query(*k, *v);
    }
    let response = req.config().timeout_global(Some(HTTP_TIMEOUT)).build().call();
    match response {
        Ok(mut r) => r.body_mut().read_to_string().map_err(|e| CalendarError::Parse(e.to_string())),
        Err(ureq::Error::StatusCode(status)) => Err(CalendarError::Status(status)),
        Err(e) => Err(CalendarError::Transport(e.to_string())),
    }
}

/// The identity of an `events.list` request — Google binds a `syncToken` to
/// the parameters of the full sync that minted it, so the full sync and
/// every incremental request resuming from its token must ask for the same
/// shape or risk one of two failures: `singleEvents=false` (its default)
/// returns recurring **masters** rather than instances, which collapses
/// `google_calendar_v1_key` onto one alert per series and makes
/// `ends_at_ms` the first instance's end (an `expires_at` already in the
/// past); or Google notices the drift and answers 410, which reads as
/// cursor loss and full-resyncs forever. Built ONCE, here, and used by both
/// [`calendar_events_list_sync`] and [`calendar_events_list_full`] so a
/// third builder can never drift from it — these are part of the cursor's
/// identity, not per-call tuning.
///
/// `showDeleted` must be `true`, and the constraint flows in the opposite
/// direction from the one above: it is the *incremental* request that
/// forces it, and the full sync that follows along to keep the identity.
/// An incremental sync always includes deletions — the reference states
/// "it is not allowed to set `showDeleted` to False" alongside
/// `syncToken`, and Google enforces that with an HTTP 400. `false` here
/// passed every `workflow_dispatch` smoke run (a first run is a full
/// sync, where `false` is legal) and then failed the lane's first real
/// incremental tick — the go-live bug of #486. The extra tombstones the
/// full sync now returns are already a named non-fatal skip
/// (`stream.rs`'s `Cancelled` arm), because incremental pages always
/// could carry them.
const SHARED_LIST_PARAMS: [(&str, &str); 2] = [("singleEvents", "true"), ("showDeleted", "true")];

/// The incremental request's own query, built on top of
/// [`SHARED_LIST_PARAMS`] — pure, so the class of bug that dropped
/// `singleEvents` from just this builder is testable without a network
/// mock.
fn sync_query<'a>(sync_token: &'a str, page_token: Option<&'a str>) -> Vec<(&'a str, &'a str)> {
    let mut query: Vec<(&str, &str)> = SHARED_LIST_PARAMS.to_vec();
    query.push(("syncToken", sync_token));
    if let Some(token) = page_token {
        query.push(("pageToken", token));
    }
    query
}

/// The full (bounded-resync) request's own query, built on top of
/// [`SHARED_LIST_PARAMS`] — see [`sync_query`].
fn full_query<'a>(updated_min: &'a str, max_results: &'a str, page_token: Option<&'a str>) -> Vec<(&'a str, &'a str)> {
    let mut query: Vec<(&str, &str)> = SHARED_LIST_PARAMS.to_vec();
    query.push(("updatedMin", updated_min));
    query.push(("maxResults", max_results));
    if let Some(token) = page_token {
        query.push(("pageToken", token));
    }
    query
}

/// One incremental sync attempt from a stored `syncToken`, paginated to
/// completion. Only the LAST page carries `nextSyncToken` — every earlier
/// page's raw events are still collected, `gmail_poll::gmail_history_list`'s
/// own pagination shape.
fn calendar_events_list_sync(
    access_token: &str,
    sync_token: &str,
) -> Result<hummingbird_calendar_poll::SyncPage, CalendarError> {
    let mut page = hummingbird_calendar_poll::SyncPage::default();
    let mut page_token: Option<String> = None;
    loop {
        let query = sync_query(sync_token, page_token.as_deref());
        let body = calendar_get(access_token, &query)?;
        let this_page = parse_events_list(&body).map_err(|e| CalendarError::Parse(e.to_string()))?;
        page.raw_events.extend(this_page.raw_events);
        if this_page.next_sync_token.is_some() {
            page.next_sync_token = this_page.next_sync_token;
        }
        match this_page.next_page_token {
            Some(token) => page_token = Some(token),
            None => break,
        }
    }
    Ok(page)
}

/// A bounded full sync (no `syncToken`): every event updated since
/// `updated_min_ms`, capped by [`RESYNC_MAX_RESULTS`], paginated to the
/// final page so the run always leaves with a fresh `nextSyncToken` to
/// resume incrementally from next time.
fn calendar_events_list_full(access_token: &str, updated_min_ms: i64) -> Result<(Vec<String>, String), CalendarError> {
    let updated_min = rfc3339_ms(updated_min_ms).map_err(CalendarError::Parse)?;
    let max_results = RESYNC_MAX_RESULTS.to_string();
    let mut raw_events = Vec::new();
    let mut sync_token: Option<String> = None;
    let mut page_token: Option<String> = None;
    loop {
        let query = full_query(&updated_min, &max_results, page_token.as_deref());
        let body = calendar_get(access_token, &query)?;
        let page = parse_events_list(&body).map_err(|e| CalendarError::Parse(e.to_string()))?;
        raw_events.extend(page.raw_events);
        if page.next_sync_token.is_some() {
            sync_token = page.next_sync_token;
        }
        match page.next_page_token {
            Some(token) => page_token = Some(token),
            None => break,
        }
    }
    let sync_token = sync_token.ok_or_else(|| {
        CalendarError::Parse("a full sync's final page carried no nextSyncToken".to_string())
    })?;
    Ok((raw_events, sync_token))
}

/// Job 2's own query: every event overlapping a window around `now_ms`,
/// with no `syncToken` at all — this leg rides no cursor, since it must
/// always answer "what's true right now," including for an event this
/// poller's evaluated-stream leg already saw and evaluated on an earlier
/// poll.
fn calendar_events_list_window(access_token: &str, now_ms: i64) -> Result<Vec<String>, String> {
    let time_min = rfc3339_ms(now_ms - BUSY_WINDOW_MS)?;
    let time_max = rfc3339_ms(now_ms + BUSY_WINDOW_MS)?;
    let mut raw_events = Vec::new();
    let mut page_token: Option<String> = None;
    loop {
        let mut query: Vec<(&str, &str)> = vec![
            ("timeMin", &time_min),
            ("timeMax", &time_max),
            ("singleEvents", "true"),
            ("orderBy", "startTime"),
        ];
        if let Some(token) = &page_token {
            query.push(("pageToken", token));
        }
        let body = calendar_get(access_token, &query).map_err(|e| e.to_string())?;
        let page = parse_events_list(&body).map_err(|e| e.to_string())?;
        raw_events.extend(page.raw_events);
        match page.next_page_token {
            Some(token) => page_token = Some(token),
            None => break,
        }
    }
    Ok(raw_events)
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

fn post_cursor(base_url: &str, token: &str, sync_token: &str, now_ms: i64) -> Result<(), String> {
    hb_post(
        base_url,
        token,
        "/api/snapshots",
        &serde_json::json!({
            "source": SOURCE,
            "key": CURSOR_KEY,
            "payload": cursor_envelope(sync_token),
            "fetched_at": now_ms,
        }),
    )
}

fn post_busy(
    base_url: &str,
    token: &str,
    window: Option<hummingbird_calendar_poll::BusyWindow>,
    now_ms: i64,
) -> Result<(), String> {
    hb_post(
        base_url,
        token,
        "/api/snapshots",
        &serde_json::json!({
            "source": SOURCE,
            "key": BUSY_KEY,
            "payload": busy_envelope(window),
            "fetched_at": now_ms,
        }),
    )
}

fn post_alert(base_url: &str, token: &str, ingest: &hummingbird_domain::AlertIngest) -> Result<(), String> {
    hb_post(base_url, token, "/api/alerts", &serde_json::to_value(ingest).map_err(|e| e.to_string())?)
}

#[cfg(test)]
mod tests {
    use super::*;

    // This is the class of bug review #268 round 1 caught: the incremental
    // request builder set neither `singleEvents` nor `showDeleted`, which no
    // fixture-shaped test could see (fixtures are hand-built already in
    // instance shape). These assert the constructed request parameters
    // themselves, not the parsing of a response.

    #[test]
    fn sync_and_full_queries_both_carry_every_shared_list_param() {
        let sync = sync_query("token-123", None);
        let full = full_query("2026-08-01T00:00:00Z", "250", None);
        for pair in SHARED_LIST_PARAMS {
            assert!(sync.contains(&pair), "sync_query missing {pair:?}: {sync:?}");
            assert!(full.contains(&pair), "full_query missing {pair:?}: {full:?}");
        }
    }

    /// Not a style choice: `showDeleted=false` alongside `syncToken` is an
    /// HTTP 400 ("it is not allowed to set showDeleted to False"), and a
    /// full-sync-only smoke run cannot catch it — see `SHARED_LIST_PARAMS`.
    #[test]
    fn show_deleted_is_true_because_the_incremental_sync_forbids_false() {
        assert!(SHARED_LIST_PARAMS.contains(&("showDeleted", "true")));
    }

    #[test]
    fn sync_query_carries_its_sync_token_and_optional_page_token() {
        let first_page = sync_query("token-123", None);
        assert!(first_page.contains(&("syncToken", "token-123")));
        assert!(!first_page.iter().any(|(k, _)| *k == "pageToken"));

        let later_page = sync_query("token-123", Some("page-2"));
        assert!(later_page.contains(&("syncToken", "token-123")));
        assert!(later_page.contains(&("pageToken", "page-2")));
    }

    #[test]
    fn full_query_carries_its_updated_min_and_max_results() {
        let q = full_query("2026-08-01T00:00:00Z", "250", None);
        assert!(q.contains(&("updatedMin", "2026-08-01T00:00:00Z")));
        assert!(q.contains(&("maxResults", "250")));
    }
}
