// `runner/scripts/mint-hb-token.sh` under a stubbed `curl` and a stubbed
// `flyctl` (#272's provisioning step). Nothing here reaches the authority or
// Fly: `FLYCTL` points at a stub, and a stub `curl` earlier on `PATH` answers
// with a canned status code and response body.
//
// The case worth the harness is the replay: `POST /api/admin/tokens` is
// idempotent by `id` and stores only a hash, so a second mint under a used id
// answers 200 with the metadata and *no* token. Setting `HB_API_TOKEN=` from
// that body would deploy an empty credential and leave `microtask` failing auth
// instead of declining, so the script must refuse -- and only a test that hands
// it a tokenless 200 can say it does.
import { test } from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, writeFileSync, readFileSync, existsSync, chmodSync, statSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const SCRIPT = join(dirname(fileURLToPath(import.meta.url)), "..", "scripts", "mint-hb-token.sh");

const TOKEN = `hb_${"a1b2c3d4".repeat(8)}`;

const FLY_STUB = `#!/bin/bash
printf '%s\\n' "$*" >> "$STUB_LOG"
exit 0
`;

// Emulates the one curl invocation the script makes: writes the canned body to
// whatever `-o` names, logs its argv one argument per line, prints the status
// code as the `-w` trailer. One-per-line rather than `$*`, so an assertion can
// see an argument's exact bounds -- whitespace carried into a credential is one
// of the things these tests are here to catch.
const CURL_STUB = `#!/bin/bash
out=
prev=
for a in "$@"; do
  [ "$prev" = "-o" ] && out=$a
  prev=$a
done
printf '%s\\n' "$@" >> "$CURL_LOG"
[ -n "$out" ] && cat "$STUB_BODY" > "$out"
printf '%s' "$STUB_CODE"
exit 0
`;

/** Runs the script against both stubs; returns {status, stdout, stderr, fly, curl, dir}. */
function run({ code = "201", body = null, secret = "admin-secret-value\n", args = [], env = {} } = {}) {
  const dir = mkdtempSync(join(tmpdir(), "mint-hb-token-"));
  const bin = join(dir, "bin");
  const fly = join(dir, "flyctl");
  const flyLog = join(dir, "fly.log");
  const curlLog = join(dir, "curl.log");
  const secretFile = join(dir, "admin");

  writeFileSync(fly, FLY_STUB);
  chmodSync(fly, 0o755);
  writeFileSync(join(dir, "body"), body ?? JSON.stringify({ id: "runner", name: "hummingbird-runner", scope: "device", created_at: 1, token: TOKEN }));
  writeFileSync(secretFile, secret);

  spawnSync("mkdir", ["-p", bin]);
  writeFileSync(join(bin, "curl"), CURL_STUB);
  chmodSync(join(bin, "curl"), 0o755);

  const proc = spawnSync("bash", [SCRIPT, secretFile, ...args], {
    encoding: "utf8",
    env: {
      ...process.env,
      ...env,
      PATH: `${bin}:${process.env.PATH}`,
      FLYCTL: fly,
      STUB_LOG: flyLog,
      CURL_LOG: curlLog,
      STUB_BODY: join(dir, "body"),
      STUB_CODE: code,
    },
  });
  const read = (p) => (existsSync(p) ? readFileSync(p, "utf8").trim().split("\n").filter(Boolean) : []);
  return { status: proc.status, stdout: proc.stdout, stderr: proc.stderr, fly: read(flyLog), curl: read(curlLog), dir };
}

test("a 201 sets HB_API_TOKEN to the minted plaintext, in one un-staged call", () => {
  const { status, fly } = run();
  assert.equal(status, 0);
  assert.equal(fly.length, 1);
  assert.match(fly[0], /^secrets set /);
  assert.ok(fly[0].includes(`HB_API_TOKEN=${TOKEN}`));
  // Staging would leave the secret inert until something else deployed it.
  assert.ok(!fly[0].includes("--stage"));
  // HB_API_BASE is the runner's own default; setting it here is a separate act.
  assert.ok(!fly[0].includes("HB_API_BASE"));
});

test("posts a device-scope body carrying no `source` -- forbidden for every non-ingest scope", () => {
  const { curl } = run({ args: ["runner"] });
  assert.equal(curl.filter((a) => a === "POST").length, 1);
  assert.ok(curl.includes('{"id":"runner","name":"hummingbird-runner","scope":"device"}'));
  assert.ok(!curl.some((a) => a.includes("source")));
});

test("an idempotent replay -- 200 with no token -- refuses and never touches Fly", () => {
  const body = JSON.stringify({ id: "runner", name: "hummingbird-runner", scope: "device", created_at: 1 });
  const { status, stderr, fly } = run({ code: "200", body });
  assert.equal(status, 1);
  assert.deepEqual(fly, []);
  assert.match(stderr, /already exists/);
  assert.match(stderr, /DELETE \S+\/api\/admin\/tokens\/runner\b/);
});

test("a rejected ADMIN_SECRET is fatal before any secret is set", () => {
  const { status, stderr, fly } = run({ code: "401", body: "" });
  assert.equal(status, 1);
  assert.deepEqual(fly, []);
  assert.match(stderr, /ADMIN_SECRET/);
});

test("trims the trailing newline a clipboard read leaves on ADMIN_SECRET", () => {
  const { curl } = run({ secret: "  admin-secret-value\n" });
  // An exact argv match, so neither the leading spaces nor the newline can hide.
  assert.ok(curl.includes("Authorization: Bearer admin-secret-value"));
});

test("an empty secret file is caught before the mint", () => {
  const { status, stderr, curl } = run({ secret: "\n" });
  assert.equal(status, 2);
  assert.deepEqual(curl, []);
  assert.match(stderr, /is empty/);
});

test("HB_TOKEN_OUT keeps a mode-600 copy with no trailing newline", () => {
  const dir = mkdtempSync(join(tmpdir(), "mint-hb-token-out-"));
  const out = join(dir, "token");
  const { status } = run({ env: { HB_TOKEN_OUT: out } });
  assert.equal(status, 0);
  assert.equal(readFileSync(out, "utf8"), TOKEN);
  assert.equal(statSync(out).mode & 0o777, 0o600);
});

test("uses the token id it is given, in both the body and the revoke hint", () => {
  const body = JSON.stringify({ id: "runner-2", name: "hummingbird-runner", scope: "device" });
  const { stderr } = run({ code: "200", body, args: ["runner-2"] });
  assert.match(stderr, /DELETE \S+\/api\/admin\/tokens\/runner-2/);
});
