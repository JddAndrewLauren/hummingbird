//! What `projects.github_repo` (#625, ADR-0030 decision 2) may hold: the
//! canonical `owner/repo` slug and nothing else — never a URL, never a
//! trailing `.git`, never a leading `https://github.com/`. Storing only the
//! slug means there is exactly one spelling to compare and no half-typed
//! link to normalize; the display link is derived from it, never stored.
//!
//! One function, shared by every place that accepts the field: the
//! authority's handler ([`crate` re-export, used from
//! `server/authority/src/handlers/projects.rs`]) and the wasm seam
//! (`client/ffi-web/src/task_host.rs`) both call this, so a value neither
//! side agrees on can never reach the queue in the first place — the
//! `owner/repo` half of the brief's "validate at the handler **and** at the
//! wasm seam" instruction, from one definition rather than two that could
//! drift.

/// True when `s` is a well-formed `owner/repo` slug: exactly one `/`,
/// splitting two non-empty halves, each drawn from GitHub's own charset.
///
/// `owner` is alphanumeric-or-hyphen and may not start or end with a
/// hyphen (GitHub's username/org rule). `repo` is alphanumeric plus
/// `-`, `_`, `.`, and may not be `.` or `..` — the same dot-segment
/// exclusion `is_url_safe_id` makes, since a repo slug also rides inside a
/// derived URL path segment.
///
/// A trailing `.git` is rejected outright: `owner/repo.git` is what a clone
/// URL spells, and GitHub itself refuses to name a repository that way, so
/// accepting it would store a second spelling of one identity — exactly the
/// thing decision 2 keeps out by storing the slug alone. The dot charset
/// above would otherwise let it through.
pub fn is_valid_github_repo(s: &str) -> bool {
    let Some((owner, repo)) = s.split_once('/') else {
        return false;
    };
    if repo.contains('/') {
        return false;
    }
    is_valid_owner(owner) && is_valid_repo(repo)
}

fn is_valid_owner(owner: &str) -> bool {
    !owner.is_empty()
        && !owner.starts_with('-')
        && !owner.ends_with('-')
        && owner.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

fn is_valid_repo(repo: &str) -> bool {
    !repo.is_empty()
        && repo != "."
        && repo != ".."
        && !repo.ends_with(".git")
        && repo
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

#[cfg(test)]
mod tests {
    use super::is_valid_github_repo;

    #[test]
    fn accepts_ordinary_slugs() {
        assert!(is_valid_github_repo("JddAndrewLauren/hummingbird"));
        assert!(is_valid_github_repo("torvalds/linux"));
        assert!(is_valid_github_repo("a/b"));
        assert!(is_valid_github_repo("some-org/some.repo_name"));
    }

    #[test]
    fn rejects_a_url() {
        for bad in [
            "https://github.com/JddAndrewLauren/hummingbird",
            "github.com/JddAndrewLauren/hummingbird",
            "JddAndrewLauren/hummingbird/",
        ] {
            assert!(!is_valid_github_repo(bad), "{bad} must be rejected");
        }
    }

    #[test]
    fn rejects_missing_or_extra_slashes() {
        for bad in ["hummingbird", "", "/", "a/", "/b", "a/b/c"] {
            assert!(!is_valid_github_repo(bad), "{bad} must be rejected");
        }
    }

    #[test]
    fn rejects_an_owner_that_starts_or_ends_with_a_hyphen() {
        assert!(!is_valid_github_repo("-owner/repo"));
        assert!(!is_valid_github_repo("owner-/repo"));
    }

    #[test]
    fn rejects_a_dot_segment_repo() {
        assert!(!is_valid_github_repo("owner/."));
        assert!(!is_valid_github_repo("owner/.."));
    }

    #[test]
    fn rejects_a_clone_style_git_suffix() {
        assert!(!is_valid_github_repo("owner/repo.git"));
        assert!(!is_valid_github_repo("owner/.git"));
        // Only the suffix — a dot elsewhere is still an ordinary repo name.
        assert!(is_valid_github_repo("owner/repo.github.io"));
        assert!(is_valid_github_repo("owner/git"));
    }

    #[test]
    fn rejects_disallowed_characters() {
        for bad in ["owner space/repo", "owner/repo name", "owner/repo?x", "owner/repo#x"] {
            assert!(!is_valid_github_repo(bad), "{bad} must be rejected");
        }
    }
}
