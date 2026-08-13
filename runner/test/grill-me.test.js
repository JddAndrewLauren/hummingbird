import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { grillMe, pastOutcomes, PROVISIONAL_TURN_CAP } from "../src/skills/grill-me.js";

const repoRoot = fileURLToPath(new URL("../..", import.meta.url));

const ITEM = { id: "11111111-2222-4333-8444-555555555555", seq: 42, title: "book the flight" };
const OTHER_ITEM = { id: "other", seq: 7, title: "something else" };

function sweepFor({ grills } = {}) {
  const sweep = { items: [ITEM, OTHER_ITEM], steps: [] };
  if (grills !== undefined) sweep.grills = grills;
  return sweep;
}

const QUESTION = {
  prompt: "which airport?",
  recommendedAnswer: "SFO",
  choices: ["SFO", "OAK", "SJC"],
};

function turn(overrides = {}) {
  return { question: QUESTION, answer: "SFO", ...overrides };
}

/** An authority whose writes throw -- proof grill-me never reaches for one. */
function fakeAuthority({ sweep = { ok: true, sweep: sweepFor() } } = {}) {
  return {
    sweep: async () => sweep,
    createStep: async () => {
      throw new Error("grill-me must never call createStep");
    },
    dropStep: async () => {
      throw new Error("grill-me must never call dropStep");
    },
    moveStep: async () => {
      throw new Error("grill-me must never call moveStep");
    },
  };
}

const noProgress = () => {};

// --- the shipped schema --------------------------------------------------

test("name and resultSchemaPath match the skill directory / slash command", () => {
  assert.equal(grillMe.name, "grill-me");
  assert.equal(grillMe.resultSchemaPath, ".claude/skills/grill-me/schema.json");
});

test("the shipped schema file exists, is valid JSON, and carries no $schema key", () => {
  const schema = JSON.parse(readFileSync(`${repoRoot}${grillMe.resultSchemaPath}`, "utf8"));
  assert.equal(schema.$schema, undefined);
  assert.ok(Array.isArray(schema.oneOf));
  assert.equal(schema.oneOf.length, 2);
});

test("the schema's two branches are the closed question / proposal shapes", () => {
  const schema = JSON.parse(readFileSync(`${repoRoot}${grillMe.resultSchemaPath}`, "utf8"));
  const [question, proposal] = schema.oneOf;

  assert.deepEqual(question.required, ["kind", "question"]);
  assert.equal(question.additionalProperties, false);
  assert.deepEqual(question.properties.kind.enum, ["question"]);
  assert.equal(question.properties.question.additionalProperties, false);
  assert.deepEqual(question.properties.question.required, ["prompt", "recommendedAnswer", "choices"]);
  assert.equal(question.properties.question.properties.choices.minItems, 2);
  assert.equal(question.properties.question.properties.choices.maxItems, 4);

  assert.deepEqual(proposal.required, ["kind", "proposal"]);
  assert.equal(proposal.additionalProperties, false);
  assert.deepEqual(proposal.properties.kind.enum, ["proposal"]);
  assert.equal(proposal.properties.proposal.additionalProperties, false);
  assert.deepEqual(proposal.properties.proposal.required, ["summary", "verdict", "patch"]);
  assert.deepEqual(proposal.properties.proposal.properties.verdict.enum, ["resolved", "fog_remains"]);
});

// --- validateArgs ----------------------------------------------------------

test("validateArgs accepts a ref with an empty turns array -- the opening request", () => {
  assert.deepEqual(grillMe.validateArgs({ ref: "HB-42", turns: [] }), { ok: true });
});

test("validateArgs accepts a ref with prior well-shaped turns", () => {
  assert.deepEqual(grillMe.validateArgs({ ref: ITEM.id, turns: [turn(), turn({ answer: "OAK" })] }), {
    ok: true,
  });
});

test("validateArgs rejects a missing, empty or non-string ref", () => {
  for (const args of [{ turns: [] }, { ref: "", turns: [] }, { ref: "  ", turns: [] }, { ref: 42, turns: [] }]) {
    const result = grillMe.validateArgs(args);
    assert.equal(result.ok, false);
    assert.match(result.error, /ref/);
  }
});

test("validateArgs rejects a missing or non-array turns", () => {
  for (const args of [{ ref: "HB-42" }, { ref: "HB-42", turns: {} }, { ref: "HB-42", turns: "none" }]) {
    const result = grillMe.validateArgs(args);
    assert.equal(result.ok, false);
    assert.match(result.error, /turns/);
  }
});

// "malformed model output" threaded back by the caller: a previous turn
// whose question does not match the schema's own shape must be rejected
// here, before a model token is spent trying to make sense of it.
test("validateArgs rejects a malformed prior turn -- missing question fields", () => {
  const badQuestion = grillMe.validateArgs({
    ref: "HB-42",
    turns: [{ question: { prompt: "which airport?" }, answer: "SFO" }],
  });
  assert.equal(badQuestion.ok, false);
  assert.match(badQuestion.error, /turns\[0\]/);
});

test("validateArgs rejects a prior turn whose choices are out of the 2-4 range", () => {
  const tooFew = grillMe.validateArgs({
    ref: "HB-42",
    turns: [turn({ question: { ...QUESTION, choices: ["SFO"] } })],
  });
  assert.equal(tooFew.ok, false);

  const tooMany = grillMe.validateArgs({
    ref: "HB-42",
    turns: [turn({ question: { ...QUESTION, choices: ["a", "b", "c", "d", "e"] } })],
  });
  assert.equal(tooMany.ok, false);
});

test("validateArgs rejects a prior turn with a missing or empty answer -- free text is still required to be present", () => {
  const missing = grillMe.validateArgs({ ref: "HB-42", turns: [{ question: QUESTION }] });
  assert.equal(missing.ok, false);

  const empty = grillMe.validateArgs({ ref: "HB-42", turns: [turn({ answer: "  " })] });
  assert.equal(empty.ok, false);
});

test("validateArgs rejects a model arg that fails the charset rule", () => {
  const result = grillMe.validateArgs({ ref: "HB-42", turns: [], model: "--dangerously-skip-permissions" });
  assert.equal(result.ok, false);
  assert.match(result.error, /model/);
});

test("validateArgs accepts a well-formed model id", () => {
  assert.deepEqual(grillMe.validateArgs({ ref: "HB-42", turns: [], model: "sonnet" }), { ok: true });
});

// --- prepare: the read half, run before the model ---------------------------

test("prepare resolves the ref and carries the item and turns forward", async () => {
  const step = await grillMe.prepare(
    { ref: "HB-42", turns: [] },
    { authority: fakeAuthority(), onProgress: noProgress },
  );
  assert.equal(step.ok, true);
  assert.deepEqual(step.args.item, ITEM);
  assert.deepEqual(step.args.turns, []);
});

test("prepare resolves a bare uuid the same way as an HB-<seq> ref", async () => {
  const step = await grillMe.prepare(
    { ref: ITEM.id, turns: [] },
    { authority: fakeAuthority(), onProgress: noProgress },
  );
  assert.equal(step.ok, true);
  assert.deepEqual(step.args.item, ITEM);
});

test("an unknown ref is a named failure, not an empty answer written against something else", async () => {
  const step = await grillMe.prepare(
    { ref: "HB-999", turns: [] },
    { authority: fakeAuthority(), onProgress: noProgress },
  );
  assert.equal(step.ok, false);
  assert.match(step.error, /HB-999/);
});

test("a missing authority token ends the stream at prepare, in the authority's own words", async () => {
  const authority = {
    sweep: async () => ({ ok: false, error: "no authority token configured -- set HB_API_TOKEN" }),
  };
  const step = await grillMe.prepare({ ref: "HB-42", turns: [] }, { authority, onProgress: noProgress });
  assert.equal(step.ok, false);
  assert.match(step.error, /authority:/);
  assert.match(step.error, /HB_API_TOKEN/);
});

test("an unreachable authority is a named failure, attributed to the authority", async () => {
  const authority = {
    sweep: async () => ({ ok: false, error: "GET /api/sweep could not reach the authority: ENOTFOUND" }),
  };
  const step = await grillMe.prepare({ ref: "HB-42", turns: [] }, { authority, onProgress: noProgress });
  assert.equal(step.ok, false);
  assert.match(step.error, /authority:/);
  assert.match(step.error, /ENOTFOUND/);
});

// --- the turn cap ------------------------------------------------------------

test("a request under the turn cap proceeds", async () => {
  const turns = Array.from({ length: PROVISIONAL_TURN_CAP - 1 }, () => turn());
  const step = await grillMe.prepare(
    { ref: "HB-42", turns },
    { authority: fakeAuthority(), onProgress: noProgress },
  );
  assert.equal(step.ok, true);
});

test("a request at or past the turn cap declines at prepare, naming the cap, with no model token spent", async () => {
  const turns = Array.from({ length: PROVISIONAL_TURN_CAP }, () => turn());
  const step = await grillMe.prepare(
    { ref: "HB-42", turns },
    { authority: fakeAuthority(), onProgress: noProgress },
  );
  assert.equal(step.ok, false);
  assert.match(step.error, new RegExp(`${PROVISIONAL_TURN_CAP}-turn cap`));
});

// --- stateless turn reconstruction / the prompt -----------------------------

test("buildPrompt invokes /grill-me and carries the item, turns and prior outcomes verbatim", () => {
  const args = { item: ITEM, turns: [turn()], priorOutcomes: [] };
  const prompt = grillMe.buildPrompt(args);
  assert.match(prompt, /^\/grill-me\b/);
  assert.ok(prompt.includes(JSON.stringify({ item: ITEM, priorOutcomes: [], turns: [turn()] })));
});

test("the prompt tells the model it has no shell and to ask one thing at a time", () => {
  const prompt = grillMe.buildPrompt({ item: ITEM, turns: [], priorOutcomes: [] });
  assert.match(prompt, /no shell/);
  assert.match(prompt, /one thing/i);
});

test("every turn threaded in reaches the prompt -- the caller's whole conversation, not a summary of it", () => {
  const turns = [turn({ answer: "SFO" }), turn({ question: { ...QUESTION, prompt: "which dates?" }, answer: "next week" })];
  const prompt = grillMe.buildPrompt({ item: ITEM, turns, priorOutcomes: [] });
  assert.ok(prompt.includes("which dates?"));
  assert.ok(prompt.includes("next week"));
});

// --- prior applied outcomes vs. prior transcripts (#349 decision 3) --------

test("pastOutcomes is empty when the sweep carries no grills field at all -- today's production shape", () => {
  assert.deepEqual(pastOutcomes(sweepFor(), ITEM.id), []);
});

test("pastOutcomes filters to the named item only", () => {
  const grills = [
    { item_id: ITEM.id, summary: "picked SFO", verdict: "resolved", patch: { context: "@errands" } },
    { item_id: OTHER_ITEM.id, summary: "not this item", verdict: "resolved", patch: {} },
  ];
  assert.deepEqual(pastOutcomes(sweepFor({ grills }), ITEM.id), [
    { summary: "picked SFO", verdict: "resolved", patch: { context: "@errands" } },
  ]);
});

test("pastOutcomes drops a transcript field even if a future row carries one -- never forwarded to the model", () => {
  const grills = [
    {
      item_id: ITEM.id,
      summary: "picked SFO",
      verdict: "resolved",
      patch: {},
      transcript: [{ question: QUESTION, answer: "SFO" }],
    },
  ];
  const [outcome] = pastOutcomes(sweepFor({ grills }), ITEM.id);
  assert.equal(outcome.transcript, undefined);
  assert.deepEqual(Object.keys(outcome).sort(), ["patch", "summary", "verdict"]);
});

test("prepare's prepared args carry priorOutcomes through to what buildPrompt sees", async () => {
  const grills = [{ item_id: ITEM.id, summary: "picked SFO", verdict: "resolved", patch: {} }];
  const step = await grillMe.prepare(
    { ref: "HB-42", turns: [] },
    { authority: fakeAuthority({ sweep: { ok: true, sweep: sweepFor({ grills }) } }), onProgress: noProgress },
  );
  assert.equal(step.ok, true);
  assert.deepEqual(step.args.priorOutcomes, [{ summary: "picked SFO", verdict: "resolved", patch: {} }]);

  const prompt = grillMe.buildPrompt(step.args);
  assert.ok(prompt.includes("picked SFO"));
});

// --- the write anti-goal (#350's brief) -------------------------------------

test("grill-me declares no apply -- there is nothing for the pipeline to run after the model", () => {
  assert.equal(grillMe.apply, undefined);
});

test("prepare never calls a write method on the authority, even when one is offered", async () => {
  const step = await grillMe.prepare(
    { ref: "HB-42", turns: [] },
    { authority: fakeAuthority(), onProgress: noProgress },
  );
  // fakeAuthority's createStep/dropStep/moveStep all throw -- prepare
  // resolving at all, without an authority.sweep() failure, is the proof
  // none of them were ever called.
  assert.equal(step.ok, true);
});

test("no source line in grill-me.js names a write verb on the authority client", () => {
  const source = readFileSync(fileURLToPath(new URL("../src/skills/grill-me.js", import.meta.url)), "utf8");
  for (const verb of ["createStep", "dropStep", "moveStep"]) {
    assert.ok(!source.includes(verb), `grill-me.js must never reference authority.${verb}`);
  }
});
