# The skill-runner endpoint

> **Status (2026-08-12): deployed, all three ops live.**
> `hummingbird-runner` is provisioned in `sjc` and answering at
> `https://hummingbird-runner.fly.dev`, and all three ops are smoke-tested
> against it. `HB_API_TOKEN` is set (a device-scope token minted as id
> `runner`), so `microtask` holds a **write** credential against the live
> authority and every run of it mints real Step rows -- confirmed by writing,
> then soft-deleting, 28 of them. One finding came out of that run: an
> identical repeat request appended a further phase of steps rather than
> converging, because the model read the existing checklist as work already
> covered
> ([#307](https://github.com/JddAndrewLauren/hummingbird/issues/307)) -- the
> op is idempotent at the write layer and *not* at the request layer, which
> is the level a client retries at. **Fixed in code by
> [#312](https://github.com/JddAndrewLauren/hummingbird/issues/312): a bare
> run against a live plan is now declined before a model token is spent** --
> pending redeploy, the same operator gate as the rest of this section.
> Provisioning was and remains an operator gate: #256 and #272 are
> build-only slices, the same posture #237's server deploy used.

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
| `runner/src/skills-registry.js` | The closed map of runnable skill names -- `parse-capture` (#256), `next-up-hb` (#116), `microtask` (#272) and `grill-me` (#350). |
| `runner/src/skills/parse-capture.js` | That skill's arg validation and prompt-building. |
| `runner/src/skills/next-up-hb.js` | The same, for `/next-up-hb`: the sweep payload arrives in `args` (context-blind -- no authority token here, no HTTP call), plus the `prepare` hook that ranks before the model runs. |
| `runner/src/skills/microtask.js` | The same, for `/microtask`: `prepare` reads the item from the authority, `apply` writes the checklist back to it. The one op that holds a credential. |
| `runner/src/skills/grill-me.js` | The same, for `/grill-me` (#350): `prepare` reads the item and resolves `ref`, same as `microtask`, but there is **no `apply`** -- this op writes nothing, ever. |
| `runner/src/authority.js` | The client for the app-owned authority (`GET /api/sweep`, `POST /api/steps`, `PATCH /api/steps/:id`) and the only place a `device` token lives in this process. `fetch` injected. |
| `runner/src/step-id.js` | The deterministic step id, digit for digit the same recipe as `microtask`'s `hb.sh` -- what keeps the two arms from minting two copies of one step. |
| `runner/src/rank-bin.js` | Spawns the baked `next-up-rank` over the envelope on stdin. The one child process here that is not `claude`. |
| `runner/src/claude-cli.js` | Builds the `claude -p ... --output-format json --json-schema <path>` argv, plus `isValidModelId` -- the charset rule that keeps a flag-shaped `model` arg out of argv. |
| `runner/src/stamp.js` | The envelope's `backend`/`model` stamp (#273): the provider from `ANTHROPIC_BASE_URL`, and the four-step model precedence. |
| `runner/src/run-skill.js` | Spawns `claude` (with `cwd` = repo root, so its slash commands resolve), collects stdout/stderr, resolves ok/error. |
| `runner/src/envelope.js` | NDJSON line builders (`progress`, final ok/error) -- the one place a terminal line is built, which is what makes the stamp-presence rule structural. |
| `runner/src/main.js` | Reads env (`RUNNER_BEARER_TOKEN`, `PORT`, `CLAUDE_BIN`, `REPO_ROOT`, `HB_API_BASE`, `HB_API_TOKEN`), wires the real `child_process.spawn` and `fetch`, starts the server. |
| `runner/Dockerfile` | `node:22-slim` + the Claude Code CLI installed globally + the skills this build ships + the runner server. Build context is the **repo root**, not `runner/` -- see Deploy runbook. |
| `runner/fly.toml` | `hummingbird-runner`, `http_service` with `min_machines_running = 0` (scale-to-zero). |
| `.claude/skills/parse-capture/` | The skill itself: `SKILL.md` + `schema.json` (the versioned per-skill result schema `run-skill.js` passes to `--json-schema`). |
| `.claude/skills/next-up-hb/` | `SKILL.md` + `schema.json` + `scripts/next-up.sh` (two verbs: `survey` fetches with the operator's credential, `rank` reads a prebuilt envelope on stdin -- the by-hand equivalent of `rank-bin.js`). |
| `.claude/skills/microtask/` | `SKILL.md` + `schema.json` + `scripts/hb.sh` (the interactive arm's reads and writes -- inert in the image, since the hosted arm has no shell). |
| `.claude/skills/grill-me/` | `SKILL.md` + `schema.json` -- hosted-runner arm only for this slice (#350); no interactive script, no client, no ADR. |
| `client/next-up/` | The seam `/next-up-hb` is layered on: sweep payload in, `hummingbird_core::rank` candidates + health facts out. Its `next-up-rank` binary is built by the Dockerfile's Rust stage and baked in as `HB_NEXT_UP_BIN`. |
| `runner/test/*.test.js` | `node --test`, run from `runner/`. Every module is unit-testable with an injected fake `spawn` (and, for the authority, a fake `fetch`) -- no real `claude` binary, no network, no credentials needed. |

## The contract

`POST /run` with `{skill: string, args: object}`, `Authorization: Bearer
<token>`.

- **401** — missing or wrong bearer token. Empty body.
- **400** — malformed JSON, a missing/wrong-shaped `skill` or `args` field,
  an unknown skill name, args that fail that skill's own `validateArgs`, or
  a `model` arg that is not a model id. JSON body `{ok: false, skill,
  error}` (`skill` is `null` when the request never named a resolvable one).
- **200**, `content-type: application/x-ndjson` — the request passed every
  check and a `claude` run was attempted. The body is newline-delimited
  JSON: zero or more `{"type":"progress","message":"..."}` lines (including
  a periodic heartbeat every 20s, well under Fly's 60s idle-connection
  kill), ending in exactly one `{ok, skill, result}` or `{ok: false, skill,
  error}` line. **A failed `claude` run (non-zero exit, unparseable
  stdout, a spawn error) still ends the stream in the `ok:false` envelope
  line at HTTP 200** -- the failure is inside the contract, not a broken
  connection.

### The stamp (#273)

The terminal line carries **`backend` and `model`** naming what produced the
answer, so a client renders what actually ran rather than a name it
hardcoded (#274 makes it vary). The presence rule is part of the contract:

| Terminal line | `backend` | `model` |
| --- | --- | --- |
| `ok: true` | always | always (possibly `null`) |
| a pipeline error (a `prepare` decline, a failed run, a failed write) | always | always (possibly `null`) |
| a pre-dispatch 400 or the 413 | **absent** | **absent** |

Absent, never `null`, on the pre-dispatch failures: nothing was attempted,
and a client must be able to tell that from "we ran but could not name the
model". `runner/src/envelope.js` is the one builder of a terminal line, so
the rule holds by construction.

`backend` is `ANTHROPIC_BASE_URL`'s **hostname** (never the whole URL, which
can carry a path, a port, or in a misconfiguration credentials), or
`anthropic` when it is unset. `model` resolves in four steps: **what the CLI
reported it ran → the `model` arg the request asked for → `ANTHROPIC_MODEL`
→ `null`**.

The order exists because of a trap: `ANTHROPIC_MODEL` is set **only on the
third-party provider path** below, so on the ordinary first-party deployment
a config-only read would stamp `null`. The first step is also the one
believing something about the CLI's output shape — this repo has been burned
twice that way with green tests throughout — so it reads `modelUsage`
defensively and degrades to the next step rather than lying. **The runbook's
step 6 is what confirms it against a live run**; until that is done, treat
the reported half as unproven.

### The `model` arg

Any skill's `args` may carry `model`, validated in the pipeline (not by any
one skill, because no `validateArgs` rejects unknown keys) against a
**charset rule**, not an allowlist: `^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$`.
An allowlist would make every provider swap a code change and a redeploy,
contradicting #41 decision 2's "switching providers is `fly secrets set`
alone".

The risk closed is not shell injection — the value is a single argv element
and nothing is shelled out to — but an argv element that reads as a *flag*.
**The leading-alphanumeric requirement is that guard and must never be
relaxed.** The charset accepts `sonnet`, `claude-sonnet-4-5-20250929`,
`kimi-k3` and Bedrock-style `us.anthropic.claude-…-v1:0`.

Four ops ship today. The first two **write to nothing**; the third writes,
and is the reason this process holds an authority credential at all; the
fourth reads that same credential but writes nothing either:

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

- **`microtask`** (#272): break one already-selected item into a checklist
  of tiny steps. `args` are `{ref, grain?, replace?, model?}` -- `HB-42` or
  a uuid, SKILL.md's 1-3 grain scale (default 2), the explicit rewrite
  gesture (#317), and the model to run it on (#273: the app's Rewrite
  gesture offers grain and model together). This op **reads and writes the
  authority**, which is the whole of
  what makes it different:

  - `prepare` fetches `GET /api/sweep`, resolves the ref (no route accepts
    `HB-<seq>`; it is a client-side affordance over `Item.seq`) and puts
    the item plus its **ticked** steps in the prompt -- the unticked ones
    are the plan, and the model never sees them (#317). An unknown ref, a
    missing token and an unreachable authority all end the stream here,
    before a model token is spent.
  - `apply` runs **after** the model. It writes one `POST /api/steps` per
    line of the answer, at contiguous positions after the highest *ticked*
    position -- the plan starts where the record ends -- and on a replace
    it also moves and drops, per the `replace: true` bullet below.
    **`ok:true` means the checklist landed, not that a model answered**: a
    failed write is an `ok:false` envelope like any other.
  - Idempotence is structural at the write layer, not the request layer
    (#307). Each step's id is
    `sha256("hummingbird-skill/microtask/v1" + item + "/" + body)`, so a replay
    of the *identical* text lands on the authority's already-exists path
    (200, the stored row) rather than minting a duplicate -- and
    `runner/src/step-id.js` is the same recipe as the skill's own `hb.sh`,
    pinned against it by `runner/test/step-id.test.js`, so the interactive
    and hosted arms cannot mint two copies of one step between them. A
    second, differently-worded request is not a replay, though: see #307
    below.
  - **A bare run never continues a live plan** (#307/#312). `prepare`
    declines, before a model token is spent, if the item has any live step
    that is not `done` -- naming the count and the remedy -- and a
    different `grain` does not change that. An item whose live steps are
    all `done` has no plan to protect, so a bare run appends after them,
    the normal case.
  - **`replace: true` is the explicit gesture that rewrites the plan
    instead** (#317). `prepare` skips the decline and carries the live
    unticked steps' ids forward as `knownUndoneIds`. `apply` diffs the
    model's answer against those same steps by exact text: one the answer
    repeats verbatim is *kept* at its existing id and moved to its new
    position (`moveStep`); one absent from the answer is *dropped*
    (`dropStep`); everything else is a `createStep`. Creates and moves
    happen before any drop, so a write that fails partway leaves the old
    plan live rather than truncated, and ticked steps are never part of the
    diff -- their id, `done` state and position are untouched. The model
    never sees the plan it may be replacing and never sees or emits a step
    id, so a duplicated replace is not idempotent: it paraphrases what it
    cannot see and writes the same count back under rotated ids.
  - `apply` re-asserts `prepare`'s guard after the model runs, refusing only
    if a live undone step is present whose id is not in `knownUndoneIds`
    -- an id-aware check, not emptiness, since a replace's known set is
    the very plan it is about to diff. Ticking or dropping a step in
    between only shrinks that set and never aborts the write. The
    already-`done` steps ride in the prompt, labelled `record`, so the
    model can *report* them in `note` and never re-propose them -- on a
    bare run or a replace alike, since the model never sees the unticked
    steps either way.
  - The model is not the one holding the credential. It has no shell here
    for the same reason `next-up-hb`'s ranker runs out of process, and the
    writes are made by `authority.js` from the args the model answered
    with.

- **`grill-me`** (#350): the item-scoped interview, one typed question at a
  time, ending in a proposal. `args` are `{ref, turns, model?}` -- `turns` is
  the *whole conversation so far*, threaded by the caller on every request,
  because **this op is stateless**: there is no session here and nothing
  durable remembers a transcript between requests. Structurally the same
  shape as `microtask`'s read half and nothing like its write half:

  - `prepare` fetches `GET /api/sweep`, resolves `ref` the same way
    `microtask` does, and declines -- before a model token is spent -- on an
    unknown ref, a missing token, an unreachable authority, or a request at
    or past the **turn cap** (`PROVISIONAL_TURN_CAP` in
    `runner/src/skills/grill-me.js`; a placeholder until #351's live-run
    measurement sets the real number, the same posture as #312's live-plan
    decline for `microtask`).
  - `prepare` also carries this item's **prior *applied* grill outcomes**
    (`summary`, `verdict`, `patch` -- never a past transcript) into the
    prompt, read defensively off an optional `sweep.grills`, which nothing
    populates yet: today this is always `[]`, and the seam is positioned so
    #353's real `grills` table starts flowing through unchanged code.
  - There is **no `apply`**. This is the whole of the op's write posture:
    it calls no write method on `authority.js`, ever, and a caller decides
    what (if anything) to do with a proposal.
  - The result is one of exactly two schema-enforced shapes (`oneOf` in
    `.claude/skills/grill-me/schema.json`): `{kind: "question", question:
    {prompt, recommendedAnswer, choices}}` (2-4 choices, and free text is
    always still a valid answer regardless of what is offered) or
    `{kind: "proposal", proposal: {summary, verdict, patch}}` (`verdict` is
    `resolved` or `fog_remains`).

  No interactive arm ships in this slice -- `.claude/skills/grill-me/` is
  `SKILL.md` + `schema.json` only, and the hosted runner is the only way to
  drive it (`POST /run`, or the equivalent `curl` in the smoke-test section
  below). #349 is the plan this discharges the first slice of; #351 is the
  live-run gate the rest of that plan is blocked behind.

`/to-actions` uses the app-owned authority helpers in interactive sessions,
but it is not a hosted runner operation. The retired `next-up-personal`
selector is not registered here; `/next-up-hb` is its app-owned replacement.

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
  followed by a redeploy or restart) -- there is no rotation endpoint. Since
  #273 this is a **two-place** operation: the authority's proxy holds the
  same bearer as a Cloudflare Worker secret, and every tap from the app
  answers 502 until both sides carry the new value. See below.
- **Cost ceiling** is a spend cap set on the metered Anthropic key,
  operator-side (see the provider's own console), recorded in this runbook
  rather than enforced in the runner.

## The authority's proxy (#273, ADR-0018)

The browser never calls this app directly -- it has no CORS lane, and the
shell's CSP is `connect-src 'self'`. The app taps
**`POST /api/skills/run`** on the authority, which authorizes the caller's
existing `device` token and proxies to `POST /run` here, forwarding the
NDJSON stream unchanged. Full reasoning, the rejected alternatives, and the
flip condition are in
[ADR-0018](adr/0018-the-authority-proxies-the-skill-runner.md).

Two Worker secrets, both set from the operator's terminal and **never in
GitHub Actions** (the bearer reaches this app's own `HB_API_TOKEN`, a
write-everything device token, and spends metered model tokens):

```sh
cd server/worker
npx wrangler secret put RUNNER_BASE_URL       # https://hummingbird-runner.fly.dev
npx wrangler secret put RUNNER_BEARER_TOKEN   # the same value as `fly secrets`
```

Either one unset fails the lane closed with a 503 naming the gap. What the
proxy answers:

| Case | Status | Body |
| --- | --- | --- |
| Bad/missing device token | 401 | empty |
| Token out of scope | 403 | empty |
| Wrong method | 405 | the authority's `ApiError` JSON |
| Either secret unset | 503 | `"The cloud runner is not configured on this server."` |
| The subrequest errors | 502 | `"Cloud runner unreachable."` |
| This app answers 401 | **502** | `"The cloud runner rejected this server's credential."` |
| This app answers 400 or 413 | forwarded verbatim | this app's own NDJSON line |
| This app answers 200 | forwarded verbatim, streaming | this app's stream |
| Anything else | 502 | `"The cloud runner answered <status>."` |

Every proxy-generated failure is one NDJSON envelope line carrying
`ok:false, skill:null` and **no stamp** -- nothing was attempted. The
runner's 401 is deliberately never forwarded: it would make the app
re-prompt the user for a device token that is perfectly fine.

**Rotating `RUNNER_BEARER_TOKEN` touches two places** -- `fly secrets set`
here and `wrangler secret put` on the authority. In between, every tap from
the app is a 502.

Verify after deploying either side:

```sh
curl -sS -D- -X POST https://hb.twinion.net/api/skills/run \
  -H "Authorization: Bearer $HB_DEVICE_TOKEN" \
  -H 'content-type: application/json' \
  -d '{"skill":"microtask","args":{"ref":"<an item uuid>"}}'
```

Expect `content-type: application/x-ndjson` and a streaming body. A
`200 text/html` means the shell's SPA fallback answered instead -- the
ADR-0008 route-precedence failure mode.

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

   **And, for `microtask` (#272), the runner's own authority token** --
   its scope is `device`, the authority's only read-capable scope, which
   is write-everything (CLAUDE.md's credential blast radius). It is a
   *distinct* token from any device's, so it can be revoked on its own:

   ```sh
   runner/scripts/mint-hb-token.sh <admin-secret-file>   # mints, then sets HB_API_TOKEN
   ```

   It is minted the way every other device token is (`POST
   /api/admin/tokens` with `ADMIN_SECRET`, from the operator's terminal --
   never from Actions), and the script exists because **the plaintext
   appears only in the original 201**: the route is idempotent by `id` and
   stores only a hash, so a replay answers 200 with the metadata and no
   token, unrecoverably. So the mint and the `fly secrets set` that
   consumes it have to happen in one pass. Set `HB_TOKEN_OUT=<path>` to
   keep a mode-600 copy for 1Password. `HB_API_BASE` defaults to
   `https://hb.twinion.net`; set it by hand only to point elsewhere.

   **Leaving it unset is a supported state**: the
   runner still starts and logs one line, `parse-capture` and `next-up-hb`
   are unaffected, and `microtask` declines with a named envelope error.

   Switching providers later is `fly secrets set` alone, no deploy
   (decision 2) -- but **use `runner/scripts/switch-provider.sh`** rather
   than setting the variables by hand:

   ```sh
   runner/scripts/switch-provider.sh anthropic <key-file>
   runner/scripts/switch-provider.sh third-party <key-file> <base-url> <model-id>
   ```

   The two credentials are **mutually exclusive**, which is what the script
   is for: each direction clears the other side. With both
   `ANTHROPIC_API_KEY` and `ANTHROPIC_AUTH_TOKEN` set, the client sends both
   headers and the provider rejects every request -- and a hand-run
   `fly secrets set` sets one without clearing the other. It also reads the
   credential from a mode-600 file and strips leading/trailing whitespace, since
   a bearer token carrying a trailing newline fails auth in a way that looks
   nothing like a whitespace problem.

   Whichever way you swap, eyeball a few runs afterwards: the per-skill
   schema catches shape failures, never judgment failures -- on the
   2026-08-11 swap to Moonshot's `kimi-k3`, both read-only ops returned
   schema-valid answers and picked the same item, and the only difference
   was a vaguer `why` line, which no schema can catch. Set a spend cap in
   whichever provider console holds the key, at the same time -- the
   cost-ceiling posture above.

4. **Deploy**:

   ```sh
   fly deploy --config runner/fly.toml --dockerfile runner/Dockerfile
   ```

   Two things in this output look like failures and are not (both seen on
   the first real deploy, 2026-08-11):

   - **`WARNING The app is not listening on the expected address`**, listing
     only `/.fly/hallpass`. flyctl probes the socket within a few seconds of
     boot, before `node src/main.js` finishes starting; the machine's own log
     says `hummingbird-runner listening on :8080` immediately after. Read the
     log (`fly logs`), not the warning. A *genuine* bind failure looks the
     same in this warning but has no `listening on :8080` line behind it.
   - **`This deployment will: create 2 "app" machines`.** Fly adds a second
     machine for HA regardless of `min_machines_running = 0` -- that setting
     governs how many stay *running*, not how many exist. Both still stop
     when idle, so scale-to-zero and the idle cost are unaffected. Pass
     `--ha=false` if one machine is wanted instead.

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

   `microtask` is the one op whose smoke test leaves something behind --
   the steps land in the owned `steps` table against the item named by
   `ref`, so run it against an item you are willing to see a checklist on:

   ```sh
   curl -sS https://hummingbird-runner.fly.dev/run \
     -H "authorization: Bearer <token>" \
     -H "content-type: application/json" \
     -d '{"skill":"microtask","args":{"ref":"HB-42","grain":2}}'
   ```

   Expect NDJSON progress ending in
   `{"ok":true,"skill":"microtask","result":{"steps":[...],"note":"..."}}`,
   and the steps themselves visible in the client (or in `GET /api/sweep`)
   afterwards. **Re-running that identical request is the guard check, not
   an idempotence check** (#307/#312): the item now carries a live unticked
   plan, so the second run declines at `prepare` -- an `ok:false` envelope
   naming the unticked count and the `replace: true` remedy, with no model
   token spent and no rows added. A second run that *appends* is the #307
   defect back, and is the thing to report. Write-level idempotence
   (`sha256(namespace + item + "/" + body)`, so a retried write of the same
   answer mints nothing) is not what this exercises and cannot be reached
   from here -- the decline lands first.

   To smoke-test the rewrite the decline points at, add `"replace":true` to
   that same second request. Expect the
   `note`-adjacent progress line naming written / kept / dropped counts,
   and know the same non-idempotence applies one level further in: a
   second identical `replace` is not a no-op either, since the model
   cannot see the plan it is replacing and paraphrases it -- the count
   stays the same, the ids and wording rotate.

   `grill-me` (#350) writes nothing, so its smoke test leaves nothing
   behind. Thread the transcript by hand, one round per request -- the
   first opens the interview with an empty `turns`:

   ```sh
   curl -sS https://hummingbird-runner.fly.dev/run \
     -H "authorization: Bearer <token>" \
     -H "content-type: application/json" \
     -d '{"skill":"grill-me","args":{"ref":"HB-42","turns":[]}}'
   ```

   Expect NDJSON progress ending in
   `{"ok":true,"skill":"grill-me","result":{"kind":"question","question":{...}}}`.
   Answer it and re-send with that round appended to `turns` to get the next
   turn, and so on until a `kind":"proposal"` result -- this is the whole of
   #351's live-run gate, driven by hand rather than by a client.

6. **Confirm the model stamp** (#273) against that same live run. Every
   terminal line above now carries `backend` and `model`; what needs
   confirming is which *step* of the precedence chain answered, because the
   first one reads a key of the CLI's own output envelope
   (`modelUsage`) that nothing else in this repo depends on:

   ```sh
   curl -sS https://hummingbird-runner.fly.dev/run \
     -H "authorization: Bearer <token>" \
     -H "content-type: application/json" \
     -d '{"skill":"parse-capture","args":{"text":"call mom"}}' \
     | tail -1 | jq '{backend, model}'
   ```

   On the first-party path expect `{"backend":"anthropic","model":"<an id>"}`.
   **A `null` model here means the CLI does not report `modelUsage`** and the
   stamp is being carried by the fallbacks alone — send the same request
   again with `"model":"sonnet"` in `args` and confirm it comes back stamped
   `sonnet`, then record the finding in `runner/src/run-skill.js`'s
   `reportedModel`, which is written to degrade rather than lie for exactly
   this case.

7. **Rotate the token** later by repeating step 2-3 with a fresh value,
   then updating whatever client holds it. `HB_API_TOKEN` rotates
   separately: `DELETE /api/admin/tokens/runner` to revoke, then
   `mint-hb-token.sh` again under the same id (the mint is idempotent by
   `id`, so the revoke has to come first or the replay returns no token).

## Testing (agent-facing, not part of the operator gate)

```sh
cd runner && node --test
```

No network access, no `claude` binary, and no credentials are needed --
every test injects a fake `spawn`, and the two ops that talk to the
authority (`microtask`, `grill-me`) inject a fake `fetch` the same way.
`runner/test/grill-me.test.js` covers arg validation (including a malformed
prior turn threaded back by a caller), stateless turn reconstruction, the
one-question-at-a-time and free-text-always-allowed rules, proposals,
prior-outcome filtering (and that a transcript field never rides along),
the turn-cap decline, and -- as its own anti-goal check, the same posture
`microtask`'s write path earns from its own suite -- that no write method on
`runner/src/authority.js` is ever reachable from this op.
