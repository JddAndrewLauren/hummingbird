# ADR-0011: Context ingestion moves server-side; rules evaluate in the poll, and only matches persist

**Status:** accepted · 2026-08-09 · **amended 2026-08-10 by #135's review
round:** the Gmail poller (and the M365/calendar pollers #136/#137 will
follow onto the same scaffolding) runs **out-of-process**, on a GitHub
Actions `schedule:`, not "DO cron + daemon credential" as first decided
below — see [Amendment: the poller runs out-of-process, and its credential
is narrowed accordingly](#amendment-the-poller-runs-out-of-process-and-its-credential-is-narrowed-accordingly).
The evaluate-in-poll persistence principle, the per-source delta cursor, and
everything else below is unchanged.
**Amended 2026-08-19 by
[ADR-0028](0028-the-web-host-mints-its-google-token-at-the-authority.md):**
partially reverses the #486 amendment below for one consumer — a second,
dedicated `calendar.readonly`-only Google credential is minted for the web
host's authority-served calendar token, rather than reusing the shared
Tasks + `gmail.modify` + `calendar.readonly` credential #486 settled on. #486
declined a second credential because both its consumers were server-side;
this one hands a token to a browser, which is a different trust boundary.
**Context:** the push-notifications grilling of 2026-08-09. Narrows
[ADR-0005](0005-context-polling-lives-in-the-client-core.md) (device polling
remains the display path; a server-side ingestion path now exists beside it);
amends [ADR-0009](0009-the-owned-schema-and-context-lanes.md) (rule 3 is
scoped to push sources, and the server-polled lane gains evaluated streams).
Enables [ADR-0012](0012-the-notification-lane.md), which owns what happens to
a match. Two /tradeoff analyses were run live (calendar path; persistence
policy); their decisive facts are recorded below.

## Decision

**The server polls the operator's mail and calendar streams itself, and the
notification rule engine (ADR-0012) evaluates each poll batch in memory —
only matches persist, as alerts. Non-matching stream data never rests in the
authority.**

Four streams, two credentials, all held as Worker secrets:

| Stream | Credential | Notes |
| --- | --- | --- |
| Gmail | the existing sweeper refresh token (`gmail.modify`, Workspace **Internal** OAuth app) | no new consent; Internal apps are exempt from the 7-day testing expiry (verified 2026-08-09) |
| Google Calendar | same OAuth app, scope re-mint per the `docs/sweeper.md` runbook (`calendar.readonly` added) | the exact procedure used for the Gmail go-live |
| M365 mail | app-only Graph: application permission `Mail.Read`, client credentials | own tenant; admin consent granted by the operator |
| M365 calendar | app-only Graph: `Calendars.Read`, same app registration | certificate preferred over client secret (secrets are portal-capped at 24 months) |

### The premise correction that unlocked this

ADR-0005's daemon-auth objection — "fragile to impossible" — was, by its own
text, the *M365 leg* against a **corporate tenant under conditional-access
policy**. The operator's actual M365 tenant is their own: admin consent for
application permissions is fully available, and the trap does not exist. The
Google leg was never a trap at all — the sweeper has run headless on a
Workspace-Internal refresh token since go-live, with a rebuild runbook.
ADR-0005's other two arguments (per-device consent/revocation,
freshest-mirror-on-the-device-in-hand) are display-path arguments and are
untouched: **devices keep their own polled mirrors for display, exactly as
ADR-0005 decided.** This ADR narrows it; nothing is repealed.

### Evaluate-in-poll: the persistence principle

**The authority holds only what rules promoted, never the streams they were
promoted from.**

- Each stream keeps a **per-source delta cursor** in the DO (Gmail
  `historyId`, Graph delta links, calendar sync tokens) — the same cursor
  concept the sync engine already uses for its own delta pull.
- A poll fetches the batch since the cursor, hands it to the rule engine **in
  memory**, materializes matches as ordinary `alerts` rows (upsert on
  `(source, source_key)`, so re-fetch can never double-raise), drops
  everything else, and advances the cursor. Losing a cursor degrades to
  re-fetch-and-upsert, which the dedupe key absorbs.
- Rule edits apply **prospectively** — from the next poll onward. A
  notification about a stale event is noise by definition, so retroactive
  firing is a non-feature.
- **Backtesting needs no persistence.** "Would this rule have fired?" is
  answered at rule-edit time by re-fetching recent history *from the source*
  (Gmail search, Graph `$filter` on `receivedDateTime`) and running the draft
  rule in memory. The source already stores the history; copying it into the
  DO to query it is redundant.

Why no staging table, even with a TTL: ADR-0008's backup story is 30-day
point-in-time recovery, on by default — **anything written to DO storage is
recoverable for 30 days, including deleted rows**. A "48-hour TTL" candidate
table would rest the operator's mailboxes in the authority's recovery stream
for a month. The TTL is a fiction at the recovery layer; the only way mail
metadata stays out of the authority is to never write it.

### ADR-0009's lane table gains a row

| Lane | Transport | Examples | Lifecycle |
| --- | --- | --- | --- |
| Context, **evaluated stream** | DO cron + daemon credential; per-source delta cursor; rules evaluate in-poll | Gmail, M365 mail, Google/M365 calendar events | matches persist as `alerts`; non-matches are never written |

The existing server-polled *snapshot* lane (rows replaced wholesale) is
unchanged and remains the home for gauges. The distinction is lifecycle:
snapshots are state that is *kept*, streams are events that are *judged*.

### Rule 3, rewritten

ADR-0009 rule 3 ("authorities stay authoritative: hummingbird receives and
never configures their rules") described push sources, and for them it stands
verbatim — Home Assistant still decides what to send. For streams hummingbird
polls itself there is no external rule-holder to defer to, so the rule
gains a second clause:

> **3.** Push sources stay authoritative: Home Assistant decides what to
> send; hummingbird receives and never configures their rules. For streams
> hummingbird polls itself, the operator's own rules (ADR-0012) are the
> arbiter: the source remains authoritative over *what its data is*;
> hummingbird decides *what matters*.

### Latency, and the upgrade path that changes nothing

Cron polling (the DO alarm) bounds notification latency to the poll
interval. Both providers offer push for the same data — Gmail `users.watch`
via Pub/Sub, Graph change notifications — and both are pure latency
upgrades: a webhook replaces the timer as the trigger, the batch still flows
through the same cursor, the same in-memory evaluation, the same persistence
principle. Deferred until the poll interval demonstrably chafes. (Graph
subscriptions expire in days and renewal failure is silent notification
loss — the operational cost is real, which is why polling is v1.)

## Rejected alternatives

- **Devices replicate calendar/mail state up to the server.** The only
  option that could ever cover a *corporate* M365 tenant (delegated MSAL on
  the phone), and it died with the premise: the server's picture would be
  exactly as fresh as the last device cycle, going stale when devices are
  dormant — **the away-from-your-devices scenario is what push notifications
  exist for**. It would also have inverted the sync contract (context
  flowing up from a device for the first time) for an API surface and schema
  that persist even if abandoned. Reopen only if a stream ever must enter
  rules from a tenant the operator does not control.
- **Full repeal of ADR-0005** (server-polling becomes the only calendar
  path). Pays for reopened per-device consent, the whole calendar at rest in
  the DO, and a rewrite of the shipping `client/core/src/calendar/` stack —
  and buys nothing this ADR doesn't, since display freshness on the device
  in hand is already best served by the device polling itself.
- **Staged candidates** (short-lived table of all stream events, rules
  evaluate over it). Both selling points dissolved under found facts: the
  privacy bound is fake (PITR retains deleted rows 30 days), and backtesting
  re-fetches from the source on demand. Its one residual capability — rules
  over *aggregates of the stream* ("more than 5 emails from X this week") —
  is the named reopen trigger: counting requires the stream to rest
  somewhere, and that would be the moment to accept it deliberately.
- **The sweeper grows a context leg.** ADR-0002 rule 5 scoped it
  capture-only and ADR-0005 already rejected this shape; the sweeper is also
  off (ADR-0008) and one-shot on a 15-minute cron — the wrong host for
  cursor state the DO already owns.

## Amendment: the poller runs out-of-process, and its credential is narrowed accordingly

*2026-08-10, from #135's implementation and its review round.*

The decision above puts each evaluated-stream poller in "the DO cron +
daemon credential" — in-Worker, alongside `sweep_tick` (#138). #135 instead
built `server/gmail-poll` the way `server/city-waste` (#120) was built:
**out-of-process**, on its own GitHub Actions `schedule:`
(`.github/workflows/gmail-poll.yml`), reading `GET /api/rules` and
`GET /api/snapshots` (both added alongside it) as an `ingest` token rather
than running inside `hummingbird-authority-worker`.

*Amended 2026-09-02 (#774): the trigger moved; the shape did not. Actions
`schedule:` delivered this poller at roughly once every four hours against
the `*/15` it asked for — GitHub batches and delays scheduled runs and
guarantees no interval — measured and argued in #773. So #774 built the
binary into the `hummingbird-sweeper` image and gave it a staggered entry in
`crontab`, on the supercronic clock that always-on machine already runs for
`sweep.py`: one clock extended, not a second one started (CLAUDE.md's "no
competing clocks"). `.github/workflows/gmail-poll.yml` keeps
`workflow_dispatch:` only, for a manual run against a fresh checkout, and its
header carries the argument in full. **Out-of-process — what this amendment
actually decided, and what the Status header entry above names — is
unchanged:** the poller still runs outside `hummingbird-authority-worker`,
reading `GET /api/rules` and `GET /api/snapshots` as an `ingest` token. The
other three evaluated-stream pollers moved with it, as did `github-status`
(ADR-0017 decision 2); `crontab` is where every one of those cadences now
lives.*

**Why, standing on its own:** `server/worker` has no test harness of any
kind (the split that already separates `authority/src/fcm.rs` from
`worker/src/fcm.rs`), so a cursor-loss decision, a rule-evaluation fold, or
any other real logic expressed there would be untested by construction.
Out-of-process is what let `resume.rs`'s cursor-loss decision and
`batch.rs`'s fetch fold be pure functions with native fixture tests at all
— the exact shape #120 already proved out. #136/#137 follow the same
scaffolding.

**What this does *not* settle on its own: the credential.** The table above
places all four credentials as **Worker secrets** — reachable only from
inside Cloudflare's runtime. GitHub Actions secrets are a different trust
boundary (repo admins, workflow logs, any workflow file change in the
repo), and CLAUDE.md's blast-radius rule for that boundary is explicit:
`ADMIN_SECRET` and `FCM_SERVICE_ACCOUNT` stay out because of what they can
mint or sign; `CITY_WASTE_INGEST_TOKEN` goes in because its worst-case abuse
is bounded ("a wrong bin day"). A `gmail.modify` refresh token — read *and
modify* the operator's entire mailbox — has no comparably small worst case,
so it does not clear that bar merely by being "the credential ADR-0011
already named."

**Resolution, pending operator sign-off (tracked in issue #135):** the
poller's own Gmail calls are all reads (`history.list`, `messages.get`,
`messages.list`, `getProfile` — no `messages.modify`, no label or trash
writes anywhere in `server/gmail-poll`), so `gmail.readonly` is sufficient
in kind, not just narrower in name. The workflow
(`.github/workflows/gmail-poll.yml`) is written against a **dedicated
`gmail.readonly`-scope refresh token**, distinct from the sweeper's existing
`gmail.modify` one — which does mean the "no new consent... required"
line in #135's brief no longer holds; a fresh one-time consent for the
narrower scope is a deploy-time operator step, the same category #135's
brief already put the credential handoff itself in. Reusing the existing
broader token instead (avoiding that one extra consent) remains available
if the operator prefers it, but that is the operator's call to make
explicitly, not something an implementer may decide silently by shipping
it — see the issue-135 thread for the open question.

*Amended 2026-08-14 (#486): the operator made that call, and made it the
other way — **one broad Google credential, shared by the sweeper and both
Google poller lanes**, carrying Tasks + `gmail.modify` + `calendar.readonly`.
No dedicated `gmail.readonly` token exists or will be minted. The reasoning
above about kind-vs-name narrowing stands as written and was not refuted; it
lost to operator cost in a single-operator system — a second credential is a
second consent, a second pair of secrets, and a second rotation step, for
credentials that live in one vault behind one account either way. What is
accepted along with it: a leak of the Actions secret can modify the
operator's mailbox, not merely read it, bounded by the grant being revocable
in one gesture. This retires #135's open question; the poller workflows'
`env` blocks carry the posture, and `docs/sweeper.md` is the mint runbook for
all three consumers.*

## Addendum: #136 follows the same scaffolding, and the credential table above was already right

*2026-08-10, from #136's implementation.*

`server/calendar-poll` (#136) follows #135's amendment onto the exact same
out-of-process shape: its own GitHub Actions `schedule:`
(`.github/workflows/calendar-poll.yml`), reading `GET /api/rules` and
`GET /api/snapshots` as an `ingest` token bound to `google-calendar/v1`,
never running inside `hummingbird-authority-worker`.

*Amended 2026-09-02 (#774): this leg followed #135's off Actions `schedule:`
as well — a staggered `crontab` entry on `hummingbird-sweeper`'s supercronic
clock, `workflow_dispatch:` retained on the workflow. Out-of-process, the
`ingest` token bound to `google-calendar/v1`, and the `busy_now` snapshot
below are all unchanged; see the #774 note in the amendment above.*

Unlike Gmail, the credential question here needed no fresh resolution: the
decision table at the top of this ADR already specified "Google Calendar:
same OAuth app, scope re-mint per the `docs/sweeper.md` runbook
(`calendar.readonly` added)" — i.e. the SAME dedicated readonly token
`gmail-poll.yml` now uses (`GOOGLE_REFRESH_TOKEN`, minted narrow per the
amendment above), re-minted to also carry `calendar.readonly`. Every
Calendar call `server/calendar-poll` makes (`events.list`) is a read, so
this stays on the correct side of CLAUDE.md's Actions blast-radius rule for
the same reason `gmail.readonly` did — narrower in kind, not just in name.
One credential, two scopes, one workflow secret; the still-open question on
issue #135 (reuse the operator's broader existing token instead) covers
this leg too and is not duplicated here.

*Amended 2026-08-14 (#486): that question is now closed, and this leg
follows it — see the #486 note in the amendment above. The shared credential
is the sweeper's, widened by re-consent to carry `calendar.readonly`; it was
never possible to reuse the sweeper's token as it stood, because that token
had no calendar scope at all.*

`server/calendar-poll` also writes the `busy_now` gauge — an ordinary
server-polled *snapshot* row (the lane this ADR's "gains a row" section
distinguishes from the evaluated-stream lane by lifecycle), reusing the
same `google-calendar/v1` source and the same `ingest` token as the
evaluated-stream leg's alerts and cursor. Nothing about that snapshot
widens the credential question above: it is the same read-only API call
family, posted through the same `POST /api/snapshots` route #135 already
added.

## Amendment: #137's M365 credential is narrower in kind, but not yet narrower in blast radius

*2026-08-10, from #137's implementation.*

#135/#136 each resolved their own leg of the "Worker secret vs GitHub
Actions secret" question above by narrowing the OAuth grant to the least
the poller's calls need (`gmail.readonly`, then `calendar.readonly` on the
same token). #137's credential is an app-only Graph certificate, and the
same "narrower in kind" move applies cleanly: the brief's own named
application permissions, `Mail.Read` and `Calendars.Read`, are both
read-only, and neither `graph-mail-poll` nor `graph-calendar-poll` ever
calls a Graph write endpoint.

**What is different, and not yet resolved:** a Google OAuth grant like
`gmail.readonly` is scoped to the operator's own mailbox by construction —
the token is minted *for that account*. An app-only Graph permission has no
such built-in scoping: `Mail.Read`/`Calendars.Read` on an application grant
read access to **every** mailbox and calendar in the tenant, unless the
operator separately applies an Exchange Online **Application Access
Policy** binding the app registration to one mailbox. That policy is an
operator-side Entra/Exchange administration step this implementation
cannot perform, verify, or even detect the absence of from the poller's own
running state — it is invisible to `server/graph-poll` either way.

**Resolution, pending operator sign-off (tracked in issue #137,
cross-referencing issue #135's still-open credential question — the same
category of decision):** `GRAPH_CLIENT_PRIVATE_KEY` is written against
GitHub Actions secrets, on #135/#136's own established precedent
(out-of-process poller = Actions secrets, not the Worker secret this ADR's
original table names). Whether that credential is additionally bounded by
an Application Access Policy before it is minted is the operator's call,
not something this implementation may decide or verify silently — until
that policy exists, the certificate's actual worst-case abuse sits closer
to `ADMIN_SECRET`'s side of CLAUDE.md's blast-radius line than to
`CITY_WASTE_INGEST_TOKEN`'s.

*Amended 2026-09-02 (#774): both Graph pollers moved off Actions `schedule:`
onto `hummingbird-sweeper`'s supercronic clock with the two Google lanes, and
this credential moved with them — `GRAPH_CLIENT_PRIVATE_KEY` is a Fly secret
on that machine now, set from the operator's terminal, with the non-secret
`GRAPH_*` identifiers as `[env]` in `fly.toml`; it is no longer an Actions
secret, so the trust boundary this resolution reasoned about is not the one
it rests behind. Whether an Application Access Policy bounds the app
registration is untouched by where the key is held —
`.github/workflows/graph-mail-poll.yml`'s header carries that record.*
