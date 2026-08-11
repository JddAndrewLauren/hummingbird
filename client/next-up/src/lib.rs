//! The `/next-up-hb` skill's seam (#116): one `GET /api/sweep` payload in,
//! ranked candidates plus the footer's raw facts out.
//!
//! #116 was split at triage — [#162] built the deterministic ranker
//! (`hummingbird_core::rank`) and this crate is the caller its module doc
//! names. It exists because a `SKILL.md` on its own would leave `rank()`
//! exported, unit-tested and wired to nothing, the pattern this repo has
//! rejected three times.
//!
//! It follows the pollers' split exactly (`server/city-waste`,
//! `server/gmail-poll`, `server/calendar-poll`): **everything decidable is
//! here in the lib and natively tested; `main.rs` holds only stdin, stdout
//! and serde.** No clock read, no credential, no HTTP call anywhere in this
//! crate — "now" arrives in the envelope for the same reason
//! [`hummingbird_core::rank::Now`] takes it, and the survey fetch is the
//! shell script's job.
//!
//! **This crate is read-only.** Nothing here touches a write route. The
//! skill's delegation branch (#115/#291) does write — three CAS patches
//! through its own shell script — but never through anything in here: the
//! ranker's whole job is deciding, and a binary that could also mutate the
//! authority would be a second write path with no queue behind it.
//!
//! It is a member of the `client/` workspace rather than a `[[bin]]` inside
//! `client/core`: that crate is the binding-agnostic sync engine and its
//! wasm32 build has no business carrying a CLI.
//!
//! [#162]: https://github.com/JddAndrewLauren/hummingbird/issues/162

pub mod envelope;
pub mod health;
pub mod select;

use hummingbird_core::rank::{rank, RankedCandidate};
use serde::Serialize;

pub use envelope::{Envelope, EnvelopeProblem};
pub use health::Health;
pub use select::{Selection, Who};

/// What the binary writes to stdout: the ranked candidates (serialized by
/// `client/core`'s own derive, so the reason codes cross unchanged) and the
/// footer's facts.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Output {
    pub candidates: Vec<RankedCandidate>,
    pub health: Health,
}

/// The whole decidable job: select, rank, and gather the footer's material.
///
/// Deterministic — the same envelope produces byte-identical output, which
/// is what `rank`'s own total order buys and what this crate's end-to-end
/// fixture test pins.
pub fn run(envelope: &Envelope) -> Result<Output, EnvelopeProblem> {
    let now = envelope.now.to_rank_now();
    let axes = envelope.axes.to_rank_axes();

    let who = if envelope.agent_only { Who::AgentOnly } else { Who::Anyone };
    let selection = select::select(&envelope.sweep, &now, who);

    // The calendar context borrows from the owned wire shape, so it has to
    // outlive the `rank` call — hence the binding rather than an inline
    // `Option::map`.
    let calendar = match &envelope.calendar {
        Some(wire) => Some(wire.to_calendar_context()?),
        None => None,
    };

    let candidates = rank(&selection.candidates, &axes, &now, calendar.as_ref());
    let health = health::health(&envelope.sweep, selection.blocked_dropped);

    Ok(Output { candidates, health })
}

#[cfg(test)]
mod manifest_tests {
    /// This crate is baked into the runner image (`runner/Dockerfile`) as a
    /// prebuilt binary, and that image is `node:22-slim` — which carries
    /// neither `libssl.so.3`/`libcrypto.so.3` at runtime nor `pkg-config`
    /// and `libssl-dev` at build time. `hummingbird-core`'s default
    /// `reqwest-transport` feature pulls `reqwest` → `native-tls` →
    /// `openssl-sys`, which breaks both halves *separately*: the builder
    /// stage fails to compile, and a builder stage that somehow succeeded
    /// would emit a binary the final image cannot exec.
    ///
    /// Nothing about `rank()` needs a TLS stack — this crate makes no
    /// request at all — so the dependency is taken with
    /// `default-features = false`. Checked against the manifest's own text
    /// rather than convention, the same discipline
    /// `hummingbird_core`'s `cargo_toml_has_no_binding_macro_dependencies`
    /// follows: a `cargo build` here would *succeed* on a dev machine that
    /// has OpenSSL, so only the manifest can state the rule.
    #[test]
    fn the_core_dependency_takes_no_default_features_so_no_tls_stack_is_linked() {
        let manifest = include_str!("../Cargo.toml");
        let line = manifest
            .lines()
            .find(|line| line.trim_start().starts_with("hummingbird-core"))
            .expect("hummingbird-core is a dependency of this crate");
        assert!(
            line.contains("default-features = false"),
            "hummingbird-core must be taken without default features, or the ranker \
             links openssl and the runner image cannot build or run it; found: {line}"
        );
    }
}
