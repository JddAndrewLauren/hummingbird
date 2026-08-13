//! The poller's shell: `std::env`, the requests and the writes. Everything
//! it *decides* lives in the library and is natively tested — this file is
//! the untestable edge, kept as small as every other poller's is for the
//! same reason.
//!
//! ```text
//! read   server/uptime-probe/services.json   → which services, expected how
//! GET/POST <declared url>, unauthenticated    → one Outcome per service
//! POST   /api/snapshots                       → one row per service
//! ```
//!
//! **Every service is independent, and a failure on one must not silence
//! the rest.** Unlike `kimi-balance`'s single value, this poller answers N
//! independent facts in one run — on `github-status`'s own reasoning, a
//! write failure for one service is logged and skipped rather than
//! aborting every other service's otherwise-successful write.
//!
//! **An unreachable service is not a *poller* failure — it is exactly the
//! signal this lane exists to report.** The loop below always writes a row
//! for every declared service, whatever `probe` observed; only a failure to
//! *post* that row counts toward this process's exit code.

use std::process::ExitCode;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use hummingbird_uptime_probe::body::ProbeBody;
use hummingbird_uptime_probe::manifest::{parse_manifest, Service, SERVICES_JSON};
use hummingbird_uptime_probe::verdict::{self, Outcome, Verdict};

const HTTP_TIMEOUT: Duration = Duration::from_secs(30);

fn main() -> ExitCode {
    match run() {
        Ok(0) => {
            println!("uptime-probe: snapshots written");
            ExitCode::SUCCESS
        }
        Ok(failures) => {
            eprintln!("uptime-probe: {failures} service(s) failed to write; see above");
            ExitCode::FAILURE
        }
        Err(e) => {
            eprintln!("uptime-probe: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Returns the count of services this run failed to *write* a snapshot
/// for — `Err` is reserved for a failure that stops the whole run before
/// any per-service work could even start (missing config, a manifest that
/// does not parse).
fn run() -> Result<usize, String> {
    let hb_base_url = env("HB_BASE_URL")?;
    let hb_base_url = hb_base_url.trim_end_matches('/');
    let hb_token = env("HB_INGEST_TOKEN")?;

    let services = parse_manifest(SERVICES_JSON).map_err(|e| format!("services.json: {e}"))?;
    if services.is_empty() {
        return Err("services.json declares no services".to_string());
    }

    let mut failures = 0usize;
    for service in &services {
        let outcome = probe(service);
        let verdict = verdict::decide(service.expected, service.expect_status, &outcome);
        match write_one(hb_base_url, &hb_token, service, &outcome) {
            Ok(()) => println!(
                "uptime-probe: {} written ({})",
                service.id,
                match verdict {
                    Verdict::Agreement => "agreement",
                    Verdict::Divergent => "DIVERGENT",
                }
            ),
            Err(e) => {
                eprintln!("uptime-probe: {}: {e}", service.id);
                failures += 1;
            }
        }
    }
    Ok(failures)
}

/// Issues one unauthenticated request against a declared service and
/// reports what happened — never itself a source of `Err`, since an
/// unreachable service is a legitimate, expected `Outcome`, not a failure
/// of this function.
fn probe(service: &Service) -> Outcome {
    let result = match service.method.as_str() {
        "GET" => ureq::get(&service.url)
            .config()
            .timeout_global(Some(HTTP_TIMEOUT))
            .build()
            .call(),
        "POST" => ureq::post(&service.url)
            .config()
            .timeout_global(Some(HTTP_TIMEOUT))
            .build()
            .send_empty(),
        other => return Outcome::Unreachable(format!("unsupported method: {other}")),
    };
    match result {
        Ok(response) => Outcome::Reached(response.status().as_u16()),
        Err(ureq::Error::StatusCode(status)) => Outcome::Reached(status),
        Err(e) => Outcome::Unreachable(e.to_string()),
    }
}

fn write_one(
    hb_base_url: &str,
    hb_token: &str,
    service: &Service,
    outcome: &Outcome,
) -> Result<(), String> {
    let body = ProbeBody::from_outcome(service, outcome);
    let now_ms = now_ms()?;
    post(
        hb_base_url,
        hb_token,
        "/api/snapshots",
        &serde_json::json!({
            "source": hummingbird_domain::UPTIME_V1,
            "key": service.id,
            "payload": body.envelope(),
            "fetched_at": now_ms,
        }),
    )
}

/// A set-but-empty variable is treated as unset, on `city-waste`'s own
/// reasoning: an Actions secret that was never created expands to the empty
/// string rather than failing the step.
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
