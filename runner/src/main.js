import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import path from "node:path";
import { createServer } from "./server.js";
import { createRankRunner } from "./rank-bin.js";
import { createAuthorityClient } from "./authority.js";

const RUNNER_BEARER_TOKEN = process.env.RUNNER_BEARER_TOKEN;
if (!RUNNER_BEARER_TOKEN) {
  console.error("RUNNER_BEARER_TOKEN is not set -- refusing to start with no way to authenticate a caller.");
  process.exit(1);
}

const PORT = Number(process.env.PORT ?? 8080);
const CLAUDE_BIN = process.env.CLAUDE_BIN ?? "claude";

// The Dockerfile puts this file at /app/runner/src/main.js and the repo
// checkout (skills included) at /app -- so the repo root is two levels up
// from this file's directory. REPO_ROOT overrides for local dev, where the
// layout differs.
const DEFAULT_REPO_ROOT = path.resolve(fileURLToPath(new URL(".", import.meta.url)), "..", "..");
const REPO_ROOT = process.env.REPO_ROOT ?? DEFAULT_REPO_ROOT;

// The prebuilt ranker the image bakes in (`runner/Dockerfile` sets
// `HB_NEXT_UP_BIN`). `next-up-hb`'s `prepare` runs this before `claude`, so
// the hosted model never needs a shell -- see `rank-bin.js`. The default
// name keeps a PATH install working in local dev.
const NEXT_UP_BIN = process.env.HB_NEXT_UP_BIN ?? "next-up-rank";

// The app-owned authority (ADR-0008), for the one op that reads and writes
// it (`microtask`, #272). The token is a server-side secret and is read
// here, once, never from a request.
//
// A missing token is deliberately **not** fatal at boot, unlike
// RUNNER_BEARER_TOKEN above: two of the three ops hold no authority
// credential by design, and refusing to start would take them down over a
// secret they never touch. `createAuthorityClient` answers a named
// "not configured" error instead, which surfaces as an ordinary envelope
// error on the one op that needs it -- see `authority.js`.
const HB_API_BASE = process.env.HB_API_BASE ?? "https://hb.twinion.net";
const HB_API_TOKEN = process.env.HB_API_TOKEN ?? "";
if (!HB_API_TOKEN) {
  console.error("HB_API_TOKEN is not set -- the microtask op will decline; every other op is unaffected.");
}

const server = createServer({
  bearerToken: RUNNER_BEARER_TOKEN,
  repoRoot: REPO_ROOT,
  spawn,
  claudeBin: CLAUDE_BIN,
  runRanker: createRankRunner({ spawn, bin: NEXT_UP_BIN }),
  authority: createAuthorityClient({ fetch, baseUrl: HB_API_BASE, token: HB_API_TOKEN }),
});

server.listen(PORT, () => {
  // Fly's own log stream is the whole operational posture (#256's
  // 2026-08-10 decision) -- there is deliberately no logging integration
  // here beyond stdout/stderr.
  console.log(`hummingbird-runner listening on :${PORT}`);
});
