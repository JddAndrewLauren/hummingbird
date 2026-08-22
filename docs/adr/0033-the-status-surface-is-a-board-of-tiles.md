# ADR-0033: The Status surface is a board of tiles, and the reader owns what is open

**Status:** accepted · 2026-08-21
**Context:** a high-fidelity Claude Design handoff for the Status screen
(`design_handoff_status_screen/README.md`, grounded against this repo's
shipped Status surface) draws the web client as a labelled tile board whose
selected tile expands in place, and the Android client as a "quiet stack".
Amends [ADR-0017](0017-the-standing-question-surface-axis.md) decision 1.

No schema change, no new source, no new decision in the core: everything on
this surface is still `rankPanes(inputs, "status")`'s output, rendered
(ADR-0025).

## Decision — Status renders as a tile board, and single selection replaces collapse-when-dormant *on this surface*

ADR-0017 decision 1 chose the ranked region for Status and recorded its
rejected alternative in as many words: *"a bespoke fixed-grid `HealthTile`
framework"*. This ADR adopts that alternative for the web client, and the
honest way to state it is that the rejection is **superseded, not
reinterpreted**. Two things change with it:

- **Single selection replaces the per-band collapse override.** One open
  tile, held in `status-board/status-prefs.ts`. Nothing on this surface
  opens itself any more — a pane that turns bad announces itself by its
  tile's *treatment* (a band-coloured ring and a coloured glance word),
  not by expanding.
- **Identity ordering replaces the captured sample.** A tile's position is
  its group, then the declared question order, then its subject. Band is not
  in that list, so a band change cannot move a tile at all. The open tile
  takes a **full row** rather than two columns for the same reason: a
  two-column tile that cannot fit its row wraps, and the hole it leaves pulls
  a later tile up into it, so opening one tile moved its neighbours.

**Why the original rejection no longer binds.** ADR-0017 feared two specific
things, and both are answerable on the record rather than by assertion.

*"Every tile stays visible all the time, and the five-plus infra panes #314
anticipates crowd out anything actually wrong."* The compact tile is the
answer: a pane that is fine renders as an icon, a name, one muted line of
its own words and a 6px dot, and a pane that is not renders the same size
with a band-coloured ring and its fault in words (`stalled, last ok 1h ago`,
`unreachable — connect timeout`). Nothing is hidden and nothing shouts; ten
panes fit above the fold at every width the gate photographs. The crowding the ADR feared was of
*expanded* rows, which is what collapse-when-dormant existed to prevent —
and the board has no expanded rows to crowd with.

*"The grid grows its own show/hide rule, duplicating machinery ADR-0015
already built and tested."* `questions/collapse.ts` is **not
reimplemented**. It is not used here at all, and Now still owns it,
unchanged and still under its own twelve tests. What replaced it is not a
second collapse map but a single nullable key — the board has no per-pane
default to overrule, because it collapses no panes.

**The cost, recorded honestly.** A reader who wants to know *why* a probe is
red must click it; before, a non-dormant pane opened itself and said so
without being asked. That is a real regression in what a glance tells you,
bought for a board that shows every subject's state at once instead of a
stack whose length varied with how much was wrong. The compact tile's glance
word is what makes it a trade rather than a loss: the fault is always in
words on the tile, and only the *detail* costs a click.

**Android is not this decision.** The same handoff draws the phone as
problem panes always expanded above one quiet card of chips — which is
ADR-0017 decision 1 *executed*, not amended, and it needs no ADR. The two
clients diverging here is ADR-0025's carve-out working as intended: one set
of decided panes, two renderings, chosen per surface.

## What did not change

The pane contract (ADR-0015), the surface axis and the registry filter
(ADR-0017 decisions 2-4), the band vocabulary, `AnswerState`'s three arms,
the sort, and every pane's own words — no sentence on this surface was
rewritten for it.

The four panes' expanded bodies are the same components Now renders, reached
through an `XPaneBody` export. Stated precisely, because "verbatim" was
claimed here first and was not true: a body drops the card the tile already
is, and drops its own headline, because the tile's header carries the pane's
decided sentence and a body that drew it too would say one thing twice. What
that leaves is the supporting facts. For reachability it leaves *nothing* —
"Synced 12m ago" is that pane's whole answer — so its tile is drawn with no
disclosure control at all rather than one that opens onto an empty card, and
it is the one tile on the board with no `aria-expanded`.

A gap keeps its own arm throughout: its reason is the one thing its tile has
not already said, so an opened gap shows the reason and not the heading.

One rule this surface keeps by a different mechanism: **never an empty list
pretending to be all quiet**. The board has no loading arm, because it needs
none — no status question has a binding to be unbound from, so before
anything is polled every pane answers `bound-but-unacquired` and says so in
words. Those gap tiles, which deliberately wear no green dot, *are* the
honest first frame.
