---
name: parse-capture
description: Parse a raw capture (dictated or typed, often messy) into a title and notes. Runner-only (#41, #256) -- invoked headless via POST /run {skill: "parse-capture", args: {text}}, never interactively; not a skill a person invokes by name in a normal session.
---

# parse-capture

Turn one raw capture into the v1 minimal schema (`schema.json`, beside this
file): `{title, notes}`. This is #42's bake-off's hosted baseline arm and
the production online-parse path (#41 decision 4/6, #256) -- it writes to
**nothing**. The result is handed back through the runner's `POST /run`
response; nothing here calls Linear, the owned authority, or any other
write target. That question is deliberately deferred (#256).

## Which lane this governs

This skill governs the **unattended** dictated-capture lane only:
phone/watch/speaker -> Gemini -> Google Tasks -> sweeper -> Triage, where no
author is present when the text lands -- that absence is why the same
discipline (drop nothing, infer nothing) is what this skill's bake-off arm
applies below, mirroring the sweeper's own verbatim rule (see "What to do").

A second, **attended** lane -- in-app dictation -- is planned but does not
exist yet: it is #377's proposal (which superseded #362), gated on the probe
#378, which explicitly contemplated the gate failing and the lane closing
unbuilt. That gate passed on the desk browser -- see ADR-0022, which
constrains the lane to client-local recognition only. If it
ships as designed, it would deliberately skip this skill -- the operator
would be watching the transcript and editing it in place before submitting,
so the human would be the parser there, and no skill would be invoked for
that lane. The "same whether typed or dictated" rule in step 1 below is
scoped to the unattended lane this skill actually governs; the planned
attended lane, if and when #377 lands, would be a deliberate exception to
that rule, not a violation of it.

## What to do

Given the raw capture text as input:

1. **Find the actionable line and use it verbatim as `title`.** Do not
   clean up, summarize, or rephrase it -- the sweeper's own rule
   (`docs/sweeper.md`) is "title verbatim, no cleanup, truncation, or
   prefix," and this skill keeps the same discipline so an unattended
   capture reads the same whether it came in typed or dictated -- see
   "Which lane this governs" above for the planned attended-dictation
   exception.
2. **Everything else becomes `notes`.** If the whole capture is usable as
   the title with nothing left over, `notes` is an empty string -- never
   omitted, never null (the schema requires both keys).
3. **Infer nothing else.** No due date, no context/energy/size label, no
   project guess. `docs/sweeper.md` drops due dates on purpose ("a
   Gemini-inferred date is a scheduling decision made by a transcription
   engine") and this skill holds the same line for every stretch field
   #42 names -- those are probed separately, never guessed here.
4. **Never drop content.** If the capture is genuinely just noise (empty,
   or nothing usable as a title), fall back to `title` = the first
   non-blank line of the raw text (mirroring the sweeper's own fallback)
   rather than fabricating a title from nothing.

## Output

Respond with exactly the JSON object `schema.json` describes --
`{"title": "...", "notes": "..."}` -- and nothing else. The runner passes
this schema to `claude -p --output-format json --json-schema` to constrain
the response; there is no prose wrapper to produce.
