# ADR-0041: A glance host is a headless, read-only device that publishes to Home Assistant

**Status:** proposed · 2026-11-16, accepted when the PR that adds it merges
**Context:** the operator wants the office e-paper (a reTerminal E1003 running
ESPHome, owned by the nest repo) to show the **Frontier** and what is due
today. The panel talks only to Home Assistant. The authority serves sync rows,
not computed views (`GET /api/changes`, `/api/sweep`), and every view the
panel needs is computed by the core over a **mirror**
([ADR-0025](0025-decisions-sink-to-the-core-rendering-stays-per-client.md)).
So something that holds a core has to compute the views and hand them to Home
Assistant. This ADR decides what that something is, what it may do and what it
publishes. It amends
[ADR-0003](0003-one-rust-sync-core-embedded-per-device.md) (a fourth consumer
of the embedded core, and the first with no UI). `CONTEXT.md` gains
**Glance host**.

## The decision

1. **The glance host is a device with its own core, like the Watch
   ([ADR-0039](0039-the-watch-is-a-device-with-its-own-core.md) decision 1).**
   It is a headless Rust binary, `client/glance`, that links
   `hummingbird-core`, keeps its mirror under a directory namespace
   (`Core::init`, `storage/fs.rs`) and runs a sync **cycle** through the
   core's own `ReqwestSyncTransport`. It is not a **View**: it owns a mirror
   and a cycle. One instance runs on Kestrel, the always-on Mac that also
   hosts Home Assistant's VM, supervised by a LaunchAgent
   (`net.twinion.hummingbird.glance`).

2. **It never writes, by construction of the host, on an ordinary `device`
   token.** The admin lane mints it (`device-glance-kestrel`), as it mints
   the Watch's (ADR-0040 decision 5). The host's write transport refuses
   every mutation, and nothing in the host calls a write door on `Core`, so
   its outbound queue stays empty. The authority does not enforce this: the
   token can write, like every `device` token. The token rests in Kestrel's
   login Keychain, placed once by the operator from 1Password `Private`, and
   never in the LaunchAgent's plist, the environment of a shell or a file.

3. **It publishes two views and decides neither.**
   - **Frontier:** `Core::frontier()` in `frontier::by_priority_then_due`
     order, the frontier's display order.
   - **Today:** a new pure core decision, `decisions::glance::today(items,
     now)`. It returns every live item whose **Deadline** bands `overdue` or
     `now` under `compute_urgency`, or whose **Scheduled date** is today or
     earlier in the reader's zone (a do-date that passed undone stays on
     today's plate rather than dropping off unseen). Items in by their
     deadline come first, by deadline, so overdue leads; then items in by
     their scheduled date alone. It reads all live items, not only the
     Frontier: a blocked item with a deadline today is still due today.
     `Core::today` is its door.

   Both are core functions with core tests, so the host only maps their
   output to a payload. It renders `now` from its own local clock, as every
   reader does under the ADR-0015 amendment.

4. **What leaves Hummingbird is a title and its facts, never more.** Per item:
   `title`, `context`, `size`, `energy`, the `band` as `compute_urgency`
   spells it, `deadline` and `scheduled_date` verbatim. No description,
   checklist, file links, project links or ids. Each list is capped at a fixed
   length (a host constant, sized to the panel), with the true count beside
   it. Turning a band and a date into words (`DUE TODAY`, `OVERDUE · THU`) is
   the panel's job: rendering stays per client (ADR-0025).

5. **Transport: MQTT to Home Assistant's broker, with discovery.** The host
   publishes retained Home Assistant MQTT discovery for one device,
   "Hummingbird Glance", and one retained JSON state topic. The lists travel
   as JSON attributes, because a Home Assistant state holds at most 255
   characters. Three entities: a Frontier count sensor and a Today count
   sensor (each carrying its list as attributes), and a `timestamp` sensor
   for the last completed cycle. An availability topic with a last will,
   plus `expire_after`, turns them unavailable when the host dies or stops
   publishing. When the host is alive but its pulls fail, it keeps
   publishing the last good lists with the time of the last completed
   cycle, so the panel can say how old they are. A refused token publishes
   `offline`, never an empty list. So a panel never shows an old list as a
   current one: that is ADR-0039 decision 6's honesty rule, applied to a
   device with no screen of its own. Until a cycle completes in a process,
   it publishes nothing, so a fresh start can't pass an unpulled mirror off
   as an empty one.

6. **Cadence: one cycle every five minutes, and a publish after each
   attempt.** The
   host's own loop owns that cadence. The LaunchAgent only keeps the process
   alive and sets no interval of its own, so there is one clock (CLAUDE.md's
   "No competing clocks"). Each publish is a full retained state, even when
   nothing changed, so `expire_after` measures whether the host is alive
   rather than whether the tasks changed. There is no push lane: five minutes is the accepted
   staleness for a wall panel that redraws once a minute at most.

## Considered and not taken

- **The panel reading the authority itself.** ESPHome would re-implement the
  Frontier and Urgency in C++, against ADR-0025. It also needs a token on a
  microcontroller and internet egress from the IoT network.
- **A computed-view endpoint on the authority.** It moves decisions to the
  server against ADR-0003's one-core shape, and the Worker does not link
  `client/core`.
- **A rendered image fetched by the panel.** It loses native 1-bit drawing,
  per-element refresh and touch.
- **A `reader` token scope** (`GET /api/changes` and `/api/sweep` only), so
  the authority would refuse a write. Not taken (operator, 2026-11-16): it
  adds a fourth scope to ADR-0008's matrix and a credential kind Signed-in
  devices can't show, for one host. The `device` token's write power is
  accepted, and kept down by where the token rests (decision 2). Revisit if
  a second glance host appears, or the token ever has to leave Kestrel.
- **Home Assistant's REST API instead of MQTT.** A REST token acts as the
  operator in Home Assistant, and entities it creates do not survive a Home
  Assistant restart.

## Consequences

- **Task titles leave Hummingbird.** They sit in a broker and in Home
  Assistant's state machine. The nest side excludes the entities from Home
  Assistant's recorder, so no history of titles is written. Mosquitto's topic
  ACLs on the nest box are not enforced today, so any MQTT login there can
  read the titles. The operator accepted that on 2026-11-16, because every
  MQTT login there belongs to a device inside the house. nest's own issue
  for topic restrictions closes the gap when it lands.
- **A write-everything token lives on Kestrel**, read by a long-running
  process. `CLAUDE.md`'s "Credential blast radius" gains it in the same
  change. It shows in Signed-in devices like any `device` token, and is
  revoked there.
- A second Rust binary to build and install on Kestrel by hand. It is not
  part of `deploy.sh`.

## What this does not decide

- Anything the panel draws, or how it is laid out (nest's).
- A glance host for anything beyond these two views: a further view is a
  further decision.
- Writes from a glance surface (marking done, an ack). The panel's touch
  never reaches Hummingbird.
