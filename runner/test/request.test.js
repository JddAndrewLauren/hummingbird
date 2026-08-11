import { test } from "node:test";
import assert from "node:assert/strict";
import { parseRunRequest } from "../src/request.js";

test("parses a well-formed {skill, args} body", () => {
  const result = parseRunRequest('{"skill":"parse-capture","args":{"text":"buy milk"}}');
  assert.deepEqual(result, { ok: true, skill: "parse-capture", args: { text: "buy milk" } });
});

test("rejects invalid JSON", () => {
  const result = parseRunRequest("not json");
  assert.equal(result.ok, false);
  assert.match(result.error, /invalid json/i);
});

test("rejects a body that isn't a JSON object", () => {
  const result = parseRunRequest("[1,2,3]");
  assert.equal(result.ok, false);
  assert.match(result.error, /object/i);
});

test("rejects a missing skill field", () => {
  const result = parseRunRequest('{"args":{}}');
  assert.equal(result.ok, false);
  assert.match(result.error, /skill/i);
});

test("rejects a non-string skill field", () => {
  const result = parseRunRequest('{"skill":5,"args":{}}');
  assert.equal(result.ok, false);
  assert.match(result.error, /skill/i);
});

test("rejects a missing args field", () => {
  const result = parseRunRequest('{"skill":"parse-capture"}');
  assert.equal(result.ok, false);
  assert.match(result.error, /args/i);
});

test("rejects an args field that isn't an object", () => {
  const result = parseRunRequest('{"skill":"parse-capture","args":"nope"}');
  assert.equal(result.ok, false);
  assert.match(result.error, /args/i);
});
