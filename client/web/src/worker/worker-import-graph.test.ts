import { readFileSync, existsSync } from "node:fs";
import { dirname, relative, resolve } from "node:path";
import { describe, expect, it } from "vitest";

// ADR-0010's top-level-`await` invariant, and ADR-0025's main-thread seam,
// meet here. `core.worker.ts` gets its wasm instance itself, inside its own
// `activate()`; the main thread gets a SECOND instance through
// `src/decisions/seam.ts`. Those two arrangements only stay separable while
// the worker's STATIC import graph contains neither the seam nor the
// generated package: a static import of either would instantiate a module
// during the worker's script evaluation, before the port handshake the
// worker's header says must happen in the first synchronous turn.
//
// A comment cannot enforce that — an innocent `import { canSubmitCapture }
// from "../screens/capture-validation"` in a worker-side module is one
// keystroke away and would typecheck. So this walks the graph.

const ROOT = resolve(process.cwd(), "src");
const ENTRY = resolve(ROOT, "worker/core.worker.ts");

/** Every `from "…"` specifier a module states statically. Dynamic
 * `import(…)` is deliberately not matched: it is exactly what the worker
 * uses for the wasm package, and what the seam uses for its own. */
function staticSpecifiers(source: string): string[] {
  const stripped = source.replace(/\/\*[\s\S]*?\*\//g, "").replace(/^\s*\/\/.*$/gm, "");
  return [...stripped.matchAll(/^\s*(?:import|export)[\s\S]*?from\s*["']([^"']+)["']/gm)].map(
    (match) => match[1],
  );
}

function resolveModule(fromFile: string, specifier: string): string | null {
  if (!specifier.startsWith(".")) return null;
  const base = resolve(dirname(fromFile), specifier);
  for (const candidate of [base, `${base}.ts`, `${base}.tsx`, resolve(base, "index.ts")]) {
    if (existsSync(candidate) && !candidate.endsWith("/")) return candidate;
  }
  return null;
}

function graphFrom(entry: string): Set<string> {
  const seen = new Set<string>();
  const queue = [entry];
  while (queue.length > 0) {
    const file = queue.pop()!;
    if (seen.has(file)) continue;
    seen.add(file);
    for (const specifier of staticSpecifiers(readFileSync(file, "utf8"))) {
      const resolved = resolveModule(file, specifier);
      if (resolved) queue.push(resolved);
    }
  }
  return seen;
}

describe("core.worker.ts's static import graph", () => {
  const graph = [...graphFrom(ENTRY)].map((file) => relative(ROOT, file));

  it("is walked from a real entry point with real edges", () => {
    // Guards the walker itself: a regex that silently matched nothing would
    // make every assertion below vacuously true.
    expect(graph).toContain("worker/core.worker.ts");
    expect(graph).toContain("worker/ports.ts");
    expect(graph.length).toBeGreaterThan(5);
  });

  it("contains nothing from the main-thread decision seam (ADR-0025)", () => {
    expect(graph.filter((file) => file.startsWith("decisions/"))).toEqual([]);
  });

  it("contains no static import of the generated wasm package (ADR-0010)", () => {
    const offenders = graph.filter((file) =>
      readFileSync(resolve(ROOT, file), "utf8").match(
        /^\s*(?:import|export)[\s\S]*?from\s*["'][^"']*wasm\/pkg[^"']*["']/gm,
      ),
    );
    expect(offenders).toEqual([]);
  });
});
