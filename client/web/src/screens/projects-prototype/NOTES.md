# Projects page UX prototype — notes

**Question:** what should the Projects page (#449, slice 5/C5) look like —
how should list, detail, route, fog, actions, steps, links, properties and
archive be arranged?

**Where:** branch `prototype/projects-ux`, mounted on the Routes screen slot
in dev builds only. Run `pnpm dev` in `client/web`, open the Projects nav
entry, flip variants with the floating bar, the arrow keys, or `?variant=`:

- `?variant=a` — **Master–detail.** Persistent project list on the left,
  everything about the selected project in one dense scroll on the right.
- `?variant=b` — **Gallery + dossier.** Card grid of the portfolio, then a
  full-page drill-in per project on the sanctioned TwoColumn/Aside skeleton
  (steps live in the aside, as the old RoutesScreen framed them).
- `?variant=c` — **Outline.** No detail view: every project is an expandable
  row, all editing inline, several open at once.
- `?variant=t` — **Triage inline create.** Not a layout — the triage
  project select gaining "+ New project", the v1 round-trip posture, and
  the copy-at-mint context fill.

World state is in-memory, shared across variants, and seeded with a
pre-archived project ("Kitchen remodel") whose one earlier-archived action
demonstrates the timestamp-matched cascade: archive/unarchive Greenhouse or
Kitchen and watch the dialog copy and what comes back.

**All flows from #449 are exercised in every layout variant:** create
project (600ms simulated round-trip), rename, github_repo edit (stored
`owner/repo`, displayed as derived URL), default_context, links CRUD, route
destination + notes, fog create/edit/resolve/reopen, action reorder
(up/down; the real thing gets a dedicated reorder mutation), steps
tick/add/edit/delete, archive with the honest cascade dialog.

**To delete the prototype:** this directory, App.tsx's dev branch around
`ProjectsPrototype`, the two "Projects" strings in `shell/screens.ts`, and
the PROTOTYPE block in `components/core/Icon.tsx` (import + ICON_MAP).

---

## Verdict (operator, 2026-08-14)

- **Winning variant: B — gallery + dossier.** "The easy winner."
- **Stolen bits:** from A, inline step expansion — selecting an action in
  the dossier expands its steps under the row instead of loading them into
  the aside. (VariantB.tsx now implements this; the aside keeps
  properties, links, and archive only.)
- **Decisions this confirms/refines for #449's C5:** ProjectsScreen is a
  two-level surface — a card-grid list view (live projects, show-archived
  toggle, inline New-project card) and a full-page detail on the
  TwoColumn/Column/Aside skeleton. Reading column: route
  destination/notes, ordered actions with reorder + inline expandable
  steps, fog. Aside: properties (github repo as derived URL, default
  context), links CRUD, archive with the cascade dialog. Back affordance
  returns to the grid.
