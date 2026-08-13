//! The poller's shell: `std::env`, the workflow-directory scan, the fetches
//! and the writes. Everything it *decides* lives in the library and is
//! natively tested — this file is the untestable edge, kept as small as
//! every other poller's is for the same reason.
//!
//! ```text
//! read   <repo>/.github/workflows/*.yml     → which workflows are scheduled
//! GET    /repos/{repo}/actions/workflows/{file}/runs  → each one's run history
//! POST   /api/snapshots                     → one row per scheduled workflow
//! ```
//!
//! **Every workflow is independent, and a failure on one must not silence
//! the rest.** Unlike a single-value poller (`kimi-balance`), this poller
//! answers N independent facts in one run; a GitHub API hiccup on one
//! workflow's runs call is logged and skipped rather than aborting every
//! other workflow's otherwise-successful write. The process still exits
//! non-zero if anything failed, so a real, persistent problem is visible in
//! the Actions run log.

use std::process::ExitCode;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use hummingbird_github_status::body::WorkflowStatusBody;
use hummingbird_github_status::{cron, manifest, runs};

const HTTP_TIMEOUT: Duration = Duration::from_secs(30);

fn main() -> ExitCode {
    match run() {
        Ok(0) => {
            println!("github-status: snapshots written");
            ExitCode::SUCCESS
        }
        Ok(failures) => {
            eprintln!("github-status: {failures} workflow(s) failed; see above");
            ExitCode::FAILURE
        }
        Err(e) => {
            eprintln!("github-status: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Returns the count of workflows this run failed to fetch or write —
/// `Err` is reserved for a failure that stops the whole run before any
/// per-workflow work could even start (missing config, an unreadable
/// workflow directory).
fn run() -> Result<usize, String> {
    let hb_base_url = env("HB_BASE_URL")?;
    let hb_base_url = hb_base_url.trim_end_matches('/');
    let hb_token = env("HB_INGEST_TOKEN")?;
    let github_repository = env("GITHUB_REPOSITORY")?;
    let github_token = env("GITHUB_TOKEN")?;
    // Overridable for local/manual runs against a checkout that is not
    // this process's own cwd; defaults to the path every workflow here
    // already runs `cargo run` from (`working-directory: server`).
    let workflows_dir = std::env::var("GITHUB_WORKFLOWS_DIR")
        .unwrap_or_else(|_| "../.github/workflows".to_string());

    let scheduled = scheduled_workflows(&workflows_dir)?;
    if scheduled.is_empty() {
        return Err(format!("no scheduled workflows found under {workflows_dir}"));
    }

    let mut failures = 0usize;
    for workflow in scheduled {
        if let Err(e) = poll_and_write_one(
            hb_base_url,
            &hb_token,
            &github_repository,
            &github_token,
            &workflow,
        ) {
            eprintln!("github-status: {}: {e}", workflow.file_name);
            failures += 1;
        } else {
            println!("github-status: {} written", workflow.file_name);
        }
    }
    Ok(failures)
}

fn scheduled_workflows(dir: &str) -> Result<Vec<manifest::ScheduledWorkflow>, String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("reading {dir}: {e}"))?;
    let mut scheduled = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| format!("reading an entry in {dir}: {e}"))?;
        let path = entry.path();
        let is_yaml = path.extension().is_some_and(|ext| ext == "yml" || ext == "yaml");
        if !is_yaml {
            continue;
        }
        let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let contents = std::fs::read_to_string(&path).map_err(|e| format!("reading {file_name}: {e}"))?;
        if let Some(workflow) = manifest::parse_workflow(file_name, &contents) {
            scheduled.push(workflow);
        }
    }
    // Deterministic order, independent of the directory's own listing
    // order, which the filesystem gives no guarantee about.
    scheduled.sort_by(|a, b| a.file_name.cmp(&b.file_name));
    Ok(scheduled)
}

fn poll_and_write_one(
    hb_base_url: &str,
    hb_token: &str,
    github_repository: &str,
    github_token: &str,
    workflow: &manifest::ScheduledWorkflow,
) -> Result<(), String> {
    let runs_url = format!(
        "https://api.github.com/repos/{github_repository}/actions/workflows/{}/runs?per_page=20",
        workflow.file_name
    );
    let response_body = get_text(&runs_url, github_token)?;
    let parsed_runs = runs::parse_runs(&response_body).map_err(|e| e.to_string())?;
    let verdict = runs::decide(&parsed_runs);

    let body = WorkflowStatusBody {
        display_name: workflow.display_name.clone(),
        declared_cadence_ms: cron::tightest_cadence_ms(&workflow.cron_expressions),
        last_run_conclusion: verdict.last_run.as_ref().and_then(|r| r.conclusion.clone()),
        last_run_event: verdict.last_run.as_ref().map(|r| r.event.clone()),
        last_run_at_ms: verdict.last_run.as_ref().map(|r| r.ran_at_ms),
        last_scheduled_success_at_ms: verdict.last_scheduled_success_at_ms,
    };

    let now_ms = now_ms()?;
    post(
        hb_base_url,
        hb_token,
        "/api/snapshots",
        &serde_json::json!({
            "source": hummingbird_domain::GITHUB_HUMMINGBIRD_V1,
            "key": workflow.file_name,
            "payload": body.envelope(),
            "fetched_at": now_ms,
        }),
    )
}

/// A set-but-empty variable is treated as unset, on `city-waste`'s own
/// reasoning: an Actions secret/variable that was never created expands to
/// the empty string rather than failing the step.
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

/// GitHub's REST API. A `User-Agent` is required by GitHub (an unset one is
/// answered 403, `race-poll`'s own measured fact about a different API, and
/// GitHub's own docs state the same requirement outright for theirs).
fn get_text(url: &str, github_token: &str) -> Result<String, String> {
    ureq::get(url)
        .header("Authorization", &format!("Bearer {github_token}"))
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "hummingbird-github-status")
        .header("X-GitHub-Api-Version", "2022-11-28")
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
