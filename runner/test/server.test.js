import { test } from "node:test";
import assert from "node:assert/strict";
import { EventEmitter } from "node:events";
import { createServer } from "../src/server.js";

function fakeSpawnEmitting({ stdout = "", stderr = "", code = 0 } = {}) {
  return () => {
    const child = new EventEmitter();
    child.stdout = new EventEmitter();
    child.stderr = new EventEmitter();
    setImmediate(() => {
      if (stdout) child.stdout.emit("data", Buffer.from(stdout));
      if (stderr) child.stderr.emit("data", Buffer.from(stderr));
      child.emit("close", code);
    });
    return child;
  };
}

/** One live-shaped `--output-format json` envelope, trimmed to what this runner reads. */
function cliEnvelope(structuredOutput) {
  return JSON.stringify({
    is_error: false,
    subtype: "success",
    result: JSON.stringify(structuredOutput),
    structured_output: structuredOutput,
    type: "result",
  });
}

async function withServer(opts, run) {
  const server = createServer({
    bearerToken: "test-token",
    repoRoot: "/app",
    spawn: fakeSpawnEmitting(opts),
    // `repoRoot` is fictional here, so the schema read is faked too --
    // this suite is about the HTTP contract, not the filesystem.
    readSchema: () => '{"type":"object"}',
    heartbeatIntervalMs: 10_000,
  });
  await new Promise((resolve) => server.listen(0, resolve));
  const { port } = server.address();
  try {
    await run(`http://127.0.0.1:${port}`);
  } finally {
    // `closeAllConnections` first: `close` alone waits out the keep-alive
    // timeout on any socket the client left open, which is seconds of dead
    // time per test, not a signal about the server.
    server.closeAllConnections();
    await new Promise((resolve) => server.close(resolve));
  }
}

test("401s a request with no bearer token", async () => {
  await withServer({}, async (base) => {
    const res = await fetch(`${base}/run`, {
      method: "POST",
      body: JSON.stringify({ skill: "parse-capture", args: { text: "x" } }),
    });
    assert.equal(res.status, 401);
  });
});

test("401s a request with the wrong bearer token", async () => {
  await withServer({}, async (base) => {
    const res = await fetch(`${base}/run`, {
      method: "POST",
      headers: { authorization: "Bearer nope" },
      body: JSON.stringify({ skill: "parse-capture", args: { text: "x" } }),
    });
    assert.equal(res.status, 401);
  });
});

test("404s a request to any other path or method", async () => {
  await withServer({}, async (base) => {
    const res = await fetch(`${base}/other`, { method: "POST" });
    assert.equal(res.status, 404);
    const res2 = await fetch(`${base}/run`, { method: "GET" });
    assert.equal(res2.status, 404);
  });
});

test("400s a malformed body without touching the skill registry", async () => {
  await withServer({}, async (base) => {
    const res = await fetch(`${base}/run`, {
      method: "POST",
      headers: { authorization: "Bearer test-token" },
      body: "not json",
    });
    assert.equal(res.status, 400);
    const body = await res.json();
    assert.equal(body.ok, false);
    assert.equal(body.skill, null);
  });
});

test("400s an unknown skill name", async () => {
  await withServer({}, async (base) => {
    const res = await fetch(`${base}/run`, {
      method: "POST",
      headers: { authorization: "Bearer test-token" },
      body: JSON.stringify({ skill: "bogus", args: {} }),
    });
    assert.equal(res.status, 400);
    const body = await res.json();
    assert.equal(body.ok, false);
    assert.match(body.error, /unknown skill/);
  });
});

test("400s invalid args for a known skill", async () => {
  await withServer({}, async (base) => {
    const res = await fetch(`${base}/run`, {
      method: "POST",
      headers: { authorization: "Bearer test-token" },
      body: JSON.stringify({ skill: "parse-capture", args: {} }),
    });
    assert.equal(res.status, 400);
    const body = await res.json();
    assert.equal(body.ok, false);
    assert.equal(body.skill, "parse-capture");
  });
});

test("200s a valid request and streams NDJSON ending in the ok envelope", async () => {
  await withServer({ stdout: cliEnvelope({ title: "buy milk", notes: "" }), code: 0 }, async (base) => {
    const res = await fetch(`${base}/run`, {
      method: "POST",
      headers: { authorization: "Bearer test-token" },
      body: JSON.stringify({ skill: "parse-capture", args: { text: "buy milk" } }),
    });
    assert.equal(res.status, 200);
    assert.equal(res.headers.get("content-type"), "application/x-ndjson");

    const text = await res.text();
    const lines = text.trim().split("\n").map((l) => JSON.parse(l));
    const final = lines[lines.length - 1];
    assert.deepEqual(final, {
      ok: true,
      skill: "parse-capture",
      result: { title: "buy milk", notes: "" },
    });
    // at least one progress line preceded it
    assert.ok(lines.length >= 2);
    assert.equal(lines[0].type, "progress");
  });
});

test("413s an oversized body -- the client reads the rejection, never a socket reset", async () => {
  await withServer({}, async (base) => {
    const res = await fetch(`${base}/run`, {
      method: "POST",
      headers: { authorization: "Bearer test-token" },
      body: JSON.stringify({ skill: "parse-capture", args: { text: "x".repeat(2_000_000) } }),
    });
    // The assertion that matters is that this line is reached at all:
    // `req.destroy()` here tore down the socket under the response and
    // `fetch` threw UND_ERR_SOCKET before any status existed.
    assert.equal(res.status, 413);
    const body = await res.json();
    assert.equal(body.ok, false);
    assert.equal(body.skill, null);
    assert.match(body.error, /too large/);
  });
});

test("a failed claude run still ends the stream in an ok:false envelope, HTTP 200", async () => {
  await withServer({ stderr: "boom", code: 1 }, async (base) => {
    const res = await fetch(`${base}/run`, {
      method: "POST",
      headers: { authorization: "Bearer test-token" },
      body: JSON.stringify({ skill: "parse-capture", args: { text: "buy milk" } }),
    });
    assert.equal(res.status, 200);
    const text = await res.text();
    const lines = text.trim().split("\n").map((l) => JSON.parse(l));
    const final = lines[lines.length - 1];
    assert.equal(final.ok, false);
    assert.equal(final.skill, "parse-capture");
    assert.match(final.error, /boom/);
  });
});

// --- the prepare hook and the spawn's cwd --------------------------------

/** Like `withServer`, but records every `spawn` call so argv and options are assertable. */
async function withRecordingServer({ runRanker, authority, stdout = "", code = 0 }, run) {
  const calls = [];
  const spawn = (command, args, options) => {
    calls.push({ command, args, options });
    const child = new EventEmitter();
    child.stdout = new EventEmitter();
    child.stderr = new EventEmitter();
    setImmediate(() => {
      if (stdout) child.stdout.emit("data", Buffer.from(stdout));
      child.emit("close", code);
    });
    return child;
  };
  const server = createServer({
    bearerToken: "test-token",
    repoRoot: "/app",
    spawn,
    readSchema: () => '{"type":"object"}',
    heartbeatIntervalMs: 10_000,
    runRanker,
    ...(authority ? { authority } : {}),
  });
  await new Promise((resolve) => server.listen(0, resolve));
  const { port } = server.address();
  try {
    await run(`http://127.0.0.1:${port}`, calls);
  } finally {
    server.closeAllConnections();
    await new Promise((resolve) => server.close(resolve));
  }
}

function post(base, body) {
  return fetch(`${base}/run`, {
    method: "POST",
    headers: { authorization: "Bearer test-token" },
    body: JSON.stringify(body),
  });
}

const nextUpArgs = {
  sweep: { version: 1, items: [], projects: [], fog: [], blocked_by: [] },
  now: { local: "2026-08-11T09:53", epoch_ms: 1786553580000 },
};

/**
 * The CLI resolves a slash command against `<cwd>/.claude/skills`. The
 * image's final `WORKDIR` is `/app/runner` while the skills are baked at
 * `/app/.claude/skills`, so an inherited cwd would leave every op's prompt
 * unresolved -- and unresolved fails *softly*, as prose that still answers
 * the schema with none of SKILL.md's rules applied. Only the spawn options
 * can state this; no output would.
 */
test("claude is spawned in the repo root, so its slash command resolves", async () => {
  const answer = cliEnvelope({ title: "t", notes: "" });
  await withRecordingServer({ stdout: answer }, async (base, calls) => {
    await post(base, { skill: "parse-capture", args: { text: "t" } }).then((r) => r.text());
    assert.equal(calls[0].command, "claude");
    assert.equal(calls[0].options?.cwd, "/app");
  });
});

test("a skill declaring prepare runs it before claude, and the prompt carries its output", async () => {
  const ranked = { candidates: [], health: { triage: 0 } };
  const answer = cliEnvelope({ pick: null, alternates: [], health: {} });
  await withRecordingServer(
    { runRanker: async () => ({ ok: true, ranked }), stdout: answer },
    async (base, calls) => {
      const res = await post(base, { skill: "next-up-hb", args: nextUpArgs });
      const lines = (await res.text()).trim().split("\n").map((l) => JSON.parse(l));
      assert.equal(lines[lines.length - 1].ok, true);

      const prompt = calls[0].args[1];
      assert.ok(prompt.includes(JSON.stringify(ranked)));
      // The raw sweep was dropped on the way through -- see `prepare`.
      assert.ok(!prompt.includes('"blocked_by"'));
    },
  );
});

/**
 * The whole reason `prepare` exists: a deterministic failure is reported in
 * the ranker's own words, before a single model token is spent. The
 * assertion that matters is `calls.length === 0`.
 */
test("a failed prepare ends the stream in the envelope without spawning claude at all", async () => {
  await withRecordingServer(
    { runRanker: async () => ({ ok: false, error: "next-up-rank did not answer JSON" }) },
    async (base, calls) => {
      const res = await post(base, { skill: "next-up-hb", args: nextUpArgs });
      assert.equal(res.status, 200);
      const lines = (await res.text()).trim().split("\n").map((l) => JSON.parse(l));
      const final = lines[lines.length - 1];
      assert.equal(final.ok, false);
      assert.equal(final.skill, "next-up-hb");
      assert.match(final.error, /ranker: next-up-rank did not answer JSON/);
      assert.equal(calls.length, 0);
    },
  );
});

test("a prepare that throws is an envelope error, never a dropped connection", async () => {
  await withRecordingServer(
    {
      runRanker: async () => {
        throw new Error("unexpected");
      },
    },
    async (base) => {
      const res = await post(base, { skill: "next-up-hb", args: nextUpArgs });
      const lines = (await res.text()).trim().split("\n").map((l) => JSON.parse(l));
      assert.equal(lines[lines.length - 1].ok, false);
      assert.match(lines[lines.length - 1].error, /unexpected/);
    },
  );
});

// --- the apply hook: the write half, after the model (#272) --------------

const ITEM = { id: "11111111-2222-4333-8444-555555555555", seq: 42, title: "clear the garage" };

function fakeAuthority({ createStep } = {}) {
  const writes = [];
  return {
    writes,
    sweep: async () => ({ ok: true, sweep: { items: [ITEM], steps: [] } }),
    createStep: async (step) => {
      writes.push(step);
      return createStep ? createStep(step) : { ok: true, created: true, step };
    },
  };
}

test("a skill declaring apply writes after the model, and the envelope carries the schema result", async () => {
  const result = { steps: ["put on music", "grab a trash bag"], note: "" };
  const authority = fakeAuthority();
  await withRecordingServer({ authority, stdout: cliEnvelope(result) }, async (base, calls) => {
    const res = await post(base, { skill: "microtask", args: { ref: "HB-42" } });
    assert.equal(res.status, 200);
    const lines = (await res.text()).trim().split("\n").map((l) => JSON.parse(l));
    assert.deepEqual(lines[lines.length - 1], { ok: true, skill: "microtask", result });
    // The item rode in the prompt, and the steps landed against it.
    assert.ok(calls[0].args[1].includes(ITEM.id));
    assert.deepEqual(
      authority.writes.map((step) => [step.item_id, step.body, step.position]),
      [
        [ITEM.id, "put on music", 1],
        [ITEM.id, "grab a trash bag", 2],
      ],
    );
  });
});

/**
 * The run is not "ok" because a model answered -- it is ok because the
 * answer landed. A write that failed has to reach the caller as an
 * `ok:false` envelope at HTTP 200, like every other failure in this
 * contract.
 */
test("a failed write ends the stream in an ok:false envelope, HTTP 200", async () => {
  const authority = fakeAuthority({ createStep: () => ({ ok: false, error: "POST /api/steps answered 500" }) });
  await withRecordingServer(
    { authority, stdout: cliEnvelope({ steps: ["put on music"], note: "" }) },
    async (base) => {
      const res = await post(base, { skill: "microtask", args: { ref: "HB-42" } });
      assert.equal(res.status, 200);
      const lines = (await res.text()).trim().split("\n").map((l) => JSON.parse(l));
      const final = lines[lines.length - 1];
      assert.equal(final.ok, false);
      assert.equal(final.skill, "microtask");
      assert.match(final.error, /answered 500/);
    },
  );
});

/**
 * The default authority is the "not configured" one, so a runner started
 * without `HB_API_TOKEN` declines this op by name -- before the model runs,
 * and without taking the other two ops down at boot.
 */
test("with no authority configured, microtask declines before claude is spawned", async () => {
  await withRecordingServer({}, async (base, calls) => {
    const res = await post(base, { skill: "microtask", args: { ref: "HB-42" } });
    assert.equal(res.status, 200);
    const lines = (await res.text()).trim().split("\n").map((l) => JSON.parse(l));
    const final = lines[lines.length - 1];
    assert.equal(final.ok, false);
    assert.match(final.error, /HB_API_TOKEN/);
    assert.equal(calls.length, 0);
  });
});

test("400s a microtask request with no item reference, without spawning anything", async () => {
  await withRecordingServer({ authority: fakeAuthority() }, async (base, calls) => {
    const res = await post(base, { skill: "microtask", args: {} });
    assert.equal(res.status, 400);
    const body = await res.json();
    assert.equal(body.ok, false);
    assert.equal(body.skill, "microtask");
    assert.match(body.error, /ref/);
    assert.equal(calls.length, 0);
  });
});
