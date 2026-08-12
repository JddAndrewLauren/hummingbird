// `runner/scripts/switch-provider.sh` under a stubbed flyctl (#41 decision 2).
//
// The stub emulates *flyctl*, not the script's expectations: `secrets list`
// answers with the real human table unless `--json` is passed, exactly as
// flyctl v0.4.82 does. That is what makes these tests worth anything -- the
// staged-row case below fails against a positional parse of the table, which
// is the bug that shipped, and passes only once the script reads `--json`.
//
// Nothing here talks to Fly: the script's `FLYCTL` override points at the stub,
// which logs its argv and prints canned output.
import { test } from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, writeFileSync, readFileSync, existsSync, chmodSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const SCRIPT = join(dirname(fileURLToPath(import.meta.url)), "..", "scripts", "switch-provider.sh");

const LISTING = [
  { name: "ANTHROPIC_API_KEY", digest: "21ab50381350767c", status: "Staged" },
  { name: "RUNNER_BEARER_TOKEN", digest: "40fdb64ad20e9aac", status: "Deployed" },
];

// The `│`-separated table flyctl prints without `--json`: a header row, and a
// `*` in its own column ahead of the name of any staged secret. Derived from the
// same listing the JSON is, so the two views can never disagree in a fixture.
const asTable = (rows) =>
  [
    " NAME                 │ DIGEST           │ STATUS   ",
    ...rows.map(
      (r) => ` ${r.status === "Staged" ? "* " : ""}${r.name} │ ${r.digest} │ ${r.status} `,
    ),
    "",
  ].join("\n");

const STUB = `#!/bin/bash
printf '%s\\n' "$*" >> "$STUB_LOG"
if [ "$1 $2" = "secrets list" ]; then
  for a in "$@"; do [ "$a" = "--json" ] && { cat "$STUB_JSON"; exit 0; }; done
  cat "$STUB_TABLE"
  exit 0
fi
if [ "$1 $2" = "secrets deploy" ]; then
  [ -n "\${STUB_DEPLOY_FAIL:-}" ] && { echo "$STUB_DEPLOY_FAIL" >&2; exit 1; }
fi
exit 0
`;

/** Runs the script against the stub; returns {status, stdout, stderr, calls}. */
function run(args, { listing = LISTING, env = {} } = {}) {
  const dir = mkdtempSync(join(tmpdir(), "switch-provider-"));
  const fly = join(dir, "flyctl");
  const log = join(dir, "calls.log");
  writeFileSync(fly, STUB);
  chmodSync(fly, 0o755);
  writeFileSync(join(dir, "table"), asTable(listing));
  writeFileSync(join(dir, "json"), JSON.stringify(listing, null, 4));
  writeFileSync(join(dir, "key"), "sk-secret-value\n");

  const argv = args.map((a) => (a === "<key>" ? join(dir, "key") : a));
  const proc = spawnSync("bash", [SCRIPT, ...argv], {
    encoding: "utf8",
    env: {
      ...process.env,
      ...env,
      FLYCTL: fly,
      STUB_LOG: log,
      STUB_TABLE: join(dir, "table"),
      STUB_JSON: join(dir, "json"),
    },
  });
  const calls = existsSync(log) ? readFileSync(log, "utf8").trim().split("\n") : [];
  return { status: proc.status, stdout: proc.stdout, stderr: proc.stderr, calls };
}

const unsets = (calls) => calls.filter((c) => c.startsWith("secrets unset"));

test("clears a STAGED opposing credential -- the table's `*` column must not hide it", () => {
  const { status, calls } = run(["third-party", "<key>", "https://api.moonshot.ai/anthropic", "kimi-k3"]);
  assert.equal(status, 0);
  assert.deepEqual(
    unsets(calls).map((c) => c.split(" ").at(-1)),
    ["ANTHROPIC_API_KEY"],
  );
});

test("clears a DEPLOYED opposing credential", () => {
  const listing = [{ name: "ANTHROPIC_API_KEY", digest: "d", status: "Deployed" }];
  const { status, calls } = run(["third-party", "<key>", "https://x", "m"], { listing });
  assert.deepEqual(
    unsets(calls).map((c) => c.split(" ").at(-1)),
    ["ANTHROPIC_API_KEY"],
  );
  assert.equal(status, 0);
});

test("never mistakes the table's NAME header for a secret", () => {
  const { status, stdout, calls } = run(["third-party", "<key>", "https://x", "m"], { listing: [] });
  assert.equal(status, 0);
  assert.deepEqual(unsets(calls), []);
  assert.match(stdout, /nothing to clear/);
});

test("the anthropic direction clears exactly the third-party variables that are set, in one call", () => {
  const listing = [
    { name: "ANTHROPIC_AUTH_TOKEN", digest: "d", status: "Staged" },
    { name: "ANTHROPIC_MODEL", digest: "d", status: "Deployed" },
  ];
  const { status, calls } = run(["anthropic", "<key>"], { listing });
  assert.equal(status, 0);
  assert.equal(unsets(calls).length, 1);
  assert.match(unsets(calls)[0], /--app hummingbird-runner ANTHROPIC_AUTH_TOKEN ANTHROPIC_MODEL$/);
});

test("with nothing to clear, a failing `secrets deploy` is fatal -- the staged set never went live", () => {
  const { status, stderr } = run(["anthropic", "<key>"], {
    listing: [],
    env: { STUB_DEPLOY_FAIL: "Error: could not reach the API" },
  });
  assert.notEqual(status, 0);
  assert.match(stderr, /could not reach the API/);
});

test("with nothing to clear, a no-machines-yet `secrets deploy` is tolerated", () => {
  const { status, stderr } = run(["anthropic", "<key>"], {
    listing: [],
    env: { STUB_DEPLOY_FAIL: "Error: no machines available to deploy" },
  });
  assert.equal(status, 0);
  assert.match(stderr, /no machines yet/);
});

test("trims the trailing newline a clipboard read leaves on the credential", () => {
  const { calls } = run(["anthropic", "<key>"], { listing: [] });
  const set = calls.find((c) => c.startsWith("secrets set"));
  assert.match(set, /ANTHROPIC_API_KEY=sk-secret-value$/);
});
