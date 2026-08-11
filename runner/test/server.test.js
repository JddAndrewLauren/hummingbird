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
