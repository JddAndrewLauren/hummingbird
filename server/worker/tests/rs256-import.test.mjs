// The shim's RS256 verify path (ADR-0040 decision 4, #837), run where it can
// run: `server/worker` has no test harness, and `crypto.subtle` is the one
// thing the native suite cannot execute. So this mirrors `signin.rs`'s
// `verify_rs256` call for call — `importKey("spki", der, RS256, false,
// ["verify"])`, then `verify("RSASSA-PKCS1-v1_5", …)` — under Node's
// WebCrypto, against the fixture the native suite already pins:
//
//   - `google_signin.rs` asserts `jwk_to_spki(fixture JWK)` equals the
//     fixture's `spki_der_base64` (openssl's own DER for that key), and
//   - `auth_google.rs` asserts the fixture token splits into exactly the
//     signing input and signature this test verifies.
//
// Together: the bytes the pure crate hands the shim verify. The source scan
// at the bottom keeps the mirror honest — if the shim's import format or
// algorithm changes, this test fails until it is changed to match.
//
// Run from the repo root: node --test server/worker/tests/

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const here = new URL(".", import.meta.url);
const read = (path) => readFileSync(new URL(path, here), "utf8");
const fixture = JSON.parse(read("../../authority/fixtures/google-signin-rs256.json"));

// The same parameters `fcm.rs`'s `rs256_import_algorithm` builds.
const RS256 = { name: "RSASSA-PKCS1-v1_5", hash: { name: "SHA-256" } };

async function verify(token) {
  const [header, claims, signature] = token.split(".");
  const key = await crypto.subtle.importKey(
    "spki",
    Buffer.from(fixture.spki_der_base64, "base64"),
    RS256,
    false,
    ["verify"],
  );
  return crypto.subtle.verify(
    RS256.name,
    key,
    Buffer.from(signature, "base64url"),
    new TextEncoder().encode(`${header}.${claims}`),
  );
}

test("the fixture token's RS256 signature verifies through the spki import", async () => {
  assert.equal(await verify(fixture.id_token), true);
});

test("a token whose claims were altered does not verify", async () => {
  const [header, claims, signature] = fixture.id_token.split(".");
  const altered = JSON.parse(Buffer.from(claims, "base64url").toString());
  altered.sub = "110000000000000000002";
  const forged = [header, Buffer.from(JSON.stringify(altered)).toString("base64url"), signature];
  assert.equal(await verify(forged.join(".")), false);
});

test("the shim still imports and verifies the way this test does", () => {
  const signin = read("../src/signin.rs");
  const fcm = read("../src/fcm.rs");
  assert.match(signin, /import_key_with_object\("spki", &key_data, &algorithm, false, &usages\)/);
  assert.match(signin, /Array::of1\(&"verify"\.into\(\)\)/);
  assert.match(signin, /verify_with_str_and_js_u8_array_and_js_u8_array\(\s*RSASSA_PKCS1_V1_5,\s*&key,\s*&signature,\s*&data,/);
  assert.match(fcm, /const RSASSA_PKCS1_V1_5: &str = "RSASSA-PKCS1-v1_5";/);
  assert.match(fcm, /Reflect::set\(&hash, &"name"\.into\(\), &"SHA-256"\.into\(\)\)/);
});
