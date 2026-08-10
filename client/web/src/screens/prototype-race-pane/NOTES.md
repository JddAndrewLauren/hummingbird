# PROTOTYPE — the next-race pane (#119)

Throwaway. Delete this whole directory (and the two `RacePane` mounts plus the
`RacePaneSwitcher` line in `screens/NowScreen.tsx`) once the question below is
answered.

## The question

**What should the "when is the next race" pane look like, and what exactly is
the answer it renders?** #119 fixes the lane (server cron → one
`context_snapshots` row per followed series → read-time countdown) but not the
shape on screen, and the pane has to stay readable across states the issue
does not enumerate: a race weekend already under way, a series the binding
names that has never polled, a cron that missed, an empty binding.

## How to run

```
cd client/web && pnpm dev      # or: ./node_modules/.bin/vite
open 'http://localhost:5173/?racepane'
```

`?racepane` mounts the pane inside the **real** Now screen — real rail, real
header, real (empty) frontier — so it is judged against what it has to sit
next to. Without the param nothing renders; the whole thing is behind
`import.meta.env.DEV` as well, exactly like `fixtures/demo.ts`.

- `↑` / `↓` — cycle scenario (also the pills in the floating bar)
- `?scenario=` on the URL, so a state is shareable

## The variants

Three were built and compared; **A won and B and C are deleted**. Recorded
here because the comparison is the reason the verdict below is trustworthy:

| Key | Name | Slot | The bet | Outcome |
| --- | --- | --- | --- | --- |
| A | Series tile | context panel | One card per series, in the `ContextTile` idiom the calendar already owns. The pane is furniture; the alert lane interrupts. | **Won** |
| B | Answer banner | above the frontier | A 56px countdown across the top of the centre column, series as tabs, the weekend as a horizontal session ladder. Bets this is a glance-from-across-the-room question. | Deleted — competed with the top pick for the eye, for a question that is rarely urgent |
| C | Agenda rail | context panel | No card per series — every session of every series merged into one chronological list of hairline rows, grouped by day. | Deleted — 15 rows across two series and no horizon (finding 4) |

## The scenarios

`Quiet week` · `Race weekend` (a session under way *and* a race tomorrow) ·
`Race in 90 minutes` (the threshold alert is live) · `Cron missed` (9h old
against a 6h poll) · `One series missing` · `Nothing polled yet` ·
`No series followed`.

## What building it already surfaced

1. **"12 days before Monaco" is not what the data says.** The issue's own
   example phrasing counts to the *event*, but the next thing to happen is
   Friday practice — two days earlier. Counting to the next session and
   saying "before Monaco" is off by two days. A/C count to the next
   **session** and name it (`Practice 1 in 10 days`); B counts to the session
   but leads with the event name. **Open: does the pane answer "when is the
   next race" (the race, always) or "when is the next thing on track"?** They
   differ by two days for most of the year.
2. **Rounding must agree with the alert lead.** 90 minutes rounded to hours
   gives "2 hours" in the pane while the alert it shares a `source` with says
   "in 90 minutes". `countdown()` keeps minutes up to 120 for exactly that
   reason — whatever the real lead time is, the pane's boundary has to sit
   above it.
3. **The join is by `source` *plus* `key`.** The plan says a source's alerts
   and its snapshot must share the same `source` string, but with one row per
   series the `source` is shared by *all* series (`race-schedule`, key `f1` /
   `indycar`). The alert therefore needs the key too, or every series' pane
   shows every series' alert. The fixture models this (`RaceAlertRow.sourceKey`)
   — **it needs deciding in the real schema**: either a per-series source
   string (`race-schedule:f1`) or an explicit key column on the alert.
4. **C had no horizon.** Merged across two series it listed 15 rows and kept
   growing with the binding. It needed a cut ("this weekend + the next race")
   before it could ship — which is part of why it lost.
5. **A series in the binding with no snapshot must render as a gap, not as
   absence.** All three variants say "never polled" rather than silently
   showing one series — otherwise adding a series to the binding looks like
   it did nothing.

## Not decided here

The **schedule source**. `fixture.ts`'s payload is a deliberate guess (event
name, circuit, locality, session ladder with UTC starts) chosen as the union
of what a pane might want, so the variants are judged on what they need
rather than on what one feed happens to publish. #119's last acceptance
criterion — verify a reliable API or ICS feed per series, document it in the
adapter — is untouched by this prototype.

## Verdict

**A — Series tile, in the context panel. Decided 2026-08-10.**

The headline counts to **race day** and names the **race**, not the practice
session that happens first: *"12 days before Monaco Grand Prix"*, the issue's
own phrasing. Finding 1 above is therefore closed — the pane answers "when is
the next race", the race, always.

What that costs and how it is paid: the session that actually happens first is
still a fact worth having, so it moves to the line under the headline
(*"Practice 1 · Aug 20 4:00 AM"*). While a session is running the headline
says so instead (*"Practice 3 under way"*, ember `radio` glyph), because a
countdown to Sunday is the wrong answer while the cars are on track — and the
second line then flips to the race (*"Race · Tomorrow 6:00 AM"*), so the
"when is the race" answer is never absent from the tile.

The event is named in the under-way headline too — *"Monaco Practice 3 under
way"*, not *"Practice 3 under way"*, which says which session but not which
race.

**Units are abbreviated and do not inflect**: `90 min`, `4 hr`, and `12 days`
(days keep the word — they are the common case and read as prose). `min`/`hr`
are machine values; "1 mins" is the only way to get an inflected abbreviation
wrong, so they simply do not inflect.

**"Grand Prix" renders as "GP"** — `abbreviate()`, applied wherever the full
event name is shown (*"12 days before Monaco GP"*). Names without one
("Iowa 275") pass through untouched. The under-way headline keeps the bare
locality instead (*"Monaco Practice 3 under way"*, via `shortName()`) — "Monaco
GP Practice 3 under way" stacks two labels in front of the verb.

**All times are Pacific, and say so** (`6:00 AM PT`). `countdown.ts` pins
`America/Los_Angeles` rather than resolving to the device zone: the same race
would otherwise render at a different hour on a travelling laptop, silently,
because nothing on screen would name the zone it picked. Verified against a
non-Pacific device zone — `TZ=Europe/Berlin` still renders `6:00 AM PT`, where
the device default would have said `3:00 PM`. Day rollover ("Today" /
"Tomorrow") is compared as *Pacific calendar days*, not elapsed milliseconds,
so a late-night race is not labelled by the device's idea of the date.

Implemented in `countdown.ts`'s `sentence()` / `SeriesAnswer.race` /
`clock()` / `dayLabel()` and `VariantA.tsx`. B and C are deleted, along with
the two-slot machinery and `mergedAgenda` that only they used; what is left is
the winning pane plus the scenario harness.

Still open, and not decided by the winner:

- Finding 3 (the `source` + series-key join) — a real schema question for the
  alerts table, unchanged by the pane shape.
- The schedule source itself (see "Not decided here").
- **Where Pacific comes from.** It is a hardcoded constant here. The real
  build should decide whether that is right (a fixed home zone, stated on
  screen) or whether it belongs in the `settings` bindings next to the series
  list, like every other cross-device fact this plan holds there. Either way
  the rendered zone must stay named — an unlabelled hour is the bug this
  change fixed.

### Fold-in checklist

1. Rewrite A properly against real `settings` bindings (#118) and real
   `context_snapshots` rows — this code was written under prototype rules (no
   tests, no error handling).
2. `countdown.ts` becomes a pure `screens/*.ts` module with its own vitest
   file; a component test covers the threading, per CLAUDE.md's rule about UI
   state with no reader.
3. Delete this directory and the `<RacePane>` / `<RacePaneSwitcher>` lines in
   `screens/NowScreen.tsx`.
