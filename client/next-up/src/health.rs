//! The footer's **facts**, never its wording.
//!
//! This module renders no sentence and makes no fog judgment: it counts
//! what is countable and hands each open fog `question` back **verbatim**.
//! Whether a question names a real unknown is a reading, and the reading
//! stays in `SKILL.md` — "None — the unknowns are carried inside the two
//! investigation actions" is a non-empty question that must not flag, and
//! no regex in this crate could be trusted to know that.
//!
//! What the owned schema changed: fog arrives as **structured rows**
//! (`question`, `resolved_at`), so the exhaustion check no longer parses a
//! markdown `## Fog` section out of a project's Route the way
//! `linear.sh survey` had to.

use std::collections::HashMap;

use hummingbird_domain::{ChangesResponse, Item, Stage};
use serde::Serialize;

/// The footer's material. Every field is a count or a verbatim string.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Health {
    /// Live items sitting in `Triage` — starvation stays visible.
    pub triage: usize,
    /// Live items sitting in `Grilling`.
    pub grilling: usize,
    /// How many otherwise-qualifying items [`crate::select`] dropped for a
    /// live blocker: the footer's "4 more blocked upstream".
    pub blocked_dropped: usize,
    /// Live, unshut items carrying #10's `agent` axis — "N you could hand
    /// off". Counted off the whole sweep, never off the candidates, and
    /// deliberately so: on an ordinary survey the candidate list is *not*
    /// filtered to `agent`, so counting there would answer a different
    /// question depending on which arm asked it. It exists to make the
    /// hand-off offer possible without a second survey.
    pub agent_doable: usize,
    /// Projects whose minted actions are all shut while open fog remains.
    /// Empty is the normal case.
    pub fog_exhausted: Vec<FogExhausted>,
}

/// One project that has run out of actions with its route still in fog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FogExhausted {
    pub project_id: String,
    pub project: String,
    /// Every open fog question on that project, in the order the Route
    /// lists them, **verbatim** — the skill decides whether any of them
    /// names an actual unknown.
    pub questions: Vec<String>,
}

/// Gathers the footer's facts from the same sweep payload
/// [`crate::select`] read.
///
/// `blocked_dropped` is threaded in rather than recomputed: the items it
/// counts are exactly the ones selection removed, so re-deriving it from
/// the surviving candidates is impossible by construction.
pub fn health(sweep: &ChangesResponse, blocked_dropped: usize) -> Health {
    Health {
        triage: count_stage(sweep, Stage::Triage),
        grilling: count_stage(sweep, Stage::Grilling),
        blocked_dropped,
        agent_doable: agent_doable(sweep),
        fog_exhausted: fog_exhausted(sweep),
    }
}

/// Live `agent`-marked items that are still open. `Done` and archived are
/// both excluded — a finished chore is not something to hand off — but
/// `Blocked` is *included*, because #10's own protocol has an agent report
/// a genuine external blocker and stop there, and that item still carries
/// work the agent could resume.
fn agent_doable(sweep: &ChangesResponse) -> usize {
    sweep.items.iter().filter(|item| item.agent && !is_shut(item)).count()
}

fn count_stage(sweep: &ChangesResponse, stage: Stage) -> usize {
    sweep
        .items
        .iter()
        .filter(|item| item.archived_at.is_none() && item.stage == stage)
        .count()
}

/// A project qualifies when it has minted actions, **all** of them are
/// shut (`Done` or archived), and at least one fog row is still open.
/// A project with no actions at all has not run out of them — it has never
/// been through `/to-actions` — so it never flags.
fn fog_exhausted(sweep: &ChangesResponse) -> Vec<FogExhausted> {
    let mut counts: HashMap<&str, (usize, usize)> = HashMap::new();
    for item in &sweep.items {
        let Some(project_id) = item.project_id.as_deref() else {
            continue;
        };
        let entry = counts.entry(project_id).or_insert((0, 0));
        entry.0 += 1;
        if !is_shut(item) {
            entry.1 += 1;
        }
    }

    let mut exhausted: Vec<FogExhausted> = sweep
        .projects
        .iter()
        .filter(|project| project.archived_at.is_none())
        .filter_map(|project| {
            let (total, open) = *counts.get(project.id.as_str())?;
            if total == 0 || open > 0 {
                return None;
            }
            let questions = open_questions(sweep, &project.id);
            if questions.is_empty() {
                return None;
            }
            Some(FogExhausted {
                project_id: project.id.clone(),
                project: project.name.clone(),
                questions,
            })
        })
        .collect();

    // A total order, for the same reason `rank` ends on `id`: the footer
    // must read identically on a repeat run.
    exhausted.sort_by(|a, b| {
        a.project
            .cmp(&b.project)
            .then(a.project_id.cmp(&b.project_id))
    });
    exhausted
}

fn open_questions(sweep: &ChangesResponse, project_id: &str) -> Vec<String> {
    let mut rows: Vec<_> = sweep
        .fog
        .iter()
        .filter(|fog| fog.project_id == project_id && fog.resolved_at.is_none())
        .collect();
    rows.sort_by(|a, b| a.position.cmp(&b.position).then(a.id.cmp(&b.id)));
    rows.into_iter().map(|fog| fog.question.clone()).collect()
}

fn is_shut(item: &Item) -> bool {
    item.stage == Stage::Done || item.archived_at.is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use hummingbird_domain::{Fog, Project};

    fn sweep(items: Vec<Item>, projects: Vec<Project>, fog: Vec<Fog>) -> ChangesResponse {
        ChangesResponse {
            items,
            projects,
            fog,
            ..ChangesResponse::empty(1)
        }
    }

    fn item(id: &str, stage: Stage, project_id: Option<&str>) -> Item {
        Item {
            id: id.to_string(),
            seq: None,
            title: format!("item {id}"),
            description: None,
            stage,
            size: None,
            energy: None,
            context: None,
            priority: 0,
            project_id: project_id.map(str::to_string),
            project_pos: None,
            deadline: None,
            scheduled_date: None,
            source: None,
            source_key: None,
            source_url: None,
            vault_path: None,
            archived_at: None,
            agent: false,
            created_at: 1_000,
            updated_at: 1_000,
            version: 1,
        }
    }

    fn project(id: &str, name: &str) -> Project {
        Project {
            id: id.to_string(),
            name: name.to_string(),
            github_repo: None,
            default_context: None,
            archived_at: None,
            created_at: 1,
            updated_at: 1,
            version: 1,
        }
    }

    fn fog(id: &str, project_id: &str, question: &str, position: i64) -> Fog {
        Fog {
            id: id.to_string(),
            project_id: project_id.to_string(),
            question: question.to_string(),
            position,
            resolved_at: None,
            version: 1,
        }
    }

    #[test]
    fn triage_and_grilling_counts_skip_archived_rows() {
        let mut archived = item("t2", Stage::Triage, None);
        archived.archived_at = Some(1);
        let health = health(
            &sweep(
                vec![
                    item("t1", Stage::Triage, None),
                    archived,
                    item("g1", Stage::Grilling, None),
                    item("r1", Stage::Ready, None),
                ],
                vec![],
                vec![],
            ),
            0,
        );
        assert_eq!(health.triage, 1);
        assert_eq!(health.grilling, 1);
    }

    #[test]
    fn the_blocked_count_is_carried_through_untouched() {
        let health = health(&sweep(vec![], vec![], vec![]), 4);
        assert_eq!(health.blocked_dropped, 4);
    }

    #[test]
    fn a_project_whose_actions_are_all_shut_with_open_fog_flags_with_its_questions_verbatim() {
        let mut archived_action = item("a2", Stage::Ready, Some("p"));
        archived_action.archived_at = Some(2);
        let health = health(
            &sweep(
                vec![item("a1", Stage::Done, Some("p")), archived_action],
                vec![project("p", "Update Acumatica")],
                vec![
                    fog("f2", "p", "Which licence tier do we hold?", 2),
                    fog(
                        "f1",
                        "p",
                        "None — the unknowns are carried inside the two investigation actions",
                        1,
                    ),
                ],
            ),
            0,
        );
        assert_eq!(
            health.fog_exhausted,
            vec![FogExhausted {
                project_id: "p".to_string(),
                project: "Update Acumatica".to_string(),
                questions: vec![
                    "None — the unknowns are carried inside the two investigation actions"
                        .to_string(),
                    "Which licence tier do we hold?".to_string(),
                ],
            }]
        );
    }

    #[test]
    fn one_open_action_is_enough_to_keep_a_project_off_the_footer() {
        let health = health(
            &sweep(
                vec![
                    item("a1", Stage::Done, Some("p")),
                    item("a2", Stage::Triage, Some("p")),
                ],
                vec![project("p", "P")],
                vec![fog("f1", "p", "what?", 1)],
            ),
            0,
        );
        assert!(health.fog_exhausted.is_empty());
    }

    #[test]
    fn a_project_with_no_actions_at_all_has_not_run_out_of_them() {
        let health = health(
            &sweep(
                vec![],
                vec![project("p", "P")],
                vec![fog("f1", "p", "what?", 1)],
            ),
            0,
        );
        assert!(health.fog_exhausted.is_empty());
    }

    #[test]
    fn a_project_with_every_fog_row_resolved_does_not_flag() {
        let mut resolved = fog("f1", "p", "what?", 1);
        resolved.resolved_at = Some(99);
        let health = health(
            &sweep(
                vec![item("a1", Stage::Done, Some("p"))],
                vec![project("p", "P")],
                vec![resolved],
            ),
            0,
        );
        assert!(health.fog_exhausted.is_empty());
    }

    #[test]
    fn an_archived_project_never_flags() {
        let mut archived = project("p", "P");
        archived.archived_at = Some(1);
        let health = health(
            &sweep(
                vec![item("a1", Stage::Done, Some("p"))],
                vec![archived],
                vec![fog("f1", "p", "what?", 1)],
            ),
            0,
        );
        assert!(health.fog_exhausted.is_empty());
    }

    /// The footer's hand-off count, and every exclusion in it. `Blocked`
    /// counts — #10's protocol has an agent report a real external blocker
    /// and stop, and that chore still has agent work left on it — while
    /// `Done` and archived do not.
    #[test]
    fn the_hand_off_count_is_live_marked_and_unshut() {
        let mut ready = item("a1", Stage::Ready, None);
        ready.agent = true;
        let mut blocked = item("a2", Stage::Blocked, None);
        blocked.agent = true;
        let mut done = item("a3", Stage::Done, None);
        done.agent = true;
        let mut archived = item("a4", Stage::Ready, None);
        archived.agent = true;
        archived.archived_at = Some(1);
        let mine = item("a5", Stage::Ready, None);

        let health = health(
            &sweep(vec![ready, blocked, done, archived, mine], vec![], vec![]),
            0,
        );
        assert_eq!(health.agent_doable, 2);
    }

    #[test]
    fn the_hand_off_count_is_zero_when_nothing_is_marked() {
        let health = health(&sweep(vec![item("a1", Stage::Ready, None)], vec![], vec![]), 0);
        assert_eq!(health.agent_doable, 0, "unmarked means the human does it");
    }

    #[test]
    fn flagged_projects_come_back_in_a_total_order() {
        let health = health(
            &sweep(
                vec![
                    item("a1", Stage::Done, Some("p2")),
                    item("a2", Stage::Done, Some("p1")),
                ],
                vec![project("p2", "Beta"), project("p1", "Alpha")],
                vec![fog("f1", "p1", "q1", 1), fog("f2", "p2", "q2", 1)],
            ),
            0,
        );
        let names: Vec<&str> = health
            .fog_exhausted
            .iter()
            .map(|p| p.project.as_str())
            .collect();
        assert_eq!(names, ["Alpha", "Beta"]);
    }
}
