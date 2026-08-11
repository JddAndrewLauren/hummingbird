//! `race-schedule-poll`'s shell: `std::env`, one GET per adapted series and
//! one POST per adapted series. Everything it *decides* lives in the library
//! and is natively tested — this file is the untestable edge, kept as small
//! as `worker/src/fcm.rs` is for the same reason.
//!
//! ```text
//! GET  /api/settings/race-series   → which series to poll
//! GET  api.jolpi.ca/…/current.json → the season (F1 only, today)
//! POST /api/snapshots              → one row per adapted series
//! ```
//!
//! It raises **no alert at all** — that is `race-alert-poll`'s whole job,
//! fifteen minutes at a time off the row this binary writes. See `lib.rs`
//! for why the split is not a competing clock.
//!
//! Exit codes: `0` success (including "the binding is unset or holds no
//! text", and including an off-season, both legitimate states rather than
//! failures), `1` anything else. Every failure path writes nothing.

use std::process::ExitCode;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use hummingbird_race_poll::binding::{has_adapter, series_from_response, BINDING_KEY};
use hummingbird_race_poll::body::RaceScheduleBody;
use hummingbird_race_poll::schedule::{self, FEED_URL};
use hummingbird_domain::RACE_SCHEDULE_V1;

const HTTP_TIMEOUT: Duration = Duration::from_secs(30);

/// **A custom `User-Agent` is load-bearing, not politeness.** Jolpica
/// answers an unset or default UA with `403` — measured, 2026-08-11. This is
/// exactly the class of fact a later tidy-up of the HTTP client deletes, so
/// it is stated on the header it is set on. (See `schedule.rs`'s header for
/// the rest of what was verified at wiring time.)
const USER_AGENT: &str = "hummingbird-race-poll/1 (+https://github.com/JddAndrewLauren/hummingbird)";

fn main() -> ExitCode {
    match run() {
        Ok(Outcome::Wrote { series }) => {
            println!("race-schedule-poll: wrote {} snapshot(s): {}", series.len(), series.join(", "));
            ExitCode::SUCCESS
        }
        // Not a failure, and deliberately not an error exit: nobody has
        // chosen a series yet, or the row holds something that is not text.
        // Not configured is not broken.
        Ok(Outcome::Unconfigured(reason)) => {
            println!("race-schedule-poll: {reason}; nothing to poll, nothing written");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("race-schedule-poll: {e}");
            ExitCode::FAILURE
        }
    }
}

enum Outcome {
    Wrote { series: Vec<String> },
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

    // A series with no adapter here is skipped and logged, never an error:
    // `indycar` in the binding writes no snapshot and the pane renders the
    // gap, which is what makes the IndyCar deferral honest rather than
    // silent.
    let (adapted, skipped): (Vec<String>, Vec<String>) =
        series.into_iter().partition(|s| has_adapter(s));
    for series in &skipped {
        println!("race-schedule-poll: no adapter for `{series}`; skipped, nothing written");
    }
    if adapted.is_empty() {
        return Ok(Outcome::Unconfigured(format!(
            "`{BINDING_KEY}` names no series this build can poll"
        )));
    }

    let mut written = Vec::new();
    for series in adapted {
        let season = fetch_season(&series)?;
        post_snapshot(base_url, &token, &series, &season)?;
        written.push(series);
    }
    Ok(Outcome::Wrote { series: written })
}

/// One adapter today. When a second lands it is another arm here and another
/// entry in `binding::ADAPTED_SERIES` — the two lists agreeing is what
/// `has_adapter` promises, so a series that reaches here without an arm is a
/// bug in that pair rather than anything the operator did.
fn fetch_season(series: &str) -> Result<RaceScheduleBody, String> {
    match series {
        "f1" => fetch_f1_season(),
        other => Err(format!(
            "`{other}` passed the adapter check but has no fetch arm; \
             `binding::ADAPTED_SERIES` and this match disagree"
        )),
    }
}

/// The feed: a plain unauthenticated GET, deliberately — this is the one
/// request in the run that goes anywhere other than the authority, and it
/// must never carry the ingest token. Jolpica needs no credential of its
/// own; the `User-Agent` above is a header, not a secret.
fn fetch_f1_season() -> Result<RaceScheduleBody, String> {
    let response = ureq::get(FEED_URL)
        .header("User-Agent", USER_AGENT)
        .config()
        .timeout_global(Some(HTTP_TIMEOUT))
        .build()
        .call()
        .map_err(|e| format!("fetching {FEED_URL}: {e}"))?
        .body_mut()
        .read_to_string()
        .map_err(|e| format!("reading {FEED_URL}: {e}"))?;
    // A 200 with a shape the parser does not recognise is a named
    // per-field error and writes nothing — never something plausible.
    schedule::parse(&response).map_err(|e| e.to_string())
}

/// `Ok(Err(reason))` is "not configured" — a clean exit-0 state, not a
/// failure. The authority answers 404 for an unset binding precisely so
/// that is distinguishable here.
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
    // The decode itself lives in the library, where it is tested — this file
    // keeps only the call, the 404, and which problems are configuration
    // states rather than failures.
    match series_from_response(&text) {
        Ok(series) => Ok(Ok(series)),
        Err(problem) if problem.is_unconfigured() => Ok(Err(problem.to_string())),
        Err(problem) => Err(problem.to_string()),
    }
}

fn post_snapshot(
    base_url: &str,
    token: &str,
    series: &str,
    season: &RaceScheduleBody,
) -> Result<(), String> {
    let body = serde_json::json!({
        "source": RACE_SCHEDULE_V1,
        // The series IS the row key — which is what lets the body carry no
        // `series` field of its own, and what the alert's `subject_key`
        // joins against.
        "key": series,
        "payload": season.envelope(),
        "fetched_at": now_ms()?,
    });
    ureq::post(&format!("{base_url}/api/snapshots"))
        .header("Authorization", &format!("Bearer {token}"))
        .config()
        .timeout_global(Some(HTTP_TIMEOUT))
        .build()
        .send_json(&body)
        .map_err(|e| format!("POST /api/snapshots ({series}): {e}"))?;
    Ok(())
}

/// A set-but-empty variable is treated as unset. An Actions secret that was
/// never created expands to the empty string rather than failing the step,
/// so without this the run reaches the API with `Bearer ` and reports a 401
/// — which reads as "the token is wrong" when the truth is "the token was
/// never minted".
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

fn fmt<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}
