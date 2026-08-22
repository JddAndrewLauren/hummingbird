//! Reading `.github/workflows/*.yml` for the two facts this poller needs
//! from each one: its display `name:`, and every `cron:` string under a
//! top-level `on: schedule:` block.
//!
//! **No YAML crate — see `Cargo.toml`'s header for why.** This is a small,
//! hand-rolled, indentation-tracking line scanner scoped to exactly the
//! shape every workflow file in this repo already uses (`name:` at column
//! 0; `on:` at column 0 with `schedule:` and its `- cron: "..."` entries
//! indented under it). A workflow whose `on:` is the flow-sequence shorthand
//! (`on: [push]`) or that carries no `schedule:` at all is read as
//! **unscheduled** — [`parse_workflow`] returns `None` — which is the
//! correct reading for `deploy.yml`, `client.yml`, and every other
//! non-cron workflow in this repo, not a parse failure.
//!
//! **This is the part of the brief that "cannot forget to declare itself"**
//! (#314's own phrasing): the poller reads *this repo's own committed
//! workflow files* rather than a second, hand-maintained list of "which
//! workflows are scheduled" — a ninth `schedule:` workflow (eight exist
//! today: `calendar-poll.yml`, `city-waste.yml`, `github-status.yml`,
//! `gmail-poll.yml`, `kimi-balance.yml`, `race-alert-poll.yml`,
//! `race-schedule-poll.yml`, `uptime-probe.yml`) shows up here the moment
//! its file lands, with no second edit anywhere in this crate. The two
//! graph lanes are absent because they are absent from the repo's live
//! schedules: #487 commented their `schedule:` blocks out until #486's
//! Phase B provisions their credentials, and the restore PR that brings
//! each block back must re-add its row below — the coverage test will
//! insist, but only when something runs the `server/` tests, which a
//! workflows-only restore PR does not (path-filtered CI); do it in the
//! same PR, not on trust.

/// One workflow this build found a `schedule:` trigger on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduledWorkflow {
    /// The file's own base name, e.g. `"gmail-poll.yml"` — the
    /// `context_snapshots.key` this workflow's row is written under
    /// (`body.rs`), and the identifier GitHub's own
    /// `GET .../actions/workflows/{file_name}/runs` route takes.
    pub file_name: String,
    /// The workflow's own top-level `name:` — human words for the pane,
    /// e.g. `"gmail-poll"`.
    pub display_name: String,
    /// Every `- cron: "..."` string found under `on: schedule:`, verbatim
    /// (quotes stripped), in file order. Never empty — a workflow with none
    /// is not scheduled at all and [`parse_workflow`] returns `None` for it.
    pub cron_expressions: Vec<String>,
}

fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// The workflow's own top-level `name:` value, quotes stripped. `None` if
/// the file has no top-level `name:` line at all — every real workflow
/// file does, but a malformed one should not panic this poller.
fn top_level_name(contents: &str) -> Option<String> {
    for line in contents.lines() {
        if indent_of(line) != 0 {
            continue;
        }
        if let Some(rest) = line.strip_prefix("name:") {
            return Some(rest.trim().trim_matches('"').to_string());
        }
    }
    None
}

/// One `- cron: "..."` line's cron string, or `None` if this line is not
/// one.
fn cron_of(trimmed_line: &str) -> Option<String> {
    let rest = trimmed_line.strip_prefix("- cron:")?;
    Some(rest.trim().trim_matches('"').to_string())
}

/// Every cron string under a top-level `on: schedule:` block, tracked by
/// indentation rather than a fixed column count — this repo's own files are
/// two-space indented, but nothing here assumes exactly two.
fn schedule_crons(contents: &str) -> Vec<String> {
    let mut in_on_block = false;
    let mut on_indent = 0usize;
    let mut in_schedule_block = false;
    let mut schedule_indent = 0usize;
    let mut crons = Vec::new();

    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = indent_of(line);

        if indent == 0 {
            in_on_block = trimmed == "on:";
            in_schedule_block = false;
            on_indent = indent;
            continue;
        }

        if !in_on_block {
            continue;
        }
        if indent <= on_indent {
            in_on_block = false;
            in_schedule_block = false;
            continue;
        }

        if in_schedule_block {
            if indent <= schedule_indent {
                in_schedule_block = false;
            } else if let Some(cron) = cron_of(trimmed) {
                crons.push(cron);
                continue;
            }
        }
        if trimmed == "schedule:" {
            in_schedule_block = true;
            schedule_indent = indent;
        }
    }

    crons
}

/// Reads one workflow file's contents into a [`ScheduledWorkflow`], or
/// `None` if it declares no `schedule:` trigger (or has no top-level
/// `name:` to report at all — a file this malformed is not one this poller
/// can speak for).
pub fn parse_workflow(file_name: &str, contents: &str) -> Option<ScheduledWorkflow> {
    let crons = schedule_crons(contents);
    if crons.is_empty() {
        return None;
    }
    let display_name = top_level_name(contents)?;
    Some(ScheduledWorkflow { file_name: file_name.to_string(), display_name, cron_expressions: crons })
}

#[cfg(test)]
mod tests {
    use super::*;

    // The real committed files — this is both this parser's test and its
    // own regression guard: a workflow file edited into a shape this parser
    // stops recognising fails a build-time test here rather than silently
    // dropping out of the pane.
    //
    // **Every scheduled workflow is embedded** — ten today — not a sample
    // of them: a guard that covered four would have said nothing about the
    // rest, which is exactly the drop-out this guard exists to catch.
    // `EVERY_SCHEDULED_WORKFLOW` below is the list, and
    // `every_committed_scheduled_workflow_is_still_read_as_scheduled` walks
    // it.
    const CALENDAR_POLL: &str = include_str!("../../../.github/workflows/calendar-poll.yml");
    const CITY_WASTE: &str = include_str!("../../../.github/workflows/city-waste.yml");
    const GITHUB_STATUS: &str = include_str!("../../../.github/workflows/github-status.yml");
    const GMAIL_POLL: &str = include_str!("../../../.github/workflows/gmail-poll.yml");
    const KIMI_BALANCE: &str = include_str!("../../../.github/workflows/kimi-balance.yml");
    const RACE_ALERT_POLL: &str = include_str!("../../../.github/workflows/race-alert-poll.yml");
    const RACE_SCHEDULE_POLL: &str =
        include_str!("../../../.github/workflows/race-schedule-poll.yml");
    const UPTIME_PROBE: &str = include_str!("../../../.github/workflows/uptime-probe.yml");
    const DEPLOY: &str = include_str!("../../../.github/workflows/deploy.yml");
    const CLIENT: &str = include_str!("../../../.github/workflows/client.yml");
    const GRAPH_MAIL_POLL: &str = include_str!("../../../.github/workflows/graph-mail-poll.yml");
    const GRAPH_CALENDAR_POLL: &str =
        include_str!("../../../.github/workflows/graph-calendar-poll.yml");

    /// `(file name, contents, expected top-level `name:`, expected crons)`
    /// for every `schedule:`-carrying workflow committed in this repo.
    /// Adding an eleventh scheduled workflow without adding it here fails
    /// `the_embedded_list_covers_every_scheduled_workflow_in_the_repo`
    /// (see the module header on doing that in the same PR).
    const EVERY_SCHEDULED_WORKFLOW: &[(&str, &str, &str, &[&str])] = &[
        ("calendar-poll.yml", CALENDAR_POLL, "calendar-poll", &["*/15 * * * *"]),
        ("city-waste.yml", CITY_WASTE, "city-waste", &["40 13 * * *"]),
        ("github-status.yml", GITHUB_STATUS, "github-status", &["*/30 * * * *"]),
        ("gmail-poll.yml", GMAIL_POLL, "gmail-poll", &["*/15 * * * *"]),
        ("graph-calendar-poll.yml", GRAPH_CALENDAR_POLL, "graph-calendar-poll", &["*/15 * * * *"]),
        ("graph-mail-poll.yml", GRAPH_MAIL_POLL, "graph-mail-poll", &["*/15 * * * *"]),
        ("kimi-balance.yml", KIMI_BALANCE, "kimi-balance", &["0 */6 * * *"]),
        ("race-alert-poll.yml", RACE_ALERT_POLL, "race-alert-poll", &["*/15 * * * *"]),
        ("race-schedule-poll.yml", RACE_SCHEDULE_POLL, "race-schedule-poll", &["0 */6 * * *"]),
        ("uptime-probe.yml", UPTIME_PROBE, "uptime-probe", &["5 * * * *"]),
    ];

    /// The general guard the module header claims: **every** committed
    /// scheduled workflow, read through the real parser, with its own
    /// `name:` and cron strings pinned. A file edited into a shape this
    /// parser stops recognising — a reindented `schedule:` block, a
    /// `name:` moved off column 0, a cron rewritten — fails here rather
    /// than silently dropping out of the pane.
    #[test]
    fn every_committed_scheduled_workflow_is_still_read_as_scheduled() {
        for (file_name, contents, display_name, crons) in EVERY_SCHEDULED_WORKFLOW {
            let workflow = parse_workflow(file_name, contents)
                .unwrap_or_else(|| panic!("{file_name} carries a schedule: and must parse"));
            assert_eq!(&workflow.file_name, file_name);
            assert_eq!(&workflow.display_name, display_name, "{file_name}");
            assert_eq!(workflow.cron_expressions, *crons, "{file_name}");
        }
    }

    /// The list above is only a general guard while it is complete. This
    /// reads the workflow directory itself — the same directory `main.rs`
    /// scans — and fails if any file carrying a `schedule:` trigger is
    /// missing from it.
    #[test]
    fn the_embedded_list_covers_every_scheduled_workflow_in_the_repo() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../.github/workflows");
        let mut scheduled: Vec<String> = Vec::new();
        for entry in std::fs::read_dir(dir).expect("the workflow directory is readable") {
            let path = entry.expect("a readable directory entry").path();
            if path.extension().and_then(|e| e.to_str()) != Some("yml") {
                continue;
            }
            let contents = std::fs::read_to_string(&path).expect("a readable workflow file");
            if parse_workflow("probe.yml", &contents).is_some() {
                scheduled.push(
                    path.file_name().expect("a file name").to_string_lossy().into_owned(),
                );
            }
        }
        scheduled.sort();

        let mut embedded: Vec<String> =
            EVERY_SCHEDULED_WORKFLOW.iter().map(|(name, ..)| (*name).to_string()).collect();
        embedded.sort();

        assert_eq!(
            scheduled, embedded,
            "a scheduled workflow is not covered by this crate's regression guard — add it to \
             EVERY_SCHEDULED_WORKFLOW (and to this module's header count)"
        );
    }

    /// The header's own count, pinned: ten today — both graph lanes
    /// rejoined in #486 Phase B, mail first and calendar here. No poller
    /// lane is dormant now.
    #[test]
    fn the_repo_carries_ten_scheduled_workflows_today() {
        assert_eq!(EVERY_SCHEDULED_WORKFLOW.len(), 10);
    }

    #[test]
    fn city_waste_is_read_as_a_daily_scheduled_workflow() {
        let workflow = parse_workflow("city-waste.yml", CITY_WASTE).expect("city-waste is scheduled");
        assert_eq!(workflow.file_name, "city-waste.yml");
        assert_eq!(workflow.display_name, "city-waste");
        assert_eq!(workflow.cron_expressions, vec!["40 13 * * *"]);
    }

    #[test]
    fn kimi_balance_is_read_as_a_six_hourly_scheduled_workflow() {
        let workflow =
            parse_workflow("kimi-balance.yml", KIMI_BALANCE).expect("kimi-balance is scheduled");
        assert_eq!(workflow.display_name, "kimi-balance");
        assert_eq!(workflow.cron_expressions, vec!["0 */6 * * *"]);
    }

    #[test]
    fn gmail_poll_is_read_as_a_fifteen_minute_scheduled_workflow() {
        let workflow = parse_workflow("gmail-poll.yml", GMAIL_POLL).expect("gmail-poll is scheduled");
        assert_eq!(workflow.display_name, "gmail-poll");
        assert_eq!(workflow.cron_expressions, vec!["*/15 * * * *"]);
    }

    #[test]
    fn race_alert_poll_is_read_as_a_fifteen_minute_scheduled_workflow() {
        let workflow = parse_workflow("race-alert-poll.yml", RACE_ALERT_POLL)
            .expect("race-alert-poll is scheduled");
        assert_eq!(workflow.display_name, "race-alert-poll");
        assert_eq!(workflow.cron_expressions, vec!["*/15 * * * *"]);
    }

    /// `deploy.yml` and `client.yml` both trigger on `push`/`pull_request`,
    /// never `schedule:` — the negative case this whole parser exists to
    /// get right, on real files rather than a hand-built fixture.
    #[test]
    fn workflows_with_no_schedule_trigger_are_not_scheduled() {
        assert_eq!(parse_workflow("deploy.yml", DEPLOY), None);
        assert_eq!(parse_workflow("client.yml", CLIENT), None);
    }

    #[test]
    fn a_workflow_dispatch_only_block_under_on_is_not_a_schedule() {
        let contents = "name: manual-only\n\non:\n  workflow_dispatch:\n\njobs:\n  x:\n    runs-on: ubuntu-latest\n";
        assert_eq!(parse_workflow("manual-only.yml", contents), None);
    }

    #[test]
    fn a_bracketed_on_shorthand_is_not_a_schedule() {
        let contents = "name: shorthand\non: [push]\njobs:\n  x:\n    runs-on: ubuntu-latest\n";
        assert_eq!(parse_workflow("shorthand.yml", contents), None);
    }

    #[test]
    fn multiple_cron_lines_are_all_read_in_file_order() {
        let contents = "name: multi\n\non:\n  schedule:\n    - cron: \"0 */6 * * *\"\n    - cron: \"*/15 * * * *\"\n\njobs:\n  x:\n    runs-on: ubuntu-latest\n";
        let workflow = parse_workflow("multi.yml", contents).expect("multi is scheduled");
        assert_eq!(workflow.cron_expressions, vec!["0 */6 * * *", "*/15 * * * *"]);
    }

    #[test]
    fn a_schedule_block_that_ends_before_a_sibling_key_stops_collecting() {
        // `workflow_dispatch:` sits back at `on:`'s own indent, so its
        // sibling `schedule:` block must not accidentally swallow it.
        let contents = "name: sibling\n\non:\n  schedule:\n    - cron: \"0 */6 * * *\"\n  workflow_dispatch:\n\njobs:\n  x:\n    runs-on: ubuntu-latest\n";
        let workflow = parse_workflow("sibling.yml", contents).expect("sibling is scheduled");
        assert_eq!(workflow.cron_expressions, vec!["0 */6 * * *"]);
    }
}
