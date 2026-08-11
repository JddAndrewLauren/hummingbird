//! The poller's shell: `std::env`, one GET and two POSTs. Everything it
//! *decides* lives in the library and is natively tested — this file is the
//! untestable edge, kept as small as `worker/src/fcm.rs` is for the same
//! reason.
//!
//! ```text
//! GET  /api/settings/city-waste-page   → the address's page URL
//! GET  <that URL>                      → the council's HTML
//! POST /api/snapshots                  → the answer
//! POST /api/alerts                     → only on a week that moved
//! ```
//!
//! Exit codes: `0` success (including "the binding is unset", which is a
//! legitimate state and not a failure), `1` anything else. Every failure
//! path writes nothing.

use std::process::ExitCode;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use hummingbird_city_waste::alert::plan;
use hummingbird_city_waste::binding::{page_url_from_response, BINDING_KEY};
use hummingbird_city_waste::body::{WasteBody, SNAPSHOT_KEY};
use hummingbird_city_waste::date::Date;
use hummingbird_city_waste::judge::judge;
use hummingbird_city_waste::page;

const HTTP_TIMEOUT: Duration = Duration::from_secs(30);

fn main() -> ExitCode {
    match run() {
        Ok(Outcome::Wrote { alerted }) => {
            println!("city-waste: snapshot written{}", if alerted { " + alert raised" } else { "" });
            ExitCode::SUCCESS
        }
        // Not a failure, and deliberately not an error exit: nobody has set
        // the address yet. The correct behaviour is to write nothing —
        // never a snapshot built from a guessed address — and say so.
        Ok(Outcome::Unbound) => {
            println!("city-waste: `{BINDING_KEY}` is not set; nothing to poll, nothing written");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("city-waste: {e}");
            ExitCode::FAILURE
        }
    }
}

enum Outcome {
    Wrote { alerted: bool },
    Unbound,
}

fn run() -> Result<Outcome, String> {
    let base_url = env("HB_BASE_URL")?;
    let base_url = base_url.trim_end_matches('/');
    let token = env("HB_INGEST_TOKEN")?;

    let Some(page_url) = read_binding(base_url, &token)? else {
        return Ok(Outcome::Unbound);
    };

    let html = get_text(&page_url).map_err(|e| format!("fetching {page_url}: {e}"))?;
    let reading = page::parse(&html).map_err(|e| e.to_string())?;

    // One clock read, used for both the freshness stamp and "today". Two
    // reads could straddle midnight and make the snapshot describe a
    // different day than the alert judges.
    //
    // "Today" is resolved **in the address's zone**, never as
    // `now_ms / 86_400_000`: that is the runner's UTC day, which agrees with
    // the address at the 06:40-local cron and disagrees on any manual
    // dispatch in the local evening — quietly, and on exactly the run
    // someone would do by hand to check a holiday.
    let now_ms = now_ms()?;
    let today = Date::today_in_zone(now_ms, &reading.zone)
        .ok_or_else(|| format!("`{}` is not a time zone this build can resolve", reading.zone))?;

    // Snapshot first, alert second — deliberately. A death between them
    // leaves the pane showing the corrected day, which IS the answer, with
    // the alert catching up next poll; the inverse leaves an alert about a
    // slide the snapshot does not show.
    let body = WasteBody::new(&reading.zone, reading.cadence, reading.collected_on, reading.streams);
    post(
        base_url,
        &token,
        "/api/snapshots",
        &serde_json::json!({
            "source": hummingbird_domain::CITY_WASTE_V2,
            "key": SNAPSHOT_KEY,
            "payload": body.envelope(),
            "fetched_at": now_ms,
        }),
    )?;

    let deviation = judge(reading.cadence, reading.collected_on, today);
    let Some(plan) = plan(reading.cadence, deviation) else {
        return Ok(Outcome::Wrote { alerted: false });
    };
    let ingest = plan
        .ingest(&reading.zone)
        .ok_or_else(|| format!("`{}` is not a time zone this build can resolve", reading.zone))?;
    post(base_url, &token, "/api/alerts", &serde_json::to_value(&ingest).map_err(fmt)?)?;
    Ok(Outcome::Wrote { alerted: true })
}

/// A set-but-empty variable is treated as unset. An Actions secret that was
/// never created expands to the empty string rather than failing the step, so
/// without this the run reaches the API with `Bearer ` and reports a 401 —
/// which reads as "the token is wrong" when the truth is "the token was never
/// minted".
fn env(name: &str) -> Result<String, String> {
    match std::env::var(name) {
        Ok(value) if !value.trim().is_empty() => Ok(value),
        _ => Err(format!("{name} is not set")),
    }
}

fn now_ms() -> Result<i64, String> {
    let since = SystemTime::now().duration_since(UNIX_EPOCH).map_err(fmt)?;
    i64::try_from(since.as_millis()).map_err(fmt)
}

/// Reads the page URL out of `settings`. `Ok(None)` is the unset binding —
/// the authority answers 404 for it precisely so the two states are
/// distinguishable here.
fn read_binding(base_url: &str, token: &str) -> Result<Option<String>, String> {
    let url = format!("{base_url}/api/settings/{BINDING_KEY}");
    let response = ureq::get(&url)
        .header("Authorization", &format!("Bearer {token}"))
        .config()
        .timeout_global(Some(HTTP_TIMEOUT))
        .build()
        .call();
    let text = match response {
        Ok(mut r) => r.body_mut().read_to_string().map_err(fmt)?,
        Err(ureq::Error::StatusCode(404)) => return Ok(None),
        Err(e) => return Err(format!("reading `{BINDING_KEY}`: {e}")),
    };
    // The decode itself lives in the library, where it is tested — this file
    // keeps only the call and the 404.
    page_url_from_response(&text).map(Some).map_err(|e| e.to_string())
}

/// The council's page: a plain unauthenticated GET, deliberately — this is
/// the one request in the run that goes anywhere other than the authority,
/// and it must never carry the ingest token.
fn get_text(url: &str) -> Result<String, String> {
    ureq::get(url)
        .config()
        .timeout_global(Some(HTTP_TIMEOUT))
        .build()
        .call()
        .map_err(fmt)?
        .body_mut()
        .read_to_string()
        .map_err(fmt)
}

fn post(base_url: &str, token: &str, path: &str, body: &serde_json::Value) -> Result<(), String> {
    ureq::post(&format!("{base_url}{path}"))
        .header("Authorization", &format!("Bearer {token}"))
        .config()
        .timeout_global(Some(HTTP_TIMEOUT))
        .build()
        .send_json(body)
        .map_err(|e| format!("POST {path}: {e}"))?;
    Ok(())
}

fn fmt<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}
