# ADR-0004: The client's Linear credential is scoped, per-device, and host-supplied

**Status:** accepted · 2026-08-07 · **amended 2026-08-08 by
[ADR-0008](0008-the-authority-is-an-app-owned-server.md):** the credential is
now a per-writer bearer token for the owned API, scoped and individually
revocable server-side. The shape, resting places, never-persisted rule,
401-holds-the-queue rule and OAuth revisit trigger port unchanged; the
Linear-specific analysis is historical.
**Context:** the credentials grilling of 2026-08-07, wayfinder map
[#35](https://github.com/JddAndrewLauren/hummingbird/issues/35) ticket
[#49](https://github.com/JddAndrewLauren/hummingbird/issues/49). Amends
[ADR-0003](0003-one-rust-sync-core-embedded-per-device.md)'s persistence rule and
authenticates the direct-to-Linear path it established.

## Decision

**Each client device holds its own Linear personal API key**, scoped to `Write`
(never `Admin`) and limited to team `ION`. Keys are minted by hand in Linear's
Security & access settings, one per device, and revoked individually when a
device is lost or retired.

**The host supplies the credential at init; the core never persists it.** The
core's entry point takes the key alongside the storage directory —
`init(storage_dir, api_key)` — holds it in memory for the lifetime of the
session, and puts it in the `Authorization` header of every call to
`api.linear.app`. Each host reads it from the best secret store it has:

| Host | Where the key rests |
| --- | --- |
| Desktop web | IndexedDB — the only option in a browser |
| Android, Wear OS | App-private encrypted storage, protected by an Android Keystore key (hardware-backed where supported) |
| iPad | iOS Keychain |

The credential *shape* is identical on all four clients. Only the resting place
differs, and that is the point: the three native clients have platform-protected
secret storage the core cannot reach.

**A 401 leaves the core as an event, and the outbound queue holds.** A revoked
or mistyped key must never drain the queue. The host re-prompts; captures wait.

**The client build carries a strict XSS posture**, because that — not the choice
of credential — is what makes a non-expiring key in browser storage acceptable:

- a strict CSP on the built client: no `unsafe-inline`, and `connect-src`
  limited to `api.linear.app` and the runner origin;
- lockfile-pinned installs;
- deliberate restraint about adding dependencies to the one client that holds a
  Linear credential.

Nothing enforces this until `client/web` is scaffolded. It is recorded here so
that whoever scaffolds it inherits the constraint.

## How this amends ADR-0003

ADR-0003 states that the core owns persistence and that "the host contributes
exactly one thing — a storage directory path at init." The host now contributes
two, and the second is deliberately *not* persisted by the core.

The exception is narrow and does not reopen what ADR-0003 rejected. ADR-0003
turned down *host-implemented storage* because it would have been built on
UniFFI's async foreign traits — the Swift 6 `Sendable` conflict, unmitigated
reference cycles, a thread per invocation. A `String` argument at init is none
of those things.

Two properties motivate the carve-out:

1. **The mirror stays exportable.** ADR-0003 makes "the mirror is the export" a
   headline property: a serialised snapshot means a device's replica is one file
   you can copy, read, and paste into an issue while debugging. If the key lived
   in that store, every one of those actions would leak a `Write` credential.
2. **Native hosts have better storage than the core can offer.** The core's
   `std::fs` leg would write the key as plaintext beside the mirror. On Android
   and Wear OS, a Keystore-managed cryptographic key protects the credential in
   app-private encrypted storage, with hardware backing where supported. On
   iPad, Keychain encrypts the credential at rest.

## Why not OAuth

The question was framed as a personal key's convenience against OAuth's
security. That framing rested on a false premise, corrected during the grilling:
**Linear personal API keys are scopeable and individually revocable.** Each key
can be restricted to `Read` / `Write` / `Admin` / `Create issues` /
`Create comments`, *and* limited to specific teams, and revoked on its own.

Once that is true, the remaining comparison favours the key:

- **Scope.** OAuth scopes are workspace-wide. A personal key can say "team ION
  only." On the axis that bounds blast radius, OAuth is *wider*.
- **At-rest exposure is equal.** A public browser client has no backend, so it
  cannot receive an `HttpOnly` cookie, and a bearer token cannot be held as a
  non-extractable WebCrypto key. Under OAuth the browser persists a refresh
  token in the same IndexedDB the API key would occupy. OAuth narrows the window
  for a *stolen access token*; it does not narrow it for a *compromised app*,
  which is the threat that actually exists here.
- **Rotation is now mandatory.** Linear migrated all OAuth applications to
  rotating refresh tokens on 1 April 2026: every refresh returns a new refresh
  token and invalidates the old one, with a 30-minute replay grace window. In
  this architecture that means a durable rotation write in the core, a race
  between the Web Workers of multiple open tabs, and an undocumented refresh
  lifetime — inside a client whose defining promise is that a device dark for
  weeks wakes up and flushes its queue.

PKCE is supported by Linear (`code_challenge` / `code_verifier`, with
`client_secret` optional), so OAuth is genuinely buildable without a backend.
It is declined on balance, not on feasibility.

**Revisit trigger.** Move to OAuth the day this client is served to anyone but
its author, or the day it must act as a Linear *agent* rather than as its author.
Personal keys structurally cannot cover either case.

## Rejected alternatives

- **OAuth 2.0 + PKCE in the browser** — see above. Buildable; rejected on the
  balance of a wider scope, equal at-rest exposure, and a rotation state machine
  landing in the core's most critical path.
- **A credential-provider trait in `core` with one implementation** — speculative
  generality. The credential already reaches Linear through a single adapter
  inside `core`, so swapping the mechanism later is contained whether or not the
  abstraction is built now. What is worth recording is the revisit trigger, not
  a seam.
- **Brokering OAuth through the [#41](https://github.com/JddAndrewLauren/hummingbird/issues/41)
  runner as a confidential client** — the only shape that keeps a Linear-valid
  credential out of the browser entirely, and the strongest on blast radius. It
  puts a scale-to-zero service in the critical path of *all* sync on *all*
  devices, which is precisely the dependency ADR-0001 and ADR-0003 removed; and
  it substitutes rather than eliminates the browser secret, since the client
  must still authenticate to the broker. Overturning ADR-0003's
  no-backend-in-the-sync-path is a larger decision than this ticket, and would
  need its own ADR.
- **Core-owned credential storage, in a namespace excluded from the export** —
  keeps a `cfg`-split secret backend in the core forever, and still cannot reach
  Keychain or Keystore. It pays the cost of core-owned storage for most of its
  weakness.
- **A full-access personal key** — the workspace has two members and the author
  is an admin, so a full-access key is admin authority over another person's
  data. `Write` costs nothing and drops that.

## Relationship to the runner

[#41](https://github.com/JddAndrewLauren/hummingbird/issues/41)'s skill-runner
holds its own Linear key as a Fly secret, server-side. The client holds its own
per-device key. They are separate credentials with no interaction: the runner is
not a credential path for the sync core, and the runner being down has no effect
on authentication.
