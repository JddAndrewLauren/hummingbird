# AGENTS.md

`CLAUDE.md` is this repo's map and holds its repo-wide rules; read it first.
This file only names the commands, for agents and tools that look here.

## Test commands

Each stack's CI workflow is canonical; these are the commands it runs.

- **Client Rust** (`client/`, `client.yml`): `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo clippy --target wasm32-unknown-unknown -p hummingbird-ffi-web -- -D warnings`,
  `cargo test --workspace`
- **Web** (`client/web/`, `client.yml`): `pnpm test`, `pnpm run typecheck`, `pnpm run lint`,
  `pnpm run build`, `pnpm run assert-no-fixtures`
- **Server** (`server/`, `server-test.yml`): `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace`, `./scripts/smoke.sh`
- **Android** (`client/android/`, `android.yml`): `./gradlew :core-binding:testDebugUnitTest
  :brand:testDebugUnitTest :app:testDebugUnitTest :wear:testDebugUnitTest`, then
  `./gradlew :app:assembleDebug :wear:assembleDebug`. CI is the only gate for the Kotlin
  (CLAUDE.md, "The Kotlin is gated by CI alone").
- **Sweeper** (root, `deploy.yml`): `python3 -m unittest discover -s tests`
- **Runner** (`runner/`, `runner.yml`) and **browser capture** (`tools/browser-capture/`,
  `browser-capture.yml`): `node --test`

## Human Check rigs

`docs/agents/rigs.yaml`.
