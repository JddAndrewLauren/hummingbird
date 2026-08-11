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
| `runner/src/skills-registry.js` | The closed map of runnable skill names -- `parse-capture` (#256) and `next-up-hb` (#116). |
| `runner/src/skills/parse-capture.js` | That skill's arg validation and prompt-building. |
| `runner/src/skills/next-up-hb.js` | The same, for `/next-up-hb`: the sweep payload arrives in `args` (context-blind -- no authority token here, no HTTP call), plus the `prepare` hook that ranks before the model runs. |
| `runner/src/rank-bin.js` | Spawns the baked `next-up-rank` over the envelope on stdin. The one child process here that is not `claude`. |
| `runner/src/claude-cli.js` | Builds the `claude -p ... --output-format json --json-schema <path>` argv. |
| `runner/src/run-skill.js` | Spawns `claude` (with `cwd` = repo root, so its slash commands resolve), collects stdout/stderr, resolves ok/error. |
| `runner/src/envelope.js` | NDJSON line builders (`progress`, final ok/error). |
| `runner/src/main.js` | Reads env (`RUNNER_BEARER_TOKEN`, `PORT`, `CLAUDE_BIN`, `REPO_ROOT`), wires the real `child_process.spawn`, starts the server. |
| `runner/Dockerfile` | `node:22-slim` + the Claude Code CLI installed globally + the skills this build ships + the runner server. Build context is the **repo root**, not `runner/` -- see Deploy runbook. |
| `runner/fly.toml` | `hummingbird-runner`, `http_service` with `min_machines_running = 0` (scale-to-zero). |
| `.claude/skills/parse-capture/` | The skill itself: `SKILL.md` + `schema.json` (the versioned per-skill result schema `run-skill.js` passes to `--json-schema`). |
| `.claude/skills/next-up-hb/` | `SKILL.md` + `schema.json` + `scripts/next-up.sh` (two verbs: `survey` fetches with the operator's credential, `rank` reads a prebuilt envelope on stdin -- the by-hand equivalent of `rank-bin.js`). |
| `client/next-up/` | The seam `/next-up-hb` is layered on: sweep payload in, `hummingbird_core::rank` candidates + health facts out. Its `next-up-rank` binary is built by the Dockerfile's Rust stage and baked in as `HB_NEXT_UP_BIN`. |
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

Two ops ship today, and **both write to nothing**:

- **`parse-capture`** (#256, 2026-08-10 decision): `{title, notes}`, #42's
  own minimal schema. The write-target question (Linear vs. the ADR-0008
  owned server) is explicitly deferred, which is what let it ship without
  taking that decision early.
- **`next-up-hb`** (#116): pick what to do right now. `args` carry the
  `GET /api/sweep` payload from the calling device's mirror, so the runner
  stays **context-blind** -- it holds no authority token and makes no HTTP
  call, and the interactive arm of the skill is the one that fetches. v1 of
  the skill is read-only, so there is no write target to defer.

  Its **deterministic half runs before the model**: `prepare` spawns the
  baked `next-up-rank` and the prompt carries `ranked` instead of the raw
  sweep. The runner arm cannot shell out -- `claude -p` is
  non-interactive, so a tool call needing permission is denied outright and
  `claude-cli.js` passes no `--allowedTools` -- and granting `Bash` to save
  a process the runner can spawn itself would widen the hosted model's
  reach from "answer in this schema" to "run anything in the image". A
  ranking failure is therefore an envelope `error` in the ranker's own
  words, before a single model token is spent.

`microtask` and `/to-actions` now use the app-owned authority helpers in
interactive sessions, but they are not hosted runner operations. The retired
`next-up-personal` selector is not registered here; `/next-up-hb` is its
app-owned replacement.

**Confirmed against a live run**, and the CLI contract is narrower than it
first looked. Both halves were assumed wrong on the first pass, and both
failed every real invocation while the unit tests stayed green -- which is
the standing lesson here: a fake `spawn` can only ever pin what its author
believed the CLI does.

- **`--json-schema` takes the schema's TEXT, not a path.** A path is
  rejected before anything runs (`--json-schema is not valid JSON: JSON
  Parse error: Unrecognized token '/'`). `run-skill.js` reads the versioned
  per-skill file and passes its contents.
- **A schema file may not carry a `$schema` key.** The CLI rejects the
  usual draft ref outright (`no schema with key or ref
  "https://json-schema.org/draft/2020-12/schema"`), so a `$schema` line in
  a shipped schema is a deploy-time outage, not a style nit. Pinned by
  `runner/test/parse-capture.test.js`.
- **`--output-format json` wraps everything in the CLI's own metadata
  envelope** (`{is_error, usage, total_cost_usd, result,
  structured_output, ...}`). The schema-constrained object is
  `structured_output`; `result` is the same object as a *string*. Reading
  raw stdout as the result handed callers the metadata.

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

1. **Provision the Fly app**, from the **repo root** -- the build needs
   `.claude/skills/*`, `runner/`, and (since #116's Rust builder stage)
   `client/` plus **all of** `server/` in one build context -- the whole
   server workspace, because `server/domain` inherits `version`/`edition`
   from its root and cargo demands every declared member exist. The root
   `.dockerignore` is what keeps that context from carrying every
   `target/` and `node_modules/` in the repo:

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

   For first-party Anthropic:

   ```sh
   fly secrets set --config runner/fly.toml \
     RUNNER_BEARER_TOKEN=<the token from step 2> \
     ANTHROPIC_API_KEY=<a metered Anthropic API key>
   ```

   For a **non-default provider** (#41 decision 2's Kimi/GLM posture --
   both speak the Anthropic API natively), all three of decision 2's
   variables are needed, and the credential goes in `ANTHROPIC_AUTH_TOKEN`,
   **not** `ANTHROPIC_API_KEY`: the two are different headers
   (`Authorization: Bearer` vs. `x-api-key`), and a third-party endpoint
   wants the former. Set no `ANTHROPIC_API_KEY` at all in this case.

   ```sh
   fly secrets set --config runner/fly.toml \
     RUNNER_BEARER_TOKEN=<the token from step 2> \
     ANTHROPIC_BASE_URL=<the provider's Anthropic-compatible endpoint> \
     ANTHROPIC_AUTH_TOKEN=<that provider's key> \
     ANTHROPIC_MODEL=<that provider's model id>
   ```

   Switching providers later is `fly secrets set` alone, no deploy
   (decision 2) -- but eyeball a few runs after any swap: the per-skill
   schema catches shape failures, never judgment failures. Set a spend cap
   in whichever provider's console holds the key, at the same time -- the
   cost-ceiling posture above.

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

   `next-up-hb` is smoke-tested the same way, with the caller supplying the
   sweep payload it already holds -- the runner has no way to fetch one:

   ```sh
   curl -sS https://hummingbird-runner.fly.dev/run \
     -H "authorization: Bearer <token>" \
     -H "content-type: application/json" \
     -d "{\"skill\":\"next-up-hb\",\"args\":{\"sweep\":$(cat sweep.json),\"now\":{\"local\":\"2026-08-11T09:53\",\"epoch_ms\":1786553580000}}}"
   ```

6. **Rotate the token** later by repeating step 2-3 with a fresh value,
   then updating whatever client holds it.

## Testing (agent-facing, not part of the operator gate)

```sh
cd runner && node --test
```

No network access, no `claude` binary, and no credentials are needed --
every test injects a fake `spawn`.
