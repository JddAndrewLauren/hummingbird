# client/web

Desktop web client shell (#69): Vite + React + Tailwind + TypeScript, the
wasm sync core (`hummingbird-ffi-web`, #67) loaded in a **SharedWorker** —
one core per origin, N views (ADR-0010, #126) — PWA offline shell, served
from Cloudflare Workers static assets at `hb.twinion.net` (ADR-0006). The
shell is built on the Hummingbird Design System (see the repo `CLAUDE.md`):
a fixed nav rail over five surfaces — Now, Triage, Routes, Alerts, Settings.
Task sync against the owned authority (ADR-0008) is live as of S6–S9:
capture, the frontier and triage inbox, the device token, and ADR-0007's
sync cadence with its status readout. A Linear client adapter is never
built. Calendar context and its Google auth landed with #73 and are real.
Nothing is deployed yet — `VITE_API_BASE_URL` is unset by default, so every
cycle fast-fails as `pull_failed` until #95's H3 human gate.

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
- `pnpm test` — vitest over the store, worker-protocol, and CSP-worker unit
  tests (`src/**/*.test.ts`, `csp-worker/**/*.test.ts`).
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
  visibility/focus reports — it owns no timer; see below), plus the pure
  readouts `sync-cadence.ts` and `sync-status.ts`.
- `src/screens/` — the five surfaces. They switch on local state: there are
  no deep links to honour yet, so no router is installed.
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
  see the invariant in the repo `CLAUDE.md`, and `core.worker.ts`'s own
  header for the full account.
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
