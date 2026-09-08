// The manifest's invariants, each one a decision in ADR-0037: a popup-only
// Manifest V3 extension with the narrowest permission set that can read
// the active tab and reach the one authority.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync, existsSync } from "node:fs";
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
