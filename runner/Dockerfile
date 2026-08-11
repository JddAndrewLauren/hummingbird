# The skill-runner endpoint (#41, #256): a scale-to-zero Fly app, separate
# from the sweeper (docs/sweeper.md forbids [http_service] there). The image
# bakes in the Claude Code CLI, the skills this build ships, and the runner
# server itself -- a skill change ships by `fly deploy`, the image *is* the
# skill version (#41 decision 5).

# `/next-up-hb` (#116) is layered on `client/next-up`, whose binary does the
# selection and calls `rank()` -- so unlike `parse-capture`, this op needs a
# compiled artifact in the image. A whole Rust toolchain in a node image is
# the one genuinely heavy piece here, and it is confined to this stage:
# only the binary is copied into the final layer.
#
# The alternative -- the calling device ranking through its own wasm core
# and posting ranked candidates -- is cheaper, does not exist, and is not
# #116's scope.
#
# Pinned to the same toolchain `rust-toolchain.toml` names, for the reason
# that file gives: a floating stable can add a clippy lint or change a
# build. Bump both together, deliberately.
FROM rust:1.97.1-slim AS next-up-builder
WORKDIR /src
# `rust-toolchain.toml` is honoured by rustup from anywhere in the tree, so
# it comes along, and the whole `client/` workspace comes because cargo has
# to resolve every member's manifest even to build one of them.
#
# **`server/` comes whole, not just `server/domain`**, and that is not
# tidiness. `server/domain/Cargo.toml` inherits `version` and `edition`
# with `.workspace = true`, so cargo must find the workspace root that
# declares them; copying the crate alone fails before compilation with
# `failed to load manifest for dependency hummingbird-domain`. Copying
# `server/Cargo.toml` beside it is not enough either — that manifest names
# eight `members`, and cargo demands every one of them exist. The build
# context cost is paid back by `.dockerignore`, which keeps `target/` out.
COPY rust-toolchain.toml ./rust-toolchain.toml
COPY server ./server
COPY client ./client
WORKDIR /src/client
# No `pkg-config`/`libssl-dev` layer above this line, deliberately:
# `client/next-up` takes `hummingbird-core` with `default-features = false`,
# which drops the `reqwest-transport` feature and with it `openssl-sys`. The
# ranker is a pure function over a JSON envelope and links no TLS stack, so
# the builder needs no C toolchain and the final image below needs no
# `libssl.so.3` — see that crate's manifest, and the manifest test that
# pins it.
RUN cargo build --release -p hummingbird-next-up --bin next-up-rank

FROM node:22-slim

# The Claude Code CLI, installed globally so `claude` is on PATH for
# child_process.spawn inside the runner.
RUN npm install -g @anthropic-ai/claude-code

WORKDIR /app

# The skills this build ships (#41 decision 5). `next-up-personal` is retired;
# `microtask` and `to-actions` use the app-owned helpers interactively but are
# not hosted runner operations. Widening this COPY is the whole change needed
# to add the next registered op.
COPY .claude/skills/parse-capture ./.claude/skills/parse-capture
COPY .claude/skills/next-up-hb ./.claude/skills/next-up-hb

# The prebuilt ranker, and the env var `scripts/next-up.sh` reads to find
# it: with `HB_NEXT_UP_BIN` set, the script never falls back to `cargo run`
# -- which is what keeps this image free of a Rust toolchain at runtime.
COPY --from=next-up-builder /src/client/target/release/next-up-rank /usr/local/bin/next-up-rank
ENV HB_NEXT_UP_BIN=/usr/local/bin/next-up-rank

COPY runner/package.json ./runner/package.json
COPY runner/src ./runner/src

WORKDIR /app/runner

EXPOSE 8080

CMD ["node", "src/main.js"]
