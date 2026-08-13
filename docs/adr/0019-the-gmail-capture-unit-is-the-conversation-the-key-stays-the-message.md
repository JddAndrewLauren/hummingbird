# ADR-0019: The Gmail capture unit is the conversation; the key stays the message

**Status:** accepted · 2026-08-12
**Context:** a grilling session on 2026-08-12, issue
[#336](https://github.com/JddAndrewLauren/hummingbird/issues/336). Amends
[ADR-0002](0002-sources-join-by-role-urgency-computed-at-read-time.md)'s
Placements table, which named the message as the Gmail unit. Numbered 0019
because 0017 is reserved by open issue #310 and 0018 is taken.

Gmail's UI applies the `hummingbird/capture` label at **thread** granularity:
labelling a conversation with a forward chain in it labels every message the
conversation carries. The adapter enumerated **messages** and derived one
capture per message, `deterministic_v4(message_id, GMAIL_NAMESPACE)` — so one
labelling gesture on one conversation minted one intended capture plus one
unwanted duplicate per extra message in the chain. Observed 2026-08-12: a
three-message thread produced the intended replay plus two unwanted `Fwd:`
captures of the same conversation.

## The decision

> The **capture unit** for Gmail is the **conversation**. What *identifies* a
> capture stays the **message** — deliberately not the same thing.

Within one sweep, every labelled message sharing a `threadId` contributes
exactly one capture: the **winner**, chosen as the **oldest labelled message
by `internalDate`**, with the message id as a deterministic tiebreak. Losing
messages mint nothing and are acked (unlabelled) without creating, but only
**after the winner's create has succeeded** — a thread's fail-closed posture
is atomic to itself and never touches another thread's.

**The frozen id derivation is untouched.** `deterministic_v4(message_id,
GMAIL_NAMESPACE)` is exactly as it was; there is no new namespace, no
namespace bump, and no change to the frozen test vectors. The winner's message
id is what gets hashed, same as before this decision — only *which* message
in a thread reaches that hash changed.

**Oldest, not newest.** The winner must not depend on when the sweep happens
to run. A forward arriving in a conversation between two sweeps moves which
message is "newest" and must never move which id an earlier sweep already
minted — an observer-dependent key is exactly what turns a legitimate replay
into a silent duplicate.

## Why not key on the thread (rejected)

Keying `source_key` on `threadId` is the obvious-looking alternative, and is
**rejected**. It gives a conversation one stable id forever: a thread
captured, triaged and completed months ago, then re-labelled because the
conversation has revived, would replay to `existed` — 200, no write — and get
acked. The capture gesture would silently mint nothing, and the logs would
report it as a success. **Losing a capture is the one failure this system may
not have** (`sweep.py`'s own header: a crash "can only produce a visible
duplicate attempt on the next sweep… never a silently lost capture"; a
thread-keyed id would break exactly that guarantee for revived conversations).

Message-keying preserves re-capture: a new message in a revived thread is a
new message id, and therefore a new capture, exactly as the system is meant to
behave.

## What changes

- **Enumeration, not id derivation.** The Gmail adapter's `enumerate` groups
  the retrieved, still-labelled messages by `threadId` and yields one winner
  per thread. `source_key` is unchanged — still the winning message's id.
- **The ack contract's promise is unchanged, applied twice.** Removing exactly
  the capture label from exactly one message — never archive, mark-read,
  star, or delete — now happens for the winner *and* every collapsed message
  in its thread, winner first.
- **The existing per-message safety check composes with this unchanged.** A
  message that lost its label between listing and retrieval is still skipped,
  and cannot be a winner or a collapsed loser — it was never a candidate.
- **Observability.** The sweep's finish line gains `collapsed=N` alongside
  created/existed/completed/failed/skipped/quarantined, and each collapsed
  message logs its thread and the winning message id. Collapsed messages are
  deliberately **not** carried in the healthcheck ping body the way
  quarantined/skipped counts are — collapsing is normal operation, not
  something set aside for a human to look at.

## Rejected alternatives

- **Keying `source_key` on `threadId`.** See above — silently drops revived
  conversations' recaptures, which this system treats as an unacceptable
  failure mode.
- **A new namespace or a v2 id derivation for Gmail.** Unnecessary: the
  problem is *which message* reaches the existing, frozen derivation, not the
  derivation itself. A new namespace would also re-mint every open Gmail
  capture, which the frozen-namespace discipline (`sweep.py`'s header,
  ADR-0002 rule 5) exists specifically to avoid.
- **Deduplicating after the fact** (create every message, then merge
  duplicate items in the authority). Adds a second, server-side collapsing
  pass to a system whose authority has no notion of "these rows are the same
  conversation," and still mints — then has to un-mint — the unwanted rows the
  label gesture never intended.
- **Newest-message-wins.** Rejected by the decision itself: it makes the
  winning id depend on when the sweep happens to run relative to when a
  forward arrives, which is exactly the observer-dependent behavior that turns
  a legitimate replay into a duplicate.

## Out of scope

- The Google Tasks adapter — its capture unit is the task, one record to one
  item, untouched by this decision.
- Collapsing *across* sweeps. A thread labelled again after a completed sweep
  legitimately mints a new capture (the revived-conversation case above) —
  that is the behavior thread-keying was rejected to preserve, not a gap in
  this one.
- Deleting the two `Fwd:` items minted on 2026-08-12
  (`19fed584d7349885`, `19fed4a75660b426`). Unaffected by this decision;
  cleanup is a manual operator action.
