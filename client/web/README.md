# client/web

Desktop web client shell (#69): Vite + React + Tailwind + TypeScript, the
wasm sync core (`hummingbird-ffi-web`, #67) loaded in a **SharedWorker** —
one core per origin, N views (ADR-0010, #126) — PWA offline shell, served
from Cloudflare Workers static assets at `hb.twinion.net` (ADR-0006). The
shell is built on the Hummingbird Design System (see the repo `CLAUDE.md`):
a fixed nav rail over five surfaces — Now, Triage, Routes, Alerts, Settings.
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
- `pnpm test` — vitest over the store, worker-protocol, screen/shell
  pure-logic and CSP-worker unit tests (`src/**/*.test.ts`,
  `csp-worker/**/*.test.ts`), plus the component tests
  (`src/**/*.test.tsx`). `environment: "node"` stays the default — the
  `worker/*` tests assert against a `SharedWorker`-shaped world with no
  `document` — and a component test opts into jsdom per file with a
  `// @vitest-environment jsdom` docblock. Mount through
  `src/test/component.tsx`, never `@testing-library/react` directly: it
  registers the `afterEach(cleanup)` RTL skips without `globals: true`.
  Component tests exist because **typecheck cannot tell you a module has no
  caller** — see that file's header.
- `pnpm visual` — the Playwright visual gate: five screens x three widths x
  two themes, plus Now's empty state, captured to `visual/.captures/` for
  review. Fails on horizontal overflow, an unresolved brand token, or a
  theme switch that does not reach the page; everything else is for human
  eyes, since there is no committed golden. Needs a one-time `pnpm exec
  playwright install chromium`, and is deliberately not in CI. The registry
  is `docs/SURFACES.md`.
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

`pnpm dev` and then `http://localhost:5173/?demo` renders the design kit's
fixture data on every surface, so the shell can be compared against
`.claude/skills/hummingbird-design/ui_kits/web/`. It is gated on
`import.meta.env.DEV` as well as the query string, so a production build
cannot show it — the flag compiles away and the fixtures leave the bundle.

## Layout

- `src/components/` — the design system's 16 components, ported to `.tsx`
  from `.claude/skills/hummingbird-design/components/` (`core`, `forms`,
  `domain`, `feedback`). Inline styles over the design tokens, as in the
  source. `Icon` wraps `lucide-react` through a static name map: the design
  system's own CDN loader cannot ship under `script-src 'self'`.
- `src/shell/` — the nav rail, the header, the screen list, and the
  app-lifetime wiring hooks, independent of which screen is mounted:
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
  `main.tsx` is the only importer of `virtual:pwa-register` — and
  `build-version.ts`, the displayed build number's whole decision (its I/O
  half is `build-version.node.ts` at the package root).
- `src/screens/` — the five surfaces. They switch on local state: there are
  no deep links to honour yet, so no router is installed. Everything
  decidable is a sibling pure module the `.tsx` only threads state through
  — `frontier-order.ts`, `frontier-groups.ts`, `priority.ts`, `urgency.ts`,
  `blocked-reason.ts`, `capture-validation.ts`, `triage-order.ts`,
  `item-actions.ts`, `triage-form.ts`, `bindings.ts`. Three sub-trees keep
  the same split at more depth: `questions/` is ADR-0015's pane shell — read
  `questions/contract.ts` first, it is what a standing question owes the
  shell — with one pane directory per question (`waste-pane/`,
  `weekend-pane/`, `vacation-pane/`, `race-pane/`, each with its own header);
  and `rules/` is #140's rule editor over the exported kind registry.
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
  static-assets deploy: SPA fallback, the CSP worker, and (commented out
  until the human step is done) the `hb.twinion.net` custom-domain route.

## Out of scope here

Performing the live Cloudflare deploy and the `hb.twinion.net` DNS binding
is a human step (#69's brief) — everything above works fully locally via
`vite dev` / `vite build` / `wrangler dev`.
