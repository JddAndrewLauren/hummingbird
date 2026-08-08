# The capture→Linear sweeper

A one-way sweeper that drains capture sources into Linear Triage as bare-text
issues and reports its own liveness to healthchecks.io. One drain engine,
isolated adapters (ADR-0002): **Google Tasks** (every incomplete item outside
a denylist, fail-open; marked complete as the ack) and **Gmail** (only
messages carrying the `hummingbird/capture` label, fail-closed; the label
removed as the ack). No custom client, no endpoint. Spec: issues
[#14](https://github.com/JddAndrewLauren/hummingbird/issues/14) and
[#45](https://github.com/JddAndrewLauren/hummingbird/issues/45), decided in
[#5](https://github.com/JddAndrewLauren/hummingbird/issues/5),
[#8](https://github.com/JddAndrewLauren/hummingbird/issues/8), and
[#43](https://github.com/JddAndrewLauren/hummingbird/issues/43)/ADR-0002.

## Shape

| File | What it is |
| --- | --- |
| `sweep.py` | The whole sweeper: the drain engine plus both adapters. Python 3 stdlib only, one-shot, importable for tests. |
| `denylist.json` | Tasks lists to skip, keyed by list id, title as the value. |
| `crontab` | `*/15 * * * *` — read by supercronic inside the container. |
| `Dockerfile` | `python:3.12-slim` + supercronic pinned by version and sha256. |
| `fly.toml` | `hummingbird-sweeper`, one 256MB always-on worker. |
| `.github/workflows/deploy.yml` | Tests, then `flyctl deploy` — on push to `main` only. |
| `scripts/mint_refresh_token.py` | One-time local OAuth consent helper (Tasks + Gmail scopes). |
| `tests/test_sweep.py`, `tests/test_gmail.py` | `python3 -m unittest discover -s tests`. Cred-free. |

## How it runs

supercronic fires `/app/sweep` every 15 minutes. The sweeper stays a one-shot
script — no `while true; sleep 900` — so it is equally runnable locally and by
hand: `fly ssh console -C /app/sweep`. One run drains every adapter in turn;
each adapter logs `sweep start adapter=…`, a line per list/item, and its own
`sweep finish adapter=… ok=… created=… existed=… completed=… failed=…
skipped=… quarantined=… duration=…`.

### The adapter seam

Each source implements `enumerate` / `derive_capture` / `source_key` / `ack`
against the one shared engine, which owns everything load-bearing: the
Linear-first ordering, deterministic ids, transient-versus-terminal
classification, quarantine, per-adapter counts, and the ping. An adapter's
failure — even its own plumbing (token exchange, a missing capture label, a
missing healthcheck url) — fails only that adapter's result; the others drain
and report normally. One frozen `NAMESPACE` per source keeps id spaces
disjoint, each guarded by its own frozen test vector.

Isolation is why **each adapter validates its own config at its own boundary**.
`config_from_env()` requires only what both adapters share (the credentials);
a missing `$HEALTHCHECK_URL` or `$GMAIL_HEALTHCHECK_URL` is raised inside
`run_adapter` for that adapter alone, so one absent check can never stop the
other source's drain. That adapter then captures nothing: draining with its
dead-man's switch unarmed would be invisible, whereas a check that is never
pinged goes red on grace expiry, which is the switch working.

Per item, `describe`/`derive_capture`/id derivation sit **inside** the same
per-item `try` as the create and the ack, so a malformed item fails only itself
and the drain continues down the list.

A `fcntl.flock` on `$SWEEP_LOCK` (default `/tmp/sweep.lock`) is taken *inside*
the script rather than by a `flock -n` wrapper in the crontab, so it covers
supercronic, manual `fly ssh` runs, and local runs alike (and macOS has no
`flock(1)` for local runs). On contention the run logs, pings nothing, and
exits 0.

Exit codes: 0 = success, dry run, or lock contention; 1 = any failure.

## Per-item algorithm

For every item an adapter enumerates (Tasks: each incomplete task in each list
not in the denylist; Gmail: each message carrying the capture label):

1. `derive_capture(item)` — skip the item entirely if it carries nothing
2. `id = deterministic_v4(source_key, adapter.namespace)`
3. `issueCreate` in Linear with that client-supplied id
4. only on success → ack in the source (Tasks: `PATCH` to `status: completed`;
   Gmail: remove the capture label, and nothing else)
5. on a **transient** error → log it, **leave the task incomplete** (the next
   sweep retries), mark the sweep failed, continue to the next item
6. on a **terminal** error → log `QUARANTINE`, leave the task incomplete, and
   continue **without** failing the sweep (see Liveness)
7. after that adapter's last item, before the next adapter starts: no failures
   → ping *its* healthchecks success URL, carrying any quarantined/skipped
   summary as its body

**Create-in-Linear-first is load-bearing.** A crash between steps 3 and 4 can
only produce a visible duplicate attempt, never a silent loss — and the
deterministic id turns that retry into an "already exists" success.

### Idempotency

`IssueCreateInput` accepts a client-supplied `id`, but Linear validates it as
UUID **version 4 specifically** — a genuine RFC-4122 v5 uuid is rejected with
`id must be a UUID`, so `uuid.uuid5()` is not usable. `deterministic_v4()`
hashes `sha256(namespace + source_key)`, takes 16 bytes, and forces the
version and variant nibbles into v4 shape.

The namespaces in `sweep.py` — `NAMESPACE`
(`hummingbird-sweeper/google-tasks/v1`) and `GMAIL_NAMESPACE`
(`hummingbird-sweeper/gmail/v1`) — must never change. Every issue id an
adapter has ever minted derives from its namespace; changing one re-mints
every id in that source and duplicates every still-open capture. Frozen test
vectors in `tests/test_sweep.py` and `tests/test_gmail.py` guard them.

A duplicate create comes back as `code: INPUT_ERROR` with
`userPresentableMessage: "Entity Issue with id <uuid> already exists."` — the
sweeper matches that exactly and treats it as success. There is deliberately no
footer and no attachment on the issue: the UUID is the link (recomputable from
the task id), and the completed Tasks item is the audit trail.

### Field mapping (Google Tasks)

- **Title → title, verbatim.** No cleanup, truncation, or prefix.
- **Non-empty notes → description.** Empty notes → no `description` field.
- **Empty title, notes present → the first non-blank line of notes becomes the
  title**, and the full notes still become the description. Linear rejects an
  empty title outright (`minLength`), and the plausible real case is a
  dictation that landed entirely in the notes. Nothing is dropped; the first
  line is simply repeated as the handle.
- **Empty title *and* empty notes → skipped**, with a `WARN` line and a
  `skipped` count. These are the rows you get by pressing Enter in the Tasks
  app; they carry no information, so there is nothing to lose. The row is
  deliberately **not** marked complete — it stays visible for a human to
  delete, and re-warns every sweep until they do. Decided in
  [#24](https://github.com/JddAndrewLauren/hummingbird/issues/24).
- **Due date → dropped.** A Gemini-inferred date is a scheduling decision made
  by a transcription engine. The phrase ("Thursday") survives in the title, and
  a real date gets set deliberately during triage.

## The Gmail adapter

An inbox is a firehose, so Gmail inverts Tasks' fail-open posture to
**opt-in, fail-closed** (ADR-0002). The unit is the **message**, and the
`hummingbird/capture` label is the whole gesture:

- **Only labelled messages are enumerated.** Everything else is invisible to
  the sweeper — it never lists, reads, or touches an unlabelled message.
- **The label, not the mailbox, is the admission rule.** The listing passes
  `includeSpamTrash=true` (Gmail defaults it to false), so a deliberately
  labelled message is captured wherever it sits. Location silently overruling
  the gesture would be a second, invisible rule.
- **The label is rechecked on retrieval.** Listing and metadata fetch are two
  calls; if the label came off in between, the retrieved `labelIds` win and the
  message is skipped with a log line, not captured. Only a *currently* labelled
  message enters the drain.
- **The ack removes exactly that label from that message.** Never archive,
  mark-read, star, delete, or any other mutation. The message stays where it
  was as the audit trail; the thread deep link in the issue is the road back.
- **The label missing from the mailbox entirely fails the adapter** — visibly,
  on its own healthcheck — rather than enumerating anything. No gesture, no
  trust. Google Tasks keeps draining normally either way.

### Field mapping (Gmail)

- **Title** — the decoded, non-blank `Subject` header, verbatim. Blank or
  missing subject → the first non-blank line of the snippet → `(no subject)`.
  A labelled message earned capture by a deliberate human gesture, so unlike a
  blank Tasks row it is never skipped.
- **Description**, in a stable shape: `From:` (decoded sender), `Date:` (the
  message timestamp, UTC), `Thread:` (a Gmail thread deep link), then a blank
  line and the snippet when present.

Failure handling is the engine's, unchanged: transient errors leave the label
in place for the next sweep to retry; terminal content rejections quarantine
(the label stays, the run stays green, the item rides the success ping); a
crash between create and unlabel replays as an "already exists" success.

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
missed sweeps). **One check per capture adapter** (ADR-0002): a shared check
held red by one broken drain would hide the health of the others.
`$HEALTHCHECK_URL` is the Google Tasks check; `$GMAIL_HEALTHCHECK_URL` is the
Gmail check. Each adapter pings its own check independently, success or
failure, every run, **the moment that adapter finishes and before the next one
starts** — a later adapter grinding through 30-second timeouts must not hold an
earlier adapter's ping past the 45-minute grace and turn a healthy drain red.
An adapter whose url is unset fails itself and drains
nothing, leaving the other's reporting untouched. Success is pinged **only after that adapter's fully
successful, non-dry drain** — a sweeper that runs but errors on every call
must still trip the alarm. Any failure or exception POSTs that adapter's
accumulated failure lines to its check's `/fail` for immediate alerting. The
ping itself is wrapped in its own try/except and can never fail a run.

Fly health checks are explicitly *not* the mechanism: they restart, they don't
notify. Structural backstop: unswept items visibly accumulate in the Tasks app.

### No single item may hold the switch red forever

The invariant [#24](https://github.com/JddAndrewLauren/hummingbird/issues/24)
added, after two blank Tasks rows pinned the check red indefinitely: **a
permanently-red alarm is indistinguishable from a working one.** Retry-and-fail
is right only for failures that might clear, so failures are classified:

- **Transient** — 5xx, timeouts, a dead token, a Google `PATCH` failure, a list
  that won't enumerate. The next sweep might succeed. Behaviour is unchanged:
  leave the item, fail the run, `/fail`.
- **Terminal** — Linear refuses this capture's own content. The item is
  **quarantined**: logged as `QUARANTINE`, counted, left visible in Tasks, and
  the run still succeeds.

A rejection is terminal iff its status isn't 5xx and **every error it carries**
is a validation error (`INVALID_INPUT`, or an `INPUT_ERROR` that isn't
already-exists) naming only properties in `CONTENT_FIELDS` (`title`,
`description`). Each error has to earn quarantine separately — one recognized
`title` violation doesn't cover for a sibling error that explains nothing — and
a 5xx is transient whatever its body says, because a server that failed to
answer has told us nothing about the capture. The property test is the
guardrail: a bad capture can only be rejected on its own content, whereas a
wrong `teamId` or `stateId` — a broken sweeper, not a bad row — is rejected on
*that* field and stays transient. Anything unrecognized, including an error
naming no property, is transient too: fail loud is the default, and quarantine
has to earn itself.

`QUARANTINE_LIMIT` (10) is the backstop for shapes the rule can't read. Junk
rows arrive in ones and twos; more than ten in a single sweep means the
classification has stopped being trustworthy, so the run fails regardless.

Quarantine nobody can see would just be the original bug inverted, so
`skipped=` and `quarantined=` join the `sweep finish` line, and when a
successful sweep set anything aside its summary rides along as the **body of
the success ping**. The check reads green — capture is working — while its page
shows the junk accumulating.

**The ping URL is a bearer secret and is never logged.** Its path *is* the
credential — anyone holding it can forge a success ping and silence the alarm —
and sweeper stdout is the Fly log stream. The log lines say `healthcheck
success ping sent` / `healthcheck fail ping sent`, and exception text is passed
through `_redact()` before printing. A test asserts this. If it ever leaks,
healthchecks.io cannot rotate a ping URL in place: create a replacement check
and `flyctl secrets set HEALTHCHECK_URL=...`.

## Secrets

Set with `flyctl secrets set`; nothing on-device, nothing committed.

`GOOGLE_CLIENT_ID`, `GOOGLE_CLIENT_SECRET`, `GOOGLE_REFRESH_TOKEN`,
`LINEAR_API_KEY`, `HEALTHCHECK_URL`, `GMAIL_HEALTHCHECK_URL`.

Google auth is a **Workspace Internal** OAuth app (captures land in the
twinion.net Workspace account). Internal user type means no verification review
and no 7-day refresh-token expiry — that footgun applies only to apps in
Testing status. Desktop-app OAuth client, one-time local consent to mint the
refresh token, which carries both scopes:
`https://www.googleapis.com/auth/tasks` and
`https://www.googleapis.com/auth/gmail.modify` (read labelled messages +
remove the label; there is no narrower Gmail scope that can write labels).
Deferred alternative if token durability ever bites: a service account with
domain-wide delegation.

The Linear key goes in the `Authorization` header **raw, not `Bearer`**.
Constants in `sweep.py`: `teamId` `84ab9e0b-f455-42d7-a48a-49e65da3b2e6` (ION),
`stateId` `35cec1f9-df46-4212-9bef-8905015ad539` (Triage — verified to create
directly into Triage in one call).

Quota headroom (verified): Linear 2,500 req/hr, Google Tasks 50,000
queries/day. Nothing binds at this cadence.

## Human setup checklist

The Tasks-era steps were provisioned and live on 2026-08-07 and stay as the
rebuild runbook — what to redo if the Fly app, the OAuth client, or a
healthchecks check ever has to be recreated from scratch. The Gmail go-live
steps below them are new with [#45](https://github.com/JddAndrewLauren/hummingbird/issues/45).

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

### Gmail go-live

1. **Enable the Gmail API.** In the same Google Cloud project as the OAuth
   client, enable the Gmail API (`gcloud services enable gmail.googleapis.com`,
   or APIs & Services → Library → Gmail API → Enable). It is off by default in
   a fresh project — without it every Gmail call fails 403 with
   `Gmail API has not been used in project …` however good the token is. The
   Tasks API was enabled the same way during the 2026-08-07 provisioning.
2. **Label.** In the twinion.net Gmail account, create the label
   `hummingbird/capture` (in the UI this is a `capture` label nested under a
   `hummingbird` parent; the API sees the full `hummingbird/capture` name).
   Until it exists the Gmail adapter fails closed and red — that is by design.
3. **healthchecks.io.** Create a second, dedicated check (grace period 45
   minutes, like the first); record its ping URL. Leave it paused until
   go-live. Until `GMAIL_HEALTHCHECK_URL` is set the Gmail adapter fails and
   captures nothing, while Google Tasks keeps draining normally.
4. **Re-consent.** `python3 scripts/mint_refresh_token.py --client-id …
   --client-secret …` — the script now requests Tasks + `gmail.modify`
   together, so the one consent covers both adapters. Grant as the twinion.net
   account.
5. **Secrets.** `flyctl secrets set GOOGLE_REFRESH_TOKEN=<new token>
   GMAIL_HEALTHCHECK_URL=<new ping url>`. The old Tasks-only refresh token is
   superseded; nothing else changes.
6. **Dry run.** Label one test message, export the secrets locally, run
   `./sweep.py --dry-run`, and read both adapters' output — the Gmail adapter
   should log the labelled message and mutate nothing.
7. **Go live.** Push to `main` (which deploys), watch `flyctl logs`, unpause
   the new check, and verify one live capture: the labelled message appears in
   Linear Triage with the subject as title and the sender/date/thread-link
   description, and only the `hummingbird/capture` label disappears from it —
   still unarchived, still unread-state-untouched.

## Acceptance (post-provisioning)

All five verified live on 2026-08-07; the record is on map
[#35](https://github.com/JddAndrewLauren/hummingbird/issues/35).

- A dictated capture (phone/watch/speaker → Gemini → Tasks) appears in Linear
  Triage within ~15 minutes and is marked completed in Tasks.
- Killing the sweep between `issueCreate` and the `PATCH`, then rerunning,
  produces no duplicate issue and completes the task.
- Three consecutive failed or missed sweeps produce a healthchecks alert.
- A denylisted list is never touched; an unknown list id is swept.
- A blank Tasks row (no title, no notes) leaves the check **green**, stays put
  in Tasks, and logs one `WARN skipping empty capture` line per sweep; a
  notes-only row lands in Triage titled with its first line.

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
