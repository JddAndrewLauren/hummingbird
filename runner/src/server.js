import { createServer as createHttpServer } from "node:http";
import { checkBearerToken } from "./auth.js";
import { parseRunRequest } from "./request.js";
import { getSkill } from "./skills-registry.js";
import { runSkill } from "./run-skill.js";
import { unconfiguredAuthority } from "./authority.js";
import { isValidModelId } from "./claude-cli.js";
import { resolveModel } from "./stamp.js";
import { progressLine, finalOkLine, finalErrorLine } from "./envelope.js";

/**
 * The stamp source when none is injected: no provider named, no configured
 * model. Every terminal line still carries `backend`, so the client's
 * classifier never has to special-case a half-stamped envelope.
 */
const UNCONFIGURED_STAMP = { backend: "unknown", configuredModel: null };

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
 * @param {(envelope: unknown) => Promise<{ok: true, ranked: unknown} | {ok: false, error: string}>} [opts.runRanker] the `next-up-rank` seam (see `rank-bin.js`); only skills declaring `prepare` use it
 * @param {typeof unconfiguredAuthority} [opts.authority] the app-owned authority client (see `authority.js`); only ops that read or write it use one, and the default names its own absence
 * @param {{backend: string, configuredModel: string | null}} [opts.stamp] what the terminal line names as having produced the answer (#273) — computed from the environment in `main.js`, injected here rather than read inline like every other dependency
 */
export function createServer({
  bearerToken,
  repoRoot,
  spawn,
  claudeBin = "claude",
  heartbeatIntervalMs = HEARTBEAT_INTERVAL_MS,
  readSchema,
  runRanker,
  authority = unconfiguredAuthority,
  stamp = UNCONFIGURED_STAMP,
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
        handleRun(rawBody, res, {
          repoRoot,
          spawn,
          claudeBin,
          heartbeatIntervalMs,
          readSchema,
          runRanker,
          authority,
          stamp,
        }),
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
          // No stamp: nothing was dispatched, so there is no backend and no
          // model to name (`envelope.js`'s presence rule).
          res.writeHead(413, { "content-type": "application/json" });
          res.end(finalErrorLine(null, err.message));
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

function handleRun(rawBody, res, { repoRoot, spawn, claudeBin, heartbeatIntervalMs, readSchema, runRanker, authority, stamp }) {
  // Every pre-dispatch failure below is unstamped: nothing ran, so there is
  // no backend and no model to name (`envelope.js`'s presence rule).
  const reject400 = (skill, error) => {
    res.writeHead(400, { "content-type": "application/json" });
    res.end(finalErrorLine(skill, error));
  };

  const parsed = parseRunRequest(rawBody);
  if (!parsed.ok) {
    reject400(null, parsed.error);
    return;
  }

  const skill = getSkill(parsed.skill);
  if (!skill) {
    reject400(parsed.skill, `unknown skill: ${parsed.skill}`);
    return;
  }

  const argsCheck = skill.validateArgs(parsed.args);
  if (!argsCheck.ok) {
    reject400(skill.name, argsCheck.error);
    return;
  }

  // `model` is an argument of the *pipeline*, not of any one skill, so it is
  // read and validated here — which is also what makes this a real boundary
  // rather than a duplicate of `microtask`'s own check: no `validateArgs`
  // rejects unknown keys, so without this gate a `model` on `parse-capture`
  // or `next-up-hb` would reach argv unexamined. See `isValidModelId` for
  // what "unexamined" would cost.
  const requestedModel = parsed.args?.model;
  if (requestedModel !== undefined && !isValidModelId(requestedModel)) {
    reject400(skill.name, '"model" must be a model id when present');
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

  const onProgress = (message) => res.write(progressLine(message));

  runPipeline(skill, parsed.args, {
    repoRoot,
    spawn,
    claudeBin,
    readSchema,
    runRanker,
    authority,
    onProgress,
    model: requestedModel,
  })
    .catch((err) => ({ ok: false, error: err.message }))
    .then((outcome) => {
      clearInterval(heartbeat);
      // Past the dispatch, every terminal line is stamped — the run took a
      // lane, and naming it is what lets the client render what produced
      // the answer rather than a name it hardcoded.
      const runStamp = {
        backend: stamp.backend,
        model: resolveModel({
          reported: outcome.model,
          requested: requestedModel,
          configured: stamp.configuredModel,
        }),
      };
      if (outcome.ok) {
        res.write(finalOkLine(skill.name, outcome.result, runStamp));
      } else {
        res.write(finalErrorLine(skill.name, outcome.error, runStamp));
      }
      res.end();
    });
}

/**
 * One skill invocation, in the three beats every op shares -- and the
 * handler above stays skill-agnostic because both optional beats are
 * declared by the skill, never named here.
 *
 * 1. `prepare` (optional): the deterministic half, run **before** the model.
 *    `next-up-hb` ranks here; `microtask` reads its item from the
 *    authority. It may rewrite the args the prompt is then built from, and
 *    a failure ends the stream with the ordinary envelope without ever
 *    spawning `claude` -- which is the point: the failure a caller reads is
 *    the ranker's or the authority's own words, not a model's account of
 *    them.
 * 2. The model run itself.
 * 3. `apply` (optional): the write half, run **after** the model, handed
 *    the schema-validated result and the prepared args. `microtask` lands
 *    its checklist in the owned `steps` table here. A failed write is an
 *    envelope `error` like any other -- the run is not "ok" because a model
 *    answered, it is ok because the answer landed.
 *
 * @returns {Promise<{ok: true, result: unknown, model?: string} | {ok: false, error: string, model?: string}>}
 */
async function runPipeline(
  skill,
  args,
  { repoRoot, spawn, claudeBin, readSchema, runRanker, authority, onProgress, model },
) {
  const prepared = skill.prepare
    ? await skill.prepare(args, { runRanker, authority, onProgress })
    : { ok: true, args };
  if (!prepared.ok) return prepared;

  const outcome = await runSkill({
    skillName: skill.name,
    prompt: skill.buildPrompt(prepared.args),
    schemaPath: `${repoRoot}/${skill.resultSchemaPath}`,
    spawn,
    claudeBin,
    cwd: repoRoot,
    onProgress,
    ...(model ? { model } : {}),
    ...(readSchema ? { readSchema } : {}),
  });
  if (!outcome.ok || !skill.apply) return outcome;

  // The reported model belongs to the run, not to the write — carry it
  // across `apply` so a failed write still names what produced the answer
  // it failed to land.
  const applied = await skill.apply(outcome.result, {
    args: prepared.args,
    authority,
    onProgress,
  });
  return outcome.model ? { ...applied, model: outcome.model } : applied;
}
