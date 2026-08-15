// M1-1's measuring instrument (#499, ADR-0025): what the main-thread
// decision seam costs, against the flip conditions in that issue.
//
// Two numbers, and they answer different questions:
//
//   1. **Instantiation** — compile + instantiate of the whole
//      `hummingbird_ffi_web` binary, which is what `main.tsx` awaits before
//      the first render. Flip condition: >~300 ms p50.
//   2. **One structured payload** — ~100 `TaskItemDTO`s crossing into wasm
//      as JSON and an answer coming back, which is what M1-3's per-render
//      `orderFrontier`/`applyFacets` calls would pay on every facet toggle.
//      Flip condition: worse than single-digit ms per call. Instantiation
//      timing cannot see this cost at all — hence a second number.
//
// Node, not a browser: same V8, and it is the number CI or any reviewer can
// reproduce with `node scripts/probe-decisions.mjs`. It is a FLOOR for the
// browser's — a real page also pays the network fetch (the binary is
// precached by the service worker after first load) and browsers compile
// streaming, which node's `WebAssembly.compile` over bytes does not. The
// PR's numbers say which is which.
//
// Run after `pnpm run build:wasm`.

import { readFile } from "node:fs/promises";
import { resolve, dirname } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const PKG = resolve(dirname(fileURLToPath(import.meta.url)), "../src/wasm/pkg");
const GLUE = resolve(PKG, "hummingbird_ffi_web_bg.js");
const BINARY = resolve(PKG, "hummingbird_ffi_web_bg.wasm");

const RUNS = 25;
const ITEMS = 100;

const bytes = await readFile(BINARY);

/** One full instantiation, exactly as `seam.ts` gets one: fresh glue module
 * (cache-busted, so no run reuses a previous run's compiled module), the
 * binary compiled and instantiated against it, then wasm-bindgen's start
 * shim. Returns the module and the milliseconds it took. */
async function instantiateOnce(nonce) {
  const started = performance.now();
  const glue = await import(`${pathToFileURL(GLUE).href}?probe=${nonce}`);
  const { instance } = await WebAssembly.instantiate(bytes, {
    "./hummingbird_ffi_web_bg.js": glue,
  });
  glue.__wbg_set_wasm(instance.exports);
  instance.exports.__wbindgen_start();
  return { glue, ms: performance.now() - started };
}

function percentile(samples, p) {
  const sorted = [...samples].sort((a, b) => a - b);
  return sorted[Math.min(sorted.length - 1, Math.floor((p / 100) * sorted.length))];
}

function report(label, samples) {
  console.log(
    `${label}: p50 ${percentile(samples, 50).toFixed(2)} ms | ` +
      `p95 ${percentile(samples, 95).toFixed(2)} ms | ` +
      `min ${Math.min(...samples).toFixed(2)} ms | max ${Math.max(...samples).toFixed(2)} ms ` +
      `(n=${samples.length})`,
  );
}

const instantiation = [];
let module;
for (let run = 0; run < RUNS; run += 1) {
  const { glue, ms } = await instantiateOnce(run);
  instantiation.push(ms);
  module = glue;
}
report(`instantiate ${(bytes.byteLength / 1024).toFixed(0)} KiB wasm`, instantiation);

// The payload: the main thread's own camelCase `TaskItemDTO` shape
// (`src/store/protocol.ts`), at a frontier's plausible worst case.
const payload = JSON.stringify(
  Array.from({ length: ITEMS }, (_, index) => ({
    id: `01J8ZQ${String(index).padStart(4, "0")}-4f2a-7c1d-9e00-000000000000`,
    seq: index + 1,
    title: `Draft the quarterly note about thing number ${index}`,
    description: index % 3 === 0 ? "A couple of sentences of context on this one." : null,
    stage: ["next", "waiting", "someday", "done"][index % 4],
    size: ["quick", "normal", "deep"][index % 3],
    energy: ["low", "medium", "high"][index % 3],
    context: ["@computer", "@calls", "@errands"][index % 3],
    priority: index % 5,
    projectId: index % 2 === 0 ? "proj-01J8ZQ" : null,
    projectPos: index % 2 === 0 ? index : null,
    deadline: index % 4 === 0 ? "2026-08-20" : null,
    scheduledDate: index % 6 === 0 ? "2026-08-18" : null,
    source: "web/v1",
    sourceKey: null,
    sourceUrl: null,
    archivedAt: null,
    createdAt: 1_755_000_000_000 + index,
    updatedAt: 1_755_000_000_100 + index,
    version: 1 + (index % 3),
    pending: index % 10 === 0,
  })),
);
console.log(`payload: ${ITEMS} items, ${(payload.length / 1024).toFixed(1)} KiB of JSON`);

// A serialize-in-JS + cross + parse-out measurement, not just the wasm
// call: `JSON.stringify` on the caller's side is part of what a render pays.
const roundTrip = [];
for (let run = 0; run < 200; run += 1) {
  const started = performance.now();
  const answer = JSON.parse(module.decisions_probe_item_payload(payload));
  roundTrip.push(performance.now() - started);
  if (answer.count !== ITEMS) throw new Error(`probe disagreed: ${JSON.stringify(answer)}`);
}
report("100-item JSON round trip", roundTrip);

const withStringify = [];
const items = JSON.parse(payload);
for (let run = 0; run < 200; run += 1) {
  const started = performance.now();
  JSON.parse(module.decisions_probe_item_payload(JSON.stringify(items)));
  withStringify.push(performance.now() - started);
}
report("…including the caller's JSON.stringify", withStringify);

// The capture wrapper is the per-keystroke call M1-1 actually ships.
const keystroke = [];
for (let run = 0; run < 5000; run += 1) {
  const started = performance.now();
  module.can_submit_capture("buy milk  ");
  keystroke.push(performance.now() - started);
}
report("can_submit_capture (per keystroke)", keystroke);
