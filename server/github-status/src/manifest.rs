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
//! workflows are scheduled" — a sixth `schedule:` workflow shows up here the
//! moment its file lands, with no second edit anywhere in this crate.

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
    const CITY_WASTE: &str = include_str!("../../../.github/workflows/city-waste.yml");
    const KIMI_BALANCE: &str = include_str!("../../../.github/workflows/kimi-balance.yml");
    const GMAIL_POLL: &str = include_str!("../../../.github/workflows/gmail-poll.yml");
    const RACE_ALERT_POLL: &str = include_str!("../../../.github/workflows/race-alert-poll.yml");
    const DEPLOY: &str = include_str!("../../../.github/workflows/deploy.yml");
    const CLIENT: &str = include_str!("../../../.github/workflows/client.yml");

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
