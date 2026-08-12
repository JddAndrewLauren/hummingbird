import { test } from "node:test";
import assert from "node:assert/strict";
import { EventEmitter } from "node:events";
import { runSkill, readOutcome } from "../src/run-skill.js";

function fakeChild() {
  const child = new EventEmitter();
  child.stdout = new EventEmitter();
  child.stderr = new EventEmitter();
  return child;
}

const SCHEMA_TEXT = '{"type":"object","required":["title","notes"]}';
const readSchema = () => SCHEMA_TEXT;

/** One live-shaped `--output-format json` envelope, trimmed to the fields this runner reads. */
function cliEnvelope(structuredOutput) {
  return JSON.stringify({
    is_error: false,
    subtype: "success",
    session_id: "682e8d69",
    usage: { input_tokens: 2, output_tokens: 423 },
    total_cost_usd: 0.32,
    result: JSON.stringify(structuredOutput),
    structured_output: structuredOutput,
    type: "result",
  });
}

test("resolves ok:true with the envelope's structured_output on a clean exit", async () => {
  const child = fakeChild();
  const spawn = () => child;

  const promise = runSkill({
    skillName: "parse-capture",
    prompt: "/parse-capture\n\nbuy milk",
    schemaPath: "/app/schema.json",
    spawn,
    readSchema,
    onProgress: () => {},
  });

  child.stdout.emit("data", Buffer.from(cliEnvelope({ title: "buy milk", notes: "" })));
  child.emit("close", 0);

  const outcome = await promise;
  assert.deepEqual(outcome, { ok: true, result: { title: "buy milk", notes: "" } });
});

test("spawns claude with the schema's contents, never its path", async () => {
  const child = fakeChild();
  let seenArgs;
  const spawn = (_bin, args) => {
    seenArgs = args;
    return child;
  };

  const promise = runSkill({
    skillName: "parse-capture",
    prompt: "p",
    schemaPath: "/app/.claude/skills/parse-capture/schema.json",
    spawn,
    readSchema,
    onProgress: () => {},
  });

  child.stdout.emit("data", Buffer.from(cliEnvelope({ title: "t", notes: "" })));
  child.emit("close", 0);
  await promise;

  assert.equal(seenArgs[seenArgs.indexOf("--json-schema") + 1], SCHEMA_TEXT);
});

test("an unreadable schema is an ok:false outcome, not a throw", async () => {
  const spawn = () => {
    throw new Error("must never spawn when the schema could not be read");
  };

  const outcome = await runSkill({
    skillName: "parse-capture",
    prompt: "p",
    schemaPath: "/app/missing.json",
    spawn,
    readSchema: () => {
      throw new Error("ENOENT: no such file or directory");
    },
    onProgress: () => {},
  });

  assert.equal(outcome.ok, false);
  assert.match(outcome.error, /could not read schema/);
  assert.match(outcome.error, /ENOENT/);
});

test("resolves ok:false when claude exits non-zero, carrying stderr as the error", async () => {
  const child = fakeChild();
  const spawn = () => child;

  const promise = runSkill({
    skillName: "parse-capture",
    prompt: "p",
    schemaPath: "/app/schema.json",
    spawn,
    readSchema,
    onProgress: () => {},
  });

  child.stderr.emit("data", Buffer.from("auth error\n"));
  child.emit("close", 1);

  const outcome = await promise;
  assert.equal(outcome.ok, false);
  assert.match(outcome.error, /auth error/);
});

test("resolves ok:false with a named error when a clean exit's stdout is not valid JSON", async () => {
  const child = fakeChild();
  const spawn = () => child;

  const promise = runSkill({
    skillName: "parse-capture",
    prompt: "p",
    schemaPath: "/app/schema.json",
    spawn,
    readSchema,
    onProgress: () => {},
  });

  child.stdout.emit("data", Buffer.from("not json"));
  child.emit("close", 0);

  const outcome = await promise;
  assert.equal(outcome.ok, false);
  assert.match(outcome.error, /could not parse/i);
});

test("resolves ok:false when the child process itself errors (e.g. spawn ENOENT)", async () => {
  const child = fakeChild();
  const spawn = () => child;

  const promise = runSkill({
    skillName: "parse-capture",
    prompt: "p",
    schemaPath: "/app/schema.json",
    spawn,
    readSchema,
    onProgress: () => {},
  });

  child.emit("error", new Error("spawn claude ENOENT"));

  const outcome = await promise;
  assert.equal(outcome.ok, false);
  assert.match(outcome.error, /ENOENT/);
});

test("calls onProgress with a starting message", async () => {
  const child = fakeChild();
  const spawn = () => child;
  const seen = [];

  const promise = runSkill({
    skillName: "parse-capture",
    prompt: "p",
    schemaPath: "/app/schema.json",
    spawn,
    readSchema,
    onProgress: (message) => seen.push(message),
  });

  child.stdout.emit("data", Buffer.from(cliEnvelope({ title: "t", notes: "n" })));
  child.emit("close", 0);
  await promise;

  assert.ok(seen.length >= 1);
  assert.match(seen[0], /parse-capture/);
});

test("readOutcome unwraps structured_output rather than returning the CLI metadata", () => {
  const outcome = readOutcome(cliEnvelope({ title: "buy milk", notes: "also blue" }));
  assert.deepEqual(outcome, { ok: true, result: { title: "buy milk", notes: "also blue" } });
});

test("readOutcome reports the CLI's own is_error rather than passing it off as a result", () => {
  const outcome = readOutcome(
    JSON.stringify({ is_error: true, subtype: "error_during_execution", result: "credit balance too low" }),
  );
  assert.equal(outcome.ok, false);
  assert.match(outcome.error, /credit balance/);
});

test("readOutcome refuses a run that produced no structured output", () => {
  const outcome = readOutcome(JSON.stringify({ is_error: false, result: "sure, here you go" }));
  assert.equal(outcome.ok, false);
  assert.match(outcome.error, /no structured output/);
});

test("readOutcome refuses a non-object envelope", () => {
  const outcome = readOutcome("42");
  assert.equal(outcome.ok, false);
  assert.match(outcome.error, /envelope/);
});

// --- the reported model (#273) -------------------------------------------

/**
 * Read defensively on purpose. This module's header records this repo being
 * burned twice by believing something about the CLI's output shape, with
 * green tests throughout both times -- so what these fixtures pin is not
 * "the CLI emits `modelUsage`" but "if it does not, we degrade instead of
 * lying". `docs/runner.md`'s runbook carries the live confirmation step.
 */
test("readOutcome names the model the CLI reported running", () => {
  const outcome = readOutcome(
    JSON.stringify({
      is_error: false,
      structured_output: { title: "t", notes: "" },
      modelUsage: { "claude-opus-5": { inputTokens: 2, outputTokens: 423 } },
    }),
  );
  assert.equal(outcome.ok, true);
  assert.equal(outcome.model, "claude-opus-5");
});

test("an absent, empty or wrong-shaped modelUsage yields no reported model", () => {
  for (const usage of [undefined, null, {}, [], "claude-opus-5", 42]) {
    const outcome = readOutcome(
      JSON.stringify({
        is_error: false,
        structured_output: { title: "t", notes: "" },
        ...(usage === undefined ? {} : { modelUsage: usage }),
      }),
    );
    assert.equal(outcome.ok, true, JSON.stringify(usage));
    // Absent, never null: `stamp.js`'s chain then falls through to the
    // requested or configured model rather than stamping `null`.
    assert.equal("model" in outcome, false, JSON.stringify(usage));
  }
});

/** An `is_error` run still spent tokens on a model worth naming. */
test("the is_error path still reports its model", () => {
  const outcome = readOutcome(
    JSON.stringify({
      is_error: true,
      subtype: "error_during_execution",
      result: "credit balance too low",
      modelUsage: { "kimi-k3": {} },
    }),
  );
  assert.equal(outcome.ok, false);
  assert.equal(outcome.model, "kimi-k3");
});

test("a run that produced no structured output still reports its model", () => {
  const outcome = readOutcome(
    JSON.stringify({ is_error: false, result: "sure", modelUsage: { sonnet: {} } }),
  );
  assert.equal(outcome.ok, false);
  assert.equal(outcome.model, "sonnet");
});

test("runSkill passes the requested model through to the argv", async () => {
  let seenArgs;
  const child = new EventEmitter();
  child.stdout = new EventEmitter();
  child.stderr = new EventEmitter();
  const promise = runSkill({
    skillName: "microtask",
    prompt: "/microtask",
    schemaPath: "/app/schema.json",
    spawn: (_command, args) => {
      seenArgs = args;
      return child;
    },
    onProgress: () => {},
    readSchema: () => "{}",
    model: "claude-opus-5",
  });
  child.stdout.emit("data", Buffer.from(cliEnvelope({ title: "t", notes: "" })));
  child.emit("close", 0);
  await promise;

  assert.deepEqual(seenArgs.slice(-2), ["--model", "claude-opus-5"]);
});
