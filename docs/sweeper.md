# The Tasks→Linear sweeper

A one-way sweeper that moves every incomplete Google Tasks item (outside a
denylist) into Linear Triage as a bare-text issue, marks it complete in Tasks,
and reports its own liveness to healthchecks.io. It is the only built artifact
of v0 capture — no custom client, no endpoint. Spec: issue
[#14](https://github.com/JddAndrewLauren/hummingbird/issues/14), decided in
[#5](https://github.com/JddAndrewLauren/hummingbird/issues/5) and
[#8](https://github.com/JddAndrewLauren/hummingbird/issues/8).

## Shape

| File | What it is |
| --- | --- |
| `sweep.py` | The whole sweeper. Python 3 stdlib only, one-shot, importable for tests. |
| `denylist.json` | Lists to skip, keyed by list id, title as the value. |
| `crontab` | `*/15 * * * *` — read by supercronic inside the container. |
| `Dockerfile` | `python:3.12-slim` + supercronic pinned by version and sha256. |
| `fly.toml` | `hummingbird-sweeper`, one 256MB always-on worker. |
| `.github/workflows/deploy.yml` | Tests, then `flyctl deploy` — on push to `main` only. |
| `scripts/mint_refresh_token.py` | One-time local OAuth consent helper. |
| `tests/test_sweep.py` | `python3 -m unittest discover -s tests`. Cred-free. |

## How it runs

supercronic fires `/app/sweep` every 15 minutes. The sweeper stays a one-shot
script — no `while true; sleep 900` — so it is equally runnable locally and by
hand: `fly ssh console -C /app/sweep`. Every run logs `sweep start`, a line per
list, a line per item, and `sweep finish ok=… created=… existed=… completed=…
failed=… duration=…`.

A `fcntl.flock` on `$SWEEP_LOCK` (default `/tmp/sweep.lock`) is taken *inside*
the script rather than by a `flock -n` wrapper in the crontab, so it covers
supercronic, manual `fly ssh` runs, and local runs alike (and macOS has no
`flock(1)` for local runs). On contention the run logs, pings nothing, and
exits 0.

Exit codes: 0 = success, dry run, or lock contention; 1 = any failure.

## Per-item algorithm

For each list not in the denylist, for each incomplete task:

1. `id = deterministic_v4(task.id)`
2. `issueCreate` in Linear with that client-supplied id
3. only on success → `PATCH` the Tasks item to `status: completed`
4. on any other error → log it, **leave the task incomplete** (the next sweep
   retries), mark the sweep failed, continue to the next item
5. after all items: no failures → ping the healthchecks success URL

**Create-in-Linear-first is load-bearing.** A crash between steps 2 and 3 can
only produce a visible duplicate attempt, never a silent loss — and the
deterministic id turns that retry into an "already exists" success.

### Idempotency

`IssueCreateInput` accepts a client-supplied `id`, but Linear validates it as
UUID **version 4 specifically** — a genuine RFC-4122 v5 uuid is rejected with
`id must be a UUID`, so `uuid.uuid5()` is not usable. `deterministic_v4()`
hashes `sha256(NAMESPACE + task_id)`, takes 16 bytes, and forces the version
and variant nibbles into v4 shape.

`NAMESPACE` in `sweep.py` must never change. Every issue id the sweeper has
ever minted derives from it; changing it re-mints every id and duplicates every
still-open capture. A frozen test vector in `tests/test_sweep.py` guards it.

A duplicate create comes back as `code: INPUT_ERROR` with
`userPresentableMessage: "Entity Issue with id <uuid> already exists."` — the
sweeper matches that exactly and treats it as success. There is deliberately no
footer and no attachment on the issue: the UUID is the link (recomputable from
the task id), and the completed Tasks item is the audit trail.

### Field mapping

- **Title → title, verbatim.** No cleanup, truncation, or prefix.
- **Non-empty notes → description.** Empty notes → no `description` field.
- **Due date → dropped.** A Gemini-inferred date is a scheduling decision made
  by a transcription engine. The phrase ("Thursday") survives in the title, and
  a real date gets set deliberately during triage.

## Dry run

`./sweep.py --dry-run` enumerates every list and incomplete task and logs
exactly what would happen, mutating nothing on either side and pinging nothing.
**The first run must be a dry run** — a first live sweep would otherwise empty
every standing list (shopping, packing) into Triage. It stays a permanent
debugging tool.

Its `list id=<id> title='<title>'` lines are what seed `denylist.json`.

## Denylist

`denylist.json` is committed, keyed by list `id` with the human-readable title
as the value — ids are rename-proof, titles alone break silently. A list is
skipped iff its id is a key. A stale or unknown id **fails open**: the list
gets swept, not skipped. Noise in Triage, never a lost capture. Changing it is
a normal push-and-deploy.

## Liveness

healthchecks.io, free tier, **grace period 45 minutes** (three consecutive
missed sweeps). Success is pinged **only after a fully successful, non-dry
sweep** — a sweeper that runs but errors on every call must still trip the
alarm. Any failure or exception POSTs the accumulated failure lines to
`$HEALTHCHECK_URL/fail` for immediate alerting. The ping itself is wrapped in
its own try/except and can never fail a run.

Fly health checks are explicitly *not* the mechanism: they restart, they don't
notify. Structural backstop: unswept items visibly accumulate in the Tasks app.

## Secrets

Set with `flyctl secrets set`; nothing on-device, nothing committed.

`GOOGLE_CLIENT_ID`, `GOOGLE_CLIENT_SECRET`, `GOOGLE_REFRESH_TOKEN`,
`LINEAR_API_KEY`, `HEALTHCHECK_URL`.

Google auth is a **Workspace Internal** OAuth app (captures land in the
twinion.net Workspace account). Internal user type means no verification review
and no 7-day refresh-token expiry — that footgun applies only to apps in
Testing status. Desktop-app OAuth client, one-time local consent to mint the
refresh token. Deferred alternative if token durability ever bites: a service
account with domain-wide delegation.

The Linear key goes in the `Authorization` header **raw, not `Bearer`**.
Constants in `sweep.py`: `teamId` `84ab9e0b-f455-42d7-a48a-49e65da3b2e6` (ION),
`stateId` `35cec1f9-df46-4212-9bef-8905015ad539` (Triage — verified to create
directly into Triage in one call).

Quota headroom (verified): Linear 2,500 req/hr, Google Tasks 50,000
queries/day. Nothing binds at this cadence.

## Human setup checklist

None of this is done yet — the code is built, the provisioning is not.

1. **Fly app.** `flyctl apps create hummingbird-sweeper --org personal` (same
   account/billing as `twinion-api`). Then `flyctl tokens create deploy` and
   store the value as the GitHub Actions secret `FLY_API_TOKEN`. Adjust
   `primary_region` in `fly.toml` if `sjc` isn't wanted.
2. **healthchecks.io.** Create the check with a 45-minute grace period; record
   the ping URL. Leave it paused until go-live so setup runs don't alert.
3. **OAuth client.** Create an *Internal* desktop-app OAuth client in the
   twinion.net Workspace, scope `https://www.googleapis.com/auth/tasks`.
4. **Refresh token.** `python3 scripts/mint_refresh_token.py --client-id …
   --client-secret …` locally, then `flyctl secrets set` all five secrets.
5. **Dry run and seed.** Export the five values locally and run
   `./sweep.py --dry-run`. Read the output. Copy the real list ids and titles
   of the standing lists into `denylist.json` and commit.
6. **Go live.** Push to `main` (which deploys), watch `flyctl logs`, unpause
   the healthchecks check, and confirm both a success ping and a test capture
   landing in Triage.

## Acceptance (post-provisioning)

- A dictated capture (phone/watch/speaker → Gemini → Tasks) appears in Linear
  Triage within ~15 minutes and is marked completed in Tasks.
- Killing the sweep between `issueCreate` and the `PATCH`, then rerunning,
  produces no duplicate issue and completes the task.
- Three consecutive failed or missed sweeps produce a healthchecks alert.
- A denylisted list is never touched; an unknown list id is swept.

## Changing things

- **Cadence** — one line in `crontab`, deliberately the cheapest decision in
  the system to reverse.
- **supercronic version** — bump `SUPERCRONIC_VERSION` in the `Dockerfile` and
  replace `SUPERCRONIC_SHA256` with the real `shasum -a 256` of the downloaded
  release asset. The project publishes no checksum file; never invent the hash.
- **Never add a `schedule:` trigger** to `.github/workflows/deploy.yml`.
  Scheduling on Actions was overturned in #8 (pooled minutes, whole-minute
  billing, the $0 spending cap, 60-day auto-disable). supercronic owns cadence.
- **Never add `[http_service]` or `[[services]]`** to `fly.toml` — either would
  let Fly's autostop machinery suspend the worker between sweeps.
