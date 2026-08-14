// No demo fixture may reach a production bundle. Run after `vite build`.
//
// `fixtures/demo.ts` gates both demo worlds twice — `import.meta.env.DEV` is
// substituted with the literal `false`, leaving `if (false && …)` for Rollup to
// drop the fixture with. The gate is real, but it only *removes* a module
// Rollup can prove is side-effect-free at the top level, and that proof is
// easy to break by accident: reading the clock at module scope, or calling a
// helper while constructing a literal, is enough. #420's first cut did exactly
// that and shipped 5.3 KB of board fixture into `dist/` while the kit world's
// pure literal was correctly dropped — with a source comment claiming
// otherwise, which is the part that makes this worth a gate rather than a
// convention.
//
// A unit test cannot see this: the question is about the built artifact, and
// nothing in `src/` knows what Rollup decided. So this reads `dist/`.
//
// Adding a fixture? Add a sentinel from it below. A string is enough — pick one
// that could not plausibly appear in real UI copy.

import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";

/** One distinctive string per fixture module. Not exhaustive by design: if the
 * module survives at all, its strings survive with it, so one sentinel per
 * file detects the failure this guards. */
const SENTINELS = [
  // fixtures/demo-data.ts — the kit world.
  { fixture: "demo-data.ts", needle: "Order the replacement sensor" },
  { fixture: "demo-data.ts", needle: "ask dad about the trailer hitch" },
  // fixtures/demo-task-state.ts — the board world (#420).
  { fixture: "demo-task-state.ts", needle: "Fit the new tap washer" },
  { fixture: "demo-task-state.ts", needle: "the authority refused that edit" },
  // fixtures/demo-questions.ts — the standing-question world.
  { fixture: "demo-questions.ts", needle: "demo-questions" },
];

const DIST = new URL("../dist/", import.meta.url).pathname;

function* files(dir) {
  for (const entry of readdirSync(dir)) {
    const path = join(dir, entry);
    if (statSync(path).isDirectory()) {
      yield* files(path);
    } else if (/\.(js|mjs|cjs|html|css)$/.test(entry)) {
      yield path;
    }
  }
}

let failed = false;
let scanned = 0;

for (const path of files(DIST)) {
  scanned += 1;
  const text = readFileSync(path, "utf8");
  for (const { fixture, needle } of SENTINELS) {
    if (text.includes(needle)) {
      failed = true;
      console.error(
        `FAIL ${path.replace(DIST, "dist/")}\n` +
          `     contains ${JSON.stringify(needle)} from ${fixture}.\n` +
          `     A demo fixture reached the production bundle. The dead-branch gate in\n` +
          `     fixtures/demo.ts only drops a module with no top-level side effects —\n` +
          `     check for a module-scope Date.now(), a helper call inside a literal, or\n` +
          `     a const built at import instead of inside a function.`,
      );
    }
  }
}

if (scanned === 0) {
  console.error("FAIL no build output found in dist/ — run `pnpm build` first.");
  process.exit(1);
}

if (failed) {
  process.exit(1);
}

console.log(`ok — ${scanned} built files carry no demo fixture`);
