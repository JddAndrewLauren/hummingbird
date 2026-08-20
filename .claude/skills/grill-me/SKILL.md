---
name: grill-me
description: The item-scoped interview for a foggy capture -- one typed question at a time, ending in a proposal that either resolves the fog or records that it remains. Use for the hosted-runner /grill-me op invoked via POST /run. Writes nothing itself; a caller applies whatever it decides to do with the proposal.
---

# Grill me

Interview **one already-identified item** that carries fog, one typed turn at a time, until
the fog is resolved or the interview says it remains. This is the runner's fourth op
(#350), and everything below is the hosted runner arm -- there is no interactive script in
*this* directory and no client here. Since ADR-0029 an OpenClaw arm of the same interview
exists at `openclaw/grill-me/`, running on that agent's own model with its own SKILL.md;
a future slice (#355, the tracer) adds a person-facing client surface once #351's
live-run gate passes.

Vocabulary (**Item**, **Stage**, **Fog**) is in the root `CONTEXT.md`. An item in the
`Grilling` stage is exactly the kind of thing this interviews.

## Stateless -- every request carries the whole conversation

There is no session here and nothing durable remembers a transcript between requests. The
caller (today, `curl`; later, a client) threads the conversation itself: each request's
`turns` is the complete list of rounds so far -- the question asked and the answer given,
in order -- and this run answers with exactly the next turn. A restart, a retry, or a
totally different caller picking the same `turns` back up all look identical from here.

Invocation: `POST /run {skill: "grill-me", args: {ref, turns, model?}}`.

- `ref` -- `HB-42` (case-insensitive) or a bare uuid, resolved against the authority
  before this skill is ever called. An unknown ref never reaches you.
- `turns` -- `[]` to open the interview; otherwise one entry per round already
  completed: `{question: {prompt, recommendedAnswer, choices}, answer}`. `choices` is
  2-4 short options **and never a closed set** -- the caller (and, on the client surface
  to come, the human) may answer with free text instead of any of them, and this skill
  must accept whatever `answer` string arrives without checking it against the prior
  turn's `choices`.
- `model` -- picks which model runs you (#273); it never reaches your prompt.

The item, this session's `turns`, and this item's **prior applied grill outcomes** have
already been read from the authority and follow as JSON. You have no shell here -- do not
try to fetch anything yourself, do not write, and do not call any other skill.

## Prior applied outcomes, never a prior transcript

If this item has been through a *previous, separate* grill that ended in a proposal, its
`summary`, `verdict` and `patch` ride in `priorOutcomes` -- so you know what was already
settled and do not re-litigate it. You are **never** shown a past transcript: only the
outcome, never the back-and-forth that produced it. (Today `priorOutcomes` is always
`[]` -- nothing persists a completed grill yet; #353 is what starts populating it. The
seam is here regardless, so treat a non-empty `priorOutcomes` as real when you see one.)

## Answer with exactly one typed turn

Two shapes, and exactly one per response (`schema.json` enforces this with `oneOf` --
answering with both, or with neither, is outside the schema and a failed run, not a
partial success):

- **`{kind: "question", question: {prompt, recommendedAnswer, choices}}`** -- the next
  thing to ask. `prompt` is the one question, never a restatement of everything asked so
  far. `recommendedAnswer` is your own best guess, offered as a default. `choices` is
  2-4 short options; free text is always still a valid answer regardless of what you
  list. **Ask one thing at a time.** A turn that bundles two questions ("which airport,
  and which dates?") is a defect, not efficiency -- the whole point of typed turns is that
  each is answerable on its own.
- **`{kind: "proposal", proposal: {summary, verdict, patch}}`** -- the terminal turn, once
  the fog is resolved or you judge it exhausted. `summary` is what the interview settled,
  in your own words, not a transcript replay. `verdict` is `resolved` (the fog is gone) or
  `fog_remains` (say so rather than inventing an answer past what was actually said).
  `patch` is whatever item-field edits the interview turned up (`title`, `description`,
  `size`, `energy`, `context`, `priority`, `deadline`) -- every key optional, and a
  `fog_remains` verdict commonly carries an empty one. **You never write anything
  yourself** -- the caller decides what, if anything, to do with the proposal.

## The turn cap

`prepare` declines before you are ever called once an interview reaches its cap --
provisional today (`runner/src/skills/grill-me.js`'s `PROVISIONAL_TURN_CAP`), the real
number set by #351's live-run measurement. If you are running, the request was under the
cap; you have no cap-awareness to apply yourself.

## Failure modes

Every authority failure -- no token, an unreachable server, an unknown ref, past the turn
cap -- happens **outside** your run and ends the stream in a named `{ok:false, error}`
envelope before you are ever invoked. Your only failure mode is answering outside the
schema.

## Scope guard

This op **writes nothing** (#350's brief, restated as the thing to never relax): it has no
`apply` and calls no write method on the authority. If a later slice wants this interview
to land something durable, that is a different op's job, decided in its own issue, never
smuggled in here as an unannounced side effect of answering a question well.
