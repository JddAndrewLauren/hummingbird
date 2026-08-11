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

## What to do

Given the raw capture text as input:

1. **Find the actionable line and use it verbatim as `title`.** Do not
   clean up, summarize, or rephrase it -- the sweeper's own rule
   (`docs/sweeper.md`) is "title verbatim, no cleanup, truncation, or
   prefix," and this skill keeps the same discipline so a capture reads
   the same whether it came in typed or dictated.
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
