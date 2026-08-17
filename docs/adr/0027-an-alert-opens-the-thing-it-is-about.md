# ADR-0027: An alert opens the thing it is about, and the notification carries what that is

**Status:** accepted · 2026-08-17 · **Context:** the item-detail grilling of
2026-08-17, opened on [#141](https://github.com/JddAndrewLauren/hummingbird/issues/141)'s
last slice — the issue asks for "deep links into item/alert detail" and only
alert detail shipped, through M2. Amends
[ADR-0012](0012-the-notification-lane.md): its 2026-08-17 (#141) inline
amendment fixed the FCM payload as data-only carrying `title`, `body`,
`channel_id`, `severity`, `tier` and `alert_id`; this ADR widens that set by
two fields and says why they are not optional. Builds on
[ADR-0014](0014-occurrence-identity-and-the-source-key-conventions.md)'s
state-vs-event cut and [ADR-0025](0025-decisions-sink-to-the-core-rendering-stays-per-client.md)'s
carve-out without changing either.

## The decision

**A tapped notification opens the thing the alert is about — the item, when
the alert names one; the alert itself otherwise. The push payload carries
`source` and `source_key` so the client can tell which, offline and
instantly, without reading its mirror.**

Three parts, each load-bearing:

1. **`item-threshold/v1` notifications land on item detail.** That source is
   a *state* source (ADR-0014): its `source_key` is `item:<id>`, naming the
   thing rather than the tick, and one item has exactly one such row across
   its whole life. Its alert is therefore a *reading of the item's
   condition*, not an independent happening that mentions it. Landing on the
   alert would show a reading of a thing while withholding the thing.

2. **The destination is decided in `hummingbird-core`, from the payload, as a
   free function over two strings.** `item:` is not a prefix, it is a
   convention with an owner — `hummingbird_domain::sources::item_threshold_v1_key`,
   documented in ADR-0014's source-key table, keyed `item:<id>` and
   deliberately *not* `item:<id>:<deadline>` so a re-committed deadline
   re-raises the same row. A Kotlin `removePrefix("item:")` would be that
   convention hand-copied into a client language, which ADR-0025 forbids, and
   would fail *silently* if the key convention ever moved: the alert would
   simply stop opening its item, with nothing failing anywhere.

3. **Item detail carries the live alert keyed on the item, and can Ack it.**
   Otherwise part 1 makes the lane worse than before on its most common path:
   the Ack is what ends a **Live** alert, and moving the tap off alert detail
   would move the Ack one navigation further away on exactly the journey that
   rings. Completing or cancelling the item Acks it too — see
   `CONTEXT.md`'s amended **Ack** entry.

## Why the payload carries its own subject

The alternative was to resolve the destination from the mirror: read the
alert the tap handed us, find its `source_key`, route on that. It is free —
no server change — and it fails in precisely the wrong place.

**The race is worst exactly where it matters most.** `AlertDetailViewModel`
already documents this window: "the push arrives, the user taps immediately,
and the cycle the push enqueued has not landed." An *urgent, just-rung* alert
is the one most likely to be tapped inside that window, so a mirror-resolved
destination would be least reliable for the highest-tier deliveries and most
reliable for the ones nobody hurries to. Widening the payload does not
mitigate that window, it removes it: the thing that rang carries what it is
about.

It also keeps the decision **pure**. Over `source` and `source_key` the
answer needs no `Core`, no mirror, no clock — the same free-function door
shape ADR-0025 fixed at M1-1 for `can_submit_capture`, synchronously callable
from the deep-link collector, unit-testable on both sides of the seam. A
mirror-resolved answer would have to be a `MobileTaskHost` method holding the
interior mutex, making the deep link async for no gain.

`PushNotification` is built at `delivery.rs:162` from the full alert row, so
both fields are already in hand and merely were not copied.

**Not `subject_key`.** `AlertRecord` carries one, and it is the wrong field:
ADR-0015 defines it as the standing-question pane join, `(source,
subject_key)` ↔ `(source, key)`, and states that `sweep_tick` leaves it NULL
— so it is always empty for exactly the alerts this ADR routes.

## What this deliberately does not do

**No new atomic mutation.** Complete-then-Ack is two queue entries, the
`act` first. The **outbound queue** is durable and ordered, so both land; a
dead-letter on one and not the other is possible and is left to the
**dead-letter journal**, which names what each change was about. Inventing a
combined mutation to make one gesture atomic would add an authority-side
concept to serve a client-side gesture.

**No fallback-to-item guessing.** A payload with no `source_key`, or one
naming an unrecognised source, opens alert detail. That is the **permanent
contract**, not a version-skew allowance: 7 of the 8 registered sources name
no item, so "I cannot tell you what this is about, here is the alert" is the
honest steady-state answer. It also means an old server and a new client
degrade to M2's proven behaviour rather than to a crash.

**No editing of history.** Item detail opened onto an archived item is
readable and ackable but not editable — `CONTEXT.md`'s **Recall** rule
(#478) reached through a different door, and answering it differently here
would let the phone and the web disagree about whether history can be edited.
The Ack survives because silencing something still ringing is not an edit.

## Consequences

- **`#141`'s last slice touches the server.** `delivery.rs` and `fcm.rs`
  ship with the client change. The payload widens with an opaque item uuid
  alongside the `title` already there — no new class of exposure, but it is
  data about an item reaching FCM's infrastructure, and it was accepted
  knowingly.
- **The record is assembled in `client/core`, not in the seam.** Which
  blockers count, which alert is *the* alert, how a project name resolves,
  what an unseen blocker id does — these are answers two clients must not
  disagree about, and they are testable in core's native harness rather than
  only through generated bindings. The web's item detail, when it comes,
  inherits it.
- **The task lane and the alert lane now meet on one record.** This is the
  first place they do. The join is one-directional and narrow — an item
  detail knows the live alert about it; no alert record grows an item field.
- **#518's policy is unchanged and now covers a second destination.** A
  notification-opened destination sits directly on top of `Now`, cold or
  warm. `NavigationStructuralTest` asserts the pop policy structurally and
  must be extended to the item route, or the fix regresses through the new
  door.
