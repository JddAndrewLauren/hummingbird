import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import path from "node:path";
import { createServer } from "./server.js";

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

const server = createServer({
  bearerToken: RUNNER_BEARER_TOKEN,
  repoRoot: REPO_ROOT,
  spawn,
  claudeBin: CLAUDE_BIN,
});

server.listen(PORT, () => {
  // Fly's own log stream is the whole operational posture (#256's
  // 2026-08-10 decision) -- there is deliberately no logging integration
  // here beyond stdout/stderr.
  console.log(`hummingbird-runner listening on :${PORT}`);
});
