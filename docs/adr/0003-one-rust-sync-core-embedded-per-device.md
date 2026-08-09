# ADR-0003: One Rust sync core, embedded per device

**Status:** accepted · 2026-08-07 · **amended 2026-08-08 by
[ADR-0008](0008-the-authority-is-an-app-owned-server.md):** the "no relay and
no app-owned backend" clause is superseded — an owned authority now exists,
the Linear adapter is never built, and the CORS argument is moot (the API is
same-origin). Everything else stands: the sync engine remains a library
embedded per device, the crate layout, persistence and web-client decisions
are unchanged, and the domain types in `core/src/task/` extract into a
standalone crate the server shares. · **amended 2026-08-09 by
[ADR-0010](0010-one-core-per-origin.md):** on the web host the core is embedded
once per *origin*, not once per document — it lives in a `SharedWorker`, and
every tab and installed-PWA window is a view holding a `MessagePort`.
Per-device embedding stands everywhere else.
**Context:** the stack grilling of 2026-08-07, wayfinder map
[#35](https://github.com/JddAndrewLauren/hummingbird/issues/35) ticket
[#40](https://github.com/JddAndrewLauren/hummingbird/issues/40). Implements
[ADR-0001](0001-linear-is-the-authority-behind-a-clean-seam.md)'s seam rules and
carries [ADR-0002](0002-sources-join-by-role-urgency-computed-at-read-time.md)'s
mirror contract.

## Decision

**The sync engine is a library embedded in every client**, talking directly to
`api.linear.app`. There is no relay and no app-owned backend: ADR-0001's
rejected alternative stays rejected. Linear's API answers browser-origin
preflights with a permissive `access-control-allow-origin` and allows the
`authorization` header, so no server is required to make a web client work.

**One shared Rust core, in three crates:**

| Crate | Contents |
| --- | --- |
| `client/core` | The sync engine. Binding-agnostic: no `uniffi`, no `wasm_bindgen`. |
| `client/ffi-mobile` | `#[uniffi::export]` wrappers → Kotlin (Android, Wear OS) and Swift (iPad). |
| `client/ffi-web` | `#[wasm_bindgen]` wrappers → TypeScript. |

Keeping the core free of binding macros is load-bearing: it sidesteps any
question of the two macro systems coexisting on the `wasm32-unknown-unknown`
target, and it keeps the core's test suite binding-free.

**Two binding mechanisms, deliberately.** UniFFI's JavaScript generator is
pre-production — `uniffi-bindgen-react-native` states it "should not yet be
used in production" and `uniffi-bindgen-js` is months old with negligible
adoption — while `wasm-bindgen` is ecosystem infrastructure. These are not
rival substrates: UniFFI's Wasm path is itself implemented with `wasm-bindgen`.
When `uniffi-bindgen-javascript` stabilises, `ffi-web` is deleted and the
generated bindings converge. Nothing in `core` moves.

**The core is async, and owns HTTP.** `reqwest` compiles for `wasm32` and
delegates to the browser's Fetch API, so one HTTP path serves all four clients
— but it has no blocking API there, so async is forced rather than chosen.
UniFFI maps Rust futures to Kotlin `suspend fun` and Swift `async`/`await`.

**The core owns persistence**, behind an internal Rust trait with a
compile-target split: `indexed_db_futures` on `wasm32`, `std::fs` with
write-temp-then-rename elsewhere. The host contributes exactly one thing — a
storage directory path at init. *Amended by
[ADR-0004](0004-client-linear-credential-is-scoped-per-device-host-supplied.md):
the host also supplies the Linear credential at init, and the core holds it in
memory without ever persisting it.*

This is what enforces ADR-0001's write-path safety. The outbound queue must be
durable *before* the Linear call goes out, and here the core awaits its own
write and then makes the call. Durable-before-network is a property of the
core, not a convention three hosts must independently remember.

Serialisation is `serde_json` while the schema moves — a readable mirror file
is worth real money when debugging — with `postcard` available later if size
matters. An in-memory implementation of the same trait makes the whole core
testable with golden vectors, with no device and no browser.

**The desktop web client is Vite + React + Tailwind + TypeScript**, built with
pnpm. `vite-plugin-wasm` and `vite-plugin-top-level-await` load the core;
`vite-plugin-pwa` provides the service worker, which offline reads *require* —
without one the app shell cannot load offline at all. The core runs in a Web
Worker and is surfaced through a single `useStore(selector)` hook over
`useSyncExternalStore`, with a module-level stable `subscribe` reference.

Next.js is rejected: its value is the server, and this client has none.

**Layout.** `client/` holds a Cargo workspace plus `client/web/`, with
`client/android/` and `client/apple/` joining later. `deploy.yml` gains
`paths:` filters, because it currently redeploys the sweeper on any push to
`main`.

## How this expresses ADR-0001's seam

1. **The app's schema is the domain model.** `core` is written against
   `CONTEXT.md`'s Action / Route / Step. Linear's Issue shape, state ids, and
   label ids appear only in the Linear adapter inside `core`.
2. **Clients talk to one storage/sync interface.** That interface is the
   public API of `core`, surfaced verbatim by both FFI crates. There is exactly
   one of it, in one language.
3. **The mirror is the export.** With a serialised snapshot as the storage
   format, "every device's reconciled replica is a full copy" is literally one
   file rather than an aspiration.
4. **The authority is a single explicit declaration** the sync layer reads,
   living in `core`.

## Rejected alternatives

- **A server-side sync service** — a softer form of the app-owned backend
  ADR-0001 rejected. Every client needs a local store, outbound queue, and
  reconciler regardless, so it adds a second copy behind a network hop rather
  than removing work, and puts a service you must keep alive in the happy path.
- **Kotlin Multiplatform** — strong on Android, Wear, and iPad, but it would
  put the *first* client on its least mature target (Kotlin/Wasm is Beta in
  KMP), and its best structural payoff, a shared SQLDelight schema, is
  unavailable: SQLDelight has no Wasm support, and at this data scale SQLite is
  not wanted anyway.
- **Contract-level reuse** — per-runtime reimplementation against golden test
  vectors, as [twinion ADR-0029](https://github.com/JddAndrewLauren/twinion/blob/main/docs/adr/0029-native-android-app.md)
  chose for the Apple/Android split. Viable, and cheap while the core is only
  fetch-diff-queue, but ADR-0002 makes the mirror multi-domain with an adapter
  per source, and triplicating that is the cost this decision declines to pay.
- **UniFFI's JS bindings for the web client** — the tidiest expression of seam
  1, but pre-production, and the failure mode is spending the first client's
  build debugging a code generator.
- **An all-Rust web UI (Leptos, Dioxus)** — shares nothing with Wear OS or
  iPad, which need native UI regardless, and forfeits the npm ecosystem for the
  one client that benefits most from it.
- **A host-implemented storage callback** — architecturally clean, but built on
  UniFFI's async foreign traits, whose own documentation flags a Swift 6
  `Sendable` conflict, unmitigated Rust↔foreign reference cycles, and a thread
  per invocation. The bill would arrive at the iPad client, after the pattern
  was embedded in two others.
- **SQLite everywhere** — drags OPFS, its worker-only synchronous path, and
  COOP/COEP headers into the web client for a dataset in the low thousands of
  records that is loaded into memory anyway.
- **Svelte 5 and SolidJS** — better architectural fit, since signals map
  directly onto a core pushing changes, and far smaller runtimes. Rejected on
  failure mode under agent authorship: Svelte 5 has a documented, persistent
  LLM version-drift problem, and Solid's React resemblance produces *silent*
  reactivity loss. React has neither, and this app's state lives in Rust, which
  neutralises React's own weaknesses.

## Development environments

Recorded because this project is developed from both a Mac and a Windows PC.

- **On the PC, the repo lives in the WSL filesystem, never under `/mnt/c`.**
  Cross-boundary I/O is slow for many-small-file trees (`node_modules`,
  cargo `target/`) and, more importantly, filesystem watch events are
  unreliable across the boundary — which breaks Vite HMR.
- **One environment per machine.** Alternating between WSL and native Windows
  churns `target/` and `node_modules` between incompatible platform binaries.
- **The Apple leg is Mac-only.** Swift cannot be built on Windows. This is
  Apple's constraint, not a consequence of this decision, and iPad is last in
  the build order.
- **Android and Wear cross-compile from any host.** `cargo-ndk` supports
  Linux, macOS, and Windows as build hosts.
- **The `std::fs` persistence leg never runs on Windows.** Windows is a
  development host and a browser host, and the browser uses IndexedDB. If core
  tests ever exercise the real filesystem implementation on Windows, note that
  the `std::fs::rename` docs are silent on a destination held open by another
  process; keep the in-memory implementation as the test default.
- **`sweep.py` is Unix-only** and always has been — it imports `fcntl` at
  module scope, so on native Windows it will not import and the test suite will
  not run. In WSL it is fine.
