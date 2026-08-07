---
name: microtask
description: Break one already-selected, stalled Linear issue into a checklist of ~2–5-minute concrete physical steps written into the issue body, lowering activation energy on it. Use when the user invokes /microtask, says an issue is "too big to start", asks to "break this down", or a picked issue has stalled. Not a planning tool — /to-actions does decomposition, next-up-personal does selection.
---

# Microtask

Lower activation energy on **one already-selected, stalled issue** by writing a checklist of
tiny concrete steps into its Linear description. Write first, offer company second. Speed is
the value.

All Linear reads/writes go through `scripts/linear.sh` (in this skill's directory):

- `linear.sh get <IDENT>` — issue JSON (identifier, title, description, project, labels)
- `linear.sh set-description <IDENT> <file>` — replace the description with the file's markdown

## Invocation

`/microtask <issue-id>` (e.g. `ION-10`), or on an issue already in conversation context
(handed off from `next-up-personal` or `/to-actions`). Resolve it with `linear.sh get`
immediately.

## Read first, ask at most once

Read title, description, project, labels. Ask **one** question only if they give nothing to
work with — "what's in the way?" or "what does done look like here?". Never a second: three
questions in, the user would rather just do the chore.

## Write the steps

Steps live in a marker-delimited section of the description so re-runs replace, not duplicate:

```markdown
<!-- microtask:start -->
## Steps

- [ ] Put on music, grab a trash bag
- [ ] ...
<!-- microtask:end -->
```

- Each step is **one concrete physical action, ~2–5 minutes**. Minutes, not sittings.
- **First step deliberately trivial** ("put on music, grab a trash bag") — the ramp.
- Anti-patterns, never do these: sub-issues in any form; multi-issue breakdowns;
  planning-shaped steps ("decide what to keep" is fog, not a step); re-ranking or selection.

Write procedure — **read-modify-write on every write**; concurrent sessions edit these issues,
so a stale in-memory copy clobbers another editor:

1. `linear.sh get` the issue *now*, even if fetched moments ago.
2. If markers exist, replace only the text between them; otherwise append the whole section
   (blank line before the start marker). Leave the rest of the description untouched.
3. `linear.sh set-description` with the merged result.

## Refresh rule

If the fetched body already has a marker section containing checked steps, **report the
checked steps to the user before rewriting**. Match case-insensitively — Linear normalizes
`- [x]` to `- [X]` when it stores the description — never silently discard progress. Then rewrite
the section (resumed/refreshed, not duplicated).

## Walk-through mode

Offer it **only after** the checklist is written, never before. On accept: the user reports a
step done → re-fetch (`get`) → toggle that `- [ ]` to `- [x]` inside the markers →
`set-description` → hand over the next step. Declining costs nothing — the checklist is
already persisted and usable from Linear's mobile UI.

## Failure modes

- **Missing API key** — the script prints one provisioning line and exits non-zero. Relay
  that line; no stack trace.
- **Issue not found / GraphQL errors** — Linear returns HTTP 200 with an `errors` array; the
  script surfaces the message and exits non-zero. Report it and stop.
- **Scope guard** — write only the description field. No state changes, no labels, no new
  issues.
