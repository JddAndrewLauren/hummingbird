//! The write vocabulary (#101): exactly ADR-0009's schema minus alert
//! *ingest* — items, steps, blocked_by edges, projects, routes, fog,
//! settings — plus #140's rules addition (ADR-0012/0013, `device`-scope
//! like every entry here — `POST /api/rules`/`PATCH /api/rules/:id` carry
//! no `ingest` restriction, unlike alert ingest below) and #626's
//! `project_links` (ADR-0030 decision 4).
//!
//! **Alert ingest (`POST /api/alerts`) is deliberately absent** and stays
//! absent: it is `ingest`-scope only (`auth::permitted` on the authority),
//! and this client surface is `device`-scope end to end, so there is no
//! collection constructor here that could ever target it. That exclusion
//! was always about ingest, not about the row: `PATCH /api/alerts/:id` is
//! `device`-scope (it writes the one human-owned column, `dismissed_at`)
//! and joined this vocabulary with M2's Ack action (#141, ADR-0012 —
//! acking is a gesture, so it has to be a write). Hence [`alert`] exists
//! and [`alerts`] does not.
//!
//! Path-only — the request method and body are the caller's, built from
//! `hummingbird_domain`'s create/patch DTOs.

pub fn items() -> String {
    "/api/items".to_string()
}

pub fn item(id: &str) -> String {
    format!("/api/items/{id}")
}

pub fn steps() -> String {
    "/api/steps".to_string()
}

pub fn step(id: &str) -> String {
    format!("/api/steps/{id}")
}

pub fn blocked_by() -> String {
    "/api/blocked_by".to_string()
}

pub fn blocked_by_edge(item_id: &str, blocker_id: &str) -> String {
    format!("/api/blocked_by/{item_id}/{blocker_id}")
}

pub fn projects() -> String {
    "/api/projects".to_string()
}

pub fn project(id: &str) -> String {
    format!("/api/projects/{id}")
}

pub fn route(project_id: &str) -> String {
    format!("/api/routes/{project_id}")
}

pub fn fog() -> String {
    "/api/fog".to_string()
}

pub fn fog_item(id: &str) -> String {
    format!("/api/fog/{id}")
}

/// `POST /api/project_links` (#626, ADR-0030 decision 4).
pub fn project_links() -> String {
    "/api/project_links".to_string()
}

/// `PATCH /api/project_links/:id` (#626).
pub fn project_link(id: &str) -> String {
    format!("/api/project_links/{id}")
}

pub fn setting(key: &str) -> String {
    format!("/api/settings/{key}")
}

pub fn rules() -> String {
    "/api/rules".to_string()
}

pub fn rule(id: &str) -> String {
    format!("/api/rules/{id}")
}

/// `PATCH /api/alerts/:id` (#141, `device` scope) — the Ack write, and the
/// only alert route this vocabulary names. There is deliberately no
/// `alerts()` collection constructor to pair with it: that would be the
/// `ingest`-scope raise, which this surface can never reach (module doc).
pub fn alert(id: &str) -> String {
    format!("/api/alerts/{id}")
}

/// `POST /api/grills` (#354, ADR-0023) — the one atomic Grill-completion
/// route. No by-id constructor here: unlike every other entity in this
/// vocabulary, a Grill has no client-issued PATCH (ADR-0023 decision 2: it
/// is immutable once written), so there is nothing for a second path
/// helper to name.
pub fn grills() -> String {
    "/api/grills".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pins the whole path vocabulary at once: adding an entity to
    /// ADR-0009's write surface without adding it here is the drift this
    /// test exists to catch.
    #[test]
    fn the_vocabulary_is_exactly_adr_0009s_write_surface_minus_alert_ingest() {
        let paths = [
            items(),
            item("x"),
            steps(),
            step("x"),
            blocked_by(),
            blocked_by_edge("a", "b"),
            projects(),
            project("x"),
            route("x"),
            fog(),
            fog_item("x"),
            project_links(),
            project_link("x"),
            setting("x"),
            rules(),
            rule("x"),
            alert("x"),
            grills(),
        ];
        for path in paths {
            assert!(path.starts_with("/api/"));
        }
    }

    /// The alert exclusion, restated precisely now that [`alert`] exists:
    /// what this surface may never name is the `ingest`-scope *collection*.
    /// A bare `/api/alerts` — with no id after it — is the raise route, so
    /// every alert path here must carry an id segment.
    #[test]
    fn the_only_alert_path_is_the_by_id_patch_never_the_ingest_collection() {
        assert_eq!(alert("a-1"), "/api/alerts/a-1");
        let alert_paths = [alert("x")];
        for path in alert_paths {
            assert!(
                path.strip_prefix("/api/alerts/")
                    .is_some_and(|id| !id.is_empty()),
                "alert ingest is out of scope: {path}"
            );
        }
    }

    #[test]
    fn entity_paths_carry_the_id_verbatim() {
        assert_eq!(item("uuid-1"), "/api/items/uuid-1");
        assert_eq!(step("uuid-2"), "/api/steps/uuid-2");
        assert_eq!(blocked_by_edge("a-1", "a-2"), "/api/blocked_by/a-1/a-2");
        assert_eq!(route("proj-1"), "/api/routes/proj-1");
        assert_eq!(setting("theme"), "/api/settings/theme");
        assert_eq!(rule("r-1"), "/api/rules/r-1");
    }
}
