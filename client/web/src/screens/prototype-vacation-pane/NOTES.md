# PROTOTYPE — the vacation-countdown pane (#121)

Throwaway. Delete this whole directory (and the two `VacationPane` mounts
plus the `VacationPaneSwitcher` line in `screens/NowScreen.tsx`) once the
question below is answered.

## The question

**What should "how long to the next vacation" look like, and what exactly is
the answer it renders?** #121 fixes the lane (a designated Trips calendar →
device polling → its id in a `settings` binding → a countdown computed at
read time) and gives one example sentence — "395 days before India" — but not
the shape on screen, and not what the pane says in any of the states that
sentence does not cover: mid-trip, on the day you land, when nothing is
booked, when the calendar was never designated, when the mirror cannot see
that far.

## How to run

```
cd client/web && pnpm dev      # or: ./node_modules/.bin/vite
open 'http://localhost:5173/?vacationpane'
```

`?vacationpane` mounts the variants inside the **real** Now screen — real
rail, real header, real (empty) frontier, the real calendar tile sitting
directly underneath — so each is judged against what it has to sit next to.
Without the param nothing renders; the whole thing is behind
`import.meta.env.DEV` as well, exactly like `fixtures/demo.ts` and the
sibling race-pane prototype.

- `↑` / `↓` — cycle scenario (also the pills in the floating bar)
- `?scenario=` on the URL, so a state is shareable
- runs alongside the sibling pane prototypes; the bars step over each other

The variant switcher is gone with B and C — the shape is decided. The
scenario switcher stays, because the eleven states in `fixture.ts` are what
the real pane will have to survive and flipping through them is how the next
change gets checked.

## The variants

Three were built and judged against each other; **A won** (see Verdict) and
B and C have been deleted. Recorded here because the reasoning is the useful
part, and because a future reader should not re-run the same experiment.

| Key | Name | Slot | The bet |
| --- | --- | --- | --- |
| A | Countdown tile | context panel | **Winner.** Furniture. One card the size of the calendar tile next to it, one trip, the place and the count set large and the rest small. Sits quietly for 380 of the 395 days. |
| B | Departures board | above the frontier | *Deleted.* 64px count across the top of the centre column, and — the real claim — **the answer is the queue, not the number**. "395 days before India" alone is bleak; "Lisbon in 16, Snowdonia in 61, India in 395" is worth opening the app for. Lost the slot, won the queue: A lists every trip because of this. |
| C | Year strip | context panel | *Deleted.* No headline number at all. A scaled strip with trips as ember blocks and a "you are here" tick; the countdown is the *distance* between them. Bet the question was about distribution — where the time off sits in the year, and how much ordinary life is between. It is not, but it is what made the +90d window (finding 1) impossible to look past. |

## The scenarios

`395 days out (real horizon)` · `395 days out (wide horizon)` ·
`Three booked` · `Departing tomorrow` · `Under way` · `Landing today` ·
`Just back` · `Stale mirror` · `Nothing booked` · `Not polling here` ·
`No Trips calendar`.

## What building it already surfaced

1. **The issue's own example sentence cannot be answered by the mirror as it
   exists.** #46 persists a rolling window of **seven days back through
   ninety days ahead**. A trip 395 days out is not in it. The pane does not
   see an empty calendar — it sees *ninety days of* empty calendar, and the
   distinction is the whole pane: "nothing booked" is a lie, "nothing booked
   in the next 90 days" is the truth, and neither is "395 days before India".
   The first two scenarios are that pair side by side, and C draws the cut
   line explicitly (two thirds of its strip hatched out). **This is a
   blocking dependency #121 does not currently list**: either #46's window
   widens (for the Trips calendar at least), or the acceptance criterion
   "countdown + event name computed at read time" is unmeetable for exactly
   the case the issue uses as its example. Cheapest fix is probably a
   per-calendar window — the Trips calendar is low-volume and all-day, so
   pulling it out to +3y costs almost nothing, while widening the primary
   calendar to +3y would not.
2. **"The day you land home it is already counting to the next one" and the
   calendar disagree.** An all-day event's end is the provider's *exclusive*
   end — local midnight on the day after the last day — so on the day you
   land the trip is still live. `TripPhase` names all five positions
   (`upcoming` / `departs_today` / `under_way` / `returns_today` / `past`)
   rather than picking silently, and `returns_today` renders as "Home today
   from Lisbon". **Open: is the issue's wording a decision (advance the
   moment the return day begins) or loose phrasing?** They differ by one day,
   every trip.
3. **Nothing in the issue says what the pane reads DURING a trip**, which is
   6 days out of every trip and the one time you are most likely to look at
   it. A countdown to the *next* vacation while you are on this one is
   absurd. All three variants answer with the trip you are in; A adds "day 3
   of 6", C colours the block differently. Worth deciding explicitly.
4. **Days must be counted between local midnights, never in milliseconds.**
   A trip is a range of local days. `Math.round(deltaMs / 86_400_000)` is off
   by one across a DST boundary, and "395 days" quietly becoming "394" is
   exactly the kind of wrong that nobody notices and nobody can reproduce.
   `daysBetween` walks the calendar; every count in `countdown.ts` goes
   through it.
5. **"Auto-advances past finished events" needs no mechanism.** It is one
   `.filter(phase !== "past")` over the mirror at read time — no cursor, no
   stored "current vacation", no write. Which is the acceptance criterion
   "deleting the mirror loses nothing" holding up: there is genuinely nothing
   in this directory that could be lost.
6. **Four different reasons for "no countdown", and collapsing any two hides
   something real.** No Trips calendar designated (the slice's *human* step
   was never done) / bound but this device never opted it into polling (#46
   is per-device) / opted in but never polled / polled and genuinely empty
   inside a known window. `AnswerState` keeps them apart. A and C render the
   unbound case as **nothing at all** — no calendar designated is not an
   error, it is a question nobody asked yet — which is worth confirming.
7. **A fixed-span timeline silently drops data.** C's strip was 365 days
   until a trip 395 days out clamped to the right edge and vanished, leaving
   the strip saying "two trips" above a list saying three. It now scales to
   what is booked. Any future timeline in this app has the same trap.
8. **The trip name is the calendar event's title, and stays that way.**
   `tripName` strips a leading "Trip:"/"Holiday:" and nothing else. The
   calendar is the authority (#117); a pane that rewrites its titles has
   started keeping a vacation record of its own, which is precisely what the
   issue forbids.

## Not decided here

The **binding editor** (#118) and how the Trips calendar gets designated and
opted into polling — the prototype takes `binding` + `polling` as given.
Nothing here touches the alert lane either: a vacation countdown raises no
alerts (there is no material change to report — the number goes down by one
each day, on cadence), which is worth stating in the slice explicitly since
every sibling pane in #117 *does* have an alert leg.

## Verdict

**Winner: A — Countdown tile**, in the context panel, decided 2026-08-10.

- **Phrasing reversed, and it is the place that leads.** Not the issue's
  "16 days before Lisbon" but **"Lisbon in 16 days"**. The question is about
  the place; the number is only how far away it is, and leading with the
  number makes every trip read as a countdown to an unnamed thing until the
  eye reaches the end of the line. `sentence()` carries the new order, so
  anything that reads the answer as a string gets it too.
- **The place and the count are set at the same size** (34px display), with
  "in" and "days" left at body size between them. Neither of the two facts is
  a caption on the other. Checked at 320px against the longest realistic
  name — "Snowdonia in 61 days" still sets on one line.
- **The whole queue is listed, never truncated.** A "+1 more" would be the
  pane withholding something it already has in hand, and the tail is short by
  nature — these are vacations, not a feed. Every trip after the next one
  gets a muted mono row, name left and date right so the dates column up.
  No "then" on the first row: the rows are already in order, and the word
  only made row one look unlike the rest of the list it belongs to. A
  date in another year says so ("Sep 9 2027"): a bare "Sep 9" on a trip 395
  days out is indistinguishable from one this September, which is the same
  class of quiet lie as finding 1.
- B and C are dead. Both bet the answer was bigger than one line — B on the
  queue of trips, C on their distribution across the year — and it is not,
  though B's argument that the queue matters won the concession above.

Still to fold in, and none of it is decided by picking A: findings 1, 2 and 3
below (the +90d window, landing day, and what the pane reads mid-trip) are
open questions about the *answer*, not the shape.
