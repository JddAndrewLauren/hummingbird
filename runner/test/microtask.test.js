import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { microtask } from "../src/skills/microtask.js";
import { stepId } from "../src/step-id.js";

const repoRoot = fileURLToPath(new URL("../..", import.meta.url));

const ITEM = { id: "11111111-2222-4333-8444-555555555555", seq: 42, title: "clear the garage" };
const SWEEP = {
  items: [ITEM, { id: "other", seq: 7, title: "something else" }],
  steps: [
    { id: "s-2", item_id: ITEM.id, body: "second", position: 2, done: false, deleted_at: null },
    { id: "s-1", item_id: ITEM.id, body: "first", position: 1, done: true, deleted_at: null },
    { id: "s-x", item_id: ITEM.id, body: "dropped", position: 3, deleted_at: 1000 },
    { id: "s-o", item_id: "other", body: "not ours", position: 9, deleted_at: null },
  ],
};

/** An authority whose reads and writes are canned, and which records every write. */
function fakeAuthority({ sweep = { ok: true, sweep: SWEEP }, createStep } = {}) {
  const writes = [];
  return {
    writes,
    sweep: async () => sweep,
    createStep: async (step) => {
      writes.push(step);
      return createStep ? createStep(step, writes.length) : { ok: true, created: true, step };
    },
  };
}

const noProgress = () => {};

// --- the shipped schema --------------------------------------------------

test("name and resultSchemaPath match the skill directory / slash command", () => {
  assert.equal(microtask.name, "microtask");
  assert.equal(microtask.resultSchemaPath, ".claude/skills/microtask/schema.json");
});

test("the shipped schema file exists, is valid JSON, and carries no $schema key", () => {
  // A $schema key is a deploy-time outage, not a style nit: the CLI rejects
  // the draft-2020-12 ref outright before it runs anything.
  const schema = JSON.parse(readFileSync(`${repoRoot}${microtask.resultSchemaPath}`, "utf8"));
  assert.equal(schema.$schema, undefined);
  assert.equal(schema.type, "object");
  assert.equal(schema.additionalProperties, false);
  assert.deepEqual(schema.required, ["steps", "note"]);
});

/**
 * Steps are plain lines here and the runner mints the id and position. A
 * schema that let the model supply either would let it divide a checklist
 * from its replay -- the one property `step-id.js` exists to guarantee.
 */
test("the schema's steps are strings, at least one of them", () => {
  const schema = JSON.parse(readFileSync(`${repoRoot}${microtask.resultSchemaPath}`, "utf8"));
  assert.equal(schema.properties.steps.type, "array");
  assert.equal(schema.properties.steps.items.type, "string");
  assert.equal(schema.properties.steps.minItems, 1);
});

// --- validateArgs --------------------------------------------------------

test("validateArgs accepts a ref, with or without a grain", () => {
  assert.deepEqual(microtask.validateArgs({ ref: "HB-42" }), { ok: true });
  assert.deepEqual(microtask.validateArgs({ ref: ITEM.id, grain: 3 }), { ok: true });
});

test("validateArgs rejects a missing, empty or non-string ref -- a 400, never a failed model run", () => {
  for (const args of [{}, { ref: "" }, { ref: "  " }, { ref: 42 }]) {
    const result = microtask.validateArgs(args);
    assert.equal(result.ok, false);
    assert.match(result.error, /ref/);
  }
});

test("validateArgs rejects a grain outside SKILL.md's 1-3 scale", () => {
  for (const grain of [0, 4, "2", 2.5]) {
    assert.equal(microtask.validateArgs({ ref: "HB-42", grain }).ok, false);
  }
});

// --- prepare -------------------------------------------------------------

test("prepare resolves HB-<seq> off the sweep and carries the item's live steps in order", async () => {
  const prepared = await microtask.prepare(
    { ref: "hb-42" },
    { authority: fakeAuthority(), onProgress: noProgress },
  );
  assert.equal(prepared.ok, true);
  assert.equal(prepared.args.item.id, ITEM.id);
  // Soft-deleted rows are dropped, another item's are not ours, and
  // position orders what is left.
  assert.deepEqual(
    prepared.args.steps.map((step) => step.id),
    ["s-1", "s-2"],
  );
});

test("prepare resolves a bare uuid too, and defaults the grain to 2", async () => {
  const prepared = await microtask.prepare(
    { ref: ITEM.id },
    { authority: fakeAuthority(), onProgress: noProgress },
  );
  assert.equal(prepared.args.item.id, ITEM.id);
  assert.equal(prepared.args.grain, 2);
});

test("prepare keeps an explicit grain", async () => {
  const prepared = await microtask.prepare(
    { ref: ITEM.id, grain: 3 },
    { authority: fakeAuthority(), onProgress: noProgress },
  );
  assert.equal(prepared.args.grain, 3);
});

/** An unknown ref is a named failure, never an empty answer written against something else. */
test("prepare names an unknown ref rather than falling back to another item", async () => {
  const prepared = await microtask.prepare(
    { ref: "HB-999" },
    { authority: fakeAuthority(), onProgress: noProgress },
  );
  assert.equal(prepared.ok, false);
  assert.match(prepared.error, /no item HB-999 in the sweep/);
});

test("prepare passes an authority failure through in the authority's own words", async () => {
  const prepared = await microtask.prepare(
    { ref: "HB-42" },
    {
      authority: fakeAuthority({ sweep: { ok: false, error: "no authority token configured" } }),
      onProgress: noProgress,
    },
  );
  assert.equal(prepared.ok, false);
  assert.match(prepared.error, /no authority token configured/);
});

// --- buildPrompt ---------------------------------------------------------

test("buildPrompt invokes the slash command, names the runner arm, and carries the item JSON", () => {
  const prompt = microtask.buildPrompt({ item: ITEM, steps: [], grain: 2 });
  assert.match(prompt, /^\/microtask\b/);
  assert.match(prompt, /no shell/);
  assert.match(prompt, /do not run\s*\n?scripts\/hb\.sh/);
  assert.ok(prompt.includes(JSON.stringify({ item: ITEM, steps: [], grain: 2 })));
});

// --- apply ---------------------------------------------------------------

test("apply appends each step at a contiguous position after the live maximum", async () => {
  const authority = fakeAuthority();
  const result = { steps: ["put on music", "grab a trash bag"], note: "" };
  const prepared = await microtask.prepare({ ref: "HB-42" }, { authority, onProgress: noProgress });

  const applied = await microtask.apply(result, {
    args: prepared.args,
    authority,
    onProgress: noProgress,
  });

  assert.deepEqual(applied, { ok: true, result });
  assert.deepEqual(authority.writes, [
    {
      id: stepId(ITEM.id, "put on music"),
      item_id: ITEM.id,
      body: "put on music",
      position: 3,
    },
    {
      id: stepId(ITEM.id, "grab a trash bag"),
      item_id: ITEM.id,
      body: "grab a trash bag",
      position: 4,
    },
  ]);
});

test("apply starts at position 1 for an item with no steps yet", async () => {
  const authority = fakeAuthority();
  await microtask.apply(
    { steps: ["only step"], note: "" },
    { args: { item: ITEM, steps: [], grain: 2 }, authority, onProgress: noProgress },
  );
  assert.equal(authority.writes[0].position, 1);
});

/**
 * The idempotence the issue asks for, end to end at this seam: the same
 * request twice writes the same ids, and the authority's already-exists
 * path (200) is success rather than a duplicate row.
 */
test("a repeated identical run writes the identical ids and reports no failure", async () => {
  const first = fakeAuthority();
  const args = { item: ITEM, steps: [], grain: 2 };
  const result = { steps: ["put on music", "grab a trash bag"], note: "" };
  await microtask.apply(result, { args, authority: first, onProgress: noProgress });

  const replay = fakeAuthority({ createStep: (step) => ({ ok: true, created: false, step }) });
  const applied = await microtask.apply(result, { args, authority: replay, onProgress: noProgress });

  assert.equal(applied.ok, true);
  assert.deepEqual(
    replay.writes.map((step) => step.id),
    first.writes.map((step) => step.id),
  );
});

test("apply reports what it wrote on the progress stream, replays included", async () => {
  const messages = [];
  await microtask.apply(
    { steps: ["a", "b"], note: "" },
    {
      args: { item: ITEM, steps: [], grain: 2 },
      authority: fakeAuthority({
        createStep: (step, count) => ({ ok: true, created: count === 1, step }),
      }),
      onProgress: (message) => messages.push(message),
    },
  );
  assert.deepEqual(messages, ["wrote step 1/2", "wrote step 2/2", "1 step written, 1 already existed"]);
});

test("a failed write is an envelope error, and stops rather than grinding through the rest", async () => {
  const authority = fakeAuthority({
    createStep: (step, count) =>
      count === 1 ? { ok: true, created: true, step } : { ok: false, error: "POST /api/steps answered 500" },
  });
  const applied = await microtask.apply(
    { steps: ["a", "b", "c"], note: "" },
    { args: { item: ITEM, steps: [], grain: 2 }, authority, onProgress: noProgress },
  );
  assert.equal(applied.ok, false);
  assert.match(applied.error, /answered 500/);
  assert.equal(authority.writes.length, 2);
});

/** `ok:true` means the checklist landed, not that a model answered. */
test("a result with no usable steps is a named failure, not a zero-write success", async () => {
  const authority = fakeAuthority();
  for (const result of [{ steps: [], note: "" }, { steps: ["  "], note: "" }, { note: "" }]) {
    const applied = await microtask.apply(result, {
      args: { item: ITEM, steps: [], grain: 2 },
      authority,
      onProgress: noProgress,
    });
    assert.equal(applied.ok, false);
    assert.match(applied.error, /no steps/);
  }
  assert.equal(authority.writes.length, 0);
});
