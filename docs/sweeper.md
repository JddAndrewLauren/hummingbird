# The capture sweeper

> **Status: retargeted, awaiting its go-live gates.** The write side now
> targets the app-owned authority
> ([ADR-0008](adr/0008-the-authority-is-an-app-owned-server.md)) —
> `POST /api/items` with a `sweeper`-scope token — replacing Linear
> ([#123](https://github.com/JddAndrewLauren/hummingbird/issues/123)). The Fly
> machine is still stopped and both healthchecks still paused until the
> operator works the **Human setup checklist** below; the code change alone
> starts nothing. Captures have waited in their sources since 2026-08-08 —
> Tasks items incomplete, Gmail labels on — and the frozen namespaces make
> that whole backlog drain duplicate-free, because the ids the authority now
> receives are the ids Linear received. Go live = mint the token, run the
> gates, `flyctl machine start`, unpause the checks.

A one-way sweeper that drains capture sources into the owned authority's
Triage as bare-text items and reports its own liveness to healthchecks.io. One
drain engine, isolated adapters (ADR-0002): **Google Tasks** (every incomplete
item outside a denylist, fail-open; marked complete as the ack) and **Gmail**
(only messages carrying the `hummingbird/capture` label, fail-closed; the
label removed as the ack). No custom client, no endpoint. Spec: issues
[#14](https://github.com/JddAndrewLauren/hummingbird/issues/14) and
[#45](https://github.com/JddAndrewLauren/hummingbird/issues/45), decided in
[#5](https://github.com/JddAndrewLauren/hummingbird/issues/5),
[#8](https://github.com/JddAndrewLauren/hummingbird/issues/8), and
[#43](https://github.com/JddAndrewLauren/hummingbird/issues/43)/ADR-0002;
retargeted off Linear in
[#123](https://github.com/JddAndrewLauren/hummingbird/issues/123).

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
create-first ordering, deterministic ids, transient-versus-terminal
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
3. `POST /api/items` with that client-supplied id, the adapter's `source` and
   the raw `source_key` (Gmail also sends `source_url`, the thread deep link)
4. only on success → ack in the source (Tasks: `PATCH` to `status: completed`;
   Gmail: remove the capture label, and nothing else)
5. on a **transient** error → log it, **leave the task incomplete** (the next
   sweep retries), mark the sweep failed, continue to the next item
6. on a **terminal** error → log `QUARANTINE`, leave the task incomplete, and
   continue **without** failing the sweep (see Liveness)
7. after that adapter's last item, before the next adapter starts: no failures
   → ping *its* healthchecks success URL, carrying any quarantined/skipped
   summary as its body

**Create-in-authority-first is load-bearing**, exactly as create-in-Linear-first
was. A crash between steps 3 and 4 can only produce a visible duplicate attempt,
never a silent loss — and the deterministic id turns that retry into an
already-exists success. The window and its proof did not change with the
retarget; only the endpoint did.

### Idempotency

The create is idempotent by the client-supplied `id`: the authority answers a
replay of a known id with **200 and the stored row**, no write and no version
bump, while a fresh create is **201**. Both are success, and the sweeper
distinguishes them only to count `created=` versus `existed=`.

`deterministic_v4()` hashes `sha256(namespace + source_key)`, takes 16 bytes,
and forces the version and variant nibbles into v4 shape. The v4 shaping is
**historical**: Linear validated a client-supplied id as UUID version 4
specifically and rejected a genuine RFC-4122 v5 uuid with `id must be a UUID`,
so `uuid.uuid5()` was never usable. The owned authority treats `id` as an
opaque non-empty string and would accept either — but the function is frozen
anyway, because every id ever minted came out of it and keeping those ids
meaningful is the whole reason the deferred backlog drains clean. Do not
"simplify" it to uuid5.

The namespaces in `sweep.py` — `NAMESPACE`
(`hummingbird-sweeper/google-tasks/v1`) and `GMAIL_NAMESPACE`
(`hummingbird-sweeper/gmail/v1`) — must never change. Every item id an
adapter has ever minted derives from its namespace; changing one re-mints
every id in that source and duplicates every still-open capture. Frozen test
vectors in `tests/test_sweep.py` and `tests/test_gmail.py` guard them. They
survived the move off Linear for exactly this reason.

They are **not** the `items.source` strings. Those are provenance the
authority stores — `google-tasks/v1` and `gmail/v1`, carrying their own `/vN`
per ADR-0014 — alongside the raw `source_key` (the Google Tasks task id or
Gmail message id, the same string hashed into the item id) and, for Gmail,
`source_url` (the thread deep link). There is deliberately no footer and no
attachment on the item: the id is recomputable from the source key, and the
completed Tasks row / still-present message is the audit trail.

A 201 whose body is not the item asked for is treated as a **failure**, not a
create: the authority shares an origin with the PWA, so a misrouted request
gets the static shell back with a 2xx, and acking on that would discard the
capture.

### Field mapping (Google Tasks)

- **Title → title, verbatim.** No cleanup, truncation, or prefix.
- **Non-empty notes → description.** Empty notes → no `description` field.
- **Empty title, notes present → the first non-blank line of notes becomes the
  title**, and the full notes still become the description. An empty title is
  rejected outright (`title must be non-empty`; Linear said `minLength` and
  meant the same thing), and the plausible real case is a
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
- **Terminal** — the authority refuses this capture's own content. The item is
  **quarantined**: logged as `QUARANTINE`, counted, left visible in Tasks, and
  the run still succeeds.

A rejection is terminal iff it is a **400** whose body is
`{"error": "validation", …}` and whose `message` names a field in
`CONTENT_FIELDS` (`title`, `description`). The authority reports the offending
field in prose rather than a structured property, so the field is the message's
first word — which is the whole vocabulary the create route emits.

The guardrail is unchanged, only its vocabulary moved: **a bad capture can only
be rejected on its own content.** `title must be non-empty` is the capture's
fault; `id must be non-empty`, `priority must be between 0 and 4`,
`unknown project_id` and `deadline must be …` name fields the *sweeper*
supplied and mean a broken sweeper rather than a bad row, so they stay
transient and keep ringing — the job `teamId`/`stateId` used to do. A 5xx is
transient whatever its body says, because a server that failed to answer has
told us nothing about the capture. So is a `bad_json` rejection (the sweeper
built a body the route won't parse), a 401/403 (which answer with no body at
all), and anything else unrecognized: fail loud is the default, and quarantine
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
`HB_API_TOKEN`, `HEALTHCHECK_URL`, `GMAIL_HEALTHCHECK_URL`.

`HB_API_BASE` is **not** a secret and is normally unset: it defaults to
`https://hb.twinion.net` and exists only so a local run can be pointed at a
`wrangler dev` authority, the same knob every other client in the repo has.

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

`HB_API_TOKEN` is a **`sweeper`-scope** token (ADR-0008/0009 `tokens` table),
sent as `Authorization: Bearer …`. That scope reaches `POST /api/items` and
nothing else — it cannot read the sweep, patch an item, or write an alert — so
it is the narrowest credential in the system after the ingest tokens. It is
still a write credential: treat a leak as one, `DELETE /api/admin/tokens/:id`
to revoke, then mint and set a replacement.

There is no team or state vocabulary to resolve any more. The landing stage is
the owned schema's Triage, which is the create route's own default, so the
sweeper does not send `stage` at all.

Quota headroom: the authority is our own Worker and imposes no rate limit;
Google Tasks is 50,000 queries/day (verified). Nothing binds at this cadence.

## Human setup checklist

The Tasks-era steps were provisioned and live on 2026-08-07 and stay as the
rebuild runbook — what to redo if the Fly app, the OAuth client, or a
healthchecks check ever has to be recreated from scratch. The Gmail go-live
steps below them are new with
[#45](https://github.com/JddAndrewLauren/hummingbird/issues/45) and **were
never performed**: the code landed but the live verification would have minted
into a Linear workspace already being wound down, so they carried forward to
[#123](https://github.com/JddAndrewLauren/hummingbird/issues/123) and are the
gates that still stand between here and a running sweeper.

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

### Go-live after the retarget (#123)

The steps that remain before the sweeper can run at all. None of them is agent
work; every one touches a live account.

1. **Mint the sweeper token.** `POST /api/admin/tokens` against
   `https://hb.twinion.net`, authenticated with `ADMIN_SECRET` from the
   operator's terminal (never Actions — see the credential blast radius rule in
   `CLAUDE.md`), body
   `{"id": "<uuid>", "name": "hummingbird-sweeper", "scope": "sweeper"}`.
   Omit `source`: it is required for `ingest` and **rejected** for every other
   scope. The plaintext `hb_…` comes back exactly once and is unrecoverable
   afterwards. `runner/scripts/mint-hb-token.sh` does this for the runner's
   device token and is the shape to copy.
2. **Set it and drop the old one.** `flyctl secrets set HB_API_TOKEN=<token>`
   and `flyctl secrets unset LINEAR_API_KEY` on `hummingbird-sweeper`.
3. **Dry run over the deferred backlog.** Export the secrets locally and run
   `./sweep.py --dry-run`. This matters more than it did in #45: months of
   un-drained Tasks items and labelled messages arrive in the first real run,
   so read the volume and confirm it is the volume you expect before anything
   mutates. A dry run touches neither side and pings nothing.

Then the Gmail steps below, which were deferred from #45 and never ran.

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
7. **Go live.** Push to `main` (which deploys), `flyctl machine start`, watch
   `flyctl logs`, unpause both checks, and verify one live capture of each
   kind. The labelled message appears in the authority's Triage with the
   subject as title and the sender/date/thread-link description, and only the
   `hummingbird/capture` label disappears from it — still unarchived, still
   unread-state-untouched. Then re-run the sweep immediately and confirm it
   creates nothing new: the frozen namespaces still hold across the authority
   change.

## Acceptance (post-provisioning)

All five were verified live against Linear on 2026-08-07; the record is on map
[#35](https://github.com/JddAndrewLauren/hummingbird/issues/35). They are
restated here against the owned authority, and are what the operator re-checks
after the go-live gates above — the retarget changed the destination, not a
single one of these properties.

- A dictated capture (phone/watch/speaker → Gemini → Tasks) appears in the
  authority's Triage within ~15 minutes and is marked completed in Tasks.
- Killing the sweep between the create and the `PATCH`, then rerunning,
  produces no duplicate item and completes the task.
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
  Qualified 2026-08-10 (#120): three of those four clauses were about a
  *private* repo's Actions billing, and hummingbird is public, so the ban
  that survives in general is only the 60-day auto-disable. **For the sweeper
  it still holds absolutely** — supercronic is a live clock inside the Fly
  container and an Actions cron would compete with it, which is a
  correctness argument, not a billing one. `.github/workflows/city-waste.yml`
  is the deliberate exception: a daily poll with no competing clock anywhere,
  and self-monitoring (its pane bands its own answer stale at 26h and then
  refuses to answer), so a stalled run is visible within a day.
- **Never add `[http_service]` or `[[services]]`** to `fly.toml` — either would
  let Fly's autostop machinery suspend the worker between sweeps.
