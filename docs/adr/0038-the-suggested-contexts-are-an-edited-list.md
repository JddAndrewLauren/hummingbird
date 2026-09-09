# ADR-0038: The suggested contexts are an edited list — one synced row, with a removal that cascades

**Status:** accepted · 2026-09-09
**Context:** the operator asked for a small Settings section to add and
remove context tags, with one rule: an item carrying a context that is
removed is left with no context. Until now the *suggested* list every
capture and edit form offered was a compile-time constant
(`decisions/vocabulary.rs`'s `CONTEXTS`, pinned into `field-vocabulary.ts`
and handed to Android through `capture_form_meta()`), so a place the
operator stopped working in stayed offered forever and a new one could only
be typed. `items.context` itself was never the problem: CONTEXT.md's
**Context** is an open vocabulary and stays one. This ADR decides where the
list lives now, what removing an entry does, and why the storage shape
differs from [ADR-0034](0034-a-standing-question-can-be-switched-off.md)'s.
Amends ADR-0034 (a third vocabulary over `settings`, and one that *does*
cascade) and
[ADR-0025](0025-decisions-sink-to-the-core-rendering-stays-per-client.md)
(the list is no longer a compile-time canonical). Term **Context** is
amended in `CONTEXT.md`.

## The decision

1. **The suggested list is one `settings` row, key `contexts`, holding a
   JSON array of strings in the order the forms offer them.** The third
   typed vocabulary over ADR-0009's KV table (`client/core/src/contexts.rs`),
   read and written through the same entity-level CAS path bindings and
   question switches use (`Core::enqueue_setting_write`), overlaid so an
   edit made offline reads back at once. No row, or a row this build cannot
   read as an array of strings, means the build's `DEFAULT_CONTEXTS` — the
   six the constant used to hold — so nothing changes for a workspace that
   never edits the list, and an unreadable row never empties a form.

2. **Not a `BindingKey`.** ADR-0034 decision 2's argument holds again: a
   binding is text a pane reads, in a free-text box, and this is an ordered
   list with an editor of its own. `Core::bindings` subtracts the row as it
   subtracts the switches, so the bindings editor never offers a JSON array
   as free text.

3. **One row, not one row per context — the opposite of ADR-0034's
   choice, for a reason ADR-0034 did not have.** The list's order is
   meaningful: `frontier::contexts_of` orders the frontier's context chips
   by it. Per-name rows would carry no order and, in a table with no
   DELETE, would accrete one permanent row per context ever tried. The
   cost is concurrency: two devices editing the list in the same instant
   are two CAS writes to one version of one row, so the second 409s on the
   same field, dead-letters and its overlay reverts — visibly, in the
   dead-letter journal, never silently. A personal workspace edited from
   two devices at once is the case accepted.

4. **Removing a context cascades, client-side, as ordinary writes.**
   `Core::remove_context` enqueues the list write first, then one `PATCH`
   per live item whose context matches (`context: null`, the single-field
   clear `Core::triage` already carries), then one `PATCH` per live project
   whose `default_context` matches — the operator's call, so the next item
   to join that project is not re-tagged with a context that no longer
   exists. Each clear is its own queue entry with its own deterministic
   seed, so a conflict dead-letters one named item rather than the batch.
   The list goes first deliberately: should it later dead-letter (decision
   3's case) the clears still land, and an item with no context is never
   wrong where a surviving chip over cleared items would be. No server
   change: the authority's `PUT /api/settings/:key` accepts any key, and
   the clears are the routes every client already writes. ADR-0030
   decision 5's server-side cascade was the alternative and was not taken:
   it exists because an archive must be timestamp-matched and reversible in
   one write, and a context clear is neither.

5. **Archived items are never touched.** They are not in
   `Core::overlaid_items` at all — Recall's rule, history stays readable and
   never editable — so they keep whatever they carried. Done-but-not-archived
   items are cleared like any other live one; the Ledger keeps them
   editable.

6. **"Carries this context" is the ranker's rule.** `rank::normalize_context`
   — trimmed, leading `@` dropped, ASCII-lowercased — decides the cascade,
   its count, and duplicate detection on add, so `@Errands` and `errands`
   go when `@errands` is removed, as the ranker's hard filter already treats
   them as one context. Adding trims, refuses empty, refuses the frontier's
   own `no context` label, and refuses a normalised duplicate; `@` is not
   required, because the field is free text and an item may already carry
   `errands`.

7. **Every surface that offered the constant offers the live list.** The
   web's capture box, item panel and frontier chips read
   `TaskState.suggestedContexts` (falling back to `DEFAULT_CONTEXTS` until
   the worker has published it — never an empty list); the phone's capture
   form and item editor read `MobileTaskHost::suggestedContexts` through an
   injected door beside the compiled-in `captureFormMeta`, falling back the
   same way. The Settings section is web-only, on the precedent
   `CLAUDE.md` records for the question switches; Android carries the row
   and reads it.

## What this does not decide

- Whether a context typed into a capture should be *added to the list*.
  It is not: the capture form's suggestions stay the list unioned with the
  contexts live items carry (`field-vocabulary.ts`'s `contextSuggestions`),
  so a typo does not become a permanent entry. The same change fixes the
  reason that union looked broken — the worker client never re-read the
  triage inbox behind a capture, so a context typed once waited for the
  next sync cycle to be offered again.
- An Android Settings section. The row syncs and the phone reads it; the
  editor is the web's until asked for.
