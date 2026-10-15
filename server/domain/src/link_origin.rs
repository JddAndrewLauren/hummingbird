//! The per-token allowed link origin (ADR-0014's 2026-10-15 amendment,
//! #845): an ingest token may be minted with one `https` origin, and an
//! alert it writes whose `url` is outside that origin is refused, so a
//! stolen token cannot point the operator at a page of its choosing.
//!
//! **Hand-written, and strict by construction.** No `url` crate: this crate
//! is compiled into the wasm32 worker, which stays thin (CLAUDE.md's
//! repo-wide rule). So the parser is an allowlist rather than a faithful
//! WHATWG implementation — a host is ASCII letters, digits, `.` and `-`
//! only, and anything a browser might reinterpret (userinfo `@`, `\`, `%`,
//! whitespace, IPv6 brackets) fails the parse. Every such failure refuses
//! the link; nothing unusual is ever let through on a guess about how a
//! browser would read it.
//!
//! Comparison is by origin in the web's own sense: the scheme is always
//! `https`, the host compares without regard to ASCII case, and `:443` is
//! the same origin as no port at all.

const SCHEME: &str = "https://";
const DEFAULT_PORT: u16 = 443;

/// Parses an allowed link origin as the mint route accepts it: exactly
/// `https://host[:port]`, nothing else — no path (not even a trailing
/// `/`), no query, no fragment, no userinfo. Returns the canonical form
/// stored on the token: the host lowercased, `:443` folded away. `None`
/// for anything else, which the mint route answers with a 400.
pub fn parse_link_origin(origin: &str) -> Option<String> {
    let authority = strip_scheme(origin)?;
    let (host, port) = parse_authority(authority)?;
    Some(canonical(&host, port))
}

/// Whether `url` is on exactly `origin` (a value [`parse_link_origin`]
/// returned). The authority is everything between `https://` and the
/// first `/`, `?` or `#`; it must parse under the same host/port rules and
/// name the same host and effective port.
pub fn url_is_on_origin(url: &str, origin: &str) -> bool {
    let Some(rest) = strip_scheme(url) else { return false };
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let Some((host, port)) = parse_authority(&rest[..end]) else { return false };
    canonical(&host, port) == origin
}

fn strip_scheme(s: &str) -> Option<&str> {
    let head = s.get(..SCHEME.len())?;
    head.eq_ignore_ascii_case(SCHEME).then(|| &s[SCHEME.len()..])
}

/// `host[:port]`, the host lowercased and the port defaulted to 443.
fn parse_authority(authority: &str) -> Option<(String, u16)> {
    let (host, port) = match authority.split_once(':') {
        Some((host, port)) => (host, parse_port(port)?),
        None => (authority, DEFAULT_PORT),
    };
    let host_ok = !host.is_empty()
        && host.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-');
    host_ok.then(|| (host.to_ascii_lowercase(), port))
}

fn parse_port(port: &str) -> Option<u16> {
    if port.is_empty() || port.len() > 5 || !port.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    port.parse::<u16>().ok().filter(|p| *p != 0)
}

fn canonical(host: &str, port: u16) -> String {
    if port == DEFAULT_PORT {
        format!("{SCHEME}{host}")
    } else {
        format!("{SCHEME}{host}:{port}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_https_origin_parses_to_its_canonical_form() {
        assert_eq!(parse_link_origin("https://factory.example").as_deref(), Some("https://factory.example"));
        assert_eq!(parse_link_origin("HTTPS://Factory.Example").as_deref(), Some("https://factory.example"));
        assert_eq!(parse_link_origin("https://factory.example:8443").as_deref(), Some("https://factory.example:8443"));
        assert_eq!(parse_link_origin("https://factory.example:443").as_deref(), Some("https://factory.example"));
    }

    #[test]
    fn anything_beyond_scheme_host_and_port_is_refused() {
        for origin in [
            "",
            "factory.example",
            "http://factory.example",
            "ftp://factory.example",
            "https://",
            "https://factory.example/",
            "https://factory.example/map",
            "https://factory.example?x=1",
            "https://factory.example#map",
            "https://user@factory.example",
            "https://factory.example:",
            "https://factory.example:0",
            "https://factory.example:65536",
            "https://factory.example:84a3",
            "https://factory.example:1:2",
            "https://[::1]",
            "https://fac tory.example",
            "https://factory%2eexample",
            "https://factory.example\\evil",
            " https://factory.example",
        ] {
            assert_eq!(parse_link_origin(origin), None, "{origin:?} must be refused");
        }
    }

    #[test]
    fn a_url_on_the_origin_is_admitted() {
        let origin = "https://factory.example";
        for url in [
            "https://factory.example",
            "https://factory.example/",
            "https://factory.example/map?record=42#cause",
            "https://factory.example?record=42",
            "https://factory.example#top",
            "HTTPS://Factory.Example/map",
            "https://factory.example:443/map",
        ] {
            assert!(url_is_on_origin(url, origin), "{url:?} is on {origin}");
        }
        assert!(url_is_on_origin("https://factory.example:8443/map", "https://factory.example:8443"));
    }

    #[test]
    fn a_url_off_the_origin_is_refused() {
        let origin = "https://factory.example";
        for url in [
            "",
            "factory.example/map",
            "http://factory.example/map",
            "https://evil.example/map",
            "https://factory.example.evil.example/map",
            "https://evilfactory.example/map",
            "https://factory.example@evil.example/map",
            "https://evil.example\\@factory.example/map",
            "https://factory.example:8443/map",
            "https://sub.factory.example/map",
            "https://factory.example%2f@evil.example",
            " https://factory.example/map",
            "javascript:alert(1)//https://factory.example",
        ] {
            assert!(!url_is_on_origin(url, origin), "{url:?} must be refused for {origin}");
        }
        assert!(!url_is_on_origin("https://factory.example/map", "https://factory.example:8443"));
    }
}
