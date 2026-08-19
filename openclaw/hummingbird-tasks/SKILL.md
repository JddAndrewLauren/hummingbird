---
name: hummingbird-tasks
description: Read and work John's hummingbird task list against the app-owned authority — sweep the live items for session context, add captures, edit item fields, and mark items done. Use at session start (run sweep first, always), when John mentions something to do, asks what's on his plate, or asks to change or complete a task.
---

# hummingbird-tasks

The task authority is the app-owned server at `hb.twinion.net` (ADR-0008).
Everything here goes through one script; never call the API directly.

```bash
{baseDir}/scripts/hb-tasks.sh sweep              # live items, one per line
{baseDir}/scripts/hb-tasks.sh sweep --json       # the raw sweep payload
{baseDir}/scripts/hb-tasks.sh add "call mom about the trip" --notes "she asked Tuesday"
{baseDir}/scripts/hb-tasks.sh edit HB-42 --stage ready --priority 2
{baseDir}/scripts/hb-tasks.sh done HB-42
```

## Session start

Run `sweep` before anything else and treat its output as your working
context. It renders the live working set (not archived, not done), ordered
in_progress → ready → blocked → grilling → triage, then by priority. Do not
cache it across a conversation lull — re-run it when you are about to act
on it; other devices and pollers write to the same authority continuously.

## The verbs

- **`sweep [--json]`** — the only read. There is no by-id item route;
  everything reads `GET /api/sweep` once per script run.
- **`add <title> [--notes] [--stage] [--priority] [--deadline]`** — a new
  item. Without `--stage` it lands in `triage`, exactly like a hand
  capture. Titles come from John's words, not your paraphrase.
- **`edit <ref> [--title] [--notes] [--stage] [--priority] [--deadline]
  [--scheduled]`** — field edits under CAS (`expected_version`, one bounded
  409 retry). A same-field conflict means another writer moved the row:
  re-run `sweep`, look, and ask John rather than overwriting.
- **`done <ref>`** — stage → `done`. Already-done is success, not an error.

`<ref>` is `HB-<seq>` (what the render shows) or a raw item uuid. Stages:
`triage`, `grilling`, `ready`, `in_progress`, `blocked`, `done`.

## Boundaries

- This skill touches **items only**. Steps belong to the `microtask` skill,
  grill records to `grill-me`. Do not improvise other API calls.
- Confirm before `done` or a stage change John didn't explicitly ask for.
- Deleting is not offered anywhere; there is no delete verb by design.
- The token at `~/.config/hummingbird/api-token` is a write-everything
  device credential. Never print it, never copy it, never pass it as an
  argument.
