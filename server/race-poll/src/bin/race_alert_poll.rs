//! `race-alert-poll`'s shell: `std::env`, one GET of the binding, one GET of
//! the stored season per adapted series, and a POST only when a race is
//! inside the lead time.
//!
//! ```text
//! GET  /api/settings/race-series                    → which series to read
//! GET  /api/snapshots?source=race-schedule/v1&key=… → the stored season
//! POST /api/alerts                                  → only inside the lead
//! ```
//!
//! **It never fetches Jolpica and never writes a snapshot.** The upstream
//! feed is `race-schedule-poll`'s, six-hourly; deciding "is a race starting
//! inside the lead time" is a pure function of (stored schedule, now), which
//! is exactly why this half can run every fifteen minutes for free. See
//! `lib.rs`.
//!
//! **It is stateless, and re-posts the same alert on all ~6 runs inside one
//! 90-minute window.** That is safe rather than sloppy: `deliver`'s dedupe
//! is transitions-only and the delivery row commits before any send, so six
//! identical posts ring once — and `alert::plan` takes no clock, so the six
//! payloads are byte-identical and the authority's `restamp_on_change` never
//! restamps over a dismissal.
//!
//! Exit codes: `0` success (including "no race is starting", which is most
//! runs, and including an unset binding), `1` anything else.

use std::process::ExitCode;
use std::time::Duration;

use hummingbird_domain::RACE_SCHEDULE_V1;
use hummingbird_race_poll::alert::plan;
use hummingbird_race_poll::binding::{has_adapter, series_from_response, BINDING_KEY};
use hummingbird_race_poll::body::RaceScheduleBody;
use hummingbird_race_poll::next::{race_within_lead, LEAD_MS};

const HTTP_TIMEOUT: Duration = Duration::from_secs(30);

fn main() -> ExitCode {
    match run() {
        Ok(Outcome::Raised(races)) if races.is_empty() => {
            println!("race-alert-poll: no race inside the lead time");
            ExitCode::SUCCESS
        }
        Ok(Outcome::Raised(races)) => {
            println!("race-alert-poll: raised {}", races.join(", "));
            ExitCode::SUCCESS
        }
        Ok(Outcome::Unconfigured(reason)) => {
            println!("race-alert-poll: {reason}; nothing to read, nothing raised");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("race-alert-poll: {e}");
            ExitCode::FAILURE
        }
    }
}

enum Outcome {
    Raised(Vec<String>),
    Unconfigured(String),
}

fn run() -> Result<Outcome, String> {
    let base_url = env("HB_BASE_URL")?;
    let base_url = base_url.trim_end_matches('/');
    let token = env("HB_INGEST_TOKEN")?;

    let series = match read_series(base_url, &token)? {
        Ok(series) => series,
        Err(reason) => return Ok(Outcome::Unconfigured(reason)),
    };

    // One clock read for the whole run: two reads could straddle a lead
    // boundary and have one series answer about a race the other has
    // already let pass.
    let now_ms = now_ms()?;

    let mut raised = Vec::new();
    for series in series.into_iter().filter(|s| has_adapter(s)) {
        // No row yet (a first run, or a series whose schedule poll has not
        // landed) is silence, not a failure — the pane already renders that
        // gap, and inventing an alert about a season nobody has fetched
        // would be worse than saying nothing.
        let Some(season) = read_season(base_url, &token, &series)? else {
            println!("race-alert-poll: no schedule stored for `{series}` yet");
            continue;
        };
        let Some(event) = race_within_lead(&season.events, now_ms, LEAD_MS) else {
            continue;
        };
        let plan = plan(&series, event);
        post_alert(base_url, &token, &plan.ingest())?;
        raised.push(plan.source_key());
    }
    Ok(Outcome::Raised(raised))
}

/// `Ok(Err(reason))` is "not configured" — a clean exit-0 state, exactly as
/// on the schedule side.
fn read_series(base_url: &str, token: &str) -> Result<Result<Vec<String>, String>, String> {
    let url = format!("{base_url}/api/settings/{BINDING_KEY}");
    let response = ureq::get(&url)
        .header("Authorization", &format!("Bearer {token}"))
        .config()
        .timeout_global(Some(HTTP_TIMEOUT))
        .build()
        .call();
    let text = match response {
        Ok(mut r) => r.body_mut().read_to_string().map_err(fmt)?,
        Err(ureq::Error::StatusCode(404)) => {
            return Ok(Err(format!("`{BINDING_KEY}` is not set")))
        }
        Err(e) => return Err(format!("reading `{BINDING_KEY}`: {e}")),
    };
    match series_from_response(&text) {
        Ok(series) => Ok(Ok(series)),
        Err(problem) if problem.is_unconfigured() => Ok(Err(problem.to_string())),
        Err(problem) => Err(problem.to_string()),
    }
}

/// `Ok(None)` is the legitimate "no schedule written yet" state — the
/// authority answers 404 for it precisely so the two are distinguishable.
/// Query params, not path segments, because every source string contains a
/// slash.
fn read_season(
    base_url: &str,
    token: &str,
    series: &str,
) -> Result<Option<RaceScheduleBody>, String> {
    let url = format!("{base_url}/api/snapshots?source={RACE_SCHEDULE_V1}&key={series}");
    let response = ureq::get(&url)
        .header("Authorization", &format!("Bearer {token}"))
        .config()
        .timeout_global(Some(HTTP_TIMEOUT))
        .build()
        .call();
    let text = match response {
        Ok(mut r) => r.body_mut().read_to_string().map_err(fmt)?,
        Err(ureq::Error::StatusCode(404)) => return Ok(None),
        Err(e) => return Err(format!("reading the stored schedule for `{series}`: {e}")),
    };
    let row: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("reading `{series}`: {e}"))?;
    let payload = row
        .get("payload")
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("reading `{series}`: no `payload` in the response"))?;
    // The parse itself lives in the library, where it is tested.
    RaceScheduleBody::from_payload(payload)
        .map(Some)
        .map_err(|e| format!("`{series}`: {e}"))
}

fn post_alert(
    base_url: &str,
    token: &str,
    ingest: &hummingbird_domain::AlertIngest,
) -> Result<(), String> {
    let body = serde_json::to_value(ingest).map_err(fmt)?;
    ureq::post(&format!("{base_url}/api/alerts"))
        .header("Authorization", &format!("Bearer {token}"))
        .config()
        .timeout_global(Some(HTTP_TIMEOUT))
        .build()
        .send_json(&body)
        .map_err(|e| format!("POST /api/alerts: {e}"))?;
    Ok(())
}

/// A set-but-empty variable is treated as unset — `race_schedule_poll.rs`'s
/// own reasoning, and the same failure it prevents.
fn env(name: &str) -> Result<String, String> {
    match std::env::var(name) {
        Ok(value) if !value.trim().is_empty() => Ok(value),
        _ => Err(format!("{name} is not set")),
    }
}

fn now_ms() -> Result<i64, String> {
    let since = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(fmt)?;
    i64::try_from(since.as_millis()).map_err(fmt)
}

fn fmt<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}
