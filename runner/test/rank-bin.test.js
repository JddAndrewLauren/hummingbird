import { test } from "node:test";
import assert from "node:assert/strict";
import { EventEmitter } from "node:events";
import { createRankRunner } from "../src/rank-bin.js";

/**
 * A `next-up-rank` stand-in. Records what it was spawned with and what was
 * written to its stdin, so the envelope-on-stdin contract is asserted
 * rather than assumed -- the ranker reads stdin and nothing else, and a
 * runner that passed the envelope as argv would fail only against the real
 * binary.
 */
function fakeRanker({ stdout = "", stderr = "", code = 0, emit = true } = {}) {
  const calls = [];
  const spawn = (command, args) => {
    const child = new EventEmitter();
    child.stdout = new EventEmitter();
    child.stderr = new EventEmitter();
    const call = { command, args, stdin: "", killed: false };
    calls.push(call);
    child.stdin = new EventEmitter();
    child.stdin.end = (chunk) => {
      call.stdin += chunk;
    };
    child.kill = () => {
      call.killed = true;
    };
    if (emit) {
      setImmediate(() => {
        if (stdout) child.stdout.emit("data", Buffer.from(stdout));
        if (stderr) child.stderr.emit("data", Buffer.from(stderr));
        child.emit("close", code);
      });
    }
    return child;
  };
  return { spawn, calls };
}

const envelope = { sweep: { items: [] }, now: { local: "2026-08-11T09:53", epoch_ms: 1 } };

test("spawns the configured binary and writes the envelope to its stdin as JSON", async () => {
  const { spawn, calls } = fakeRanker({ stdout: '{"candidates":[],"health":{}}' });
  const run = createRankRunner({ spawn, bin: "/usr/local/bin/next-up-rank" });

  const outcome = await run(envelope);

  assert.deepEqual(outcome, { ok: true, ranked: { candidates: [], health: {} } });
  assert.equal(calls[0].command, "/usr/local/bin/next-up-rank");
  // No argv at all: the envelope is stdin's job. A payload on the command
  // line would blow ARG_MAX on a real sweep long before it was wrong.
  assert.deepEqual(calls[0].args, []);
  assert.deepEqual(JSON.parse(calls[0].stdin), envelope);
});

test("a non-zero exit carries the ranker's own stderr, not a bare exit code", async () => {
  // The ranker names its envelope problems itself (`EnvelopeProblem`), and
  // that wording is the actionable half of the failure.
  const { spawn } = fakeRanker({ stderr: "calendar status/event mismatch\n", code: 1 });
  const outcome = await createRankRunner({ spawn })(envelope);
  assert.equal(outcome.ok, false);
  assert.match(outcome.error, /calendar status\/event mismatch/);
});

test("a non-zero exit with silent stderr still names the exit code", async () => {
  const { spawn } = fakeRanker({ code: 2 });
  const outcome = await createRankRunner({ spawn, bin: "next-up-rank" })(envelope);
  assert.equal(outcome.ok, false);
  assert.match(outcome.error, /next-up-rank exited with code 2/);
});

test("a clean exit that did not answer JSON is a failure, never an empty success", async () => {
  const { spawn } = fakeRanker({ stdout: "not json" });
  const outcome = await createRankRunner({ spawn })(envelope);
  assert.equal(outcome.ok, false);
  assert.match(outcome.error, /did not answer JSON/);
});

test("a missing binary (spawn ENOENT) is an outcome, not a throw", async () => {
  const spawn = () => {
    const child = new EventEmitter();
    child.stdout = new EventEmitter();
    child.stderr = new EventEmitter();
    child.stdin = new EventEmitter();
    child.stdin.end = () => {};
    setImmediate(() => child.emit("error", new Error("spawn next-up-rank ENOENT")));
    return child;
  };
  const outcome = await createRankRunner({ spawn })(envelope);
  assert.equal(outcome.ok, false);
  assert.match(outcome.error, /ENOENT/);
});

test("a synchronous spawn throw is an outcome too", async () => {
  const spawn = () => {
    throw new Error("EACCES");
  };
  const outcome = await createRankRunner({ spawn, bin: "next-up-rank" })(envelope);
  assert.equal(outcome.ok, false);
  assert.match(outcome.error, /could not spawn next-up-rank: EACCES/);
});

test("a ranker that never exits is killed and reported, not awaited forever", async () => {
  // `rank()` is a pure fold over one payload: slow is wedged, not busy, and
  // a hung child would otherwise hold the HTTP stream open until Fly's own
  // kill -- with no envelope line to explain it.
  const { spawn, calls } = fakeRanker({ emit: false });
  const outcome = await createRankRunner({ spawn, bin: "next-up-rank", timeoutMs: 20 })(envelope);
  assert.equal(outcome.ok, false);
  assert.match(outcome.error, /did not answer within 20ms/);
  assert.equal(calls[0].killed, true);
});

test("a late close after the timeout cannot re-resolve the promise", async () => {
  const child = new EventEmitter();
  child.stdout = new EventEmitter();
  child.stderr = new EventEmitter();
  child.stdin = new EventEmitter();
  child.stdin.end = () => {};
  child.kill = () => {};
  const run = createRankRunner({ spawn: () => child, timeoutMs: 10 });
  const outcome = await run(envelope);
  assert.equal(outcome.ok, false);
  // Settling twice would be an unhandled resolution, invisible here but a
  // double envelope line on the wire.
  child.stdout.emit("data", Buffer.from('{"candidates":[]}'));
  child.emit("close", 0);
  await new Promise((resolve) => setImmediate(resolve));
  assert.match(outcome.error, /did not answer within/);
});
