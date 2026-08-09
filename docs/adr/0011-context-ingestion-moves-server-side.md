# ADR-0011: Context ingestion moves server-side; rules evaluate in the poll, and only matches persist

**Status:** accepted · 2026-08-09
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
