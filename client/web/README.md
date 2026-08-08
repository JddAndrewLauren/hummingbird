# client/web

Desktop web client shell (#69): Vite + React + Tailwind + TypeScript, the
wasm sync core (`hummingbird-ffi-web`, #67) loaded in a Web Worker, PWA
offline shell, served from Cloudflare Workers static assets at
`hb.twinion.net` (ADR-0006). This is a placeholder shell — no Linear sync,
auth, or calendar UI yet (see #73).

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
  Worker script.
- `pnpm wrangler:dev` — build, then `wrangler dev`, serving `dist/` through
  `csp-worker/worker.ts` (adds the strict CSP header) with the `wrangler.toml`
  `[assets]` config (SPA fallback via `not_found_handling`).

## Layout

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
