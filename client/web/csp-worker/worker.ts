// The Cloudflare Worker `main` script referenced by wrangler.toml (ADR-0006,
// ADR-0004). Runs in front of the `[assets]` binding on every request
// (`run_worker_first = true`) so the strict CSP is a served response header
// on every asset, not a meta tag — and SPA fallback (`not_found_handling`)
// still applies because we always route through `env.ASSETS.fetch`.
//
// Also sets a same-origin `Permissions-Policy` for `microphone` and
// `on-device-speech-recognition` (issue #368) — an intentional statement
// gating `SpeechRecognition.install()` per MDN, not a prerequisite: both
// directives default to a `self` allowlist, so dictation works same-origin
// with no header at all.
//
// Only the default export lives here: workerd treats every named export of
// a Worker's main module as a candidate handler/entrypoint, so the CSP
// string constant lives in ./csp.ts instead.

import { CONTENT_SECURITY_POLICY } from "./csp";

export interface AssetsFetcher {
  fetch(request: Request): Promise<Response>;
}

export interface Env {
  ASSETS: AssetsFetcher;
}

async function fetch(request: Request, env: Env): Promise<Response> {
  const assetResponse = await env.ASSETS.fetch(request);
  const headers = new Headers(assetResponse.headers);
  headers.set("Content-Security-Policy", CONTENT_SECURITY_POLICY);
  headers.set(
    "Permissions-Policy",
    "microphone=(self), on-device-speech-recognition=(self)",
  );
  return new Response(assetResponse.body, {
    status: assetResponse.status,
    statusText: assetResponse.statusText,
    headers,
  });
}

export default { fetch };
