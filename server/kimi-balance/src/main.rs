//! The poller's shell: `std::env`, one GET and one POST. Everything it
//! *decides* lives in the library and is natively tested — this file is the
//! untestable edge, kept as small as `city-waste`'s and `race-poll`'s are
//! for the same reason.
//!
//! ```text
//! GET  <MOONSHOT_API_BASE_URL>/v1/users/me/balance   → the account's balance
//! POST /api/snapshots                                → the answer
//! ```
//!
//! Exit codes: `0` success, `1` anything else — an unreachable endpoint, a
//! shape this poller does not recognise, or `code != 0`
//! (`exceeded_current_quota_error`'s own shape). Every failure path writes
//! nothing: a fabricated balance is a wrong answer, not a stale one, and the
//! pane already bands staleness honestly once `fetched_at` stops moving.

use std::process::ExitCode;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use hummingbird_kimi_balance::balance;
use hummingbird_kimi_balance::body::{KimiBalanceBody, SNAPSHOT_KEY};

const HTTP_TIMEOUT: Duration = Duration::from_secs(30);

fn main() -> ExitCode {
    match run() {
        Ok(()) => {
            println!("kimi-balance: snapshot written");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("kimi-balance: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let hb_base_url = env("HB_BASE_URL")?;
    let hb_base_url = hb_base_url.trim_end_matches('/');
    let hb_token = env("HB_INGEST_TOKEN")?;

    // Config, not a constant — see lib.rs's header. `platform.kimi.ai` and
    // `platform.kimi.com` keys are independent, and hardcoding either host
    // is a silent 401 the day the key is re-minted on the other one.
    let moonshot_base_url = env("MOONSHOT_API_BASE_URL")?;
    let moonshot_base_url = moonshot_base_url.trim_end_matches('/');
    let moonshot_key = env("MOONSHOT_API_KEY")?;

    let response_body = get_text(
        &format!("{moonshot_base_url}/v1/users/me/balance"),
        &moonshot_key,
    )
    .map_err(|e| format!("fetching the Moonshot balance: {e}"))?;
    let balance = balance::parse(&response_body).map_err(|e| e.to_string())?;

    let now_ms = now_ms()?;
    let body = KimiBalanceBody::from_balance(balance);
    post(
        hb_base_url,
        &hb_token,
        "/api/snapshots",
        &serde_json::json!({
            "source": hummingbird_domain::KIMI_BALANCE_V1,
            "key": SNAPSHOT_KEY,
            "payload": body.envelope(),
            "fetched_at": now_ms,
        }),
    )?;
    Ok(())
}

/// A set-but-empty variable is treated as unset, on `city-waste`'s own
/// reasoning: an Actions secret that was never created expands to the empty
/// string rather than failing the step, so without this the run reaches an
/// API with `Bearer ` and reports a 401 that reads as "the token is wrong"
/// when the truth is "the token was never minted".
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

/// The Moonshot balance endpoint. Bearer-authenticated with the Moonshot
/// key — deliberately never the `HB_INGEST_TOKEN`, which this request must
/// not carry.
fn get_text(url: &str, moonshot_key: &str) -> Result<String, String> {
    ureq::get(url)
        .header("Authorization", &format!("Bearer {moonshot_key}"))
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
