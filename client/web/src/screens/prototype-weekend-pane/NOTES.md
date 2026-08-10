# PROTOTYPE — the weekend-plans pane (#122)

Throwaway. Delete this whole directory (and the three `WeekendPane` /
`WeekendPaneSwitcher` mounts in `screens/NowScreen.tsx`) once the question
below is answered.

## The question

**What should "what are my plans this coming weekend" look like, and where
does the `scheduled_date` affordance live?** #122 fixes the merge (calendar
events + items scheduled in the window + items due in the window, at read
time, nothing stored) and the dedupe rule, but not the shape on screen — and
it also asks for an affordance that **exists nowhere in the app today**: no
surface can set or clear a do-date, so there is no precedent to copy and the
three variants each propose a different one.

The pane also has to stay honest across states the issue does not enumerate:
no calendar connected at all, a stale mirror, a weekend already under way,
and a completely clear weekend.

## How to run

```
cd client/web && pnpm dev      # or: ./node_modules/.bin/vite
open 'http://localhost:5173/?weekendpane'
```

`?weekendpane` mounts the variants inside the **real** Now screen — real
rail, real header, real (empty) frontier, real context panel — so each is
judged against what it has to sit next to. Without the param nothing renders;
the whole thing is behind `import.meta.env.DEV` as well, exactly like
`fixtures/demo.ts` and the sibling `prototype-race-pane`.

- `←` / `→` — cycle variant (also the `‹ ›` arrows in the floating bar)
- `↑` / `↓` — cycle scenario (also the pills in the bar)
- both are `?variant=` / `?scenario=` on the URL, so a state is shareable
- the bar's footer prints the merged counts and a **reset** for stub plan edits

`?racepane` (#119) and `?vacationpane` (#121) have their own switchers at the
same fixed position — turning two on at once overlaps them. Use one at a time.

## What is shared and what is not

`weekend.ts` is the merge, and all three variants call it: the window, the
day grouping, the dedupe, `entryUrgency`. That is deliberate — the merge
rules should be decided once and judged three times. Everything above it
(layout, hierarchy, the affordance) is each variant's own; nothing shares a
layout component.

## The variants

| Key | Name | Slot | The bet |
| --- | --- | --- | --- |
| A | Weekend card | context panel | **One** card for the whole window, days as sections inside it, everything interleaved chronologically within a day, kind carried by glyph and colour. Bets the weekend is one thing you are asking about, and that events/deadlines/do-dates belong mixed because that is how a day is lived. Affordance: **inline day chips on the row** — plan while you read. |
| B | The ribbon | banner above the frontier | Time-proportional columns, booked time drawn as blocks against a 7am–11pm axis, plus a computed "6h clear from 5pm". Deadlines and do-dates are deliberately *not* in the columns (they are not blocks of time). Bets the question is really "does something else fit". Affordance: **a dedicated strip** listing every deadline with no day chosen. |
| C | Three ledgers | context panel | Refuses the merge: Booked / Owed / Chosen as three lists, day named on the row. Bets the domain's own distinction is the answer and that interleaving hides which rows the reader can actually move. Affordance: **a labelled day field** (`Select`, with "Not planned" first-class) on every editable row. |

## The scenarios

`Typical week` · `Packed weekend` (11 events, an all-day span) · `Quiet
weekend` · `Nothing at all` · `No calendar` · `Stale mirror` (9h) · `Friday
evening` · `Saturday 2pm` · `Sunday 9pm`.

## What building it already surfaced

1. **The window's edges are not decided, and Friday is the worst one.** The
   window is Sat 00:00 – Sun 23:59 local. But "what are my plans this
   weekend" is most often asked *on Friday afternoon* — and at that moment
   Friday's own dinner is outside the window and invisible. The
   `friday-evening` scenario shows exactly that hole. Either the window
   starts Friday evening (at what hour?), or the pane is answering a
   narrower question than the one people ask.
2. **The mid-weekend window degenerates.** Asked at 9pm Sunday, the window
   is two hours long and the pane is honest and nearly useless
   (`sunday-night`). Nothing here decides whether it rolls forward to next
   weekend, and if it does, at what hour on Sunday.
3. **Nothing drops past entries.** The window is whole days, so at 2pm
   Saturday the pane still lists 9am parkrun, and at 9pm Sunday it still
   lists the 6pm call. Fine for "what are my plans", wrong for "what's left
   of the weekend" — and B's headline ("6h clear from 5pm") silently means
   the second while the list means the first.
4. **The dedupe rule needs a residue, not just a winner.** #122's criterion
   says an item both scheduled and due in the window appears once, as due.
   Implemented — but the do-date is a fact the human *chose*, and swallowing
   it makes the pane look like it ignored them, so the merge keeps it on the
   entry (`alsoScheduledOn`) and A/C render it. B currently drops it. Whether
   that residue should show is a design call, not an implementation detail.
5. **The inverse case is more common than the criterion's case.** An item
   scheduled this weekend but due *next Wednesday* is the everyday pattern,
   and the pane must show the deadline or the work reads as having none
   (`deadlineOutsideWindow`). The issue's criteria never mention it.
6. **"No calendar connected" cannot render as a quiet weekend.** Half the
   answer is missing. A says it in one warn line; C says it in a full
   sentence inside the empty Booked ledger; B degrades the header to
   "no calendar — deadlines only". C's version is the honest one and the
   most expensive in space.
7. **The affordance may not fit the 320px context panel.** C's day `Select`
   is 132px wide, which squeezes the title to two lines in the panel — a
   real argument that a *field*-shaped affordance belongs in the centre
   column (B's slot) and only a *chip*-shaped one fits the rail (A).
8. **The ribbon needs lane-splitting.** In `packed`, three overlapping
   Saturday-morning events draw on top of each other, because every block is
   full-width. Before B could ship it needs overlapping events side by side —
   which costs it the width that made the metaphor readable.
9. **A do-date is a day, never a time.** Scheduled entries anchor to 00:00
   and read "anytime". That is a decision this prototype made; it is what
   lets them sit above a day's booked time rather than at midnight inside it.
10. **One card or one card per day** (operator instruction, 2026-08-10:
    *"Day Cards, but condense them into a single weekend card"* — A now
    renders one card, days as ruled sections inside it). What it bought:
    the weekend reads as one answer rather than a stack of two, the
    per-day chrome halves, and the legend and the `as of` stamp finally
    belong to something instead of floating under it. What it cost:
    - **The card has no internal cut.** In `packed` it runs 21 rows and
      falls off the bottom of the context panel, which is `position:
      sticky` and does not scroll on its own (`screens/layout.tsx`). Two
      cards had the same total height but at least broke somewhere. A
      shipped version needs a horizon — collapse Sunday, or cap each day
      and say "+4 more".
    - **The day heading is doing more work.** It is now the only thing
      separating the two days, on a `--type-body-strong` rule rather than
      a card edge. In `packed` the Sunday heading scrolls out of view and
      the rows below it lose their day.
    - With **one day left** (`sunday-night`) the section heading is
      furniture, so it collapses: the day is named in the card header and
      the sections disappear entirely. That case is strictly better than
      it was.

## Confirmed here

- Setting/clearing a do-date re-runs the merge and **never** moves the
  urgency dot: `entryUrgency` reads `item.deadline` and nothing else
  (#122's third criterion). Visible live — re-plan "Draft the trip
  itinerary" in A and its `due 2026-08-19` meta does not flinch.
- An item neither due nor scheduled in the window never appears (ION-146).
- An item due in the window appears once even when also scheduled there
  (ION-141, `typical`).

## Not decided here

- **The calendar mirror's event shape** (#46, this issue's blocker). The
  `WeekendEvent` in `weekend.ts` is a guess chosen as the union of what a
  pane could want — title, start/end, all-day, source calendar name. All-day
  handling in particular (a span across both days appears on both) is a
  prototype decision with no upstream authority yet.
- **The write path.** `plan-store.ts` is a stub. In the real app a do-date is
  a `Core::triage` CAS `PATCH` through the SharedWorker, and the pane would
  have to render optimistically and mark the row pending like every other
  surface — none of which is prototyped here.
- **Where else `scheduled_date` gets set.** This prototype only proposes
  setting it *from the weekend pane*. Item detail and Triage will need one
  too, and it should probably be the same control.

## Verdict

Decided by the operator, 2026-08-10.

- **Winner: A — the weekend card**, in the context panel, titled
  **"This Weekend"**. One card for the whole window, days as ruled sections
  inside it, everything interleaved chronologically within a day, kind
  carried by glyph and colour. The `scheduled_date` affordance is A's inline
  day chips on the row.
- **Why:** the weekend is one question and it gets one answer-shaped card;
  the day is a divider inside it, not a second answer. It sits in the panel
  next to the calendar context tile without competing with the frontier for
  the centre column, and its chip affordance is the only one of the three
  that fits the 320px panel (finding 7).
- **Stolen from the others:** nothing yet — B and C are not folded in and
  are not deleted. Two of their ideas are worth carrying if they survive
  contact with the real pane: B's "N due this weekend · no day chosen"
  strip (the only call to action any variant had) and C's honest full
  sentence when no calendar is connected. Both are `unplanned()` /
  `calendar === null` cases the merge already supports.

### Still open at the moment A won

The findings above are not closed by picking A. Before it is folded into
`NowScreen` for real, these need answers — they are decisions about the
question, not the layout:

- The **Friday hole** (finding 1) and the **degenerate Sunday-night window**
  (finding 2) — both are `weekendWindow`, not `VariantA`.
- **Past entries are never dropped** (finding 3).
- The condensed card has **no internal cut** and overflows a sticky,
  non-scrolling panel on a packed weekend (finding 10) — the one thing that
  will bite first in real use.

## Next step

Fold A into `NowScreen` properly (rewritten, not promoted — it was built
under prototype rules: no tests, stub mutation), wire the real
`scheduled_date` write through `Core::triage`, then delete this whole
directory along with B, C, the switcher and the three mounts in
`screens/NowScreen.tsx`.
