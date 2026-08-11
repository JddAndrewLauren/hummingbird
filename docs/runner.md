# The skill-runner endpoint

> **Status (2026-08-11): built, not deployed.** No Fly app is provisioned,
> no secrets are set, and no bearer token is minted -- #256 is a build-only
> slice; provisioning is an operator gate (see Deploy runbook below), the
> same posture #237's server deploy used.

A fourth actor (#41 decided this, #256 builds it): a Fly app that takes
`POST /run {skill, args}` and runs one Claude Code skill headlessly,
streaming NDJSON progress ending in an `{ok, skill, result, error?}`
envelope. Structurally a sibling of the sweeper that takes orders over HTTP
instead of a cron tick -- if it is down, capture/read/triage/sync all still
work; only "run a skill for me" degrades. Spec: issue
[#41](https://github.com/JddAndrewLauren/hummingbird/issues/41), filed and
decided in [#256](https://github.com/JddAndrewLauren/hummingbird/issues/256)
(grilling, 2026-08-10).

## Shape

| File | What it is |
| --- | --- |
| `runner/src/server.js` | The `POST /run` HTTP handler: auth, request shape validation, skill dispatch, NDJSON streaming. |
| `runner/src/auth.js` | Constant-time bearer-token check. |
| `runner/src/request.js` | `{skill, args}` body parsing/shape validation. |
| `runner/src/skills-registry.js` | The closed map of runnable skill names -- v1 holds `parse-capture` only. |
| `runner/src/skills/parse-capture.js` | That skill's arg validation and prompt-building. |
| `runner/src/claude-cli.js` | Builds the `claude -p ... --output-format json --json-schema <path>` argv. |
| `runner/src/run-skill.js` | Spawns `claude`, collects stdout/stderr, resolves ok/error. |
| `runner/src/envelope.js` | NDJSON line builders (`progress`, final ok/error). |
| `runner/src/main.js` | Reads env (`RUNNER_BEARER_TOKEN`, `PORT`, `CLAUDE_BIN`, `REPO_ROOT`), wires the real `child_process.spawn`, starts the server. |
| `runner/Dockerfile` | `node:22-slim` + the Claude Code CLI installed globally + the skills this build ships + the runner server. Build context is the **repo root**, not `runner/` -- see Deploy runbook. |
| `runner/fly.toml` | `hummingbird-runner`, `http_service` with `min_machines_running = 0` (scale-to-zero). |
| `.claude/skills/parse-capture/` | The skill itself: `SKILL.md` + `schema.json` (the versioned per-skill result schema `run-skill.js` passes to `--json-schema`). |
| `runner/test/*.test.js` | `node --test`, run from `runner/`. Every module is unit-testable with an injected fake `spawn` -- no real `claude` binary or credentials needed. |

## The contract

`POST /run` with `{skill: string, args: object}`, `Authorization: Bearer
<token>`.

- **401** — missing or wrong bearer token. Empty body.
- **400** — malformed JSON, a missing/wrong-shaped `skill` or `args` field,
  an unknown skill name, or args that fail that skill's own
  `validateArgs`. JSON body `{ok: false, skill, error}` (`skill` is `null`
  when the request never named a resolvable one).
- **200**, `content-type: application/x-ndjson` — the request passed every
  check and a `claude` run was attempted. The body is newline-delimited
  JSON: zero or more `{"type":"progress","message":"..."}` lines (including
  a periodic heartbeat every 20s, well under Fly's 60s idle-connection
  kill), ending in exactly one `{ok, skill, result}` or `{ok: false, skill,
  error}` line. **A failed `claude` run (non-zero exit, unparseable
  stdout, a spawn error) still ends the stream in the `ok:false` envelope
  line at HTTP 200** -- the failure is inside the contract, not a broken
  connection.

v1 ships **`parse-capture` only** (#256, 2026-08-10 decision): `{title,
notes}`, #42's own minimal schema. It writes to nothing -- no authority
call anywhere in the runner or the skill. The write-target question (Linear
vs. the ADR-0008 owned server) is explicitly deferred; `next-up-personal`
and `microtask` wait behind that decision before they become runner ops.

**Unconfirmed against a live run**, recorded rather than silently assumed
(`runner/src/run-skill.js`'s own header): `claude -p --output-format json
--json-schema <schema>`'s stdout is read as the schema-constrained result
object directly. If a live run instead wraps it in a `{type, result,
...}` envelope the way plain `--output-format json` does, `run-skill.js`'s
JSON.parse step is the one place to adjust.

## Operational posture (#256, 2026-08-10 decision)

- **Logging** is Fly's own log stream -- `console.log`/`console.error` only,
  no logging integration.
- **A failed run** terminates the stream with the `{ok: false, error}`
  envelope; there is no retry inside the runner.
- **Token rotation** is by re-set (`fly secrets set RUNNER_BEARER_TOKEN=...`
  followed by a redeploy or restart) -- there is no rotation endpoint.
- **Cost ceiling** is a spend cap set on the metered Anthropic key,
  operator-side (see the provider's own console), recorded in this runbook
  rather than enforced in the runner.

## Deploy runbook (operator gate -- #256 does not perform any of this)

Nothing below is run by the agent slice. It is recorded here so the
operator can close the provisioning gate #256's issue thread leaves open.

1. **Provision the Fly app**, from the **repo root** (the build needs both
   `.claude/skills/parse-capture` and `runner/` in one build context):

   ```sh
   fly launch --config runner/fly.toml --dockerfile runner/Dockerfile --no-deploy
   ```

   Confirm the app name (`hummingbird-runner`) and region when prompted, or
   accept `runner/fly.toml`'s own values.

2. **Mint the bearer token** locally (never checked in):

   ```sh
   openssl rand -hex 32
   ```

3. **Set secrets**:

   ```sh
   fly secrets set --config runner/fly.toml \
     RUNNER_BEARER_TOKEN=<the token from step 2> \
     ANTHROPIC_API_KEY=<a metered Anthropic API key>
   ```

   Optional provider overrides (`ANTHROPIC_BASE_URL`, `ANTHROPIC_MODEL`) go
   in the same command if a non-default provider is wanted (#41 decision
   2). Set a spend cap on the Anthropic key in the provider console at the
   same time -- the cost-ceiling posture above.

4. **Deploy**:

   ```sh
   fly deploy --config runner/fly.toml --dockerfile runner/Dockerfile
   ```

5. **Smoke-test** (replace `<token>`):

   ```sh
   curl -sS https://hummingbird-runner.fly.dev/run \
     -H "authorization: Bearer <token>" \
     -H "content-type: application/json" \
     -d '{"skill":"parse-capture","args":{"text":"call mom Thursday about the trip"}}'
   ```

   Expect a stream of NDJSON lines ending in `{"ok":true,"skill":"parse-capture","result":{"title":"...","notes":"..."}}`.

6. **Rotate the token** later by repeating step 2-3 with a fresh value,
   then updating whatever client holds it.

## Testing (agent-facing, not part of the operator gate)

```sh
cd runner && node --test
```

No network access, no `claude` binary, and no credentials are needed --
every test injects a fake `spawn`.
