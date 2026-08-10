# which-cans prototype — #120

> **Superseded in part, 2026-08-10.** After this ran, the operator described
> the real service: there is only ever **one collection day** (everything that
> week goes out together), and only four states — standard or holiday, times
> with or without recycling. This crate's per-stream fan-out therefore models
> a variation that does not exist, and its `SkippedCycle` arm models a week
> that never happens.
>
> **Everything in "What driving it settled" below still holds** — those are
> findings about materiality and alert lifecycle, not about the fan-out. The
> browser twin at `client/web/src/screens/prototype-waste-pane/` carries them
> forward against the corrected domain and is the current one. Collapse this
> crate to one collection day before trusting it again, or delete it.

**Run it:**

```
cargo run --manifest-path server/prototypes/waste-cadence/Cargo.toml
```

(Line-driven, not raw-keystroke: type a command and hit enter. `n` = next day.)

**Throwaway.** Delete it, or lift `src/waste.rs` into the real adapter and
delete the rest. It is deliberately outside the `server/` cargo workspace, so
`cargo clippy --workspace` / `cargo test --workspace` in CI never sees it.

## The question

Given a daily poll of the city's address-specific collection page, **what does
the snapshot payload have to carry, and what exactly counts as a material
change**, so that the ordinary cadence roll-forward stays silent, a holiday
slide rings exactly once, a human dismissal stays dismissed across the next
four daily polls, and the pane's holiday text appears and disappears on its
own?

`src/waste.rs` is the portable answer — pure functions, no I/O, no clock of
its own. `src/main.rs` is the throwaway shell that lets you be the clock and
the city; `src/date.rs` is throwaway civil-date support.

## What it does

Three streams (trash weekly Mon, recycling fortnightly Mon, yard weekly Wed).
You slide a pickup for a holiday, correct the slide, skip a cycle, dismiss the
alert, advance days. Each frame shows the city page, the
`context_snapshots` payload the poll would write, the per-stream judgment, the
`alerts` rows with their real ADR-0014 `is_live` verdict, and the pane
rendered at read time.

It uses the real `hummingbird_domain::is_live` and the real
`city_waste_v1_key` — the two things it would be easiest to accidentally model
more kindly than production.

## What driving it settled

1. **Materiality is deviation from cadence, and needs no previous snapshot.**
   `judge` never sees the last poll. That is precisely why the roll-forward is
   silent: the morning after a pickup, the page jumps a whole week, which is a
   large diff and a zero deviation. Bonus properties that fall out for free —
   the adapter is correct on its very first poll, and correct again after a
   wiped or lost snapshot. **The payload must therefore carry the cadence, not
   just the next date.** Without it there is nothing to deviate *from*.

2. **A daily poll must not re-stamp `raised_at`.** The slide sits on the page
   for days; a naive upsert re-raises every morning, and ADR-0014's `is_live`
   compares `raised_at` against `dismissed_at`, so the human's dismissal is
   undone daily. `mint` returns `restamp_raised_at`, true only when the row is
   new **or the change itself changed** (a Tue → Wed correction). Same shape
   as `sweep_tick`'s `raised_at: None`.

3. **Stamp the raise with the write clock, never the poll's nominal slot.**
   This one bit during the session: with the poll stamped at "today 06:00" (a
   cron bucket), a correction re-raised at 06:00 landed *before* a dismissal
   made at 08:00 the same morning and stayed silently quiet. Real wall clock
   at write time is load-bearing, not incidental.

4. **A value-identical upsert must be no write at all.** Otherwise each daily
   poll bumps `version` and pushes a meaningless delta to every device for the
   whole life of the slide. Same rule #221 landed for rules PATCHes.

5. **`expires_at` is the end of the *later* of scheduled and slid-to.** The
   registry says "end of the affected collection date"; taken as the
   originally scheduled Monday, the holiday text vanishes from the pane on the
   Tuesday morning it exists to warn about. With the later date, the alert
   expires on its own and the pane goes quiet with no human action and no
   resolution pass — the thing `item-threshold/v1` still lacks (#217).

6. **Deviation-from-cadence has one real blind spot, and it is worth naming
   rather than papering over.** A page that skips a whole cycle is *on*
   cadence, one period out. `Deviation::SkippedCycle` catches it by counting
   periods ahead of the current one — but on the collection day itself, "no
   pickup today, next is next week" is byte-identical to "pickup happened,
   rolled forward." That case is undetectable from the page alone, by
   construction. Drive it: `s 1` on a Monday says nothing; `n` then `s 1`
   raises "trash: no Monday collection this week."

## Left open

- `when_phrase` ("this week" / "next week") is a placeholder using a 7-day
  window from today, not real week boundaries — Mon the 17th reads as "this
  week" when polled on Tue the 11th. Needs a real week boundary, and the
  address's timezone, before it goes near a user.
- The prototype pretends the city page and the device share one timezone. The
  real adapter must not.
- A *backward* slide (pickup moved earlier) keys on the previous cadence date
  and would read as a large forward slide. Not modelled; probably wants an
  explicit guard.
- `city-waste/v2` is not in the frozen registry yet (`v1` is retired). The
  payload shape and key recipe above are what that registration would pin.
