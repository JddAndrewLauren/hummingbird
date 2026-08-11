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
use hummingbird_city_waste::body::{WasteBody, SNAPSHOT_KEY};
use hummingbird_city_waste::date::Date;
use hummingbird_city_waste::judge::judge;
use hummingbird_city_waste::page;

/// The binding key, resolved by name at the seam exactly as
/// `client/core/src/bindings.rs` does — unversioned, so a
/// `city-waste/v1 → /v2` source bump cannot orphan it.
const BINDING_KEY: &str = "city-waste-page";

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

    let html = get_text(&page_url, None).map_err(|e| format!("fetching {page_url}: {e}"))?;
    let reading = page::parse(&html).map_err(|e| e.to_string())?;

    // One clock read, used for both the freshness stamp and "today". Two
    // reads could straddle midnight and make the snapshot describe a
    // different day than the alert judges.
    let now_ms = now_ms()?;
    let today = Date::from_days(now_ms.div_euclid(86_400_000));

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
    let Some(plan) = plan(reading.cadence, deviation, today) else {
        return Ok(Outcome::Wrote { alerted: false });
    };
    let ingest = plan
        .ingest(&reading.zone)
        .ok_or_else(|| format!("`{}` is not a time zone this build can resolve", reading.zone))?;
    post(base_url, &token, "/api/alerts", &serde_json::to_value(&ingest).map_err(fmt)?)?;
    Ok(Outcome::Wrote { alerted: true })
}

fn env(name: &str) -> Result<String, String> {
    std::env::var(name).map_err(|_| format!("{name} is not set"))
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
    let setting: serde_json::Value = serde_json::from_str(&text).map_err(fmt)?;
    // `Setting::value` is canonical JSON text, so a URL arrives as a quoted
    // JSON string and has to be parsed once more. A row holding something
    // that is not a string is a bound-but-unusable binding, which the pane
    // renders as a gap — here it is a refusal, not a guess.
    match serde_json::from_str::<serde_json::Value>(setting["value"].as_str().unwrap_or_default()) {
        Ok(serde_json::Value::String(url)) if !url.is_empty() => Ok(Some(url)),
        _ => Err(format!("`{BINDING_KEY}` does not hold a page URL")),
    }
}

fn get_text(url: &str, token: Option<&str>) -> Result<String, String> {
    let mut request = ureq::get(url).config().timeout_global(Some(HTTP_TIMEOUT)).build();
    if let Some(token) = token {
        request = request.header("Authorization", &format!("Bearer {token}"));
    }
    request.call().map_err(fmt)?.body_mut().read_to_string().map_err(fmt)
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
