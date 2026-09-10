# Handoff: put the Urgency axis on the phone

## Goal

Android's frontier board offers four grouping axes; the web offers five. Close
that gap — or decide deliberately that it stays open and record why.

> **2026-09-10:** the watch now consumes this axis — `client/android/wear/`'s
> Items screen and capture tile both read `nowBoard(MobileFrontierAxis.URGENCY,
> …, MobileCalmOrder.OLDEST)` (ADR-0039 as amended). The seam is proven from
> Kotlin; what is still open is only the phone's fifth chip and its layout
> budget below.

## Why it was left open (2026-09-08)

The web slice that added `FrontierAxis::Urgency` deliberately stopped at the
seam. `MobileFrontierAxis` carries the variant and `MobileTaskHost.now_board`
takes a `MobileCalmOrder`, so the core is reachable from Kotlin today — but
`NowScreen.kt` holds its own four-entry `FRONTIER_AXES` / `AXIS_LABEL` /
`NO_VALUE_LABEL` maps and none of them names it.

The blocker is a layout budget, not plumbing:
`client/android/app/src/test/kotlin/net/twinion/hummingbird/AxisRowWrappingTest.kt`
pins the **whole axis strip to one unwrapped line at 419dp** — every axis
label plus the Filter chip with a count in it, measured *unconstrained* so a
squeezed chip cannot pass. A fifth chip has to come out of that budget. The
web has no equivalent constraint (its strip wraps, and does wrap to three
lines at 390px).

## The decision this needs — and it is not a code question

**How does the strip absorb a fifth axis?** Candidates, none costed:

1. Let the strip wrap on Android too — contradicts `AxisRow`'s own header,
   which states the one-line rule as a deliberate property.
2. Scroll the strip horizontally — ADR-0021 decision 3 rejects sideways
   scrolling for the *columns*; whether that argument reaches the control
   strip is exactly the open question.
3. Shorten labels, or move the Filter chip, to buy the width back.
4. An overflow affordance for axes past the fourth.

And a second, smaller one: the phone has **no home for the calm-order
direction control**. The web renders two chips beside the axis strip only
while Urgency is live; on a 390px phone that is two more chips on a strip
that already cannot fit one.

## What to load

- `client/android/app/src/main/kotlin/net/twinion/hummingbird/NowScreen.kt` —
  the three maps (~lines 94-120) and `AxisRow` (~line 785); its header carries
  the one-line rule and the accepted clipping limit.
- `client/android/app/src/test/kotlin/.../AxisRowWrappingTest.kt` — the budget,
  and why it measures unconstrained.
- `client/ffi-mobile/src/lib.rs` — `MobileFrontierAxis`'s doc (what is already
  carried and what is not), `map_calm_order`, `build_now_board`, and the drift
  gate `the_now_screen_facet_vocabularies_match_the_core`, whose
  `NOT_YET_ON_THE_PHONE` list is the one line to delete when this lands.
- `docs/adr/0021-the-frontier-in-columns.md`, decision 1's 2026-09-08
  amendment, point (g).

## Two things to fix while in there

- **`FrontierPrefs.readAxis` has no clamp.** It degrades to `CONTEXT` only
  when `valueOf` *throws*, so a stored `"URGENCY"` now parses and is returned
  even though this build's UI cannot select it. Harmless today (nothing on the
  phone writes it) but it is the net the next mobile-omitted axis needs. The
  web's equivalent clamps against the axes its board actually offers
  (`readFrontierAxis`'s `allowedAxes`). `MobileFrontierAxis`'s doc in
  `ffi-mobile/src/lib.rs` states this gap.
- **The collapsed-column prune fights a closed vocabulary.** `FrontierColumns
  .tsx`'s `liveKeys` prune discards a collapsed key whose column no longer
  exists — right for `@garden` or an archived project, wrong for urgency
  bands, which empty and refill constantly. Collapse `overdue`, clear your
  overdue items, touch any column header, and the collapse is forgotten. The
  band vocabulary is bounded at four, so the unbounded-accretion reason the
  prune exists cannot apply to it. Web-side, but the same code shapes the
  phone's `FrontierPrefs` behaviour and both should move together.
