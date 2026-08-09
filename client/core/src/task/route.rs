//! [`Route`]: a project's plan, as `CONTEXT.md` defines it — Destination,
//! Fog, notes, and the ordered actions.
//!
//! A Route lives in the Linear project's **`content`** field, not its
//! `description` (which is a separate short summary and never holds a Route).
//! `/to-actions` writes it and `/next-up-personal` reads the `## Destination`
//! and `## Fog` sections **verbatim**, so this module parses those sections
//! out for display while keeping the whole `content` intact: the client is
//! allowed to write Routes back, and a writer that cannot reproduce the parts
//! it did not model would quietly eat them.

use serde::{Deserialize, Serialize};

/// A project's plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Route {
    /// The project name — the key [`super::Item::project`] joins on, for the
    /// same complexity reason recorded there.
    pub name: String,
    /// The project's `content` field, verbatim and unparsed.
    ///
    /// The source of truth for everything below, kept because this client may
    /// write Routes: round-tripping through parsed sections alone would drop
    /// any part of the document this parser does not model.
    pub content: Option<String>,
    pub updated_at_ms: i64,
    pub presence: super::item::Presence,
}

impl Route {
    pub fn new(name: impl Into<String>, content: Option<String>) -> Self {
        Self {
            name: name.into(),
            content,
            updated_at_ms: 0,
            presence: super::item::Presence::Live,
        }
    }

    /// The `## Destination` section, verbatim, or `None`.
    pub fn destination(&self) -> Option<&str> {
        self.section("Destination")
    }

    /// The `## Fog` section, verbatim, or `None`.
    ///
    /// Uninterpreted on purpose: a Fog section reading `* None — the path is
    /// clear` is a non-null string, and deciding what that *means* is a
    /// consumer's job, not this parser's.
    pub fn fog(&self) -> Option<&str> {
        self.section("Fog")
    }

    /// The body of a `## <title>` section: everything up to the next heading
    /// at the same level or above, trimmed.
    ///
    /// Returns `None` for a missing section but `Some("")` for a present but
    /// empty one — "the author wrote nothing here" and "the author wrote no
    /// such section" are different facts, and collapsing them would make an
    /// emptied Fog read as an unwritten one.
    pub fn section(&self, title: &str) -> Option<&str> {
        let content = self.content.as_deref()?;
        let mut body_start = None;
        let mut body_end = content.len();

        // Located by byte offset into `content` rather than by rebuilding the
        // text from `lines()`, so the returned slice borrows the author's own
        // bytes. Offsets come from [`subslice_offset`] rather than a running
        // `line.len() + 1` counter, because that counter silently drifts by
        // one per line on CRLF input — `lines()` strips `\r\n` but the
        // arithmetic would only put back the `\n`.
        for line in content.lines() {
            let line_start = subslice_offset(content, line);
            match body_start {
                None if heading_title(line) == Some(title) => {
                    body_start = Some(line_start + line.len());
                }
                Some(_) if heading_title(line).is_some() => {
                    body_end = line_start;
                    break;
                }
                _ => {}
            }
        }

        let start = body_start?;
        Some(
            content[start..body_end]
                .trim_start_matches(['\n', '\r'])
                .trim_end(),
        )
    }
}

/// The byte offset of `part` within `whole`, where `part` is known to be a
/// subslice of it (as everything `str::lines` yields is).
///
/// Address arithmetic, not a search: a `find` would return the *first* match,
/// which is wrong the moment a Route repeats a line.
fn subslice_offset(whole: &str, part: &str) -> usize {
    part.as_ptr() as usize - whole.as_ptr() as usize
}

/// The title of a `##`-level (or shallower) markdown heading, if this line is
/// one.
///
/// `###` and deeper are deliberately *not* headings here: they are content
/// inside a section, and treating them as terminators would truncate any
/// Route whose Fog has sub-points.
fn heading_title(line: &str) -> Option<&str> {
    let rest = line.strip_prefix('#')?;
    let level_extra = rest.len() - rest.trim_start_matches('#').len();
    if level_extra > 1 {
        return None;
    }
    let title = rest.trim_start_matches('#').trim();
    if title.is_empty() {
        None
    } else {
        Some(title)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONTENT: &str = "\
## Destination
A working desktop client.

## Fog
* Capture UX is unspecified.
* Parse posture is unresolved.

## Notes
Ordered actions below.
";

    fn route() -> Route {
        Route::new("Build the client", Some(CONTENT.to_string()))
    }

    #[test]
    fn destination_and_fog_come_back_verbatim() {
        assert_eq!(route().destination(), Some("A working desktop client."));
        assert_eq!(
            route().fog(),
            Some("* Capture UX is unspecified.\n* Parse posture is unresolved.")
        );
    }

    #[test]
    fn a_section_stops_at_the_next_heading_not_at_the_end_of_the_document() {
        // The Notes section must not be swallowed into Fog.
        assert!(!route().fog().unwrap().contains("Ordered actions"));
    }

    #[test]
    fn subheadings_do_not_terminate_a_section() {
        // `###` is content inside Fog, not the start of a new section —
        // treating it as a terminator would truncate the author's text.
        let content = "## Fog\n### Open\n* one\n* two\n\n## Notes\nx\n";
        let route = Route::new("p", Some(content.to_string()));
        let fog = route.fog().unwrap();
        assert!(fog.contains("### Open"), "{fog}");
        assert!(fog.contains("* two"), "{fog}");
        assert!(!fog.contains("Notes"), "{fog}");
    }

    #[test]
    fn a_missing_section_is_none_but_an_empty_one_is_some_empty() {
        // Different facts: "no such section" vs "the author emptied it".
        let route = Route::new("p", Some("## Fog\n\n## Notes\nx\n".to_string()));
        assert_eq!(route.fog(), Some(""));
        assert_eq!(route.destination(), None);
    }

    #[test]
    fn crlf_content_does_not_drift_the_section_boundaries() {
        // A running `line.len() + 1` offset counter loses a byte per line
        // here, which silently truncates the section from the right.
        let content = "## Destination\r\nA client.\r\n\r\n## Fog\r\n* one\r\n* two\r\n";
        let route = Route::new("p", Some(content.to_string()));

        assert_eq!(route.destination(), Some("A client."));
        assert_eq!(route.fog(), Some("* one\r\n* two"));
    }

    #[test]
    fn a_repeated_line_does_not_confuse_the_offset_lookup() {
        // A `find`-based offset would match the first "* one" and cut the
        // section short.
        let content = "## Notes\n* one\n\n## Fog\n* one\n* two\n";
        let route = Route::new("p", Some(content.to_string()));

        assert_eq!(route.fog(), Some("* one\n* two"));
    }

    #[test]
    fn a_route_with_no_content_has_no_sections() {
        let route = Route::new("p", None);
        assert_eq!(route.fog(), None);
        assert_eq!(route.destination(), None);
    }

    #[test]
    fn the_whole_content_survives_so_a_write_can_round_trip() {
        // The client may write Routes; anything the parser does not model
        // still has to come back out byte-for-byte.
        let route = route();
        assert_eq!(route.content.as_deref(), Some(CONTENT));
    }
}
