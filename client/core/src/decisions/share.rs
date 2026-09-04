//! The share-payload mapping (#782): what a `text/plain` share from another
//! app becomes when it lands on the capture form, decided once here for
//! every client (ADR-0025) — Android's share target today, a web share
//! target whenever one is wanted.
//!
//! **This seeds a draft; it does not parse a capture.** [`crate::capture`]'s
//! `parse_seam` is a named no-op guarding #42, and nothing here touches it:
//! the human sees the seeded title, description and link on the form, edits
//! them, and [`crate::Core::capture`] still receives whatever title they
//! submit verbatim (ADR-0022 — the human is the parser). The only decision
//! made here is which *field* each piece of the payload starts in.
//!
//! The mapping, as grilled: the first `http(s)://` URL in the text becomes
//! the item's Link and is removed from the text. The title is the subject
//! when one is given, else the first non-empty remaining line, else the
//! URL's host — **a title is never a raw URL**, because the title is what
//! names an item everywhere and a bare URL never should. The link's name
//! stays empty (the host stands in for it — [`link_display_label`]). The
//! rest of the text, multi-line, is the description. A subject that is
//! itself a bare URL is a link rather than a title; when the text carries
//! its own URL the subject's becomes the description's first line, so
//! nothing shared is dropped.
//!
//! [`link_label_problem`] is the one rule about the pair itself — a name
//! needs a URL — stated here so both capture forms, both seams and every
//! item editor refuse the same shape with the same words.
//!
//! [`url_host`] is hand-rolled rather than pulled from a URL crate: the
//! wasm32 worker build stays thin, and the host is the only part of a URL
//! anything here reads.

/// What a share seeds the capture form with. `title` may be empty when the
/// payload carried nothing at all — [`super::can_submit_capture`] then
/// refuses the submit exactly as it would a hand-typed blank.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ShareDraft {
    pub title: String,
    pub description: Option<String>,
    pub link_url: Option<String>,
}

/// The mapping in the module header, over `EXTRA_SUBJECT` and `EXTRA_TEXT`
/// as Android hands them over (either may be empty).
pub fn parse_share_payload(subject: &str, text: &str) -> ShareDraft {
    let subject = subject.trim();
    // A subject that is itself a bare URL is not a title (a title is never
    // a raw URL); it is a link, used only when the text carries none.
    let subject_is_url = first_http_url(subject).is_some_and(|url| url == subject);
    let (link_url, remainder) = match first_http_url(text) {
        Some(url) => {
            let (before, after) = text.split_once(url).expect("url was found in text");
            // The whole whitespace-delimited token goes, not just the URL:
            // the punctuation `first_http_url` trimmed off its end would
            // otherwise be left behind as debris in the title.
            let after = after.trim_start_matches(|c: char| !c.is_whitespace());
            let before = before.trim_end_matches([' ', '\t']);
            let after = after.trim_start_matches([' ', '\t']);
            (Some(url.to_string()), format!("{before} {after}"))
        }
        None if subject_is_url => (Some(subject.to_string()), text.to_string()),
        None => (None, text.to_string()),
    };

    let mut lines: Vec<&str> = remainder.lines().map(str::trim).collect();
    while lines.first().is_some_and(|line| line.is_empty()) {
        lines.remove(0);
    }
    while lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }

    let title = if !subject.is_empty() && !subject_is_url {
        subject.to_string()
    } else if let Some(first) = lines.first() {
        let first = first.to_string();
        lines.remove(0);
        while lines.first().is_some_and(|line| line.is_empty()) {
            lines.remove(0);
        }
        first
    } else {
        link_url.as_deref().and_then(url_host).unwrap_or_default()
    };

    // A subject URL the text's own URL displaced is kept, not dropped.
    if subject_is_url && link_url.as_deref() != Some(subject) {
        lines.insert(0, subject);
    }
    let description = if lines.is_empty() { None } else { Some(lines.join("\n")) };
    ShareDraft { title, description, link_url }
}

/// The first `http://` or `https://` URL in `text` (the scheme matched
/// case-insensitively, as a browser would): from the scheme to the
/// next whitespace, minus any trailing punctuation a sentence wrapped it in
/// (`https://example.test/a.` shares as `https://example.test/a`). A
/// closing `)` is stripped only when the URL has no matching `(` — a
/// Wikipedia-style `/Foo_(bar)` keeps its paren.
pub fn first_http_url(text: &str) -> Option<&str> {
    // ASCII lowercasing keeps every byte offset, so the match found in the
    // lowered copy indexes the original.
    let lowered = text.to_ascii_lowercase();
    let start = ["https://", "http://"]
        .iter()
        .filter_map(|scheme| lowered.find(scheme))
        .min()?;
    let rest = &text[start..];
    let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
    let mut url = &rest[..end];
    while let Some(last) = url.chars().last() {
        let strip = match last {
            '.' | ',' | ';' | ':' | '!' | '?' | '\'' | '"' | '>' | ']' | '}' => true,
            ')' => url.matches('(').count() < url.matches(')').count(),
            _ => false,
        };
        if !strip {
            break;
        }
        url = &url[..url.len() - last.len_utf8()];
    }
    // A bare scheme is not a link.
    let scheme_len = if lowered[start..].starts_with("https://") { 8 } else { 7 };
    (url.len() > scheme_len).then_some(url)
}

/// The host of an `http(s)` URL as a person would name it: the authority
/// minus any userinfo, minus the port, minus a leading `www.`, lowercased.
/// `None` for anything that is not an `http(s)://` URL with a host — a
/// caller drawing a link uses that answer to refuse to draw one.
pub fn url_host(url: &str) -> Option<String> {
    let after_scheme = strip_http_scheme(url)?;
    let authority_end = after_scheme
        .find(['/', '?', '#'])
        .unwrap_or(after_scheme.len());
    let authority = &after_scheme[..authority_end];
    let host_port = authority.rsplit_once('@').map_or(authority, |(_, host)| host);
    let host = host_port.rsplit_once(':').map_or(host_port, |(host, port)| {
        if port.chars().all(|c| c.is_ascii_digit()) { host } else { host_port }
    });
    let host = host.strip_prefix("www.").unwrap_or(host);
    (!host.is_empty()).then(|| host.to_ascii_lowercase())
}

/// `url` minus its `http(s)://` scheme, matched case-insensitively;
/// `None` when it has no such scheme.
fn strip_http_scheme(url: &str) -> Option<&str> {
    let n = if url.get(..8).is_some_and(|p| p.eq_ignore_ascii_case("https://")) {
        8
    } else if url.get(..7).is_some_and(|p| p.eq_ignore_ascii_case("http://")) {
        7
    } else {
        return None;
    };
    Some(&url[n..])
}

/// #782's one rule about the pair: a name is only meaningful beside a
/// URL. `Some(message)` when `label` has content and `url` has none —
/// the same shape the authority answers 400 to, refused on the form with
/// these words and at each seam with the same test, so a stranded name
/// never reaches the dead-letter journal.
pub fn link_label_problem(url: &str, label: &str) -> Option<String> {
    (!label.trim().is_empty() && url.trim().is_empty()).then(|| LINK_LABEL_NEEDS_URL.to_string())
}

/// The message [`link_label_problem`] carries.
pub const LINK_LABEL_NEEDS_URL: &str = "A link name needs a URL";

/// Whether a stored Link is one a client may draw as a tap that leaves the
/// app: an `http(s)://` URL with a host, and nothing else. The column is
/// plain text the authority checks for non-blankness alone, so a
/// `javascript:` or `intent:` string can reach it from any writer; drawing
/// nothing is how a client refuses. Decided here so the web's anchor and
/// Android's `ACTION_VIEW` cannot disagree about what is safe to follow.
pub fn is_followable_link(url: &str) -> bool {
    url_host(url).is_some()
}

/// What a Link is called wherever it is drawn: its name when one was
/// given, else its host, else the URL itself. The name is what changes
/// least, and the host is usually enough.
pub fn link_display_label(url: &str, label: Option<&str>) -> String {
    if let Some(label) = label.map(str::trim).filter(|label| !label.is_empty()) {
        return label.to_string();
    }
    url_host(url).unwrap_or_else(|| url.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_subject_and_a_url_title_by_the_subject_and_link_the_url() {
        let draft = parse_share_payload(
            "Knee rehab video",
            "Watch this later https://www.youtube.com/watch?v=abc",
        );
        assert_eq!(draft.title, "Knee rehab video");
        assert_eq!(draft.link_url.as_deref(), Some("https://www.youtube.com/watch?v=abc"));
        assert_eq!(
            draft.description.as_deref(),
            Some("Watch this later"),
            "the URL is removed from the text, and what is left is the description",
        );
    }

    #[test]
    fn a_url_alone_titles_by_its_host() {
        let draft = parse_share_payload("", "https://www.youtube.com/watch?v=abc");
        assert_eq!(draft.title, "youtube.com", "a title is never a raw URL");
        assert_eq!(draft.link_url.as_deref(), Some("https://www.youtube.com/watch?v=abc"));
        assert_eq!(draft.description, None);
    }

    #[test]
    fn multi_line_text_without_a_subject_titles_by_the_first_line() {
        let draft = parse_share_payload(
            "",
            "\nPhysio notes\n\nDo the stretches twice a day.\nBook the follow-up.\n",
        );
        assert_eq!(draft.title, "Physio notes");
        assert_eq!(
            draft.description.as_deref(),
            Some("Do the stretches twice a day.\nBook the follow-up."),
            "the remainder keeps its lines; leading and trailing blanks go",
        );
        assert_eq!(draft.link_url, None);
    }

    #[test]
    fn a_one_line_share_with_a_url_titles_by_the_words_around_it() {
        let draft = parse_share_payload("", "Buy this https://shop.example.test/thing");
        assert_eq!(draft.title, "Buy this");
        assert_eq!(draft.link_url.as_deref(), Some("https://shop.example.test/thing"));
        assert_eq!(draft.description, None);
    }

    #[test]
    fn a_share_with_no_url_seeds_no_link() {
        let draft = parse_share_payload("A thought", "Just a paragraph of text.");
        assert_eq!(draft.title, "A thought");
        assert_eq!(draft.description.as_deref(), Some("Just a paragraph of text."));
        assert_eq!(draft.link_url, None);

        let empty = parse_share_payload("", "");
        assert_eq!(empty, ShareDraft::default(), "nothing in, nothing seeded");
    }

    #[test]
    fn the_host_drops_www_userinfo_port_and_case() {
        assert_eq!(url_host("https://www.YouTube.com/watch?v=abc").as_deref(), Some("youtube.com"));
        assert_eq!(url_host("http://user:pw@example.test:8080/x").as_deref(), Some("example.test"));
        assert_eq!(url_host("https://example.test").as_deref(), Some("example.test"));
        assert_eq!(url_host("ftp://example.test/x"), None, "not http(s)");
        assert_eq!(url_host("https:///x"), None, "no host");
        assert_eq!(url_host("not a url"), None);
    }

    #[test]
    fn trailing_punctuation_is_not_part_of_the_url() {
        assert_eq!(
            first_http_url("Read https://example.test/a, then https://b.test."),
            Some("https://example.test/a"),
        );
        assert_eq!(first_http_url("(see https://example.test/a)"), Some("https://example.test/a"));
        assert_eq!(
            first_http_url("https://en.wikipedia.org/wiki/Foo_(bar)"),
            Some("https://en.wikipedia.org/wiki/Foo_(bar)"),
            "a balanced paren stays",
        );
        assert_eq!(first_http_url("https:// is not a link"), None);
    }

    #[test]
    fn a_subject_that_is_only_a_url_is_a_link_not_a_title() {
        let draft = parse_share_payload("https://example.test/page", "");
        assert_eq!(draft.title, "example.test");
        assert_eq!(draft.link_url.as_deref(), Some("https://example.test/page"));

        // With a URL in the text as well, the text's is the link and the
        // subject's is kept as the description's first line — never dropped.
        let draft = parse_share_payload("https://subject.test/a", "Read this https://body.test/b");
        assert_eq!(draft.title, "Read this");
        assert_eq!(draft.link_url.as_deref(), Some("https://body.test/b"));
        assert_eq!(draft.description.as_deref(), Some("https://subject.test/a"));
    }

    #[test]
    fn a_url_mid_sentence_leaves_no_debris_in_the_title() {
        let draft = parse_share_payload("", "See https://a.test/x. Then more");
        assert_eq!(draft.title, "See Then more", "the stripped `.` and the doubled space go too");
        assert_eq!(draft.link_url.as_deref(), Some("https://a.test/x"));

        let draft = parse_share_payload("", "Title\nhttps://a.test/x\nBody line");
        assert_eq!(draft.title, "Title", "line structure survives the removal");
        assert_eq!(draft.description.as_deref(), Some("Body line"));

        let draft = parse_share_payload("", "https://a.test/x rest");
        assert_eq!(draft.title, "rest");
    }

    #[test]
    fn the_scheme_is_matched_case_insensitively() {
        assert_eq!(first_http_url("HTTPS://Example.test/A"), Some("HTTPS://Example.test/A"));
        assert_eq!(url_host("HTTPS://Example.test/A").as_deref(), Some("example.test"));
        assert!(is_followable_link("Http://example.test"));
    }

    #[test]
    fn a_link_name_needs_a_url() {
        assert_eq!(link_label_problem("", "Shop").as_deref(), Some("A link name needs a URL"));
        assert_eq!(link_label_problem("  ", " Shop "), Some(LINK_LABEL_NEEDS_URL.to_string()));
        assert_eq!(link_label_problem("https://shop.test/", "Shop"), None);
        assert_eq!(link_label_problem("", ""), None);
        assert_eq!(link_label_problem("https://shop.test/", ""), None);
    }

    #[test]
    fn only_an_http_url_with_a_host_is_followable() {
        assert!(is_followable_link("https://example.test/x"));
        assert!(is_followable_link("http://example.test"));
        for url in ["javascript:alert(1)", "intent://x#Intent;end", "mailto:x@y", "https:///x", ""] {
            assert!(!is_followable_link(url), "{url}");
        }
    }

    #[test]
    fn the_display_label_is_name_then_host_then_url() {
        assert_eq!(link_display_label("https://www.youtube.com/x", Some("Rehab")), "Rehab");
        assert_eq!(link_display_label("https://www.youtube.com/x", Some("  ")), "youtube.com");
        assert_eq!(link_display_label("https://www.youtube.com/x", None), "youtube.com");
        assert_eq!(link_display_label("mailto:x@y", None), "mailto:x@y");
    }
}
