# ADR-0030: The project lane is shared-owned and client-writable

**Status:** accepted · 2026-08-20
**Context:** #449 and its brainstorm of 2026-08-14 (that issue holds the
settled decision list and the UX verdict). Amends
[ADR-0009](0009-the-owned-schema-and-context-lanes.md)'s project lane:
`projects` grows two columns, a `project_links` table joins the synced set,
and the "`/to-actions` owns it" claim its `routes` DDL comment records is
withdrawn. Inherits [ADR-0020](0020-no-delete-rows-are-flagged-not-erased.md)
for the new table's flagged deletes and
[ADR-0008](0008-the-authority-is-an-app-owned-server.md)'s CAS discipline
for the ownership change. Nothing here touches ranking, the frontier, or
[ADR-0021](0021-the-frontier-in-columns.md)'s axes. *(That last clause held
for this ADR's own slices and no longer describes the surface: the dossier's
centre column is now that frontier board, minus the degenerate Project axis
— see the Consequences amendment below, and ADR-0021's decision 1.)*

## The problem

The project lane is server-complete and client-dead. `projects`, `routes`
and `fog` have had tables, DTOs and write routes since ADR-0009, but the
only writer is `/to-actions`' `hb.sh`; the client core exposes a single
read (`projects()`, used to resolve frontier group names) and not one
mutation; and the web's Routes screen is a demo fixture that has never been
wired to real state.

That is a shape, not an accident: ADR-0009 wrote `routes` as a separate
table *because a skill owned it*, and a skill-owned lane needs no UI. The
operator wants projects to be a real surface — creatable from the app,
including inline while triaging, carrying properties the skill never
invented, and fully editable. Every one of those gestures is a second
writer on rows a skill has been treating as its own.

## Decision

1. **Shared ownership; CAS arbitrates.** Route content (destination,
   notes), fog rows, and each item's `project_pos` stop belonging to
   `/to-actions` and become shared between the skill and every client. No
   new mechanism: these are ordinary versioned rows, and the
   version-counter compare-and-set that already arbitrates two devices
   editing one item arbitrates a skill and a device editing one route. The
   skill loses no capability; it loses only the assumption that nothing
   else writes. `/to-actions` re-running over a route a human has edited is
   therefore a normal 409-and-rebase, not a corruption.

   This ADR is the reason the stale claim is being removed from three
   places at once (ADR-0009's DDL comment by pointer, `CREATE_ROUTES`'s doc
   comment, CONTEXT.md's **Route** entry). A comment asserting an ownership
   the code no longer has is the false-comment bug class: it reads as a
   constraint and is enforced by nothing.

2. **`projects` grows `github_repo` and `default_context`; there is no
   description field.** `github_repo` stores the canonical `owner/repo` and
   nothing else — the URL is derived for display, never stored, so there is
   one spelling to compare and no half-typed link to normalize. GitHub is
   deliberately special-cased rather than generalized: a future integration
   wants a repo identity, not a link. Everything else a project wants to
   point at is a **link** (decision 4), which is why no free-prose
   description is added — the fields that exist are the ones something
   reads.

3. **`default_context` is copy-at-mint, never a read-time join.** When an
   item enters a project carrying no context of its own, the project's
   `default_context` is *copied onto the item* at that moment, at the three
   entry points where the gesture happens: triage promotion with a project
   selected, `/to-actions` minting actions, and assigning a project to an
   already-existing context-less item. Never retroactively, and never
   resolved at read time.

   This is deliberately in tension with CONTEXT.md's **Context** entry,
   which says an item naming no context is doable anywhere, so *absence
   widens rather than narrows*. Copy-at-mint narrows such an item the
   moment it joins a project. The tension is resolved in favour of copying
   because the copy is **visible and editable**: what the item shows is
   what the item is, one field, clearable by the human like any other. A
   read-time join would preserve the letter of the glossary while making an
   item's context depend on a row somewhere else — the same value, but
   unexplainable at the point of use and unclearable without leaving the
   project. Narrowing visibly beats widening invisibly.

4. **Links are a first-class table, flagged never deleted.**
   `project_links` (`id`, `project_id`, `url`, `label`, `position`,
   `removed_at`, `version`) joins the synced set and rides the
   version-counter delta like every other table, under
   [ADR-0020](0020-no-delete-rows-are-flagged-not-erased.md)'s rule:
   removing a link stamps `removed_at`, it does not erase the row. It
   references `projects`, not `items`, so it is not part of the item
   cascade machinery. `ChangesResponse` gains the table behind
   `#[serde(default)]`, on the #131 precedent — a response predating this
   slice carries no key at all and must still deserialize.

5. **Archiving a project is a timestamp-matched cascade.** Archiving
   stamps every live item in the project with the project's **exact**
   `archived_at`; unarchiving clears the stamp only on items whose
   `archived_at` matches the project's. The round trip therefore restores
   exactly what the archive took down, and an item the human archived
   individually beforehand stays archived — which a blanket "clear all
   `archived_at` in this project" would silently undo. The matched
   timestamp is the whole mechanism: it is the record of *which* archive
   gesture took a row down, stored in a column that already exists.

   Steps cascade **implicitly**. A step is reachable only through its item,
   so it disappears and returns with it; the cascade never stamps
   `deleted_at` on a step. Deleting is a different verb from archiving and
   this ADR does not conflate them.

## Rejected alternatives

- **Keeping `/to-actions` the sole writer** and giving the UI a read-only
  Projects page — the ask is editing, and a read-only page defers the
  ownership question rather than answering it.
- **A read-time join for `default_context`** — preserves the glossary's
  "absence widens" letter, but makes an item's context depend on a row it
  does not name, unclearable from the item and invisible in every surface
  that shows the item alone. See decision 3.
- **A free-prose `description` on projects** — a second place for the
  Destination the route already holds, readable by nothing.
- **Storing the GitHub URL** rather than `owner/repo` — two spellings of
  one identity, and a normalization step at every comparison.
- **A generic `integrations` table** instead of special-casing GitHub — an
  abstraction with exactly one member and no second use in sight.
- **Clearing every `archived_at` in the project on unarchive** — simpler,
  and it silently resurrects items the human had archived deliberately
  before the project was.
- **Cascading `deleted_at` onto steps** — a cascade that outlives the
  unarchive it came from, and conflates delete with archive.

## Consequences

- `/to-actions` and the runner's copies of `hb.sh` must be prepared for a
  409 on route, fog and `project_pos` writes; they already carry the
  read/CAS/rebase discipline for items, so this is a widening of an
  existing path, not a new one.
- Two schema growths follow from decisions 2 and 4. Both are additive, so
  each is an `add_missing_columns`/`CREATE TABLE IF NOT EXISTS` growth with
  its own fixture — and the `CREATE_PROJECTS` literal must be spelled
  exactly as the ALTERs splice it, or the byte-identical-DDL growth tests
  fail (`server/authority/src/schema.rs`'s own header states the trap).
- `default_context` makes the item's `context` field carry a value the
  human did not type. It is indistinguishable from a typed one by design;
  the provenance is not recorded, and no surface explains where it came
  from beyond the project's own properties.
- The client core gains its first project-lane mutations. They follow the
  rules stack's contract — enqueue, CAS, **no optimistic overlay** — so a
  newly created project appears on the next completed sync cycle, which is
  why creating a project inline from triage round-trips before the picker
  updates.

*Amended 2026-08-21: **the web client withdrew two of its readers.** The
dossier's centre column became [ADR-0021](0021-the-frontier-in-columns.md)'s
frontier board filtered to the open project (see that ADR's own amendment to
its decision 1), replacing the ordered action list and the fog card the
#628/#629 slices put there. Nothing in this ADR is reversed: fog and
`project_pos` remain shared-owned, versioned and client-writable at the
authority, `/to-actions` still writes both, `Core`'s `open_fog_for`/
`create_fog`/`patch_fog`/`actions_for`/`patch_action_position` and the step
writes are untouched for the other clients — what went is the web's fog and
action-list *rendering*, and with it that surface's own reads. The project
lane's client write door on the web is now the aside's four record cards:
Route, properties, links, archive. Membership on the new centre column is
`project_id` **alone**, which is a widening, not a narrowing: the list it
replaced required `project_pos`, so an action assigned to a project and never
positioned was invisible on the very page that was supposed to hold it.*
