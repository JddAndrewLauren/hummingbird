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
//
// A sentinel that matches nothing in `src/` is a FAILURE here, not a harmless
// spare. The needle for `demo-questions.ts` was the literal `"demo-questions"`
// — a filename, which appears nowhere in the file it was supposed to guard —
// so that fixture shipped in the production bundle while this script reported
// ok. A gate that cannot detect its own vacuity goes stale the first time a
// fixture is renamed, and reports success for as long as nobody checks by
// hand. So the sentinels are checked against the source too.

import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
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
  { fixture: "demo-questions.ts", needle: "example.gov/waste/collection-day" },
  // fixtures/demo-data.ts again — the route checklist, which lived in
  // RoutesScreen.tsx as a module-level literal until it was moved here.
  { fixture: "demo-data.ts", needle: "Regenerate the Gmail fixture set" },
];

const DIST = new URL("../dist/", import.meta.url).pathname;
const FIXTURES = new URL("../src/fixtures/", import.meta.url).pathname;

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

// Every needle must still occur in the module it claims to come from. Checking
// against the named file rather than all of `src/` is the stronger question:
// it also catches a needle that survived a move to somewhere this script is not
// guarding.
for (const { fixture, needle } of SENTINELS) {
  const source = readFileSync(join(FIXTURES, fixture), "utf8");
  if (!source.includes(needle)) {
    failed = true;
    console.error(
      `FAIL sentinel ${JSON.stringify(needle)} does not occur in src/fixtures/${fixture}.\n` +
        `     It can never match, so this script has been reporting ok without\n` +
        `     guarding that fixture. Pick a string the module actually contains.`,
    );
  }
}

// `existsSync` rather than letting `files()` throw: without it a missing build
// is an ENOENT stack trace, and the "run `pnpm build` first" message below —
// the one thing that tells you what to do — is unreachable.
for (const path of existsSync(DIST) ? files(DIST) : []) {
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

console.log(
  `ok — ${SENTINELS.length} live sentinels, ${scanned} built files carry no demo fixture`,
);
