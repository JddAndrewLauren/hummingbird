# ADR-0031: Calendar write is one credential at the authority, gated on a token id

**Status:** accepted · 2026-08-20
**Context:** giving the OpenClaw agent (ADR-0029) Google Calendar write.
Extends [ADR-0028](0028-the-web-host-mints-its-google-token-at-the-authority.md)
with a second, write-scoped sibling route over its own credential (that
ADR's readonly credential, cache and contract are untouched), and adds a
capability to [ADR-0029](0029-an-openclaw-agent-is-a-third-interactive-arm.md)'s
agent — its first that is not a call to this repo's own authority. Narrows
CLAUDE.md's blast-radius rule, whose statement of what a `device` token can
do was flat prose about a uniform population and no longer is.

## The problem

The agent is the operator's conversational arm over their work, and "put
that on my calendar Thursday at three" has no home in it. It has no calendar
capability at all — not read, not write.

The repo already mints Google calendar tokens: `POST
/api/google/calendar_token` (ADR-0028). That route **cannot** simply be
widened. It is gated on `device` scope, and `device` is the credential class
every browser holds; turning its credential into a write credential would
hand every browser session the power to edit the operator's calendar —
precisely the blast-radius widening ADR-0028 was written to refuse.

## Decision

**The calendar-write credential stays server-side, exactly like ADR-0028's
readonly one, and a second route — reachable only by named token ids —
mints from it.**

### Route contract

| | |
| --- | --- |
| `POST /api/google/calendar_write_token` | no body, `device` scope **and** a listed token id |
| 200 | `{"access_token":"ya29.…","expires_at_ms":…}`, `cache-control: no-store` |
| 401 | empty — bad device token |
| 403 | empty — wrong scope, **or** a good device token that is not a listed holder |
| 503 / 502 | write secrets unset / transport, `invalid_grant`, upstream |

The 200 body is ADR-0028's `CalendarTokenResponse`, reused unchanged. **No
path answers 401 for a provisioning or upstream failure**, for ADR-0028's
reason: a false 401 makes a client discard a working device token. A caller
that authenticates fine and is simply not the agent is a 403 — the same
answer, and the same empty body, the scope matrix itself gives.

### The gate is a token id, checked in the handler

`authenticate` already reads the calling token's `id` to stamp `last_seen`
and threw it away; it now carries it on `Principal`, and the handler checks
membership in a `const` list (`CALENDAR_WRITE_HOLDERS`, today
`["openclaw-agent"]`).

Handler-level narrowing is the established pattern here, not an invention:
`#145`'s ingest source-binding, `rules::event_kinds_readable_by` and
`snapshots::get` all narrow inside the handler, because the scope matrix has
no access to what they need. `auth.rs` says so explicitly. What is new is
narrowing on the token's identity rather than on its binding or the
response's content, and that is worth naming: this is the first route in the
repo whose gate is *which* credential is calling.

**A list with a membership test, not an equality**, from day one, and a
reviewed `const` rather than a secret or a settings row. Granting another
host calendar write — `"runner"`, when the agent moves in-app — is then a
one-line diff plus a test, visible in review, rather than an invisible
`wrangler secret put`.

### Why not a new `Scope`

The obvious shape is a fourth token scope, so only the agent's token can
reach a write route at all. It was rejected for a mechanical reason, not for
ADR-0029's opinion of scope invention: `tokens.scope` carries a DDL `CHECK`
constraint, and SQLite cannot alter a `CHECK` in place. A fourth scope means
`SCHEMA_VERSION` 8 and a **rebuild of the live `tokens` table** — the 6→7
`items` rebuild in `schema.rs` documents how many traps that path holds, and
here the table being rebuilt holds every device's token hash, where a
mistake is unrecoverable (a mint burns its id permanently, and revoke is a
soft delete). That is a large, risky migration to buy one route's gate,
against a handler check that needs no schema change at all.

`server/domain/src/token.rs` and `server/authority/src/schema.rs` are
therefore **untouched by design**. If a change in this lane starts wanting
either file, this decision has been abandoned; stop and re-decide rather
than half-migrating.

### The credential

**A third dedicated Google OAuth credential**, carrying
`https://www.googleapis.com/auth/calendar.events` alone —
`GOOGLE_CALENDAR_WRITE_CLIENT_ID` / `_SECRET` / `_REFRESH_TOKEN`, Wrangler
secrets on `hummingbird-authority`, set from the operator's terminal and
never GitHub Actions. Three reasons, all ADR-0028's surviving ones: this
lane's secret store never holds a broader token than the lane needs; it is
revocable alone, without taking down the readonly lane, the sweeper or
either poller; and the exchange sends its `scope` (#581 measured that Google
honours it) so pasting the wrong refresh token in fails closed instead of
silently granting more.

`calendar.events`, not full `calendar`: the difference is creating calendars
and changing who they are shared with — powers no verb of the skill uses,
and ones whose abuse is not confined to the operator's own data.

The shim's two minters hold **separate caches**. Handing a readonly caller a
cached write bearer would defeat the whole decision, and one shared cache
keyed on nothing would do exactly that.

### The client half

A fourth OpenClaw skill, `openclaw/calendar/`, following the per-skill
self-contained-script pattern: it reads the same device-token file every
other openclaw script reads, calls the new route once for a short-lived
Google bearer, holds it in a shell variable, and calls the Calendar API
directly. **No new credential file on the gateway** — which is the property
that keeps revocation a single call, `DELETE
/api/admin/tokens/openclaw-agent`, killing task writes and calendar writes
together.

Three recipes in it are durable and are marked as such, because they port
byte-for-byte if calendar write ever moves to another arm: a **frozen
event-id recipe** (`sha256("hummingbird-openclaw/gcal/v1" + calendarId + "/"
+ title + "/" + start)`, prefixed `hb`, truncated — so a retried insert
after a timeout 409s instead of double-booking, the same double-mint
discipline as `hb.sh`'s step ids); an **explicit `timeZone` on every
`dateTime`**, read off the calendar's own `events.list` response — not
`calendars.get`, which `calendar.events` is not authorised for — and used as
`TZ` for every datetime sum too, because a bare local datetime is how an
event lands an hour off and wall-clock arithmetic in the gateway's zone is
the other way; and **cancel as a status patch, never
`events.delete`**, the closest thing to
[ADR-0020](0020-no-delete-rows-are-flagged-not-erased.md)'s posture that
Google offers.

## Rejected alternatives

- **Widening `POST /api/google/calendar_token`'s credential to a write
  scope.** One route, no new secrets, no `Principal.id` — and every browser
  holding a `device` token could edit the operator's calendar. This is the
  thing ADR-0028 exists to refuse; it is named here because it is by far the
  cheapest diff and will look tempting again.
- **A fourth `Scope`.** The `CHECK`-constraint rebuild above.
- **`gog`, getopenclaw.ai's built-in-skill route.** That site is an
  unofficial "OpenClaw Guide", not the project's docs, and its model is a
  browser OAuth flow writing a Google refresh token of unstated scope to the
  gateway's filesystem via a third-party CLI: an un-inventoried credential
  in exactly the shape the vault-first and blast-radius rules exist to
  prevent, on the machine that already holds the agent's device token. It is
  also a dead end for the in-app direction — no filesystem or browser story
  exists for the runner, the Worker, or a client.
- **Skipping the route: a refresh token in a local file on the gateway.**
  Cheaper today (no route, no `Principal.id`, no smoke blocks), and it
  surrenders both properties this decision is for — one credential file on
  that machine, and one-call revocation — while being thrown away entirely
  when the agent moves in-app, since the route would have to be built then
  anyway.

## Consequences

- **The `device` population stops being uniform**, and CLAUDE.md's
  blast-radius paragraph had to be rewritten rather than annotated: one
  named member can now reach a place the others cannot.
- **The loss this accepts, stated plainly: writes to the operator's real
  calendar, decided by an agent's own model, with a chat confirmation as the
  only gate.** A wrong `cancel` is visible to other people, not just to the
  operator. The confirmation policy — `agenda` free, `add` riding the ask
  that prompted it, `edit`/`move`/`cancel` on an event the agent did not
  create in this session always confirming first — lives in the skill's
  `SKILL.md` and in the agent's charter, which are prose gates, not
  mechanical ones. The mechanical bounds are narrower: events only, one
  calendar, no delete, and a deterministic id that makes a retry idempotent.
- **Whoever holds the gateway's token file can move a meeting.** That host
  was already write-everything against the task authority and could already
  cause runner spend; this is a new *kind* of reach for it, and
  `docs/openclaw.md`'s credential section says so.
- **What changes when the agent moves into the app** — a runner op, a chat
  surface in the web client: **the credential and the route do not; the
  holder list does.** Half of this decision already is the in-app shape,
  ADR-0028's (the authority mints, the caller polls), so whoever the agent
  becomes authenticates the way everything else already does and asks for a
  bearer. This sentence exists so a future session adds an id to a list
  instead of re-deriving the whole decision.
- **A fourth near-identical Google OAuth client in the vault.** Telling them
  apart is `tokeninfo`'s `aud` plus what the token is *refused* — never the
  granted scope string (#581). The item's notes carry its scope.
