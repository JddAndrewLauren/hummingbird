import { test } from "node:test";
import assert from "node:assert/strict";
import {
  DESTINATIONS,
  PENDING_TTL_MS,
  buildBody,
  canSubmit,
  classify,
  draftFromTab,
  isCapturableUrl,
  isValidId,
  reusablePending,
  rowMatches,
  todayDeadline,
  tokenProblem,
  urlHost,
} from "./lib.js";

// The cases `share.rs`'s `the_host_drops_www_userinfo_port_and_case` and
// `the_scheme_is_matched_case_insensitively` carry, verbatim.
test("the host drops www, userinfo, port and case", () => {
  assert.equal(urlHost("https://www.YouTube.com/watch?v=abc"), "youtube.com");
  assert.equal(urlHost("http://user:pw@example.test:8080/x"), "example.test");
  assert.equal(urlHost("https://example.test"), "example.test");
  assert.equal(urlHost("http://[::1]:8080/x"), "[::1]", "an IPv6 literal keeps its brackets and drops only the port after them");
  assert.equal(urlHost("https://[2001:db8::1]/"), "[2001:db8::1]");
  assert.equal(urlHost("ftp://example.test/x"), null, "not http(s)");
  assert.equal(urlHost("https:///x"), null, "no host");
  assert.equal(urlHost("not a url"), null);
  assert.equal(urlHost("HTTPS://Example.test/A"), "example.test");
  assert.equal(urlHost(undefined), null);
});

test("only an http URL with a host is capturable", () => {
  assert.equal(isCapturableUrl("https://example.test/x"), true);
  assert.equal(isCapturableUrl("Http://example.test"), true);
  for (const url of ["javascript:alert(1)", "chrome://extensions", "about:blank", "file:///C:/x.pdf", "https:///x", ""]) {
    assert.equal(isCapturableUrl(url), false, url);
  }
});

test("a tab titles by its title, and a title is never a raw URL", () => {
  assert.deepEqual(draftFromTab({ url: "https://www.youtube.com/watch?v=abc", title: " Knee rehab video " }), {
    title: "Knee rehab video",
    host: "youtube.com",
    url: "https://www.youtube.com/watch?v=abc",
  });
  assert.equal(draftFromTab({ url: "https://www.youtube.com/watch?v=abc", title: "" }).title, "youtube.com", "an empty title falls back to the host");
  assert.equal(draftFromTab({ url: "https://a.test/doc.pdf", title: "https://a.test/doc.pdf" }).title, "a.test", "a title that is its own URL is not a title");
  assert.equal(draftFromTab({ url: "https://a.test/doc.pdf", title: "HTTP://other.test/x" }).title, "a.test");
  assert.equal(draftFromTab({ url: "https://a.test/", title: "See https://a.test/ now" }).title, "See https://a.test/ now", "a URL inside words stays a title");
  assert.equal(draftFromTab({ url: "https://a.test/" }).title, "a.test", "no title at all");
});

test("a tab that cannot be captured says why", () => {
  assert.deepEqual(draftFromTab({ url: "chrome://extensions", title: "Extensions" }), { problem: "Only http(s) pages can be captured." });
  assert.deepEqual(draftFromTab({ url: "about:blank" }), { problem: "Only http(s) pages can be captured." });
  assert.deepEqual(draftFromTab({ title: "No url granted" }), { problem: "This tab cannot be read." });
  assert.deepEqual(draftFromTab(undefined), { problem: "This tab cannot be read." });
});

test("a blank title does not submit", () => {
  assert.equal(canSubmit("Read this"), true);
  assert.equal(canSubmit("   "), false);
  assert.equal(canSubmit(""), false);
  assert.equal(canSubmit(undefined), false);
});

test("the body carries exactly the keys CreateItem knows", () => {
  const body = buildBody({ id: "abc", title: " Read this ", description: "  ", url: "https://a.test/x" });
  assert.deepEqual(Object.keys(body).sort(), ["id", "link_url", "title"], "Triage: no stage, no link_label, no empty description");
  assert.equal(body.title, "Read this");
  const withText = buildBody({ id: "abc", title: "T", description: " line one\nline two ", url: "https://a.test/x", destination: "triage" });
  assert.equal(withText.description, "line one\nline two", "trimmed at the ends, lines kept");
  assert.deepEqual(Object.keys(withText).sort(), ["description", "id", "link_url", "title"]);
});

test("a mint is born in Ready, and mint-for-today stamps the deadline", () => {
  const mint = buildBody({ id: "abc", title: "T", url: "https://a.test/x", destination: "ready" });
  assert.deepEqual(mint, { id: "abc", title: "T", link_url: "https://a.test/x", stage: "ready" });
  const today = buildBody({ id: "abc", title: "T", url: "https://a.test/x", destination: "ready", deadline: "2026-09-08" });
  assert.deepEqual(Object.keys(today).sort(), ["deadline", "id", "link_url", "stage", "title"]);
  assert.equal(today.deadline, "2026-09-08");
  assert.deepEqual(DESTINATIONS, ["triage", "ready"], "the web capture box's closed vocabulary");
  assert.throws(() => buildBody({ id: "abc", title: "T", url: "https://a.test/x", destination: "grilling" }), /not a capture destination/);
});

test("today's deadline is the local date, date-only", () => {
  const noon = new Date(2026, 8, 8, 12, 0, 0).getTime();
  assert.equal(todayDeadline(noon), "2026-09-08");
  const lateEvening = new Date(2026, 0, 1, 23, 59, 0).getTime();
  assert.equal(todayDeadline(lateEvening), "2026-01-01", "local, not UTC");
});

test("an id is the route's own charset", () => {
  assert.equal(isValidId(crypto.randomUUID()), true);
  assert.equal(isValidId("a-b_c.d~e"), true);
  for (const id of ["", ".", "..", "a/b", "a b", "ü", undefined]) assert.equal(isValidId(id), false, String(id));
});

test("a pending id is reused for the same URL inside the TTL", () => {
  const pending = { id: "p1", url: "https://a.test/x", mintedAt: 1000 };
  assert.equal(reusablePending(pending, "https://a.test/x", 1000 + PENDING_TTL_MS - 1), "p1");
  assert.equal(reusablePending(pending, "https://a.test/x", 1000 + PENDING_TTL_MS), null, "expired");
  assert.equal(reusablePending(pending, "https://a.test/y", 2000), null, "another page");
  assert.equal(reusablePending(pending, "https://a.test/x", 500), null, "a clock that went backwards");
  assert.equal(reusablePending(undefined, "https://a.test/x", 2000), null);
  assert.equal(reusablePending({ url: "https://a.test/x", mintedAt: 1000 }, "https://a.test/x", 2000), null, "no id");
});

test("a token is hb_ and 64 hex, 67 bytes", () => {
  const ok = "hb_" + "0123456789abcdef".repeat(4);
  assert.equal(ok.length, 67);
  assert.equal(tokenProblem(ok), null);
  assert.equal(tokenProblem(ok.toUpperCase().replace("HB_", "hb_")), null, "case is not the check");
  assert.match(tokenProblem(""), /Paste the token/);
  assert.match(tokenProblem(ok + "\r"), /not a hummingbird token/, "a stray CR is refused here, not diagnosed as a 401 later");
  assert.match(tokenProblem(ok.slice(0, 66)), /67 in all/);
  assert.match(tokenProblem("Bearer " + ok), /not a hummingbird token/);
});

const JSON_TYPE = "application/json";
const ROW = (id) => JSON.stringify({ id, title: "T", stage: "triage" });

test("a 201 or 200 that echoes the id is saved", () => {
  const row = JSON.parse(ROW("x"));
  assert.deepEqual(classify({ status: 201, contentType: JSON_TYPE, bodyText: ROW("x"), id: "x" }), { kind: "saved", replay: false, row, message: "Saved" });
  assert.deepEqual(classify({ status: 200, contentType: "application/json; charset=utf-8", bodyText: ROW("x"), id: "x" }), { kind: "saved", replay: true, row, message: "Already saved" });
});

test("a 200 from the static shell is misrouted, not saved", () => {
  const html = "<!doctype html><title>hummingbird</title>";
  assert.equal(classify({ status: 200, contentType: "text/html; charset=utf-8", bodyText: html, id: "x" }).kind, "misrouted");
  assert.equal(classify({ status: 201, contentType: JSON_TYPE, bodyText: "not json", id: "x" }).kind, "misrouted");
  assert.equal(classify({ status: 201, contentType: JSON_TYPE, bodyText: ROW("other"), id: "x" }).kind, "misrouted", "the row must be ours");
  assert.equal(classify({ status: 200, contentType: null, bodyText: "", id: "x" }).kind, "misrouted");
});

test("401 is the token, 403 is scope unless Cloudflare said it", () => {
  assert.equal(classify({ status: 401, contentType: null, bodyText: "", id: "x" }).kind, "token");
  assert.equal(classify({ status: 403, contentType: null, bodyText: "", id: "x" }).kind, "scope");
  assert.equal(classify({ status: 403, contentType: "text/html", bodyText: "error code: 1010", id: "x" }).kind, "edge");
});

test("a 400 carries the authority's own words", () => {
  const body = JSON.stringify({ error: "validation", message: "title must be non-empty" });
  const outcome = classify({ status: 400, contentType: JSON_TYPE, bodyText: body, id: "x" });
  assert.equal(outcome.kind, "rejected");
  assert.match(outcome.message, /title must be non-empty/);
  assert.equal(classify({ status: 400, contentType: "text/plain", bodyText: "?", id: "x" }).message, "The authority refused the item.");
});

test("anything else is an error that keeps the form", () => {
  const outcome = classify({ status: 503, contentType: "text/html", bodyText: "", id: "x" });
  assert.equal(outcome.kind, "error");
  assert.match(outcome.message, /503/);
});

test("the host refuses a broken IPv6 literal and keeps a non-numeric port", () => {
  assert.equal(urlHost("http://[::1/x"), null, "unclosed bracket");
  assert.equal(urlHost("http://[::1]x/"), null, "junk after the bracket");
  assert.equal(urlHost("http://example.test:abc/"), "example.test:abc", "a non-numeric port is not a port");
  assert.equal(urlHost("http://example.test:/"), "example.test", "an empty port is dropped, as in Rust");
  assert.equal(urlHost("https://www."), null, "www. alone is no host");
  assert.equal(urlHost("https://user@host.test"), "host.test");
  assert.equal(urlHost("https://WWW.Example.test"), "www.example.test", "www. is stripped before lowercasing, the Rust's own quirk");
  assert.equal(urlHost("https://Ñandú.test/"), "Ñandú.test", "ASCII-only lowercasing, as to_ascii_lowercase");
});

test("classify's remaining edges", () => {
  assert.equal(classify({ status: 403, contentType: JSON_TYPE, bodyText: "{}", id: "x" }).kind, "scope");
  assert.equal(classify({ status: 400, contentType: JSON_TYPE, bodyText: "{not json", id: "x" }).message, "The authority refused the item.");
  assert.equal(classify({ status: 400, contentType: JSON_TYPE, bodyText: JSON.stringify({ error: "e" }), id: "x" }).message, "The authority refused the item.");
  assert.equal(classify({ status: 201, contentType: JSON_TYPE, bodyText: "null", id: "x" }).kind, "misrouted");
  assert.equal(classify({ status: 200, contentType: JSON_TYPE, bodyText: ROW("x"), id: "x" }).row.id, "x", "the replayed row rides along");
});

test("a replayed row is this capture only when every sent field agrees", () => {
  const body = { id: "x", title: "T", link_url: "https://a.test/x" };
  assert.equal(rowMatches({ id: "x", title: "T", link_url: "https://a.test/x", description: null, stage: "triage" }, body), true);
  assert.equal(rowMatches({ id: "x", title: "Other", link_url: "https://a.test/x", stage: "triage" }, body), false, "another title");
  assert.equal(rowMatches({ id: "x", title: "T", link_url: "https://a.test/x", stage: "ready" }, body), false, "another stage");
  assert.equal(rowMatches({ id: "x", title: "T", link_url: "https://a.test/x", description: "notes", stage: "triage" }, body), false, "a description this send did not carry");
  const mint = { ...body, stage: "ready", description: "notes" };
  assert.equal(rowMatches({ id: "x", title: "T", link_url: "https://a.test/x", description: "notes", stage: "ready" }, mint), true);
  assert.equal(rowMatches(null, body), false);
});

test("the remaining body and pending edges", () => {
  const body = buildBody({ id: "abc", title: "T", url: "https://a.test/x", destination: "ready", deadline: "" });
  assert.deepEqual(Object.keys(body).sort(), ["id", "link_url", "stage", "title"], "an empty deadline is not sent");
  assert.equal(buildBody({ id: "abc", title: "T", url: "https://a.test/x", description: 42 }).description, undefined);
  assert.equal(reusablePending({ id: "p", url: "https://a.test/x" }, "https://a.test/x", 2000), null, "no mintedAt");
  assert.match(tokenProblem(undefined), /Paste the token/);
});
