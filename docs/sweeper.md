# The capture sweeper

> **Status: live since 2026-08-12.** The write side targets the app-owned
> authority ([ADR-0008](adr/0008-the-authority-is-an-app-owned-server.md)) —
> `POST /api/items` with a `sweeper`-scope token — replacing Linear
> ([#123](https://github.com/JddAndrewLauren/hummingbird/issues/123)). Machine
> `d8de469c7e13d8` is started (its standby deliberately is not), both
> healthchecks are unpaused and green, and supercronic has ticked `*/15` since
> 23:00Z that day. The **Human setup checklist** below is worked and stays as
> the record of what go-live cost and the procedure for rebuilding this from
> nothing. Both lanes sweep empty in the steady state now — what the first
> live sweeps actually established is under *Acceptance (post-provisioning)*.

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
| `crontab` | Six staggered entries read by supercronic inside the container: `/app/sweep` (`*/15`) plus the five poller binaries below (#774) — see the file's own header for the staggering and CLAUDE.md's "no competing clocks" rule. |
| `Dockerfile` | `python:3.12-slim` + supercronic pinned by version and sha256, plus a `rust:1.97.1-slim` builder stage (#774, on `runner/Dockerfile`'s own pattern) that compiles the five poller binaries `crontab` runs. |
| `fly.toml` | `hummingbird-sweeper`, one 256MB always-on worker; `[env]` carries the non-secret config the five pollers need (#774). |
| `.github/workflows/deploy.yml` | Tests on push to `main` and on `pull_request`; `flyctl deploy` on push to `main` only. Redeploys on a `server/**`, `rust-toolchain.toml` or `.github/workflows/**` change too (#774), so a poller edit actually ships. |
| `server/{gmail-poll,calendar-poll,graph-poll,github-status}` | The five poller binaries this machine now runs (#774) — `hummingbird-gmail-poll`, `hummingbird-calendar-poll`, `graph-mail-poll`, `graph-calendar-poll`, `github-status-poll`. Each crate documents itself (ADR-0011 for the first four, ADR-0017 decision 2 for the fifth); this doc covers only their presence on this machine, not their own behaviour. |
| `scripts/mint_refresh_token.py` | One-time local OAuth consent helper (Tasks + Gmail + Calendar scopes by default; `--scope` mints a dedicated narrower credential — its header says when that is the right call). |
| `tests/test_sweep.py`, `tests/test_gmail.py` | `python3 -m unittest discover -s tests`. Cred-free. |

## How it runs

supercronic fires `/app/sweep` every 15 minutes. The sweeper stays a one-shot
script — no `while true; sleep 900` — so it is equally runnable locally and by
hand: `fly ssh console -C /app/sweep`. One run drains every adapter in turn;
each adapter logs `sweep start adapter=…`, a line per list/item, and its own
`sweep finish adapter=… ok=… created=… existed=… completed=… failed=…
skipped=… quarantined=… duration=…`.

**Since #774, this machine also runs five other one-shot binaries off the
same `crontab`**, staggered off `/app/sweep` and off one another: the four
evaluated-stream pollers (`hummingbird-gmail-poll`,
`hummingbird-calendar-poll`, `graph-mail-poll`, `graph-calendar-poll`, every
15 minutes) and `github-status-poll` (every 30). They share nothing with
`sweep.py` beyond the clock and the container — no lock, no shared state —
and each is exactly the same binary GitHub Actions used to run, just invoked
by supercronic instead of `actions/checkout@v4` + `cargo run`. Their own
crates document what each run does; this file's job is only "why they run
here now" (#773's measurements: Actions `schedule:` delivered them roughly
once every four hours against their declared cadence, and this machine is
already an always-on clock that can just run them on time).

### The adapter seam

Each source implements `enumerate` / `derive_capture` / `source_key` / `ack`
(plus `describe_ack`, which is only what the dry run narrates) against the one
shared engine, which owns everything load-bearing: the
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
**opt-in, fail-closed** (ADR-0002). The **capture unit is the conversation;
the key stays the message** (ADR-0019, #336) — deliberately not the same
thing. The `hummingbird/capture` label is the whole gesture:

- **Only labelled messages are enumerated.** Everything else is invisible to
  the sweeper — it never lists, reads, or touches an unlabelled message.
- **The label, not the mailbox, is the admission rule.** The listing passes
  `includeSpamTrash=true` (Gmail defaults it to false), so a deliberately
  labelled message is captured wherever it sits. Location silently overruling
  the gesture would be a second, invisible rule.
- **The label is rechecked on retrieval.** Listing and metadata fetch are two
  calls; if the label came off in between, the retrieved `labelIds` win and the
  message is skipped with a log line, not captured, and it can never be a
  thread's winner. Only a *currently* labelled message enters the drain.
- **One conversation, one capture per sweep.** Gmail's UI applies the label at
  **thread** granularity, so a forward chain labels every message in it.
  Enumeration groups the still-labelled, retrieved messages by `threadId` and
  selects one **winner**: the **oldest labelled message by `internalDate`**,
  with the message id as a deterministic tiebreak. Oldest, not newest — a
  forward arriving between sweeps moves "newest" and must never move which id
  a thread mints, since an observer-dependent key is exactly what turns a
  replay into a duplicate. A message whose `internalDate` cannot be read at all
  sorts **last** — it never beats a message carrying a real timestamp, and a
  thread of only such messages still collapses by the message-id tiebreak
  rather than crashing. Gmail always supplies the field, so that is a guard
  against a shape nobody has seen rather than an observed one.
  The **id derivation is untouched**: the winner's
  message id is still what `deterministic_v4` hashes; there is no thread-keyed
  id and none is planned (ADR-0019 rejects it by name).
- **Losing messages are acked without creating.** A thread's non-winning
  messages mint nothing and count as no capture, but their label is removed
  too, so they do not re-enumerate forever.
- **The collapse is thread-atomic and fail-closed.** Losing messages are acked
  **only after the winner's create has succeeded**; if that create fails,
  nothing in the thread is acked and the whole conversation stays labelled for
  the next sweep to re-enumerate and retry. One thread's failure never touches
  another's — the engine's per-item isolation (below) is unchanged.
- **The ack removes exactly that label from that message.** Never archive,
  mark-read, star, delete, or any other mutation. The message stays where it
  was as the audit trail; the thread deep link in the issue is the road back.
  This applies to the winner *and* every collapsed message.
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

Each item gets a `DRY-RUN would create …` line and a `DRY-RUN would ack …` line.
The ack line names every mutation the ack would make, not just the one item's:
for a collapsed Gmail thread it names the sibling messages it would unlabel too
(`… and unlabel 2 collapsed message(s) in thread <id>: <id>, <id>`), because a
dry run that mentioned only the winner under-narrated N−1 real mutations.

## Denylist

`denylist.json` is committed, keyed by list `id` with the human-readable title
as the value — ids are rename-proof, titles alone break silently. A list is
skipped iff its id is a key. A stale or unknown id **fails open**: the list
gets swept, not skipped. Noise in Triage, never a lost capture. Changing it is
a normal push-and-deploy.

`Shopping` is the only entry. `My Tasks` and `Default List` are both swept, and
leaving `Default List` out of the file is a **decision taken at go-live on
2026-08-12**, not an omission — it was empty at the time and is meant to stay
sweepable. Recorded here because failing open makes an unprotected list and a
forgotten one look identical from inside the file.

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

**A per-adapter green proves that adapter's drain ran without error — nothing
more.** An empty enumeration (nothing to capture) makes no authority call at
all, so both lanes ping green on a day with nothing to do, and since the
2026-08-12 go-live an empty drain is the steady state for both
([#328](https://github.com/JddAndrewLauren/hummingbird/issues/328)). Neither
adapter check is evidence the authority is reachable; only the third check
below is.

### The authority-reachability check

A third healthchecks.io check, owned by no adapter and no capture source
(ADR-0002 rule 6, amended — see the ADR-0002 inline amendment). One probe per
sweep, independent of both drains: a `GET` to an existing `/api/` route
(`/api/rules`) carrying a **deliberately invalid** bearer token, via the same
`http_json` choke point every other request uses (so it inherits `USER_AGENT`
too). Success is **exactly 401** — the authority resolves a bearer by
querying the `tokens` table before it can answer 401, so a 401 proves edge,
Worker *and* storage are all live; a storage fault surfaces as 500. A 403 is
not a pass: Cloudflare's own Browser Integrity Check also answers 403 (#326),
so 403 cannot distinguish "the authority said out-of-scope" from "the edge
blocked us." Any 403, any 5xx, any timeout, or any connection error fails the
probe. The probe never sends `$HB_API_TOKEN` and never writes — there is no
benign authenticated read on the `sweeper` scope to spend instead, since that
scope reaches only `POST /api/items`.

`$AUTHORITY_HEALTHCHECK_URL` is this check's ping url. **Blast radius is
purely observational**: the probe never fails the run and never touches
either adapter's result — both drains attempt exactly what they would have
otherwise, whatever the probe found, so the sweep itself stays fail-closed
through the adapters as always. Unset is **inert**, unlike a missing adapter
check: the probe is skipped with a `WARN` line rather than failing anything,
because this code is meant to land before the check exists. A dry run pings
nothing, same convention as both adapters.

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

`collapsed=` joins the same `sweep finish` line (#336), counting the Gmail
adapter's non-winning thread messages that were acked without creating; it is
**never** carried in the success ping body, unlike `skipped=`/`quarantined=`
above — a collapsed message is normal operation, not something set aside for
a human to look at. Each collapsed message also gets its own log line naming
its thread and the winning message id.

`collapsed=` counts the messages **actually unlabelled**, incremented as each
one is, on the same line as that message's log line — so the count and those
log lines can never disagree. That matters on the one path where they could: a
sibling's unlabel raising partway through a thread. The thread stays atomic
(the winner's create had already succeeded, and nothing is acked before it), the
item is counted `failed=` and left for the next sweep, and the collapses that
did happen are still reported rather than silently becoming `collapsed=0`
beside log lines saying otherwise.

**The ping URL is a bearer secret and is never logged.** Its path *is* the
credential — anyone holding it can forge a success ping and silence the alarm —
and sweeper stdout is the Fly log stream. The log lines say `healthcheck
success ping sent` / `healthcheck fail ping sent`, and exception text is passed
through `_redact()` before printing. A test asserts this.

Each of those lines names the check it belongs to: `adapter=google-tasks` and
`adapter=gmail` for the two capture adapters, and `check=authority` for the
reachability check, which belongs to no adapter and no capture source and
must not be logged as one. Both adapters' lines are unchanged. If it ever leaks,
healthchecks.io cannot rotate a ping URL in place: create a replacement check
and `flyctl secrets set HEALTHCHECK_URL=...`.

## Secrets

Set with `flyctl secrets set`; nothing on-device, nothing committed.

`GOOGLE_CLIENT_ID`, `GOOGLE_CLIENT_SECRET`, `GOOGLE_REFRESH_TOKEN`,
`HB_API_TOKEN`, `HEALTHCHECK_URL`, `GMAIL_HEALTHCHECK_URL`,
`AUTHORITY_HEALTHCHECK_URL`.

**Seven more, since #774**, one per moved poller (plus the Graph pollers'
shared signing key): `GMAIL_INGEST_TOKEN`, `CALENDAR_INGEST_TOKEN`,
`M365_MAIL_INGEST_TOKEN`, `M365_CALENDAR_INGEST_TOKEN`,
`GH_STATUS_INGEST_TOKEN` (each an `ingest`-scope token bound to that
poller's own source, minted from the operator's terminal against
`ADMIN_SECRET` — never Actions, per CLAUDE.md's credential blast-radius
rule), `GRAPH_CLIENT_PRIVATE_KEY` (the Graph app registration's signing
key — see `.github/workflows/graph-mail-poll.yml`'s header for why it sits
on the `ADMIN_SECRET` side of that rule rather than the ingest-token side),
and `GH_STATUS_PAT` (a fine-grained PAT, `contents: read` + `actions: read`
on this repo only — `.github/workflows/github-status.yml`'s header explains
why this machine needs one where the Actions run never did). `crontab`
maps each ingest token onto the singular `HB_INGEST_TOKEN` every poller
binary reads, and `GH_STATUS_PAT` onto `GITHUB_TOKEN`, on the command line
— see that file's own entries.

`HB_API_BASE` is **not** a secret and is normally unset: it defaults to
`https://hb.twinion.net` and exists only so a local run can be pointed at a
`wrangler dev` authority, the same knob every other client in the repo has.

Google auth is a **Workspace Internal** OAuth app (captures land in the
twinion.net Workspace account). Internal user type means no verification review
and no 7-day refresh-token expiry — that footgun applies only to apps in
Testing status. Desktop-app OAuth client, one-time local consent to mint the
refresh token, which carries three scopes:
`https://www.googleapis.com/auth/tasks`,
`https://www.googleapis.com/auth/gmail.modify` (read labelled messages +
remove the label; there is no narrower Gmail scope that can write labels), and
`https://www.googleapis.com/auth/calendar.readonly`.
Deferred alternative if token durability ever bites: a service account with
domain-wide delegation.

**This one credential is every Google consumer in the repo** (operator decision
on [#486](https://github.com/JddAndrewLauren/hummingbird/issues/486), which also
closed out #135's open question): the sweeper reads it from a Fly secret, and
`gmail-poll` and `calendar-poll` read it from the GitHub Actions secret of the
same name. The calendar scope exists for the poller, not for anything here —
the sweeper never touches Calendar. The consequence is that **re-minting is a
three-place operation**: 1Password, then `flyctl secrets set`, then
`gh secret set`. Leave one behind and the lane reading it fails on a revoked
grant, not on a missing secret, which is the harder failure to read.

**#774 narrows that three-place rotation rather than widening it.** Both
pollers now read `GOOGLE_REFRESH_TOKEN` (and its two siblings) from this
machine's own Fly secret — the same one the sweeper already held — instead
of also carrying an independent Actions-secret copy. Once the now-unused
`GOOGLE_*` Actions secrets are deleted (this issue's own "Verification the
agent cannot do" step 7), re-minting drops back to the two places every
other Fly-only credential here already needs: 1Password, then
`flyctl secrets set`. Until that deletion happens both copies are live and
the three-place rotation above still applies.

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
   afterwards — so **use `scripts/mint-hb-token.sh`**, which mints and stages the
   secret in one pass and keeps the plaintext out of a shell history. Set
   `HB_TOKEN_OUT` when you run it: the mint takes the token `id` the moment it
   succeeds, so if the staging step then fails, the only copy of an unreissuable
   plaintext dies with the process. (`runner/scripts/mint-hb-token.sh` is the
   same gesture for the runner's *device* token; the two divergences are
   documented in this one's header.)
2. **Set it and drop the old one.** The script above stages `HB_API_TOKEN`
   already; `flyctl secrets unset --stage LINEAR_API_KEY` drops the stale key.
   Use `--stage` for both while the machine is stopped: a plain `secrets set`
   restarts the machines, which hands Fly the choice of when supercronic starts
   ticking.
3. **Dry run over whatever is waiting.** Export the secrets locally and run
   `./sweep.py --dry-run`, read the volume, and confirm it is the volume you
   expect before anything mutates. A dry run touches neither side and pings
   nothing. When this was written it braced for months of un-drained Tasks
   items arriving in the first real run; by the 2026-08-12 go-live the Tasks
   lane had drained itself and the run reported nothing. Read the volume, but
   do not read a small one as a fault.

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
   --client-secret …` — the script requests Tasks + `gmail.modify` +
   `calendar.readonly` together, so the one consent covers both sweeper
   adapters *and* the two Google poller lanes. Grant as the twinion.net
   account.
5. **Secrets.** Store the token on the 1Password item first and read it back,
   then `flyctl secrets set GOOGLE_REFRESH_TOKEN=<new token>
   GMAIL_HEALTHCHECK_URL=<new ping url>` and
   `gh secret set GOOGLE_REFRESH_TOKEN` (plus `GOOGLE_CLIENT_ID` /
   `GOOGLE_CLIENT_SECRET`, which `gmail-poll` and `calendar-poll` also read).
   Any earlier, narrower refresh token is superseded; nothing else changes.
6. **Dry run.** Label one test message, export the secrets locally, run
   `./sweep.py --dry-run`, and read both adapters' output — the Gmail adapter
   should log the labelled message and mutate nothing.
7. **Go live.** Push to `main` (which deploys), then, **in this order**:

   a. **Unpause both checks first.** healthchecks.io has a per-check rule for
      what a *paused* check does with an incoming ping, and one setting of it
      discards the ping and stays paused. Unpausing before anything can ping
      makes that rule unreachable; unpausing afterwards leaves a window where
      the first ping — success or fail — can vanish silently, which is a live
      sweeper with no dead-man's switch. While you are there, confirm the new
      check has a notification integration attached: healthchecks.io does not
      reliably copy them onto a new check, and one without any goes red with
      nobody told.
   b. **`flyctl machine start <machine-id>` — by id, never bare.** There are
      two machines and the second is a standby. A bare `flyctl machine start`
      can bring up both, which is two supercronics racing each other —
      **six jobs each, since #774**, not the one `*/15` sweep this warning
      was first written against: `/tmp/sweep.lock` is per-container and
      protects `/app/sweep` alone, and the five poller binaries share no
      lock at all, so a doubled machine means every one of the six racing
      its own twin. That is the competing-clocks failure `CLAUDE.md` bans,
      by a route [#8](https://github.com/JddAndrewLauren/hummingbird/issues/8)
      did not anticipate. Confirm with `flyctl machine list` that the
      standby stayed stopped.

   Note that `flyctl deploy` does **not** start a stopped machine — it updates
   it in place and leaves it stopped. The start above is always its own action.
   Nothing runs at boot either: `crontab` is `*/15` and supercronic fires on
   the boundary, so the first sweep is up to fifteen minutes after the start.

   Then watch `flyctl logs` and verify one live capture of each kind. The
   labelled message appears in the authority's Triage with the subject as title
   and the sender/date/thread-link description, and only the
   `hummingbird/capture` label disappears from it — still unarchived, still
   unread-state-untouched.

   Then check the replay guard — but note that **a bare re-run cannot check
   it**, and reads like a pass while checking nothing. The ack removed the
   label, `gmail_list_message_ids` enumerates strictly by `labelIds`, and so a
   sweep run straight after a successful one logs `carries 0 message(s)` and
   finishes `created=0 existed=0` having made no authority call at all. Google
   Tasks has the same shape once its lane is drained. What the guard needs is
   something to enumerate: re-apply `hummingbird/capture` to a message the
   sweep just acked, give Gmail's index a minute or two — `messages.list`
   trails a label mutation — and sweep again. The pass condition is
   **`existed=1`**, the authority answering the replayed id with 200 and the
   stored row, alongside `created=0`, an unchanged `version` on that item, and
   the label acked off a second time. `created=1` and a second item for the
   same message means the frozen namespace did not survive whatever changed
   underneath it; that is the failure this step exists to catch, and the only
   one that matters here.

   Labelling a thread that carries a forward chain is no longer a hazard for
   this check ([#336](https://github.com/JddAndrewLauren/hummingbird/issues/336),
   ADR-0019): the adapter now collapses every labelled message in a
   conversation to the one winner it created. Re-labelling from the
   **conversation view** puts the label back on every message of the
   already-acked thread; they re-group under the same `threadId` and the
   winner is once again the same oldest message, so the check still gets
   `existed=1`, now alongside `collapsed=N-1` for the rest. Labelling a
   single **newer** message on its own instead enumerates as a thread of
   one and legitimately mints `created=1` — the ADR-0019 recapture the
   adapter is designed to allow, not the namespace break the pass condition
   above exists to catch.

### Authority-reachability go-live (#328)

The code lands inert — `$AUTHORITY_HEALTHCHECK_URL` unset skips the probe
with a `WARN` and fails nothing — so this can be provisioned whenever, on its
own schedule, independent of the Tasks/Gmail gates above.

1. **healthchecks.io.** Create a third, dedicated check — same 45-minute
   grace period as the other two; record its ping URL. Leave it paused until
   go-live, same reasoning as the other checks.
2. **Secret.** `flyctl secrets set AUTHORITY_HEALTHCHECK_URL=<ping url>`.
3. **Unpause and verify.** After a live sweep, confirm the check went green,
   then prove the failure path once by hand: point `HB_API_BASE` at something
   unreachable for a manual `./sweep.py` run (not `--dry-run`, since a dry run
   pings nothing) and confirm the check goes red while both adapter checks —
   run against the same broken base — behave exactly as they did before this
   issue.

## Acceptance (post-provisioning)

All five were verified live against Linear on 2026-08-07; the record is on map
[#35](https://github.com/JddAndrewLauren/hummingbird/issues/35). They are
restated here against the owned authority, and are what the operator re-checks
after the go-live gates above — the retarget changed the destination, not a
single one of these properties.

The owned-authority go-live then ran on 2026-08-12. Three things it established
differ from what the gates above expect, and the difference is the point:

- **The Tasks lane arrives empty, and that is its steady state.** The deferred
  backlog the checklist braces for drained itself between 08-07 and 08-11, so a
  healthy Tasks sweep now reports `created=0` and will until something new is
  dictated. The "months of un-drained items arrive in the first real run"
  expectation is retired; a first run reporting nothing is no longer evidence
  that anything is wrong.
- **Both adapters ping their check green on an empty sweep**, having made no
  authority call at all. A green check therefore no longer implies the
  authority is reachable —
  [#328](https://github.com/JddAndrewLauren/hummingbird/issues/328) holds that,
  filed against the Tasks lane and true of both.
- **The frozen namespaces survived the change of authority.** A message
  relabelled after its ack replayed as `existed=1` against an id minted for
  Linear, with the stored item's `version` unbumped — the property the whole id
  derivation exists to protect, now checked against the owned API rather than
  argued from the code.

- A dictated capture on **this lane** (phone/watch/speaker → Gemini → Tasks)
  appears in the authority's Triage within ~15 minutes and is marked completed
  in Tasks. This is one of two dictation lanes now: the other is Hummingbird's
  own client-local voice capture straight into the capture box (ADR-0022),
  which never touches Tasks, Gemini, or this sweeper at all.
- Killing the sweep between the create and the `PATCH`, then rerunning,
  produces no duplicate item and completes the task.
- Three consecutive failed or missed sweeps produce a healthchecks alert.
- A denylisted list is never touched; an unknown list id is swept.
- A blank Tasks row (no title, no notes) leaves the check **green**, stays put
  in Tasks, and logs one `WARN skipping empty capture` line per sweep; a
  notes-only row lands in Triage titled with its first line.

## Changing things

- **Cadence** — one line per job in `crontab` (six since #774, one per
  binary), still the cheapest decision in the system to reverse; each
  poller's own `POLLED_EVERY_MS` must move with its line, and the drift-gate
  test in that crate's `tests/contract.rs` will say so if it doesn't.
- **supercronic version** — bump `SUPERCRONIC_VERSION` in the `Dockerfile` and
  replace `SUPERCRONIC_SHA256` with the real `shasum -a 256` of the downloaded
  release asset. The project publishes no checksum file; never invent the hash.
- **Never add a `schedule:` trigger** to `.github/workflows/deploy.yml`, nor
  restore one to `gmail-poll.yml`, `calendar-poll.yml`, `graph-mail-poll.yml`,
  `graph-calendar-poll.yml` or `github-status.yml` (#774 dropped theirs
  deliberately; `workflow_dispatch:` on each still allows a manual run).
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
