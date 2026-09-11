// The manifest's invariants, each one a decision in ADR-0037: a popup-only
// Manifest V3 extension with the narrowest permission set that can read
// the active tab and reach the one authority.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync, existsSync } from "node:fs";
import { createHash } from "node:crypto";
import { AUTHORITY_BASE } from "./lib.js";

const dir = new URL("./", import.meta.url);
const manifest = JSON.parse(readFileSync(new URL("manifest.json", dir), "utf8"));

test("manifest v3, popup only", () => {
  assert.equal(manifest.manifest_version, 3);
  assert.equal(manifest.action.default_popup, "popup.html");
  assert.equal(manifest.background, undefined, "no background worker: the popup page does the fetch");
  assert.equal(manifest.content_scripts, undefined, "a content script would be CORS-bound");
});

test("the permission set is exactly activeTab and storage", () => {
  assert.deepEqual([...manifest.permissions].sort(), ["activeTab", "storage"]);
  assert.equal(manifest.optional_permissions, undefined);
});

test("the one host permission is the authority lib.js posts to", () => {
  assert.deepEqual(manifest.host_permissions, [`${AUTHORITY_BASE}/*`]);
});

test("every file the manifest names exists", () => {
  const named = [
    manifest.action.default_popup,
    manifest.options_ui.page,
    ...Object.values(manifest.icons),
  ];
  for (const file of named) assert.ok(existsSync(new URL(file, dir)), file);
});

test("the Firefox id is set, so a later signed build needs no manifest change", () => {
  assert.equal(typeof manifest.browser_specific_settings.gecko.id, "string");
});

test("the keyboard command and the options page are pinned", () => {
  assert.equal(manifest.commands._execute_action.suggested_key.default, "Alt+Shift+H");
  assert.equal(manifest.options_ui.page, "options.html");
});

// #798: the id is pinned by a public key, not by the directory the tree was
// loaded from, so every unpacked load shares one `storage.local` and the
// token stored under one checkout is present under the next. Chrome's id is
// the first 32 hex digits of SHA-256 over the DER key, each mapped onto
// a-p; the README records that id, and this keeps the record honest.
test("the manifest `key` is a 2048-bit RSA SubjectPublicKeyInfo", () => {
  assert.equal(typeof manifest.key, "string");
  const der = Buffer.from(manifest.key, "base64");
  assert.equal(der.length, 294, "a 2048-bit RSA SPKI is 294 bytes of DER");
  assert.deepEqual([...der.subarray(0, 4)], [0x30, 0x82, 0x01, 0x22], "SEQUENCE of length 290");
});

test("the README names the id Chrome derives from that key", () => {
  const der = Buffer.from(manifest.key, "base64");
  const hex = createHash("sha256").update(der).digest("hex").slice(0, 32);
  const id = [...hex].map((c) => String.fromCharCode(97 + parseInt(c, 16))).join("");
  const readme = readFileSync(new URL("README.md", dir), "utf8");
  assert.ok(readme.includes(`\`${id}\``), `README should name ${id}`);
});
