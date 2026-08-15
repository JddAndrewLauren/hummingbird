// The main-thread decision seam (ADR-0025, #141/M1-1).
//
// Every decision ADR-0025 sinks into `hummingbird-core` is reached from the
// web through here, and only through here: `initDecisions` instantiates the
// `hummingbird_ffi_web` wasm module a SECOND time — on the main thread,
// beside the SharedWorker's own instance — and the wrappers below are plain
// synchronous functions over it.
//
// **Why a second instantiation rather than a worker round trip.** The
// modules being sunk run synchronously during React render: capture
// validation on every keystroke, urgency per card, ordering/faceting per
// render off main-thread-only UI state (the grouping axis, the facet
// selection). A `postMessage` hop cannot be spliced into a render, and the
// worker cannot see the state anyway.
//
// **Why that does not touch ADR-0010.** ADR-0010's three failure modes —
// divergent mirrors, two sync timers, duplicate writes — all require a
// second *queue*. Everything reachable through this file is a free function
// over scalars and JSON: it constructs no `Core`, opens no storage, starts
// no timer, and takes `now` as an argument where it needs one.
// `ffi-web`'s `decisions.rs` states the same rule on the Rust side, and the
// ADR-0025 amendment records it as a scope note rather than folklore. The
// invariant to keep is structural: nothing under `src/decisions/` may enter
// `core.worker.ts`'s static import graph (`worker-import-graph.test.ts`
// fails the build if it does).
//
// **Not-ready is a throw, never a fallback.** A TS re-implementation used
// while the module loads would be exactly the second copy this whole plan
// exists to delete — it would go stale silently and be right often enough
// that nobody noticed. `main.tsx` awaits `initDecisions()` before the first
// `createRoot().render()`, and vitest awaits it in `src/test/wasm-setup.ts`,
// so a throw here means a new render path opened that skipped the gate.
// A *failed* `initDecisions()` renders `shell/SeamFailure.tsx` instead of
// `App` for the same reason: there is no error boundary in this app, so
// mounting a decision consumer against a rejected seam would trade a stated
// failure for a blank page on the reader's first capture.

/** The wasm module's shape, named here rather than imported from the
 * generated `.d.ts`: this file is also the type the node-side test loader
 * satisfies, and the generated package is a build artifact (gitignored) the
 * typechecker only sees after `pnpm run build:wasm`. */
export interface DecisionsModule {
  can_submit_capture(draft: string): boolean;
  decisions_probe_item_payload(itemsJson: string): string;
}

let loaded: DecisionsModule | null = null;
let inFlight: Promise<DecisionsModule> | null = null;

/** The browser loader: the same wasm-pack `--target bundler` package the
 * SharedWorker imports, resolved by `vite-plugin-wasm` +
 * `vite-plugin-top-level-await`. A dynamic import, so nothing pulls the
 * binary in at module-evaluation time and a test that supplies its own
 * loader never touches this path. */
async function loadInBrowser(): Promise<DecisionsModule> {
  return (await import("../wasm/pkg/hummingbird_ffi_web")) as DecisionsModule;
}

/** Instantiate the decision module. Idempotent and concurrency-safe: the
 * second caller gets the first caller's promise, never a second
 * instantiation. `load` is the seam's own seam — vitest passes a node
 * loader (`src/test/wasm-setup.ts`), production passes nothing. */
export function initDecisions(load: () => Promise<DecisionsModule> = loadInBrowser): Promise<void> {
  if (loaded) return Promise.resolve();
  const startedAt = performance?.now?.() ?? 0;
  inFlight ??= load().then((module) => {
    loaded = module;
    // The gate's own cost, on the timeline the browser already keeps —
    // this is the number M1-1's flip condition (">~300 ms p50 added to the
    // loading gate") is written against, and the later M1 issues re-read it
    // as they sink more decisions rather than re-deriving a harness. A
    // `PerformanceMeasure`, not a `console.log`: it is readable from
    // devtools and from a Playwright run, and costs nothing when nobody
    // looks. `performance` is absent in no environment this file runs in
    // except a bare node one, hence the guard.
    performance?.measure?.("hb:decisions-init", { start: startedAt });
    return module;
  });
  return inFlight.then(() => undefined);
}

/** Whether the synchronous wrappers below can be called — the App-side half
 * of the loading gate. */
export function decisionsReady(): boolean {
  return loaded !== null;
}

/** Test-only reset, so a test can prove the not-ready throw without leaking
 * a poisoned module into the rest of the file's suite. Exported rather than
 * reached through a mock because the state it clears is this module's own. */
export function resetDecisionsForTest(): void {
  loaded = null;
  inFlight = null;
}

function required(): DecisionsModule {
  if (!loaded) {
    throw new Error(
      "decision seam used before initDecisions() resolved — see src/decisions/seam.ts",
    );
  }
  return loaded;
}

/** Whether `draft` is a real capture worth submitting (#110/S12). The rule
 * itself is `hummingbird_core::decisions::can_submit_capture`; this is the
 * call, and nothing else. */
export function canSubmitCapture(draft: string): boolean {
  return required().can_submit_capture(draft);
}

/** What M1-1's structured-payload benchmark measures — see `decisions.rs`.
 * A probe instrument, not a product read: M1-3 replaces it with the real
 * ordering call. */
export function probeItemPayload(itemsJson: string): string {
  return required().decisions_probe_item_payload(itemsJson);
}
