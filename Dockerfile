# The five out-of-process poller binaries this issue (#774) moved off GitHub
# Actions `schedule:` onto this container's own supercronic clock:
# `hummingbird-gmail-poll`, `hummingbird-calendar-poll`, `graph-mail-poll`,
# `graph-calendar-poll` and `github-status-poll`. Same builder-stage pattern
# `runner/Dockerfile` established for `next-up-rank` — read that file's
# header first; this one does not re-derive the reasoning, only restates
# what differs.
#
# Pinned to the same toolchain `rust-toolchain.toml` names, for that file's
# own reason (a floating stable can add a clippy lint or change a build).
# Bump both together, deliberately.
FROM rust:1.97.1-slim AS poller-builder
WORKDIR /src
# `server/` comes whole, not just the five poller crates' own directories —
# `runner/Dockerfile`'s own comment on this explains why: `server/domain`
# inherits `version`/`edition` from the workspace root with
# `.workspace = true`, so cargo must find the workspace root that declares
# them, and that root's `members` list names every crate cargo then demands
# exist. `.dockerignore`'s `**/target` is what makes copying the whole tree
# affordable.
COPY rust-toolchain.toml ./rust-toolchain.toml
COPY server ./server
WORKDIR /src/server
# No `pkg-config`/`libssl-dev` layer, deliberately: every poller built below
# takes `ureq` with `default-features = false, features = ["rustls",
# "json"]`, so nothing here links OpenSSL — `runner/Dockerfile`'s own
# builder stage records the same reasoning for the same feature choice.
#
# `jsonwebtoken 9` (`hummingbird-graph-poll`) pulls in `ring`, which needs a
# C toolchain to build its assembly/C sources; `rust:1.97.1-slim` carries
# one (unlike the Debian-slim final stage below), so this stage — not the
# final image — is where that dependency actually compiles.
RUN cargo build --release \
    -p hummingbird-gmail-poll \
    -p hummingbird-calendar-poll \
    -p hummingbird-graph-poll --bin graph-mail-poll --bin graph-calendar-poll \
    -p hummingbird-github-status --bin github-status-poll

FROM python:3.12-slim

# supercronic pinned by version AND checksum. The sha256 is of the actual
# v0.2.33 linux-amd64 release asset (the project publishes no checksum file);
# re-verify with `shasum -a 256` after downloading if you bump the version.
ARG SUPERCRONIC_VERSION=v0.2.33
ARG SUPERCRONIC_SHA256=feefa310da569c81b99e1027b86b27b51e6ee9ab647747b49099645120cfc671

# `tzdata` is for `calendar-poll` and `graph-poll`, both of which take
# `jiff = "0.2"` and read the system zoneinfo database on Unix (RFC3339
# `dateTime`/`date` parsing off Google/Graph event shapes) — `jiff` supplies
# no bundled tzdata of its own. `python:3.12-slim`'s Debian base does not
# install it by default. Cheap (a few MB) and harmless to install
# unconditionally even where it turns out to already be present; this
# issue's "Verification the agent cannot do" section names the local
# `docker build` that actually proves a zone resolves, rather than this
# comment asserting it.
RUN set -eux; \
    apt-get update; \
    apt-get install -y --no-install-recommends ca-certificates curl tzdata; \
    rm -rf /var/lib/apt/lists/*; \
    curl -fsSLo /usr/local/bin/supercronic \
      "https://github.com/aptible/supercronic/releases/download/${SUPERCRONIC_VERSION}/supercronic-linux-amd64"; \
    echo "${SUPERCRONIC_SHA256}  /usr/local/bin/supercronic" | sha256sum -c -; \
    chmod +x /usr/local/bin/supercronic

WORKDIR /app

# The repo file is sweep.py so tests can import it; the container wants a bare
# executable so `fly ssh console -C /app/sweep` works verbatim.
COPY sweep.py /app/sweep
COPY denylist.json /app/denylist.json
COPY crontab /app/crontab
RUN chmod +x /app/sweep

# `github-status-poll` reads this directory via `GITHUB_WORKFLOWS_DIR` (set
# in `fly.toml`) instead of a fresh `actions/checkout@v4` on every run —
# `server/github-status/src/manifest.rs`'s header records what that trades
# away (per-commit freshness becomes per-deploy).
COPY .github/workflows /app/workflows

# The five poller binaries built above.
COPY --from=poller-builder /src/server/target/release/hummingbird-gmail-poll /app/bin/hummingbird-gmail-poll
COPY --from=poller-builder /src/server/target/release/hummingbird-calendar-poll /app/bin/hummingbird-calendar-poll
COPY --from=poller-builder /src/server/target/release/graph-mail-poll /app/bin/graph-mail-poll
COPY --from=poller-builder /src/server/target/release/graph-calendar-poll /app/bin/graph-calendar-poll
COPY --from=poller-builder /src/server/target/release/github-status-poll /app/bin/github-status-poll

# Absolute path, not a bare `supercronic`: as PID 1 supercronic re-execs itself
# to reap dead processes, and that re-exec does not search PATH -- a bare
# argv[0] dies instantly with "Failed to fork exec: no such file or directory".
CMD ["/usr/local/bin/supercronic", "-passthrough-logs", "/app/crontab"]
