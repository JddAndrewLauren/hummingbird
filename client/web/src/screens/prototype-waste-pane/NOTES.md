# PROTOTYPE — the which-cans pane (#120)

Throwaway. Delete this whole directory (and the two `WastePane` mounts plus
the `WastePaneSwitcher` line in `screens/NowScreen.tsx`) once the question
below is answered.

## The question

**What does the pane render, when does it deserve attention, and when does the
holiday text appear and disappear?** #120 fixes the lane (daily server poll →
one `context_snapshots` row → read-time answer, plus a holiday alert joined by
`source`) but not the shape on screen, and the interesting part is not any
single state — it is the *sequence*: a holiday raises, the next few daily
polls must stay quiet, an ack must stay acked, and the whole thing must go
quiet on its own once the affected date passes.

## How to run

```
cd client/web && pnpm dev
open 'http://localhost:5173/?wastepane'
```

`?wastepane` mounts the pane inside the **real** Now screen — real rail, real
header, real (empty) frontier — so it is judged against what it has to sit
next to. Without the param nothing renders; the whole thing is behind
`import.meta.env.DEV` as well, exactly like `fixtures/demo.ts`.

The floating bar is the control panel and also the state readout the pane
deliberately hides: the snapshot payload, the judgment, the alert's lifecycle
stamps, and the computed `prominence`.

- **`1`–`4`** jump straight to the four states: standard · standard +
  recycling · holiday · holiday + recycling
- `n` next day (which polls) · `→ Sunday` / `→ Monday` · `p` poll · `a` ack
- the whole scenario is the command history in `?do=`, so any state is
  shareable and survives a reload — e.g.
  [`?wastepane&do=x3.g0`](http://localhost:5173/?wastepane&do=x3.g0) is
  "holiday week with recycling, seen on Sunday"

## The domain, as the operator gave it (2026-08-10)

This replaced an earlier model that gave every stream its own cadence and next
date. That was wrong, and the correction is a large simplification:

- **There is only ever one collection day.** Everything going out that week
  goes out together, so a per-stream next date modelled a variation that does
  not exist.
- **There are only four states.** Standard or holiday, times with or without
  recycling. Trash and yard go every week; recycling rides along every other.
- **The tile is furniture most of the week.** Dormant Tuesday–Saturday, awake
  on Sunday (the night the cans go out) and on the collection day itself, and
  awake all week when a holiday moves the day.
- **A holiday needs no separate alert card.** It changes the answer rather
  than interrupting it.

`Deviation` therefore has no skipped-cycle arm any more: a week with no
collection at all is not one of the four states.

## What the pane does with that

`prominence` and `headline` (both in `waste.ts`) are the whole design, and
both are computed at read time — never stored:

| | rendering |
| --- | --- |
| `dormant` | one muted line, **no card at all**: coloured dots, the weekday, days away. It must not compete with the frontier, and most of the week it doesn't. |
| `prominent` | the eve, the collection day itself, or **any** holiday week. The full tile, `accent`, three words and a date. |

`prominent` when the collection is today or tomorrow, **or** whenever the day
has moved. The copy is deliberately tiny, because the coloured bins already
say which cans — the words only have to say when:

- collection today → **Trash Today**
- collection tomorrow → **Trash Tonight**
- a holiday → **Trash Tuesday** (its actual day, named even when that day is
  tomorrow: on the one week the day is unusual, "Tonight" would hide the very
  thing that changed)

**The bins carry their real colours** — trash grey, recycling light blue, yard
green (`BIN` in `waste.ts`). A deliberate exception to the design system's
"colour always encodes status, never decoration" rule: here it encodes *object
identity*, the one thing on this tile a person matches against the real world
before walking outside. Literal hex, not brand tokens, because the bins are
not part of the palette. Each bin keeps an `aria-label`, since colour alone is
not a label to a screen reader.

Staleness appears only past ~26h. The poll is daily, so an older answer may be
wrong and the brand rule is to keep showing it and say its age; on a fresh
snapshot the line would be pure noise.

## Deliberate deviation from #120's acceptance criteria

The issue says the pane shows holiday text *"only while such an alert is
unresolved (the pane-level join from ADR-0009)"*. **This prototype does not
join the alert lane at all.** A holiday is not an interruption laid over the
answer — it *is* the answer, so it changes the words, and there is nothing to
ack away. `holiday` is read straight off the snapshot (`collectedOn !==
scheduled`).

`mint` still produces the alert row, unchanged, because the notification lane
(push, ADR-0012) genuinely wants one; nothing in the pane reads it. The `a ·
ack` button in the control bar still drives that lifecycle so it stays visible
— and demonstrates that the tile no longer flinches when it fires.

If the join is wanted back, it is one line in `pane()`.

## Confirmed by driving it

- A holiday raises exactly one alert row for the notification lane, naming the
  change ("Collection: Monday → Tuesday this week"), while the tile's own
  answer changes independently of it.
- The next day's poll does **not** re-raise: `mint` only restamps `raisedAt`
  when the row is new or the change itself changed, so an ack stays acked.
  (Drivable from the bar's `a · ack`; the tile is unaffected either way.)
- The alert expires at the end of the *later* of scheduled and slid-to —
  expiring at the end of the original Monday would kill the notification on
  the Tuesday morning it exists to warn about.
- A value-identical upsert is no write at all (the no-op rule #221 landed for
  rules PATCHes), so a week-long holiday does not push a delta every morning.
- Verified in a real browser (Playwright, 1440 and 768, both themes): no
  horizontal overflow, no console errors, all four states across dormant, the
  eve, the day itself and a holiday.

## The hole this closed

An earlier cut elevated the tile on Sunday only, which left a holiday week
without an eve: the pickup was Tuesday, so the night before was Monday, and an
acked holiday alert rendered Monday `dormant` — quiet on exactly the evening
the cans had to go out. Both halves of the current rule fix it: Monday is
prominent in its own right, and a holiday is prominent for the whole week with
no ack able to quiet it.

## Left open

- Only variant A exists. If the pane wants to be a banner on collection
  morning rather than a context-panel tile, that is a second variant.
- No timezone: the world's dates are whole days. The real pane must resolve
  the address's local day, not the device's.
- `city-waste/v2` is not in the frozen registry (`v1` is retired). The payload
  shape here — one cadence, one collection date, a stream list — is what that
  registration would pin.
- The Rust twin in `server/prototypes/waste-cadence/` still models the
  superseded per-stream world. Its lifecycle findings all survive; its
  fan-out does not. See that directory's NOTES.md.
