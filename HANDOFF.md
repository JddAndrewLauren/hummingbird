# Handoff — one failure at a time: the per-item error surface

**Repo:** `/home/john/hummingbird` · **Written:** 2026-08-13

## The job

`TaskState` reports write failures one at a time, for the whole app. Decide
whether that is enough, and if not, change it. This is a store change with a
design decision in front of it — which is why it is a session rather than a
ticket.

## Where this came from

#418 fixed a failed triage that was displayed nowhere on Now. The fix
(`client/web/src/screens/triage-failure.ts`, merged or in PR #421) is honest
about its own ceiling, and the ADR-0021 amendment says so explicitly:

> `TaskState.lastTriage` holds the most recent result and not a map, so exactly
> one failure exists at a time and this line is honest about precisely that. A
> per-item error surface — for triage or for `lastAct`, which has the same
> shape and the same limit — is a store change, and a bigger decision than the
> bug that prompted this one.

That is the decision this session owns.

## The shape of the problem

`client/web/src/store/store.ts` carries five of these, all the same shape —
`{ seed, kind, error }` plus an identifier — and all "most recent only":

| Field | Identifier | Who reads it today |
| --- | --- | --- |
| `lastTriage` | `itemId` | `TriageRow` (per row), `RealFrontier` (screen-level, #418) |
| `lastAct` | `itemId` | `RealFrontier` → `ItemDetailPanel`'s `actError` |
| `lastCapture` | none — the seed *is* the identity | `CaptureBox` |
| `lastBindingWrite` | `key` | the bindings editor |
| `lastRuleWrite` | `ruleId` | `RulesScreen` |

**`lastAct` is the live twin of the bug #418 fixed.** `RealFrontier` computes
`actError` only when `task.lastAct.itemId === selectedItemId`, so an act that
fails after the reader closes the detail panel is displayed nowhere — exactly
the #418 defect, on the other mutation, and still unfixed. Confirm this by
reading `NowScreen.tsx`'s `actError` before doing anything else; do not take
this document's word for it.

Two failures in flight at once is not hypothetical: mutations queue in the
outbound queue and drain per sync cycle, so a cycle can resolve several.

## The decision

1. **Leave it single.** Cheapest, and defensible — one person, one device, and
   a second concurrent failure is rare. Then #418's screen-level line is the
   pattern, and `lastAct` should get the same treatment for symmetry. Small.
2. **Grow a map**, `Record<itemId, result>`, per mutation kind. Honest, and it
   unlocks per-card error text. Costs: eviction (nothing prunes it — see
   `deadLetters`, which "only ever grows until a re-apply flow exists"), and
   every reader changes.
3. **One failure lane.** A single list of recent write failures, whatever the
   mutation, with the item id on each — one surface, one eviction rule, and the
   per-kind `last*` fields go back to being just the clear-on-ok signal that
   `TriageRow` and `CaptureBox` actually use them for. Biggest change, possibly
   the right one, and it touches the dead-letter journal's territory — read
   `sync::queue` before assuming they are separate.

**The missing decision is which of these three.** Everything else is
mechanical once it is made.

## Where to look

- `client/web/src/store/store.ts` — the five result types and `TaskState`.
- `client/web/src/store/worker-client.ts` — where each is written from a
  broadcast; the `seed` contract lives here.
- `client/web/src/screens/triage-failure.ts` — #418's module, and the shape any
  general solution should subsume.
- `client/web/src/screens/NowScreen.tsx` — `actError`, the unfixed twin.
- `client/web/src/components/domain/ItemDetailPanel.tsx` — the other reader.
- ADR-0021's Consequences (the #418 amendment) states the ceiling; ADR-0007/8
  for the sync engine's contract; `CONTEXT.md` for **Dead-letter journal**,
  which is the nearest existing concept and may already be the answer.

## Constraints

- **The `seed` guard is load-bearing.** `TriageRow` and `CaptureBox` clear a
  draft only on an `"ok"` whose seed they have not processed (issue #222 — a
  draft must survive a failed write). Any restructuring keeps that or silently
  reintroduces #222.
- **Never a stored class.** Whatever is added is view state over broadcasts,
  not something the mirror persists — `CONTEXT.md`'s "computed by consumers at
  read time" applies.
- **ADR-0021 decision 2** still forbids colour on a card meaning anything but
  urgency, so a per-card error is text or nothing.
- If this changes what a surface shows, ADR-0021 wants an inline amendment
  under rule 2 of `docs/adr/README.md`, and `docs/SURFACES.md`'s row moves.

## Verification

`cd client/web` and call the binaries directly: `./node_modules/.bin/tsc -b`,
`./node_modules/.bin/eslint .`, `./node_modules/.bin/vitest run` (baseline
**1366**), then `./node_modules/.bin/playwright test` (**75** cases) if any
surface moves. `pnpm build && pnpm assert-no-fixtures` if anything under
`fixtures/` is touched.

`?demo=board` (#420) now renders the real Now board with a seeded failure, so
an error surface can be eyeballed without a device — it is the fastest way to
see whatever you build.

## Suggested skills

- **`/grilling`** — the decision above is exactly what it is for; take it
  before writing code.
- **`/hummingbird-design`** — mandatory before styling any new error surface.
- **`/tdd`** — a per-item surface is a red-green shape: assert two failures are
  both reachable, then make it so.
