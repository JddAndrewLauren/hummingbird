//! **The tzdb fence (#856): no time-zone database is reachable from the
//! crates CLAUDE.md's thin-build rule protects.**
//!
//! This crate holds a tzdb (`jiff`, for `src/clamp.rs`),
//! and ADR-0035 lets it do so on one condition: the tzdb never becomes
//! reachable from `domain`, `hummingbird-core`, any `ffi-*` crate or the
//! authority worker. The panes keep crossing ADR-0025's zone bridge for
//! their own civil-date reasoning. Inspection cannot hold that line, because
//! the edge that breaks it is usually transitive. A crate the core already
//! takes can gain a tz feature in a minor bump.
//!
//! So this walks the two **committed lockfiles**, `server/Cargo.lock` and
//! `client/Cargo.lock`, from each protected root and fails on the first
//! tzdb crate it reaches, naming the path. A lockfile rather than
//! `cargo tree`, because the server job never fetches the client
//! workspace's sources, and the lockfile needs nothing fetched at all. The
//! lockfile lists every target's and every enabled feature's edges, so the
//! walk over-approximates. A false alarm costs a look; a missed edge costs
//! the wasm build.

use std::collections::{BTreeMap, VecDeque};

const SERVER_LOCK: &str = include_str!("../../Cargo.lock");
const CLIENT_LOCK: &str = include_str!("../../../client/Cargo.lock");

/// A crate that carries or reads a time-zone database. `jiff` is the one
/// this repo's pollers use; `chrono-tz` is the one the core removed.
/// `chrono` itself is not here: it has no zone table, which is why the core
/// may keep it.
fn is_tzdb(name: &str) -> bool {
    name == "jiff"
        || name.starts_with("jiff-tzdb")
        || name.starts_with("chrono-tz")
        || name.starts_with("tzdb")
        || matches!(name, "tz-rs" | "tzfile" | "time-tz" | "parse-zoneinfo")
}

struct Package {
    name: String,
    version: String,
    dependencies: Vec<String>,
}

/// The `[[package]]` blocks of a `Cargo.lock`, read line by line. The
/// format is cargo's own and stable: one `name`, one `version` and an
/// optional `dependencies = [ ... ]` list per block.
fn packages(lock: &str) -> Vec<Package> {
    let mut packages = Vec::new();
    let mut in_dependencies = false;
    for line in lock.lines() {
        let line = line.trim();
        if line == "[[package]]" {
            packages.push(Package { name: String::new(), version: String::new(), dependencies: Vec::new() });
            in_dependencies = false;
            continue;
        }
        let Some(package) = packages.last_mut() else { continue };
        if in_dependencies {
            if line == "]" {
                in_dependencies = false;
            } else {
                package.dependencies.push(line.trim_end_matches(',').trim_matches('"').to_owned());
            }
        } else if let Some(name) = line.strip_prefix("name = ") {
            package.name = name.trim_matches('"').to_owned();
        } else if let Some(version) = line.strip_prefix("version = ") {
            package.version = version.trim_matches('"').to_owned();
        } else if line == "dependencies = [" {
            in_dependencies = true;
        }
    }
    packages
}

/// The index of the package a lockfile dependency entry names: `"name"`
/// when the lockfile holds one version of it, `"name version"` (with an
/// optional source after) when it holds several.
fn resolve(packages: &[Package], entry: &str) -> usize {
    let mut parts = entry.split_whitespace();
    let name = parts.next().expect("a dependency entry names a crate");
    let version = parts.next();
    let matches: Vec<usize> = packages
        .iter()
        .enumerate()
        .filter(|(_, package)| package.name == name && version.is_none_or(|v| package.version == v))
        .map(|(index, _)| index)
        .collect();
    assert_eq!(matches.len(), 1, "the lockfile entry {entry:?} names exactly one package");
    matches[0]
}

/// The first tzdb crate reachable from `root`, as the path that reaches
/// it, or `None` when there is none.
fn tzdb_path(lock: &str, root: &str) -> Option<Vec<String>> {
    let packages = packages(lock);
    let start = resolve(&packages, root);
    let mut parent: BTreeMap<usize, usize> = BTreeMap::new();
    let mut queue = VecDeque::from([start]);
    parent.insert(start, start);
    while let Some(at) = queue.pop_front() {
        if is_tzdb(&packages[at].name) {
            let mut path = vec![packages[at].name.clone()];
            let mut step = at;
            while step != start {
                step = parent[&step];
                path.push(packages[step].name.clone());
            }
            path.reverse();
            return Some(path);
        }
        for entry in &packages[at].dependencies {
            let next = resolve(&packages, entry);
            if let std::collections::btree_map::Entry::Vacant(slot) = parent.entry(next) {
                slot.insert(at);
                queue.push_back(next);
            }
        }
    }
    None
}

fn assert_fenced(lock: &str, root: &str) {
    if let Some(path) = tzdb_path(lock, root) {
        panic!(
            "a tzdb is reachable from {root}: {}. CLAUDE.md's thin-build rule and \
             ADR-0035 keep it in the out-of-process pollers",
            path.join(" -> ")
        );
    }
}

#[test]
fn no_tzdb_is_reachable_from_the_domain_or_the_worker() {
    assert_fenced(SERVER_LOCK, "hummingbird-domain");
    assert_fenced(SERVER_LOCK, "hummingbird-authority-worker");
}

/// Every `ffi-*` crate is read off the lockfile rather than listed, so a
/// third binding is fenced the day it is added.
#[test]
fn no_tzdb_is_reachable_from_the_core_or_any_ffi_crate() {
    assert_fenced(CLIENT_LOCK, "hummingbird-domain");
    assert_fenced(CLIENT_LOCK, "hummingbird-core");
    let ffi: Vec<String> = packages(CLIENT_LOCK)
        .into_iter()
        .map(|package| package.name)
        .filter(|name| name.starts_with("hummingbird-ffi-"))
        .collect();
    assert!(ffi.len() >= 2, "the client workspace holds ffi-web and ffi-mobile, found {ffi:?}");
    for root in &ffi {
        assert_fenced(CLIENT_LOCK, root);
    }
}

/// The positive control: a fence that sees no edges passes everything.
/// This crate and `city-waste` really do reach `jiff`, so the walk must
/// find it from both.
#[test]
fn the_walk_finds_the_tzdb_the_pollers_really_hold() {
    for root in ["hummingbird-fantasy-poll", "hummingbird-city-waste"] {
        let path = tzdb_path(SERVER_LOCK, root).unwrap_or_else(|| panic!("{root} holds jiff"));
        assert_eq!(path.first().map(String::as_str), Some(root));
        assert_eq!(path.last().map(String::as_str), Some("jiff"));
    }
}

/// The failure the fence exists for, on a lockfile small enough to read: a
/// tzdb two hops down a dependency the domain already takes, behind a
/// version-qualified entry.
#[test]
fn a_transitive_tzdb_under_the_domain_fails_the_fence() {
    let lock = r#"
[[package]]
name = "hummingbird-domain"
version = "0.1.0"
dependencies = [
 "serde 1.0.0",
]

[[package]]
name = "serde"
version = "1.0.0"
dependencies = [
 "chrono-tz",
]

[[package]]
name = "serde"
version = "2.0.0"

[[package]]
name = "chrono-tz"
version = "0.10.0"
"#;
    assert_eq!(
        tzdb_path(lock, "hummingbird-domain"),
        Some(vec!["hummingbird-domain".to_owned(), "serde".to_owned(), "chrono-tz".to_owned()])
    );
    let caught = std::panic::catch_unwind(|| assert_fenced(lock, "hummingbird-domain"));
    assert!(caught.is_err(), "the fence fails on it");
}
