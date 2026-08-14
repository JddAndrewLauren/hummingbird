# ADR-0023: The Grill interview is a native typed turn contract

**Status:** accepted · 2026-08-13
**Context:** issue [#352](https://github.com/JddAndrewLauren/hummingbird/issues/352),
the first slice of the in-app Grill plan (#349). Closes
[#41](https://github.com/JddAndrewLauren/hummingbird/issues/41) open
question 2 — "interactive interviews… render natively in the client… vs.
stay an agent conversation relayed turn-by-turn" — in favour of the native
arm. Numbered 0023 because 0017 and 0019 are both taken by unrelated,
already-accepted ADRs (the standing-question surface axis, and the Gmail
capture unit) and 0018/0020/0021/0022 are taken as well; the Grill plan's
own scope card had assumed 0019 was still free when it was written.
New glossary terms **Grill** and **triage process** land in `CONTEXT.md`.

## Decision 1 — a native typed turn contract, not a relayed agent conversation

A Grill interview is **one typed turn at a time**: `{question: {prompt,
recommendedAnswer, choices}, answer}`, threaded by the caller rather than
held in any server-side session. This is already how the runner's `grill-me`
skill works (`.claude/skills/grill-me/SKILL.md`, `docs/runner.md`) — this
ADR is the decision record for a shape #350 already built, closing #41 Q2
for every surface that follows it, native or otherwise.

**Rejected: relaying an agent conversation turn-by-turn.** The alternative
#41 named was rendering an underlying agent chat verbatim — prose the
client would need to parse or merely display — one exchange at a time. That
would make the client a transcript viewer with no stable contract to code
against: a future rendering surface (#355) would be reverse-engineering
prose instead of reading a `{prompt, recommendedAnswer, choices}` shape it
can lay out deterministically. The native contract is what makes "ask one
thing at a time" (`grill-me`'s own rule) a schema property instead of a
prompting convention that can drift.

**`choices` is never a closed set.** 2–4 short options are offered, but the
answer is always free text too — a human on #355's future surface, exactly
like the runner's own caller today, may answer with anything.

## Decision 2 — durable Grill records are immutable item-scoped attachments

A completed Grill is stored as an **immutable attachment on the item it
interviewed** — never edited after it ends, never replayed as a live
session. What is retained is the outcome (`summary`, `verdict`,
`model_proposal`, `applied_patch`, `resulting_stage`) plus the transcript
that produced it, not a resumable state machine. `grill-me`'s own
`priorOutcomes` seam already reads this way: a later Grill on the same item
sees what a prior one *decided*, never its back-and-forth
(`.claude/skills/grill-me/SKILL.md`: "You are never shown a past
transcript: only the outcome").

**#353 is where this becomes a table** (the `grills` table this ADR names
but does not design); this decision fixes only its shape — one immutable
row per completed Grill, keyed to the item it interviewed — so #353 has no
second decision to make about whether a Grill can be mutated or resumed.

## Decision 3 — rejected: an opaque server-side agent session

No Grill holds server-side conversational state — no session id, no
in-memory or database-backed chat history the runner remembers between
calls. `docs/runner.md` already states the invariant this ADR ratifies:
"There is no session here and nothing durable remembers a transcript
between requests" — every request carries the whole conversation in
`turns`, and a restart, a retry, or a different caller resuming the same
`turns` are indistinguishable from here.

**Why rejected:** an opaque session would make the runner (a Fly app with
`min_machines_running = 0`, per ADR-0017 decision 6) the sole holder of
in-progress interview state — unrecoverable across a cold start, and a
second place (beside the `grills` table decision 2 makes) that could claim
to know how a Grill is going. Stateless-per-request plus decision 2's
immutable-on-completion record between them cover both "mid-interview" and
"finished" without a third, transient state to keep consistent.

## Decision 4 — the transcript's wire disposition: `GET /api/grills/:id`, not the sweep

A completed Grill's full transcript is served **on request**, by its own
route, never carried inside `GET /api/sweep`'s payload. The sweep carries
what every device needs to reconstruct its mirror on every open — items,
steps, alerts, rules — and a Grill transcript is not read-path state any
device needs unconditionally; it is looked up when a human opens one
specific interview's history.

This narrows "the mirror is the export" the same way
[ADR-0016](0016-the-alert-horizon.md) narrowed it for old, settled alerts:
there the *wire* stopped carrying everything the *storage* holds, once an
alert was both settled and old. Here the same split falls along a different
line — never on the unconditional sweep, always available in full by direct
lookup — but it is the identical move: not every retained record earns a
place in the payload every device pulls every time.

**What #353 owns from here:** the `grills` table's DDL, `resulting_stage`'s
storage (computed from #352's `hummingbird_domain::resulting_stage`, never
re-derived), and `GET /api/grills/:id`'s handler are #353's decisions to
make; this ADR fixes only that the route exists and the sweep does not
carry the transcript.

## What this obliges

- **CLAUDE.md's map table** gains a **Grill** entry once #353 lands code to
  point it at (this ADR is docs-only; #352 carries no schema change).
- **[ADR-0009](0009-the-owned-schema-and-context-lanes.md)** gains an
  amendment-pointer entry in its Status header, per
  [the pointer convention](README.md): #353's `grills` table is a new
  first-class record this ADR's schema section did not anticipate.
- **#353** builds the `grills` table and the transcript route against
  decisions 2 and 4, using `hummingbird_domain::resulting_stage` (#352) for
  every stage it stores — never a second inference of the same mapping.
- **#354** builds the atomic Grill-completion mutation against decision 2's
  immutable-attachment shape.
- **#355** is the first native surface to render decision 1's typed turn
  contract to a human, with no comforts — the tracer for whether the
  contract holds up outside the runner's own `curl` caller.
