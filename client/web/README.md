# client/web

Desktop web client shell (#69): Vite + React + Tailwind + TypeScript, the
wasm sync core (`hummingbird-ffi-web`, #67) loaded in a Web Worker, PWA
offline shell, served from Cloudflare Workers static assets at
`hb.twinion.net` (ADR-0006). The shell is built on the Hummingbird Design
System (see the repo `CLAUDE.md`): a fixed nav rail over five surfaces — Now,
Triage, Routes, Alerts, Settings. **No task sync yet**, so every surface but
Settings reports an honest empty state; sync will target the owned API
(ADR-0008), and a Linear client adapter is never built. Calendar context and
its Google auth landed with #73 and are real.

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
- `src/shell/` — the nav rail, the header, the screen list, and
  `useCalendarWiring`, which owns the calendar lifecycle (consent, token
  rotation, the 15-minute poll and the staleness clock) for the app's whole
  lifetime, independent of which screen is mounted.
- `src/screens/` — the five surfaces. They switch on local state: there are
  no deep links to honour yet, so no router is installed.
- `src/theme/` — the `light | dark | system` preference, persisted at
  `hb.theme` and resolved onto `[data-theme]`. "Follow system" is resolved in
  JS because the stylesheet only knows `[data-theme="dark"]`.
- `src/store/` — the single React ↔ core surface: `useStore(selector)` over
  `useSyncExternalStore`, a module-level `coreStore` singleton with a stable
  `subscribe` reference, and `worker-client.ts` wiring the worker's messages
  (`src/store/protocol.ts`) into store writes. No second state channel.
- `src/worker/core.worker.ts` — the Web Worker that loads the wasm core.
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
