// The async wasm setup every vitest file gets, in BOTH environments
// (ADR-0025, #141/M1-1). Registered as `setupFiles` in `vitest.config.ts`,
// so it runs once per test file before that file's imports are evaluated —
// which is what lets a component test call a synchronous decision wrapper
// during render without knowing a wasm module is behind it. Later M1 issues
// reuse this file; nothing should re-invent a per-file loader.
//
// **Why this does not just `import("../wasm/pkg/hummingbird_ffi_web")`.**
// `vitest.config.ts` is its own config with no plugins — deliberately, so
// unit tests run against source rather than a bundle — and the wasm-pack
// `--target bundler` entry point is a JS file that imports a `.wasm` file,
// which only `vite-plugin-wasm` can resolve. Rather than bolt the app's
// bundler plugins onto the test runner (and then be testing the bundler's
// wasm shim), this does what the bundler's shim does, in ~5 lines, using
// the package's own documented door: `hummingbird_ffi_web_bg.js` holds all
// the JS glue and exposes `__wbg_set_wasm`, and the binary's only import
// module is that same file.
//
// The bytes come off disk with `node:fs` — vitest runs in node under both
// `environment: "node"` and `environment: "jsdom"`, so this one path serves
// both and no test file needs an environment-specific hack. `pnpm test`
// runs `build:wasm` first (package.json), so the package is always present.

import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { beforeAll } from "vitest";

import { initDecisions, type DecisionsModule } from "../decisions/seam";

/** wasm-bindgen's `--target bundler` glue module: all the JS imports the
 * binary needs, plus the `__wbg_set_wasm` setter its own entry point calls.
 * Typed here because the generated package ships declarations for its entry
 * point only. */
interface BindgenGlue extends DecisionsModule {
  __wbg_set_wasm(exports: WebAssembly.Exports): void;
}

// Anchored on the vitest root (`client/web`), NOT on `import.meta.url`:
// under `environment: "jsdom"` that is an `http://localhost/…` URL, and
// node's ESM loader rejects an import of one outright ("Only URLs with a
// scheme in: file and data are supported"). `process.cwd()` is the config's
// root in both environments, which is what makes one loader serve both.
const PKG_DIR = resolve(process.cwd(), "src/wasm/pkg");
const GLUE_PATH = resolve(PKG_DIR, "hummingbird_ffi_web_bg.js");
const BINARY_PATH = resolve(PKG_DIR, "hummingbird_ffi_web_bg.wasm");

/** Instantiate the wasm package for a node-hosted test run. Exported so a
 * test that needs the module directly (a benchmark, a not-ready proof) can
 * reuse the exact loader the setup uses. */
export async function loadDecisionsForTest(): Promise<DecisionsModule> {
  const glue = (await import(/* @vite-ignore */ pathToFileURL(GLUE_PATH).href)) as BindgenGlue;
  const bytes = await readFile(BINARY_PATH);
  const { instance } = await WebAssembly.instantiate(bytes, {
    "./hummingbird_ffi_web_bg.js": glue as unknown as WebAssembly.ModuleImports,
  });
  glue.__wbg_set_wasm(instance.exports);
  // The entry point calls this too: wasm-bindgen's start shim, which runs
  // the module's initialisation. Skipping it leaves the module half-built.
  (instance.exports.__wbindgen_start as () => void)();
  return glue;
}

beforeAll(async () => {
  await initDecisions(loadDecisionsForTest);
});
