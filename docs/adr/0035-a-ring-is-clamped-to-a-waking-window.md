# ADR-0035: A ring is clamped to a waking window

**Status:** accepted · 2026-08-23
**Context:** the fantasy-football grilling of 2026-08-23. The lead time for
a lineup alert was designed as an offset from the deadline, and the operator
broke it in one sentence: NFL games are played in London and São Paulo, so a
roster locks at 6:30am — or 5:30am — Pacific, and every offset-based ring
for those locks fires while he is asleep. Amends
[ADR-0012](0012-the-notification-lane.md), which gives a delivery a
severity, a generation and a lifecycle but no *civil* dimension at all.

## The decision

**A ring fires at `deadline − MIN_LEAD`. If that instant falls outside the
operator's waking window, it moves backwards to the end of the previous
window — never forwards. A ring with no waking moment inside its lead range
is not delivered at all.**

`WAKING_START = 08:00`, `WAKING_END = 22:00`, `MIN_LEAD = 3h`, local civil
time, as named constants in the poller.

## Why an offset is unsound, not merely coarse

An offset assumes the deadline lands at a civil hour where a notification
means something. Every deadline this system had rung for until now did:
a race start, a bin collection, a calendar event, an email. A roster lock
does not.

| Lock (Pacific) | `lock − 3h` | Offset would ring | Clamp rings |
| --- | --- | --- | --- |
| Sun 13:00 | 10:00 Sun | 10:00 Sun | 10:00 Sun |
| Thu 17:15 | 14:15 Thu | 14:15 Thu | 14:15 Thu |
| **Sun 06:30** (London) | 03:30 Sun | **03:30 Sun** | **22:00 Sat** |
| **Sun 05:30** (Munich) | 02:30 Sun | **02:30 Sun** | **22:00 Sat** |
| Mon 17:15 | 14:15 Mon | 14:15 Mon | 14:15 Mon |

The 03:30 ring is **worse than silence**: it rings into an empty room, is
stale by the time it is read, and the pane's `imminent` band has already
expired into a locked slot. A lane that does that twice has taught the
operator that its alerts lie, which destroys the state-shaped alert's whole
premise — that a ring rarely fires and always means something.

The clamp is not a special case for international games. It falls out of one
rule, and it fixes an unrelated case nobody had raised: a league whose
waiver deadline is set at 09:00 Eastern (06:00 Pacific) rings at 22:00 the
night before, without anyone thinking about it.

## What the backwards-only rule costs, and why it is the honest cost

Some deadlines get **one** ring rather than two.

The escalation ring obeys the same clamp. For a 06:30 lock,
`deadline − 90min` is 05:00 — inside no waking window — so there is no
second ring. That is not a degraded delivery; it is an accurate one. For a
London game there genuinely is no second chance, and 22:00 the previous
evening was the last moment the operator could act. The pane still bands
`live`; the phone stays quiet.

Recorded because the alternative is tempting and wrong: ringing at 05:00
anyway "so at least it tried" converts an accurate silence into an
inaccurate noise, and trains the operator to ignore the channel.

## Where the constants live, and where the tzdb comes from

**Constants in the poller, not `settings`.** ADR-0016's Q3 reasoning
applies verbatim — `settings` is device-writable with no DELETE, so a
mistaken value is an unrecoverable change to what every device sees, and
there is no reader who benefits from tuning it. The tripwire is stated
plainly: if 22:00 turns out to be past the operator's bedtime in October,
the fix is editing a named constant and redeploying.

**The tzdb is the poller's, and may never be the worker's.** A civil-time
clamp needs a timezone database, and this one runs in an out-of-process
poller crate — which is what CLAUDE.md's thin-build rule protects:
*"nothing reachable from `hummingbird-authority-worker` may take an HTTP
client, a tzdb or an HTML parser as a dependency."* The panes' own
civil-date reasoning is untouched and still crosses ADR-0025's zone bridge,
because the core has no tzdb and this decision does not give it one.

## Consequences

- The delivery timing of a machine-raised alert is now a function of civil
  time, so it is a function of the operator's zone. A poller that clamps
  must know that zone; one that does not clamp must not pretend to.
- Two rings are no longer guaranteed. Any surface that says "you will be
  reminded again" would be lying for early-morning deadlines.
- This is written as a general rule rather than a fantasy-football one
  deliberately: the next obligation this system watches whose deadline can
  land at 05:30 inherits it, and should not have to rediscover the London
  case to get it right.
