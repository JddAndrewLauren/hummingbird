# client/web

The web client shell (#69): Vite + React + Tailwind + TypeScript, the
wasm sync core (`hummingbird-ffi-web`, #67) loaded in a **SharedWorker** —
one core per origin, N views (ADR-0010, #126) — PWA offline shell, served
from Cloudflare Workers static assets at `hb.twinion.net` (ADR-0006). The
shell is built on the Hummingbird Design System (see the repo `CLAUDE.md`):
a nav over nine surfaces — Now, Triage, Projects, Alerts, Rules, Done, Ledger,
Status, Settings — which is a **fixed rail on a desktop and a bottom bar on a
phone**. That is the app's one breakpoint, 640px, defined in
`src/shell/breakpoints.ts` and spelled again in `src/shell/responsive.css`
(pinned equal by `src/shell/responsive-breakpoint.test.ts`); `docs/SURFACES.md`
carries the argument for the value and for where the class-versus-hook line
falls.
Task sync against the owned authority (ADR-0008) is live as of S6–S13:
capture (plus the global "c" focus hotkey), the frontier with its project
groups, item detail and the relation-blocked explanation, the act
affordances (start / complete / block / cancel), triage promotion with its
one-mutation multi-field edit, the device token, and ADR-0007's sync
cadence with its status readout. A Linear client adapter is never
built. Calendar context and its Google auth landed with #73 and are real.
The API is same-origin with the shell by decision (ADR-0006/0008), so
`src/worker/core.worker.ts` takes the authority's base URL from
`self.location.origin` at runtime and `VITE_API_BASE_URL` is an unset
override, not a setting: under `vite dev` the dev-server proxy forwards
`/api` to `wrangler dev` on 127.0.0.1:8787, and in production a path-scoped
Worker route puts `hb.twinion.net/api/*` on the authority worker. Both halves
are deployed as of 2026-08-10 (#237, #95's H3 gate closed); locally, with no
`wrangler dev` running, a cycle fails its connection as `pull_failed`.

Local dictation into the capture box (`src/speech/local-dictation.ts`,
`src/screens/capture-dictation.ts`) is **Chrome/Edge desktop only, with no
fallback.** It requires the browser's speech recognizer to run with
processing forced local, and only Chromium's constructor supports that flag
today — a browser that cannot establish local processing (every browser on
iOS/iPadOS, including desktop-mode Safari; likely Firefox and Safari on
desktop, unmeasured) renders no microphone at all, never a degraded or
warned-about one. There is no cloud-backed fallback on any path, including
error paths — see ADR-0022.

## Local development

```sh
pnpm install
pnpm dev            # build the wasm core, then vite dev
```

`pnpm dev` / `pnpm build` both run `build:wasm` first, which runs
`wasm-pack build --target bundler` against `../ffi-web` into
`src/wasm/pkg/` (git-ignored, regenerated every time — never commit it).

## Commands

- `pnpm build` — wasm core build, `tsc -b`, then `vite build` into `dist/`.
- `pnpm test` — a wasm core build (the decision seam is a real wasm module
  under test, so the tests need the package the same way `typecheck` does),
  then vitest over the store, worker-protocol, screen/shell
  pure-logic and CSP-worker unit tests (`src/**/*.test.ts`,
  `csp-worker/**/*.test.ts`), plus the component tests
  (`src/**/*.test.tsx`). Every test file gets `src/test/wasm-setup.ts`
  (vitest `setupFiles`), which instantiates the decision seam before the
  file's imports run — so a component can call a synchronous decision
  wrapper during render without knowing wasm is behind it, in either
  environment. `environment: "node"` stays the default — the
  `worker/*` tests assert against a `SharedWorker`-shaped world with no
  `document` — and a component test opts into jsdom per file with a
  `// @vitest-environment jsdom` docblock. Mount through
  `src/test/component.tsx`, never `@testing-library/react` directly: it
  registers the `afterEach(cleanup)` RTL skips without `globals: true`.
  Component tests exist because **typecheck cannot tell you a module has no
  caller** — see that file's header.
- `pnpm visual` — the Playwright visual gate: nine screens x four widths
  (1440 / 1024 / 768 / 390) x two themes, plus the capture popover and Now's
  empty state, captured to `visual/.captures/` for review. Fails on
  horizontal overflow, an unresolved brand token, a theme switch that does
  not reach the page, or (#453) the page loading a different demo world than
  the one `openApp` asked for; everything else is for human eyes, since there is no
  committed golden — and the overflow assertion proves much less than it
  sounds like (the shell is `overflow: hidden`, so it mostly measures what
  cannot happen; `docs/SURFACES.md` and the spec's own docstring say how
  little). The `-g` flag does **not** reach Playwright, so every run is the
  whole matrix. Needs a one-time `pnpm exec playwright install chromium`, and
  is deliberately not in CI. The registry is `docs/SURFACES.md`.
- `pnpm typecheck` — `tsc -b --noEmit` across the app and the Cloudflare
  Worker script. Note `tsc -p tsconfig.json` checks **nothing** — the root
  config is solution-style (`"files": []`); always go through this script.
- `pnpm lint` — ESLint. `react-hooks/exhaustive-deps` is an error (the
  suppressions in `shell/useCalendarWiring.ts` are deliberate and reviewed);
  the `jsx-a11y` violations in the ported design-system components are
  warnings until `HANDOFF-a11y-designsystem.md` is worked.
- `pnpm wrangler:dev` — build, then `wrangler dev`, serving `dist/` through
  `csp-worker/worker.ts` (adds the strict CSP header) with the `wrangler.toml`
  `[assets]` config (SPA fallback via `not_found_handling`).

## Demo mode

Two mutually exclusive worlds, both gated on `import.meta.env.DEV` as well as
the query string, so a production build cannot show either — the flag
compiles away and the fixtures leave the bundle.

`pnpm dev` and then `http://localhost:5173/?demo` (bare, or any spelling but
`?demo=kit`) renders the **board world** (#420): a seeded `TaskState`, the
same shape the sync engine publishes, so the screens take their real render
path with fictional data in it — this is what the visual gate photographs
(`pnpm visual` above) and the default since #455.

`http://localhost:5173/?demo=kit` renders the **design kit's** fixture data
instead, so the shell can be compared against
`.claude/skills/hummingbird-design/ui_kits/web/`. Since #456/#457 that is
Alerts only (#624 deleted the other reader, Routes) — its other readers were
render branches that shipped to nobody, and the screens that had them (Now,
Triage, Settings, Rules) now take their real/empty path under `?demo=kit`
like any other world.

## Layout

- `src/decisions/` — the **main-thread decision seam** (ADR-0025, #499):
  a second instantiation of the same `hummingbird_ffi_web` wasm module,
  beside the SharedWorker's, exposing the decisions every client shares as
  plain synchronous functions a render can call. `main.tsx` awaits it
  before the first render; `seam.ts`'s header carries why that is not a
  second core under ADR-0010, and `worker/worker-import-graph.test.ts`
  keeps it out of the worker's static graph.
- `src/components/` — the design system's 16 components, ported to `.tsx`
  from `.claude/skills/hummingbird-design/components/` (`core`, `forms`,
  `domain`, `feedback`). Inline styles over the design tokens, as in the
  source. `Icon` wraps `lucide-react` through a static name map: the design
  system's own CDN loader cannot ship under `script-src 'self'`.
- `src/shell/` — the two nav forms (`NavRail.tsx` and `NavBar.tsx` + its More
  sheet, partitioned by `nav-bar.ts`; `App.tsx` mounts exactly one),
  `ShellMeta.tsx` (the core-state and build-version lines both of them show),
  the responsive layer (`breakpoints.ts`, `responsive.css`, `useIsPhone.ts`),
  the header, the screen list, and the app-lifetime wiring hooks,
  independent of which screen is mounted:
  `useCalendarWiring` (consent, token rotation, the 15-minute poll and the
  staleness clock), `useTaskTokenWiring` (the device token's entry, rest and
  re-prompt, #106), `useSyncWiring` (the per-cycle reads and the view's own
  visibility/focus reports — it owns no timer; see below),
  `useFrontierWiring` / `useItemDetailWiring` / `useCaptureWiring` (the same
  once-ready-then-per-cycle refresh, keyed on `syncOutcomeSeq`, never a
  timer of their own), `useMicrotaskWiring` (#273's skill-runner lane —
  main-thread and outside the sync engine entirely; its header says why a
  worker-hosted run would be #269's banned queue in all but name),
  `useBackendSelection` (#274's picker choice — a device preference in the
  same shape as the theme's, read once at mount, never synced) and the
  one-line send wrappers `useItemActions` / `useTriageWiring`, plus the pure readouts `sync-cadence.ts`,
  `sync-status.ts` and `capture-hotkey.ts` (DOM-free: the caller extracts
  the facts from the real `KeyboardEvent`). Also the app-update lane —
  `UpdateBanner.tsx` / `useAppUpdate.ts` / `app-update.ts` / `update-check.ts`,
  where a new deploy is announced rather than applied behind your back and
  `main.tsx` is the only importer of `virtual:pwa-register`, plus
  `reload-on-activate.ts`, which converges every open view once a new worker
  takes control — and `build-version.ts`, the displayed build number's whole
  decision (its I/O half is `build-version.node.ts` at the package root).
- `src/screens/` — the nine surfaces. They switch on local state: there are
  no deep links to honour yet, so no router is installed. Everything
  decidable is a sibling pure module the `.tsx` only threads state through
  — `frontier-order.ts`, `frontier-groups.ts`, `priority.ts`, `urgency.ts`,
  `blocked-reason.ts`, `capture-validation.ts` (as of #499 a re-export of
  the core's own rule — see `src/decisions/` below), `triage-order.ts`,
  `item-actions.ts` (as of #502 no longer pure — `availableActions` /
  `canMarkDone` / `canGrill` / `grillButtonLabel` / `applyItemAction` all
  call through the seam into the wasm core, so each throws before
  `initDecisions()` has resolved rather than answering synchronously; only
  `resolveFallbackPending`'s local DTO-mutation plumbing stays a plain
  function), `triage-form.ts`, `bindings.ts`. Three sub-trees keep
  the same split at more depth: `questions/` is ADR-0015's pane shell — read
  `questions/contract.ts` first, it is what a standing question owes the
  shell — with one pane directory per question (`homework-pane/`,
  `waste-pane/`, `weekend-pane/`, `vacation-pane/`, `race-pane/`, each with
  its own header; `homework-pane/` is #675's, the first question keyed on
  the operator's own items rather than an outside source);
  `rules/` is #140's rule editor over the exported kind registry; and
  `projects/` is #624's Projects grid — `roster.ts` holds the whole of what
  the screen decides (the card order and counts, what the Show-archived
  toggle leaves on screen, whether a create is still in flight, and what a
  write that did not go through says).
- `src/skills/` — #273's skill-runner lane, and **the one place in this app
  that speaks HTTP without going through the Rust core**: `POST
  /api/skills/run` on the authority, which proxies to the cloud runner
  (ADR-0018). Everything decidable is a pure module — `ndjson.ts` (chunk to
  line), `envelope.ts` (line to meaning), `run-state.ts` (the four phases and
  the duplicate-tap rule), `microtask-affordance.ts` (which gesture the
  item's own steps make legal, #307), `microtask-args.ts`, `decline.ts` —
  around **two** layers, both async generators that never throw:
  `run-skill.ts` is one request to one backend, and `route-run.ts` (#274) is
  what the app actually calls, choosing which backend that is. Its decision
  is itself pure and lives next door: `backend-registry.ts` (the ordered
  entries), `backend-selection.ts` (the device-local choice, with
  `shell/useBackendSelection.ts` as its hook), `reachability-memo.ts` (a
  short-TTL record written only as the side effect of a real attempt) and
  `route-plan.ts` (what to try, given all four). It writes nothing: the
  runner writes the checklist to the authority, and the steps arrive through
  the normal step read path. `skills/no-queue.test.ts` pins from the source
  text that nothing in the lane — the two `shell/` hooks included — can
  reach the sync queue, the pending overlay, the dead-letter journal or a
  hand-rolled timer.
- `src/theme/` — the `light | dark | system` preference, persisted at
  `hb.theme` and resolved onto `[data-theme]`. "Follow system" is resolved in
  JS because the stylesheet only knows `[data-theme="dark"]`.
- `src/store/` — the single React ↔ core surface: `useStore(selector)` over
  `useSyncExternalStore`, a module-level `coreStore` singleton with a stable
  `subscribe` reference, and `worker-client.ts` wiring the worker's messages
  (`src/store/protocol.ts`) into store writes. No second state channel.
- `src/worker/` — the SharedWorker layer. `core.worker.ts` is the shim: it
  loads the wasm core, wires `PortRegistry` (`ports.ts`, one core → N views
  and their handshakes), and owns ADR-0007's single 60-second interval for
  the whole origin. Everything decidable is a sibling pure module a node
  test can execute — `dispatch.ts` (cadence-vs-task-vs-calendar routing and
  the app-open trigger), `request-router.ts`, `task-worker.ts`,
  `calendar-worker.ts`, `serial-queue.ts`, `visibility-tracker.ts`,
  `announce.ts`.
  **No top-level `await` may enter `core.worker.ts`'s static import graph** —
  see that file's own header for the full account, and
  `worker/sync-timer-ownership.test.ts` for what is pinned from the source
  text.
- `csp-worker/` — the Cloudflare Worker `main` script wrangler.toml points
  at: adds the strict CSP header to every asset response so it ships as a
  served header (ADR-0004), not a meta tag.
- `wrangler.toml` — checked-in source of truth for the Cloudflare Workers
  static-assets deploy: SPA fallback, the CSP worker, and the
  `hb.twinion.net` custom-domain route.
