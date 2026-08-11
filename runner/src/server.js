import { createServer as createHttpServer } from "node:http";
import { checkBearerToken } from "./auth.js";
import { parseRunRequest } from "./request.js";
import { getSkill } from "./skills-registry.js";
import { runSkill } from "./run-skill.js";
import { progressLine, finalOkLine, finalErrorLine } from "./envelope.js";

const MAX_BODY_BYTES = 1_000_000; // 1MB -- a capture is text, never a payload this size
const HEARTBEAT_INTERVAL_MS = 20_000; // well under Fly's 60s idle-connection kill

/**
 * The `POST /run` HTTP server (#41 decision 4 / #256): auth, then shape
 * validation, then a streamed NDJSON body ending in the `{ok, skill,
 * result, error?}` envelope. Every dependency the handler needs beyond the
 * request itself -- the bearer token, where `claude` lives on disk, how to
 * spawn it, the repo root a skill's schema path resolves against -- is
 * passed in here rather than read from the environment inline, so the
 * whole server is constructible in a test with a fake `spawn` and no real
 * `claude` binary.
 *
 * @param {object} opts
 * @param {string} opts.bearerToken
 * @param {string} opts.repoRoot absolute path a skill's `resultSchemaPath` resolves against
 * @param {(command: string, args: string[]) => import("node:events").EventEmitter} opts.spawn
 * @param {string} [opts.claudeBin]
 * @param {number} [opts.heartbeatIntervalMs]
 * @param {(path: string) => string} [opts.readSchema] how a skill's schema file is read (see `run-skill.js`)
 */
export function createServer({
  bearerToken,
  repoRoot,
  spawn,
  claudeBin = "claude",
  heartbeatIntervalMs = HEARTBEAT_INTERVAL_MS,
  readSchema,
}) {
  return createHttpServer((req, res) => {
    if (req.method !== "POST" || req.url !== "/run") {
      res.writeHead(404).end();
      return;
    }

    if (!checkBearerToken(req.headers.authorization, bearerToken)) {
      res.writeHead(401).end();
      return;
    }

    readBody(req, MAX_BODY_BYTES)
      .then((rawBody) =>
        handleRun(rawBody, res, { repoRoot, spawn, claudeBin, heartbeatIntervalMs, readSchema }),
      )
      .catch((err) => {
        // Nothing has been written yet at this point -- readBody rejects
        // before any response headers are sent -- so a plain JSON error is
        // still accurate, not a truncated stream. It only actually reaches
        // the client because readBody drains rather than destroys; see
        // there.
        if (!res.headersSent) {
          // Deliberately NOT `connection: close`: the client is typically
          // still writing when this is sent, and closing underneath it
          // resets the socket before it can read -- the same failure
          // `req.destroy()` had, measured. The drain in `readBody` is what
          // leaves the connection in a state worth keeping.
          res.writeHead(413, { "content-type": "application/json" });
          res.end(JSON.stringify({ ok: false, skill: null, error: err.message }));
        } else {
          res.end();
        }
      });
  });
}

function readBody(req, maxBytes) {
  return new Promise((resolve, reject) => {
    let body = "";
    let bytes = 0;
    let overflowed = false;
    req.on("data", (chunk) => {
      if (overflowed) return;
      bytes += chunk.length;
      if (bytes > maxBytes) {
        overflowed = true;
        // Drain and discard the rest -- never `req.destroy()`, which tears
        // down the very socket the 413 has to travel on, so the client
        // sees a bare connection reset (UND_ERR_SOCKET) instead of the
        // rejection this line intends. Nothing further accumulates: the
        // `overflowed` guard above discards every remaining chunk.
        req.resume();
        reject(new Error("request body too large"));
        return;
      }
      body += chunk;
    });
    req.on("end", () => resolve(body));
    req.on("error", reject);
  });
}

function handleRun(rawBody, res, { repoRoot, spawn, claudeBin, heartbeatIntervalMs, readSchema }) {
  const parsed = parseRunRequest(rawBody);
  if (!parsed.ok) {
    res.writeHead(400, { "content-type": "application/json" });
    res.end(JSON.stringify({ ok: false, skill: null, error: parsed.error }));
    return;
  }

  const skill = getSkill(parsed.skill);
  if (!skill) {
    res.writeHead(400, { "content-type": "application/json" });
    res.end(JSON.stringify({ ok: false, skill: parsed.skill, error: `unknown skill: ${parsed.skill}` }));
    return;
  }

  const argsCheck = skill.validateArgs(parsed.args);
  if (!argsCheck.ok) {
    res.writeHead(400, { "content-type": "application/json" });
    res.end(JSON.stringify({ ok: false, skill: skill.name, error: argsCheck.error }));
    return;
  }

  // From here on the response is a stream: any failure past this point
  // (a rejected runSkill, a thrown error) must still end in the
  // {ok:false, error} envelope line, never a bare dropped connection
  // (#256's "a failed run terminates the stream with the envelope,
  // already the contract").
  res.writeHead(200, { "content-type": "application/x-ndjson" });

  const heartbeat = setInterval(() => {
    res.write(progressLine("still running"));
  }, heartbeatIntervalMs);

  const prompt = skill.buildPrompt(parsed.args);
  const schemaPath = `${repoRoot}/${skill.resultSchemaPath}`;

  runSkill({
    skillName: skill.name,
    prompt,
    schemaPath,
    spawn,
    claudeBin,
    onProgress: (message) => res.write(progressLine(message)),
    ...(readSchema ? { readSchema } : {}),
  })
    .catch((err) => ({ ok: false, error: err.message }))
    .then((outcome) => {
      clearInterval(heartbeat);
      if (outcome.ok) {
        res.write(finalOkLine(skill.name, outcome.result));
      } else {
        res.write(finalErrorLine(skill.name, outcome.error));
      }
      res.end();
    });
}
