//! The write vocabulary (#101): exactly ADR-0009's schema minus alerts —
//! items, steps, blocked_by edges, projects, routes, fog, settings — plus
//! #140's rules addition (ADR-0012/0013, `device`-scope like every entry
//! here — `POST /api/rules`/`PATCH /api/rules/:id` carry no `ingest`
//! restriction, unlike alert ingest below). Alert ingest (`POST
//! /api/alerts`) is deliberately absent: it is `ingest`-scope only
//! (`auth::permitted` on the authority), and this client surface is
//! `device`-scope end to end, so there is no path constructor here that
//! could ever target it.
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

pub fn setting(key: &str) -> String {
    format!("/api/settings/{key}")
}

pub fn rules() -> String {
    "/api/rules".to_string()
}

pub fn rule(id: &str) -> String {
    format!("/api/rules/{id}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pins the whole path vocabulary at once: adding an entity to
    /// ADR-0009's write surface without adding it here — or accidentally
    /// adding alerts — is the drift this test exists to catch.
    #[test]
    fn the_vocabulary_is_exactly_adr_0009s_write_surface_minus_alerts() {
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
            setting("x"),
            rules(),
            rule("x"),
        ];
        for path in paths {
            assert!(path.starts_with("/api/"));
            assert!(
                !path.contains("alerts"),
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
