import { test } from "node:test";
import assert from "node:assert/strict";
import { createAuthorityClient, unconfiguredAuthority } from "../src/authority.js";

/** A `fetch` that answers one canned response and records every call. */
function fakeFetch(responses) {
  const calls = [];
  const queue = Array.isArray(responses) ? [...responses] : [responses];
  const fetch = async (url, options) => {
    calls.push({ url, options });
    const next = queue.length > 1 ? queue.shift() : queue[0];
    if (next instanceof Error) throw next;
    return {
      status: next.status,
      text: async () => next.body,
    };
  };
  fetch.calls = calls;
  return fetch;
}

const SWEEP = JSON.stringify({ version: 1, items: [{ id: "i-1", seq: 42 }], steps: [] });

function client(fetch, overrides = {}) {
  return createAuthorityClient({
    fetch,
    baseUrl: "https://hb.example",
    token: "device-token",
    ...overrides,
  });
}

test("an empty token yields the unconfigured client, whose every call names the gap", async () => {
  const authority = createAuthorityClient({ fetch: fakeFetch({ status: 200, body: SWEEP }), baseUrl: "https://hb.example", token: "" });
  assert.equal(authority, unconfiguredAuthority);
  const read = await authority.sweep();
  assert.equal(read.ok, false);
  assert.match(read.error, /HB_API_TOKEN/);
  const write = await authority.createStep({ id: "s", item_id: "i", body: "b", position: 1 });
  assert.equal(write.ok, false);
});

test("sweep GETs /api/sweep with the token as a bearer header, never in the URL", async () => {
  const fetch = fakeFetch({ status: 200, body: SWEEP });
  const read = await client(fetch).sweep();
  assert.equal(read.ok, true);
  assert.deepEqual(read.sweep.items, [{ id: "i-1", seq: 42 }]);
  assert.equal(fetch.calls[0].url, "https://hb.example/api/sweep");
  assert.equal(fetch.calls[0].options.method, "GET");
  assert.equal(fetch.calls[0].options.headers.authorization, "Bearer device-token");
  assert.ok(!fetch.calls[0].url.includes("device-token"));
});

test("a trailing slash on the base URL does not become a double slash", async () => {
  const fetch = fakeFetch({ status: 200, body: SWEEP });
  await client(fetch, { baseUrl: "https://hb.example/" }).sweep();
  assert.equal(fetch.calls[0].url, "https://hb.example/api/sweep");
});

test("a non-200 sweep is a named outcome carrying the status, never a throw", async () => {
  const read = await client(fakeFetch({ status: 401, body: "unauthorized" })).sweep();
  assert.equal(read.ok, false);
  assert.match(read.error, /answered 401/);
});

/**
 * The SPA shell answers `200 text/html` on an unmatched path, so a 200 is
 * not on its own proof the API was reached -- the trap `smoke-prod.sh` and
 * `hb.sh` both guard against.
 */
test("a 200 that is not JSON is a named outcome, not an empty sweep", async () => {
  const read = await client(fakeFetch({ status: 200, body: "<!doctype html>" })).sweep();
  assert.equal(read.ok, false);
  assert.match(read.error, /non-JSON/);
});

test("a 200 of the wrong shape is rejected as not a sweep payload", async () => {
  const read = await client(fakeFetch({ status: 200, body: '{"items":[]}' })).sweep();
  assert.equal(read.ok, false);
  assert.match(read.error, /did not answer a sweep payload/);
});

test("an unreachable authority is a named outcome -- the caller must still end in the envelope", async () => {
  const read = await client(fakeFetch(new Error("getaddrinfo ENOTFOUND hb.example"))).sweep();
  assert.equal(read.ok, false);
  assert.match(read.error, /could not reach the authority/);
  assert.match(read.error, /ENOTFOUND/);
});

test("createStep POSTs the step body as JSON and reads 201 as a create", async () => {
  const fetch = fakeFetch({ status: 201, body: '{"id":"s-1","version":7}' });
  const step = { id: "s-1", item_id: "i-1", body: "put on music", position: 3 };
  const write = await client(fetch).createStep(step);
  assert.deepEqual(write, { ok: true, created: true, step: { id: "s-1", version: 7 } });
  assert.equal(fetch.calls[0].url, "https://hb.example/api/steps");
  assert.equal(fetch.calls[0].options.method, "POST");
  assert.equal(fetch.calls[0].options.headers["content-type"], "application/json");
  assert.deepEqual(JSON.parse(fetch.calls[0].options.body), step);
});

/** Idempotence, the authority's half: already-exists is success and returns the stored row. */
test("createStep reads 200 as a replay, not a duplicate and not a failure", async () => {
  const write = await client(fakeFetch({ status: 200, body: '{"id":"s-1"}' })).createStep({
    id: "s-1",
    item_id: "i-1",
    body: "put on music",
    position: 3,
  });
  assert.equal(write.ok, true);
  assert.equal(write.created, false);
});

test("a rejected create names the status and the step it was writing", async () => {
  const write = await client(fakeFetch({ status: 400, body: '{"error":"unknown item_id"}' })).createStep({
    id: "s-1",
    item_id: "ghost",
    body: "put on music",
    position: 1,
  });
  assert.equal(write.ok, false);
  assert.match(write.error, /answered 400/);
  assert.match(write.error, /put on music/);
});
