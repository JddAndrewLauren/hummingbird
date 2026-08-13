# PROTOTYPE — Now screen grouping & filtering

Throwaway. Delete this whole directory (and the `?variant=` branch in
`../NowScreen.tsx`) once the question below has an answer.

## The question

Today's Now screen groups the frontier by **project** and filters it not at
all: `orderFrontier` sorts by priority then deadline, `groupByProject`
sections it, and every startable action is on screen. Is project the right
grouping axis when the reader is picking what to do *right now* — and does
this surface want a filter at all?

## How to run it

```sh
cd client/web && pnpm dev
```

Then `http://localhost:5173/?variant=A` (or `B`, `C`). Arrow keys and the
floating bottom bar switch variants; the URL is the state, so a variant is
reloadable and shareable. With fewer than 8 real frontier items the variants
render `fixture.ts` — 26 in-memory actions across six projects, five contexts
and a spread of deadlines — and say `fixture` in the switcher; with a live
authority behind `wrangler dev` they render the real frontier and say `live`.
Nothing here writes anything, and over the fixture the rows are inert.

## The three variants

| Key | Name | The claim it makes |
| --- | --- | --- |
| A | Narrow it down | Grouping is the wrong tool — one flat run ordered by attention, with sticky facet chips (where / size / energy / pressing). Project demoted to a leading label. Primary gesture: **subtract**. |
| B | Lanes | The grouping *is* the filter — six computed lanes (Overdue, Due today, Scheduled today, Quick wins, Deep work, Everything else) in a fixed descending-claim order, collapsible, empty lanes stated rather than hidden. No controls at all. |
| C | Board | Wrapping columns, with the **axis** as the everyday control (context / project / size / energy). Compact cards instead of rows; picking a column is the filter. **Carries the round-2 grafts below.** |

They deliberately disagree about structure, not colour: A has no groups, B has
no controls, C has no rows.

## Round 2 — C wins, with A's filters and B's colour

Operator verdict after the first pass: *"I like C the best overall, but I
appreciate the colour coding in B and the filters in A. Let's hide A's filters
behind a button in C. I also don't like how it ends up scrolling
horizontally."* C was reworked in place; A and B are untouched, kept for
reference.

- **A's facets, behind a Filter button.** The axis switch is the everyday
  control and filtering the occasional one, so only one of them holds
  permanent space. The button carries a count badge and an `n of m shown`
  readout whenever anything is active — a filtered board that looks
  unfiltered is a lie. The chip row is now shared (`facet-chips.tsx`), so A
  and C differ over whether it is *visible*, not over how it is drawn.
- **B's lane colours, on each card's leading edge.** The lanes are a
  partition, so a coloured edge always means exactly one thing, and it reads
  *across* the grouping axis: colour answers "how much claim does this have",
  position answers "where does it belong". The first pass's urgency dot is
  gone — two encodings of overlapping facts on one 240px card was one too
  many. The lanes are named once in a legend instead of six times as
  headings.
- **No horizontal scrolling.** Columns wrap onto as many lines as the width
  needs, in reading order, and each is capped at six cards with an `n more`
  toggle. The cap is what makes wrapping work — one fat column would
  otherwise set the height of a whole line and strand its neighbours in
  whitespace — and it is the honest cap for this surface anyway: the top few
  of a column is what "what's next" is asking about, and the count never lies
  about what is hidden. A CSS multi-column container was tried first and
  rejected: height-balancing pushed the 13-card column's neighbours below the
  fold. Checked for page overflow at 1440 and 1024, filters open and shut.

## Round 3 — collapsible column headers, collapsing in place

Operator: *"Make the context headers collapsable."* Each column header is a
collapse control (chevron + label + count), and the count stays readable while
shut — a closed column must still say how much is inside it.

The first attempt at this moved a collapsed column out of the board into a
`folded` chip strip above it. The operator rejected that: *"I want to be able
to collapse the context headers, but I don't want them to fold into the
header."* A collapsed column now stays exactly where it is in the board order.

What makes that work is **shrink-to-fit width**, and this is the wrinkle worth
recording, because it is the same one as the horizontal-scroll fix: a wrapping
row takes its height from the tallest column in its line, so a collapsed
column that still claimed a full 240–380px slot would leave a hole rather than
buy any space. Collapsed columns switch to `flex: 0 0 auto` and shrink to
their header, so the neighbours reflow around them and the only cost is a thin
strip of empty page under the header for the height of that line. Collapse
state is keyed by the column label, so switching the axis clears it along with
the per-column `n more` expansions.

## Round 4 — the selected item expands above the board, which stays put

Operator: *"when I click on an item, I want it to expand into the existing
selected item UI. That's where the 'Grill Me', and other buttons will exist."*
Then, on the first attempt: *"I don't want it to take over the whole screen. I
want it to expand the pill and go to the top, but leave the rest of the board
visible."*

So selection is **not** a takeover, which is what the frontier does today
(`RealFrontier` returns `ItemDetailPanel` *instead of* the list). The expanded
item mounts at the top of the centre column and the board stays standing under
it: picking one action should not cost you the view of everything you might
have picked instead. Three consequences worth recording:

- **The panel is the real `ItemDetailPanel`, not a prototyped one.**
  `NowScreen` threads it the app's own `onAct`, `task.stepsByItem`, the
  `lastAct` error and the `microtask` wiring, so every affordance that lives on
  item detail — the act row today, Grill me at #359 — arrives unchanged and
  needs no second code path. Over the fixture (whose ids exist in no query) the
  same panel mounts with an inert `onAct` and no `microtask`: `?demo`'s
  precedent for an affordance that must not issue a real request.
- **The card stays marked while it is open** (`aria-current`, an
  `--accent-quiet` fill; `ItemRow`'s own `selected` prop in A and B). With the
  board still on screen the reader has to be able to see where the thing at the
  top came from, and what it was sitting next to.
- **The panel is scrolled into view on select.** A card near the bottom of a
  long board would otherwise expand off-screen, which makes "it goes to the
  top" true of the DOM and false for the reader.

The slot above the frontier is Now's own: ADR-0015 reserves the *aside* for the
ranked region and says standing questions never take the banner. The aside
survives selection here, which is the property #359 calls "the one thing this
surface has that Triage does not".

Deliberately **not** copied from `RealFrontier`: its optimistic post-act
fallback (the frozen item that keeps the panel open when an act moves the item
out of every live query). That is real machinery for a real problem and it is
not what this prototype is asking about.

## Round 5 — the names, checked against the glossary

Every name the prototype invented was checked against `CONTEXT.md` and the
ADRs before any of it could become a ticket, because names end up in module
names, test names and an ADR. Most of them were already spent:

| Prototype word | Already means | Decision |
| --- | --- | --- |
| "lane" | ADR-0009's transport lanes, ADR-0012's notification lane | **dropped** — see below |
| "board" | `StatusScreen.tsx` and ADR-0017's ranked region | **columns** |
| "band" | ADR-0015's pane *salience*, which CONTEXT.md deliberately separates from urgency | not used |
| "pressing" | a fifth synonym beside urgency / band / salience / severity | **urgency** |
| "Context" | CONTEXT.md defines *Context source* and *Context snapshot*, never bare "Context"; the clash is with Now's aside, labelled `Context` | **Context**, and the aside's label gets corrected |

**Colour encodes urgency and nothing else.** The six-bucket partition mixed
three unrelated facts — the world's pressure (overdue, due today), your own
intention (scheduled today) and the shape of the work (quick, deep) — which is
why no single honest name existed for it. Cutting it to `Urgency` costs
nothing visible (size and scheduled date are already chips on the card),
satisfies the design system's "colour always encodes stage, tier or urgency"
rule with no exception to argue, and is what ADR-0015's own tripwire
prescribes: when a vocabulary clusters, cut it rather than extend it. Variant
B keeps its six lanes, as the record of what was tried.

**"Context" is correct for the axis, and frees itself.** Now's aside is
labelled `Context` but has held the ranked region since ADR-0015 replaced the
calendar context tile — so the label is stale, and correcting it (to what the
panel actually holds) is a fix the repo owes anyway rather than a concession
to this design. It is also the only aside carrying that name; the other three
are "Core and calendar status", "Steps and notes", "Alert rules".

Two facts found in the same pass, for whoever writes the tickets: the glossary
term is **Deadline**, not "Due date" (ADR-0013 renamed it); and the schema
says `size IN ('quick','short','deep')` while the design-system mirror's README
says `quick / normal / deep` — the schema wins and the mirror needs correcting.

## Verdict

<!-- Still open: whether this replaces the project grouping on Now or becomes
a second view of it. Then fold it in properly — with tests, and with the lane
partition and the facet predicate as pure sibling modules the way
frontier-order.ts already is — and delete this directory. -->

_C, reworked as above. Not yet folded into the real screen._
