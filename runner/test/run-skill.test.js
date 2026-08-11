import { test } from "node:test";
import assert from "node:assert/strict";
import { EventEmitter } from "node:events";
import { runSkill } from "../src/run-skill.js";

function fakeChild() {
  const child = new EventEmitter();
  child.stdout = new EventEmitter();
  child.stderr = new EventEmitter();
  return child;
}

test("resolves ok:true with the parsed stdout JSON on a clean exit", async () => {
  const child = fakeChild();
  const spawn = () => child;

  const promise = runSkill({
    skillName: "parse-capture",
    prompt: "/parse-capture\n\nbuy milk",
    schemaPath: "/app/schema.json",
    spawn,
    onProgress: () => {},
  });

  child.stdout.emit("data", Buffer.from('{"title":"buy milk","notes":""}'));
  child.emit("close", 0);

  const outcome = await promise;
  assert.deepEqual(outcome, { ok: true, result: { title: "buy milk", notes: "" } });
});

test("resolves ok:false when claude exits non-zero, carrying stderr as the error", async () => {
  const child = fakeChild();
  const spawn = () => child;

  const promise = runSkill({
    skillName: "parse-capture",
    prompt: "p",
    schemaPath: "/app/schema.json",
    spawn,
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
    onProgress: (message) => seen.push(message),
  });

  child.stdout.emit("data", Buffer.from('{"title":"t","notes":"n"}'));
  child.emit("close", 0);
  await promise;

  assert.ok(seen.length >= 1);
  assert.match(seen[0], /parse-capture/);
});
