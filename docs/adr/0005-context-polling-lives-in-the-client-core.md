# ADR-0005: Context polling lives in the client core; credentials are per-device and host-owned

**Status:** accepted · 2026-08-08
**Context:** the context-polling grilling of 2026-08-08, wayfinder map
[#35](https://github.com/JddAndrewLauren/hummingbird/issues/35) ticket
[#50](https://github.com/JddAndrewLauren/hummingbird/issues/50). Resolves the
placement ADR-0002 left ambiguous; extends
[ADR-0003](0003-one-rust-sync-core-embedded-per-device.md) and
[ADR-0004](0004-client-linear-credential-is-scoped-per-device-host-supplied.md).

## Decision

**The client's Rust sync core polls the context sources** (Google Calendar
first, M365 calendar later) directly from each device. The sweeper never grows
a context leg: its scope stays capture-only, and ADR-0002's context adapters
are implemented in `client/core`, behind the same mirror the task records live
in.

**Context is per-device opt-in.** A device whose host has supplied no
credential for a provider simply has no context from that provider: no
calendar tile, unconstrained ranking. That is a valid steady state, not an
error. In practice desktop and phone opt in; Wear likely never does.

**Hosts own the OAuth lifecycle; the core is a credential consumer.** Each
host uses its platform-native auth stack (Google Identity Services on web,
AppAuth/Credential Manager on Android, MSAL for the Microsoft side) to run
consent, persist what is durable, and mint fresh access tokens. The host
**pushes** tokens into the core — at init and on every rotation. When a token
expires or a call returns 401, the core surfaces an event and **that
provider's polling holds** until a fresh token arrives; the stale "as of…"
tile is the visible symptom, per ADR-0002. The core persists no credential of
any provider — ADR-0004's rule, now uniform across Linear, Google, and
Microsoft.

**The skill-runner stays context-blind.** Calendar context reaches
`/next-up-personal` as an optional field in the `{skill, args}` payload,
supplied from the calling device's mirror. The runner holds no calendar
credential and never queries a calendar API.

## Why

- **CORS is verified, not assumed.** Live preflights (2026-08-08) against
  `www.googleapis.com/calendar/v3` and `graph.microsoft.com` both pass for a
  browser origin with an `authorization` header — Google echoes the origin,
  Graph allows `*`. The web client needs no server to reach either authority,
  same as Linear (ADR-0003).
- **The sweeper alternative has no acceptable distribution path.** The sweeper
  machine cannot serve (`no [http_service]` is a frozen invariant in
  `docs/sweeper.md`); the thinnest workaround — a snapshot in object storage —
  means calendar data at rest in third-party storage, a *shared* read
  credential on every device (the exact all-or-nothing shape ADR-0004
  rejected), a second staleness hop, and new infrastructure in the read path
  of a personal app.
- **The M365 leg decides it.** A headless daemon against a work M365 tenant
  needs org-wide admin consent (client credentials) or a refresh token living
  on a Fly box under corporate conditional-access policy — fragile to
  impossible. Interactive clients with MSAL are the path corporate M365 is
  designed to allow. Under sweeper-polls, the harder provider lands on the
  harder auth model; under core-polls, both land on the easy one.
- **Rotation stays out of the core's critical path.** ADR-0004 rejected OAuth
  for Linear partly because it would put a mandatory-rotation state machine in
  the core. Context polling forces OAuth *somewhere*; the same argument
  decides *where* — in the hosts, whose platform SDKs substantially provide
  it. A push API also avoids host-implemented async callback traits, which
  ADR-0003 rejected on UniFFI grounds.
- **Per-device tokens are a feature.** One consent per device per provider,
  one-time, buys individual revocation across all providers and requires no
  secret-distribution machinery.

## Consequences

- [#46 (Google Calendar context)](https://github.com/JddAndrewLauren/hummingbird/issues/46)
  is amended: it no longer reuses the sweeper's refresh token, so its blocking
  edge on [#45](https://github.com/JddAndrewLauren/hummingbird/issues/45)
  dissolves — the sweeper's Gmail scope re-mint and the client's calendar
  consent are independent credentials on independent schedules.
- [#47 (M365 calendar context)](https://github.com/JddAndrewLauren/hummingbird/issues/47)
  is amended: the public client and its refresh token are per-device,
  host-stored, never core-persisted.
- The `next-up-personal` skill schema
  ([#41](https://github.com/JddAndrewLauren/hummingbird/issues/41)) gains an
  optional calendar-context field, riding with #46's consumer work.
- ADR-0002's "context pollers get no healthchecks" now has a concrete reading:
  the poller is on-device, its alarm is the tile, and no healthchecks.io check
  ever attaches to context.

## Rejected alternatives

- **The sweeper polls, clients read a distributed snapshot** — see "Why";
  shared secret, second staleness hop, data at rest in a bucket, and the M365
  daemon-auth trap.
- **Materializing context into Linear as the transport** — ADR-0002 rule 1
  forbids it outright; a copy would be a second authority.
- **The runner polls calendars itself** — a fifth credential holder doing
  daemon OAuth; rejected for the same reasons as the sweeper, plus the
  freshest mirror at invocation time is by construction on the device the
  human is holding.
- **Core-owned OAuth (core runs refresh internally)** — puts provider-specific
  rotation state machines in the core, working identically across wasm and
  three native targets, for machinery the platform SDKs already provide; and
  Google's web model issues SPAs no refresh token at all, so the core could
  not own the web leg even in principle.
