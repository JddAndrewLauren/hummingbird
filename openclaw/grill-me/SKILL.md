---
name: grill-me
description: Interview John about one foggy hummingbird item — one typed question at a time, ending in a proposal that resolves the fog or records that it remains, then record the accepted outcome. Use when John wants to grill an item, an item sits in the grilling stage, or a capture is too vague to act on.
---

# Grill me (OpenClaw arm)

Interview **one already-identified item** that carries fog, one question at a
time, until the fog is resolved or the interview says it remains. This is
the OpenClaw interactive arm of the hummingbird repo's `/grill-me` skill
(ADR-0029), a sibling of its hosted-runner op (#350). Where the runner op is
stateless and caller-threaded, **this session is the transcript** — you hold
the conversation yourself.

Vocabulary (**Item**, **Stage**, **Fog**) is the hummingbird repo's
`CONTEXT.md`. An item in the `grilling` stage is exactly the kind of thing
this interviews.

## The interview

Read the item first (`hummingbird-tasks` sweep context, or its `sweep
--json` for the full row). Then:

- **One question per turn.** Never bundle two asks — "which airport, and
  which dates?" is a defect, not efficiency. Offer your own recommended
  answer as a default, and 2–4 short choices where they help; free text is
  always a valid answer.
- Do not re-litigate what a prior grill already settled: prior applied
  outcomes (summary/verdict, never a past transcript) are visible in the
  sweep's `grills`.
- Stop when the fog is resolved or genuinely exhausted — never invent an
  answer past what John actually said. A handful of turns is normal;
  double digits means wrap up.

## The proposal

End with a proposal, stated plainly in chat: a **summary** of what the
interview settled, a **verdict** — `resolved` (the fog is gone) or
`fog_remains` (say so honestly) — and a **patch**: whatever item-field
edits the interview turned up (title, notes, size, energy, context,
priority, deadline). A `fog_remains` verdict commonly carries an empty
patch. Wait for John to accept, amend, or decline.

## On acceptance — apply, then record

Order matters (the record CAS-checks the item's version and this script
reads it fresh, so edits must land first):

1. Apply the accepted patch fields through the `hummingbird-tasks` skill's
   `edit` verb. Never set `--stage` here — the record applies the
   verdict→stage move itself, server-side (ADR-0023).
2. Write the full interview (questions and answers, in order) to a temp
   file and record the outcome:

```bash
{baseDir}/scripts/grill-record.sh record HB-42 \
  --verdict resolved \
  --summary "what the interview settled, in your words" \
  --transcript-file /tmp/grill-HB-42.md \
  --proposal "the proposal as you stated it" \
  --applied-patch "the patch fields John accepted, as stated"
```

Add `--delete-unticked-plan` only if John explicitly said the item's
existing unticked steps are now moot — it soft-deletes them server-side in
the same atomic write.

A declined proposal records **nothing** — no edit, no grill row, and say so.

## Boundaries

- The record script is one verb wide (`POST /api/grills`) and this skill
  never writes anything else; item edits go through `hummingbird-tasks`,
  step writes through `microtask`. Do not improvise API calls.
- A 409 from the record means the item moved after your edits — re-read,
  re-check with John, record again. A replayed record of the same
  transcript is idempotent (deterministic id) and returns the stored row.
- The interview itself costs nothing durable: only acceptance writes.
