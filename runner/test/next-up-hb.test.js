import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { nextUp } from "../src/skills/next-up-hb.js";

const repoRoot = fileURLToPath(new URL("../..", import.meta.url));

const sweep = { version: 1, items: [], projects: [], fog: [], blocked_by: [] };
const now = { local: "2026-08-11T09:53", epoch_ms: 1786553580000 };

test("name matches the skill directory / slash command", () => {
  assert.equal(nextUp.name, "next-up-hb");
});

test("resultSchemaPath points beside the SKILL.md, versioned in this repo", () => {
  assert.equal(nextUp.resultSchemaPath, ".claude/skills/next-up-hb/schema.json");
});

// The schema file lives outside `runner/` but is baked into the same image
// (`runner/Dockerfile`) and passed to `--json-schema` on every real run, so
// these are the runner's own tests, not the skill's.
test("the shipped schema file exists at resultSchemaPath and is valid JSON", () => {
  const raw = readFileSync(`${repoRoot}${nextUp.resultSchemaPath}`, "utf8");
  assert.doesNotThrow(() => JSON.parse(raw));
});

test("the shipped schema carries no $schema key -- the CLI rejects the 2020-12 ref outright", () => {
  const schema = JSON.parse(readFileSync(`${repoRoot}${nextUp.resultSchemaPath}`, "utf8"));
  assert.equal(schema.$schema, undefined);
});

test("the shipped schema is the {pick, alternates, health} object, closed to extra fields", () => {
  const schema = JSON.parse(readFileSync(`${repoRoot}${nextUp.resultSchemaPath}`, "utf8"));
  assert.equal(schema.type, "object");
  assert.deepEqual(schema.required, ["pick", "alternates", "health"]);
  assert.equal(schema.additionalProperties, false);
  // An empty frontier answers with no pick rather than an invented one, so
  // `pick` has to admit null.
  assert.ok(schema.properties.pick.type.includes("null"));
  // "Never the full list" is a schema fact, not only prose.
  assert.equal(schema.properties.alternates.maxItems, 5);
});

test("validateArgs accepts a sweep payload plus a well-shaped now", () => {
  assert.deepEqual(nextUp.validateArgs({ sweep, now }), { ok: true });
});

test("validateArgs rejects a missing or non-sweep payload", () => {
  assert.equal(nextUp.validateArgs({ now }).ok, false);
  assert.equal(nextUp.validateArgs({ sweep: {}, now }).ok, false);
  assert.equal(nextUp.validateArgs({ sweep: [], now }).ok, false);
});

test("validateArgs rejects a missing or malformed now", () => {
  assert.equal(nextUp.validateArgs({ sweep }).ok, false);
  assert.equal(nextUp.validateArgs({ sweep, now: { local: "", epoch_ms: 1 } }).ok, false);
  assert.equal(nextUp.validateArgs({ sweep, now: { local: "2026-08-11" } }).ok, false);
  assert.equal(
    nextUp.validateArgs({ sweep, now: { local: "2026-08-11", epoch_ms: "1" } }).ok,
    false,
  );
});

test("axes are optional, and each one independently so", () => {
  assert.deepEqual(nextUp.validateArgs({ sweep, now, axes: {} }), { ok: true });
  assert.deepEqual(nextUp.validateArgs({ sweep, now, axes: { size: "quick" } }), { ok: true });
  assert.deepEqual(
    nextUp.validateArgs({ sweep, now, axes: { context: "@computer", energy: "low", size: "deep" } }),
    { ok: true },
  );
});

test("validateArgs rejects an axis outside the owned schema's closed vocabulary", () => {
  const energy = nextUp.validateArgs({ sweep, now, axes: { energy: "medium-ish" } });
  assert.equal(energy.ok, false);
  assert.match(energy.error, /energy/);
  const size = nextUp.validateArgs({ sweep, now, axes: { size: "medium" } });
  assert.equal(size.ok, false);
  assert.match(size.error, /size/);
});

test("agent_only is optional, boolean, and rides through to the ranker", () => {
  assert.deepEqual(nextUp.validateArgs({ sweep, now }), { ok: true });
  assert.deepEqual(nextUp.validateArgs({ sweep, now, agent_only: true }), { ok: true });
  assert.deepEqual(nextUp.validateArgs({ sweep, now, agent_only: false }), { ok: true });
  const bad = nextUp.validateArgs({ sweep, now, agent_only: "yes" });
  assert.equal(bad.ok, false);
  assert.match(bad.error, /agent_only/);
});

// The ranker names a status/event mismatch itself; re-deciding it here
// would be a second copy of the rule that can drift from the deciding one.
test("a calendar block rides through validation untouched", () => {
  const calendar = { current_or_next: { status: "none", event: null }, today: [] };
  assert.deepEqual(nextUp.validateArgs({ sweep, now, calendar }), { ok: true });
});

// --- prepare: the deterministic half, run before the model ---------------
//
// The runner arm cannot shell out: `claude -p` is non-interactive, a tool
// call needing permission cannot be prompted for, and `claude-cli.js`
// passes no `--allowedTools`. So `next-up-rank` runs here instead, and the
// model receives an answer rather than an instruction to go and get one.

const ranked = { candidates: [{ item: { id: "a" }, reasons: ["overdue"] }], health: { triage: 0 } };
const ranker = (outcome) => async () => outcome;

test("prepare runs the ranker over the whole envelope, args verbatim", async () => {
  const args = { sweep, now, axes: { size: "quick" } };
  let seen;
  const runRanker = async (envelope) => {
    seen = envelope;
    return { ok: true, ranked };
  };
  const step = await nextUp.prepare(args, { runRanker });
  assert.equal(step.ok, true);
  assert.deepEqual(seen, args);
});

test("prepare hands buildPrompt the ranked answer and drops the raw sweep", async () => {
  const args = { sweep, now, axes: { size: "quick" } };
  const step = await nextUp.prepare(args, { runRanker: ranker({ ok: true, ranked }) });

  assert.deepEqual(step.args.ranked, ranked);
  // Every fact the model needs about the sweep is already in the ranker's
  // answer; forwarding both would spend context on a payload nothing reads.
  assert.equal(step.args.sweep, undefined);
  // Everything the model DOES read is untouched.
  assert.deepEqual(step.args.now, now);
  assert.deepEqual(step.args.axes, { size: "quick" });
});

// The selector, not the model, applies the axis -- so it has to reach the
// ranker, and it must NOT survive into the prompt as a second instruction
// the model might act on a second time.
test("agent_only reaches the ranker inside the envelope", async () => {
  let seen;
  const runRanker = async (envelope) => {
    seen = envelope;
    return { ok: true, ranked };
  };
  await nextUp.prepare({ sweep, now, agent_only: true }, { runRanker });
  assert.equal(seen.agent_only, true);
});

test("a calendar block survives prepare -- the display line is the model's job", async () => {
  const calendar = { current_or_next: { status: "none", event: null }, today: [] };
  const step = await nextUp.prepare({ sweep, now, calendar }, { runRanker: ranker({ ok: true, ranked }) });
  assert.deepEqual(step.args.calendar, calendar);
});

test("a failed ranking is a named failure, attributed to the ranker", async () => {
  const step = await nextUp.prepare(
    { sweep, now },
    { runRanker: ranker({ ok: false, error: "next-up-rank exited with code 1" }) },
  );
  assert.deepEqual(step, { ok: false, error: "ranker: next-up-rank exited with code 1" });
});

test("buildPrompt invokes the /next-up-hb slash command and carries the prepared args", async () => {
  const step = await nextUp.prepare({ sweep, now }, { runRanker: ranker({ ok: true, ranked }) });
  const prompt = nextUp.buildPrompt(step.args);
  assert.match(prompt, /^\/next-up-hb\b/);
  assert.ok(prompt.includes(JSON.stringify(step.args)));
});

test("the prompt tells the model the ranking is done and that it has no shell", () => {
  // Both halves are load-bearing: without the first the model re-sorts an
  // already-total order, and without the second it tries `scripts/next-up.sh`
  // and burns the run on a denied Bash call.
  const prompt = nextUp.buildPrompt({ ranked, now });
  assert.match(prompt, /ALREADY run/);
  assert.match(prompt, /Do not re-rank/);
  assert.match(prompt, /no shell/);
});

test("parse-capture declares no prepare, so the hook is genuinely optional", async () => {
  const { parseCapture } = await import("../src/skills/parse-capture.js");
  assert.equal(parseCapture.prepare, undefined);
});
