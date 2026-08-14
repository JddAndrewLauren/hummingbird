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

## Verdict (fill in before deleting)

- Winning variant:
- Stolen bits from other variants:
- Decisions this changes or confirms in #449's C5:
