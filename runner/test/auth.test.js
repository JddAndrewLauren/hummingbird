import { test } from "node:test";
import assert from "node:assert/strict";
import { checkBearerToken } from "../src/auth.js";

test("accepts a header carrying exactly the expected token", () => {
  assert.equal(checkBearerToken("Bearer secret-token", "secret-token"), true);
});

test("rejects a missing Authorization header", () => {
  assert.equal(checkBearerToken(undefined, "secret-token"), false);
});

test("rejects a header with the wrong scheme", () => {
  assert.equal(checkBearerToken("Basic secret-token", "secret-token"), false);
});

test("rejects a header carrying the wrong token", () => {
  assert.equal(checkBearerToken("Bearer wrong", "secret-token"), false);
});

test("rejects a token of different length without throwing", () => {
  assert.equal(checkBearerToken("Bearer short", "a-much-longer-secret-token"), false);
});

test("rejects when no expected token is configured, even if the header is empty-string equal", () => {
  assert.equal(checkBearerToken("Bearer ", ""), false);
});
