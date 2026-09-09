# ADR-0021: The frontier in columns — a switchable grouping axis, and urgency as the only colour

**Status:** accepted · 2026-08-13 · **amended 2026-08-14 by
[ADR-0024](0024-the-size-vocabulary-and-the-level-ramp.md):** decision 2's
"urgency and nothing else" is narrowed to the *card's own* colour — size and
energy badges now render on frontier cards carrying a level ramp that reuses
the urgency tokens, so an amber mark on a card can mean "due soon" or "normal
size" depending on which element wears it. The size vocabulary is also
renamed (`short` → `normal`), which reverses the first of the "Two
corrections" below: the design mirror's `quick / normal / deep` was the right
spelling after all, and the schema is the side that moved.
**Context:** #400, the first slice of the #399 batch that also carries #401
(Now's aside label), #402 (the columns themselves), #403 (their controls), #404
(selection above them) and #405 (deleting the prototype). Docs only — no code,
no schema change, no dependency change. Lands first so those five slices are
reviewed against a written decision rather than against a comment thread, which
is [ADR-0017](0017-the-standing-question-surface-axis.md)'s own precedent.
Amends [ADR-0015](0015-the-standing-question-read-contract.md) — the aside it
calls "Now's Context aside" is renamed by decision 6 below. New glossary terms
**Size**, **Energy** and the item's **Context** land in `CONTEXT.md`.

`SCHEMA_VERSION` does **not** move, and nothing here writes. `items.size`,
`items.energy` and `items.context` have been columns since ADR-0009's DDL; this
decision puts three existing columns on screen as controls. `Core::frontier`
and `Core::blocked` are untouched, and no mutation entry point is added — the
whole of #399 is a read-time presentation change over the same queries.

*Amended 2026-09-08 (#801): **the board writes now.** A card is dragged into a
column and takes that column's value — decision 9 below, which is the one part
of this ADR that is not a read. Still no schema change and no new mutation
entry point: a drop calls the `triage` write the item panel and the Triage
screen already call, with the fields the core decides. What moves is that the
columns are no longer only a way of reading the frontier; they are also the
control that sets the field they group by.*

## Where this decision came from

The answer was not reasoned to; it was **tried**. A throwaway prototype — once
at `client/web/src/screens/now-prototype/`, with its findings in that
directory's `NOTES.md`; **both deleted by #405, so do not go looking** —
mounted three structurally disagreeing variants — A "narrow it
down" (no groups, sticky facets), B "lanes" (six computed lanes, no controls),
C "board" (wrapping columns, axis as the control) — **inside the real Now
screen**, with its real header, rail, aside and density, behind `?variant=`.
Five rounds of operator verdicts picked C and grafted A's filters and B's
colour onto it.

That the variants ran inside the real screen rather than in a sandbox is why
their verdicts are usable here: the horizontal-overflow problem, the
collapsed-column hole and the takeover-on-select complaint were all found at
real widths against the real aside, and none of them is visible in a mockup.

**Everything below that came from the prototype is recorded here, because this
ADR is where it survives** — in particular the four *rejected* approaches
(decisions 2, 3 and 4), which are the expensive knowledge and are written down
nowhere else now that `NOTES.md` is gone.

## Decision 1 — the grouping axis is switchable, and there are four of them

Now's centre column renders the frontier as **columns** grouped by an axis the
reader chooses: **Context**, **Project**, **Size**, **Energy**.

*Amended 2026-09-08: there is a **fifth axis, Urgency**, offered on both
surfaces (Now and a project's dossier), on the operator's own request for a
board partitioned by what the world is pressing. Everything the set's original
argument establishes still holds; what follows is what this axis does not
share with the four, and why each departure is required rather than chosen.*

*(a) **It does not fall to the tripwire below.** That tripwire fires on an
axis nobody ever switches away from Context, and prescribes deleting the
switch — it is a claim about the switch earning its chrome, not a cap on the
set's size. A fifth axis makes the switch more used, not less.*

*(b) **The word was already spent, and this is the reading that frees it.**
Decision 6 assigns "urgency" to the card's colour, and the `FrontierAxis`
enum's own doc used to give the asymmetry as: urgency is a facet but not an
axis, because colour already carries it. That argument is about a **redundant
encoding**, and it holds — see the amendment to decision 2 below. It never
was an argument that the partition is not worth having, and a reader who
wants the board cut by pressure could not get there from a filter: a filter
hides, and this reader wants everything, arranged.*

*(c) **Its columns are severity-ordered, not fullest-first.** `overdue`,
`now`, `soon`, `calm`, always, with empty bands omitted. Fullest-first is a
rule for axes whose values have no order of their own; these have one, and a
board whose column order re-arranged itself as items crossed band boundaries
would move under the reader for reasons they did not cause.*

*(d) **It has no no-value column, and cannot grow one.** Urgency is total —
`compute_urgency` answers `calm` for an item with no deadline and for one
whose deadline will not resolve — so "the no-value bucket always last" is
vacuously true here rather than violated. That is pinned by a test
(`urgency_never_yields_a_no_value_column`), not by this paragraph, because a
"No urgency" column appearing would be a silent change of meaning.*

*(e) **It is the one axis that reads a clock, and the one whose column is
re-sorted.** `group_frontier` gains a `now` argument, injected in the usual
deadline-shaped form; the other four ignore it and remain, as before,
incapable of depending on when they were asked. And the `calm` column is
ordered by `created_at` rather than by `by_priority_then_due`. **Which
direction is the reader's to choose**: oldest-first (the backlog: what has
been sitting here) or newest-first (the inbox: what just turned up). Neither
reading is right enough to hard-code.*

*This one has a real cost, and an earlier draft of this paragraph hid it
behind a false premise — that `calm` is the deadline-less items, so
priority-then-deadline has nothing to say about it. It is not: `calm` is
everything the world is not pressing on, which includes every item due
beyond the three-day `soon` window. So an Urgent item due in five days sits
in that column by its capture date rather than by its priority, until it
crosses into `soon` and rejoins the ordering. **Accepted rather than
overlooked**: `calm` is the column read when nothing is pressing, "what has
been sitting here longest" is the question actually being asked of it, and
the priority ordering is one axis-switch away. Recorded here because a
tradeoff justified by a false premise is one that gets re-argued.*

*(f) **The direction is persisted; the filter still is not.** Decision 5's
non-persistence rule is about a remembered filter being a remembered lie
about what you have to do. A direction hides nothing — it re-reads one
column — so it persists like the axis, in the same device-local key
namespace (`hb.<screen>.frontier-calm-order`).*

*(g) **The phone does not offer it.** `MobileFrontierAxis` carries the
variant, but `NowScreen.kt`'s switch strip is pinned by
`AxisRowWrappingTest` to one unwrapped line at 419dp, and a fifth chip is a
layout budget to spend rather than a label to append. The drift gate in
`ffi-mobile` names `urgency` as a deliberate omission rather than checking a
subset, so a **new** axis nobody has thought about still fails it.*

*Amended 2026-08-21: the board has a **second surface**. An open project's
dossier renders this same board given only that project's items, in place of
the ordered action list and fog card ADR-0030's own slices put there — a
project should read as "what can I do on this, right now" rather than as a
records page. Three consequences, all of them this decision's own rules
applied rather than relaxed. (a) The **Project axis is not offered there**,
for exactly the reason delegation is not one of the four here: on a
single-project board it produces one real column and nothing else. The
vocabulary is unchanged — the subset is a rendering prop on
`FrontierColumns`, and a stored axis outside it degrades to the default the
same way an unrecognised one does. (b) The board became a component of its
own (`client/web/src/screens/FrontierBoard.tsx`, extracted verbatim from
`NowScreen.tsx`) rather than being composed a second time from
`FrontierColumns`: the optimistic-fallback dance, the capture-vs-action slot
of decision 7 and the Blocked section are precisely the "second
implementation" this ADR refuses to grow. (c) The **filter is a TS
re-slice**, not a decision — an identity comparison against a stored
`project_id`, with ordering, grouping and faceting still flowing through the
seam untouched, so ADR-0025's carve-out is unaffected.*

**Why these four, and why they are the licensed set.** `CONTEXT.md`'s
**Delegation axis** entry already names them: "the fourth axis, alongside
**size**, **energy** and context". Three of that entry's four axes are
groupings a reader might want; the fourth — delegation — is deliberately not
one, because it is a two-valued marker whose absence *is* the default, so
grouping by it produces one real column and one "everything else". It stays a
chip. The set is therefore not invented for this surface: it is the glossary's
own axis vocabulary minus the one member that does not group.

**Why project stops being *the* grouping.** Project answers *what does this
belong to*. The surface is asking *what can I do right now, from where I am,
with the time and energy I have* — and the axes that answer that are exactly
context, size and energy. Project returns as one of the four, so no capability
is lost; it simply stops being the only answer.

**Ordering *within* a column reuses `orderFrontier`
(`client/web/src/screens/frontier-order.ts`) unchanged.** One ordering rule,
one spelling. It is already documented, already unit-tested, and already
licensed by [ADR-0002](0002-sources-join-by-role-urgency-computed-at-read-time.md)'s
"ranking is a read-time query over lifecycle state". A second ordering function
for the columns would be a second thing to keep true.

*The prototype floated in-progress-first ordering. If that is ever wanted it is
a change to `orderFrontier` with its own test — never a second ordering
function.*

**Bucket order between columns:** fullest column first, and the
"no context / no project / no size / no energy" bucket **always last**,
whichever axis is live. That is the rule `frontier-groups.ts` already stated
for unassigned items ("an unassigned item should never visually outrank a real
project's section"), now needed four times over rather than once.

### Tripwire

**If the axis is in practice never switched off Context, the switch is the
thing to cut, not to extend.** The switch costs permanent chrome; it earns that
only if it is used. A surface that always shows one axis wants that axis
hard-coded and the space back — and the honest response then is to delete the
control, not to add a fifth axis to justify it.

## Decision 2 — the card's own colour encodes urgency, and nothing else

A card's leading edge carries its **urgency** (`calm` / `soon` / `now` /
`overdue`, `client/web/src/screens/urgency.ts`). `calm` gets no swatch — the
default is not a claim worth colouring — so a legend names the three that are.
**Colour the card itself introduces means urgency and nothing else.**

That is deliberately narrower than "nothing else on the card is coloured",
which an earlier draft of this ADR claimed and the card does not honour: it also
renders a `StageBadge` for a non-`ready` stage and the priority label in
`--text-brand`. Both are inherited unchanged from
`components/domain/ItemRow.tsx` rather than invented here, and both are
*already* licensed by the very design-system rule this decision leans on — "a
coloured pill always encodes stage, tier or urgency, never decoration". Stage is
one of the three. Dropping them would make the literal sentence true at the cost
of the at-a-glance in-progress signal every other list in the app gives, and
would make this one surface disagree with `ItemRow` about what a stage looks
like.

So the claim worth holding is the one that actually does work: **this surface
adds no fourth meaning to colour.** The design system's rule needs no exception
either way — which was the point.

*Amended 2026-09-08: with the `urgency` axis live (decision 1's own
amendment), this decision's "colour reads *across* the axis and position
reads *along* it" argument has one case where the two say the same thing —
on that axis, a card's colour is a second statement of the column it is
already in. That is redundancy, not a fourth meaning, so the rule stands
unchanged and the swatch is not conditionally suppressed: a card carries its
band wherever it is read, including in the item panel above the board and on
the phone, and a mark that disappeared on one axis would be the surface's
only piece of colour whose absence meant something.*

*Amended 2026-08-13 (#399): this decision's heading and its closing claim were
narrowed from "colour encodes urgency, and nothing else" / "nothing else on the
card is coloured" to the card's own colour, in the review round that also
removed the selected card's accent fill. Rule 2 of `README.md` — a review round
changing one paragraph has nowhere else to live.*

**Why colour must say exactly one thing here, more than elsewhere.** Colour
reads *across* whichever axis the columns are grouped by, and position reads
*along* it: colour answers "how much claim does this have", position answers
"where does it belong". Two independent questions, one encoding each. A colour
that meant several things at once would be the only mark on the surface whose
meaning changed when the axis switched.

**And exactly one encoding of it per card.** The first prototype pass carried
both a leading edge *and* `ItemRow`'s urgency dot. Two encodings of overlapping
facts on one 240px card was one too many, so the dot is not carried onto the
card — which is also why the card is a separate component rather than a denser
`ItemRow`.

**Why, and what was cut to get here.** The prototype's round-2 graft took
variant B's colour, which partitioned items into six computed buckets:
Overdue, Due today, Scheduled today, Quick wins, Deep work, Everything else.
It looked good and it was wrong, and the tell was that **no honest name existed
for what it encoded** — because it mixed three unrelated facts:

| The bucket | What it is really about |
| --- | --- |
| Overdue, Due today | the world's pressure on you |
| Scheduled today | your own intention |
| Quick wins, Deep work | the shape of the work |

Every candidate name for that mixture was a synonym of something else already
in the vocabulary — "pressing" would have been a fifth word beside urgency,
band, salience and severity.

Cutting it to urgency is what [ADR-0015](0015-the-standing-question-read-contract.md)'s
own tripwire prescribes: **when a vocabulary clusters, cut it rather than
extend it.** And it costs nothing visible, because the two facts the partition
was also carrying are already legible as their own chips — `size` and
**scheduled date** stay on the card as text.

**The secondary gain, worth naming because it is the kind of thing that
otherwise gets argued twice.** The design system's colour discipline is that
"a coloured pill always encodes stage, tier or urgency, never decoration"
(`.claude/skills/hummingbird-design/README.md`). Urgency is one of the three, so
this needs **no exception to argue**. The six-bucket partition would have needed
one.

**Colour is never the only carrier.** The card states its urgency in words as
well. `components/domain/ItemRow.tsx`'s urgency dot sets the precedent, but
only partly — it puts the words in a `title` tooltip, which a keyboard or
screen-reader user does not reliably get. The card does better: the urgency is
available as text, not as a hover.

## Decision 3 — the columns wrap; they never scroll sideways

Columns wrap onto as many lines as the width needs, in reading order. Each is
capped at **six cards** with an `n more` toggle stating how many are hidden.

**Why the cap is load-bearing rather than cosmetic.** A wrapping row takes its
height from the tallest column in its line. Without a cap, one fat column sets
the height of its whole line and strands its neighbours in whitespace. The cap
is also the honest cap for this surface anyway: the top few of a column is what
"what's next" is asking about, and the count never lies about what is hidden.

**No new independent scroll container in the centre column.** `docs/SURFACES.md`
records the triage section's `60dvh` cap as the *only* one there — everywhere
else the shell's single container scrolls the page. That stays true: no
per-column overflow, and no sideways-scrolling strip.

*Amended 2026-08-13: the triage section is gone (see the amendment to the
Consequences below), and its cap with it, so the centre column now has **no**
independent scroll container. The constraint this decision states is unchanged
and strictly easier to hold.*

*Amended 2026-09-08 (#795): **the cap is measured, not six.** The columns stopped
wrapping when #402 packed them into vertical lanes, so the reason stated above —
a wrapping row takes its height from the tallest column in its line — no longer
holds up the number; a lane is a stack, and a fat column costs its lane and
nobody else. What the number was still doing was guessing at the height, and on
a tall screen it guessed badly: at a 1400px viewport the board stopped 638px
above the fold with 23 items behind an `n more`. `screens/frontier-lanes.ts`'s
`columnCapFor` now spends the room between the board and the fold on cards —
fourteen at 1400, seven at 900, a floor of four, and the old six as the answer
for a runtime that cannot measure. **The second sentence of the paragraph above
is the part that survives**: the top few of a column is what "what's next" is
asking about, and the count never lies about what is hidden. What a column
defers is now what genuinely does not fit.*

*Amended 2026-09-08 (#795): **a column can be drawn across more than one lane.**
Decision 1's own amendment makes the urgency axis three bands of one card in
front of one very full `calm`, which can never fill the board between them — so
the packing leaves a lane it has no column for, permanently, as a property of
the axis rather than a state that resolves. The column alone in the last packed
lane now runs on into that room under a repeated, de-emphasised label, with its
reveal control at the foot of the last lane. The columns still never scroll
sideways and still have no overflow of their own, which is what this decision
actually constrains.*

### Two rejected alternatives, both tried in the browser

**A sideways-scrolling strip of columns.** Rejected: a board you scroll
sideways hides columns, and hiding columns is precisely what this surface must
not do — the reader is choosing *among* everything startable, so an offscreen
column is a choice silently withheld. This is also the operator's own verdict,
in those words: *"I don't like how it ends up scrolling horizontally."*

**A CSS multi-column container.** Rejected on measurement, not taste:
height-balancing pushed a tall column's neighbours below the fold — with 26
fixture items the 13-card column's neighbours were not on screen at all.
Multi-column optimises for even column heights, which is the wrong objective
when each column is a semantic group rather than a continuation of prose.

## Decision 4 — a collapsed column keeps its place, and shrinks to fit

A column header collapses its own column. A collapsed column **stays exactly
where it is** in the board order, and keeps its count readable — a shut column
must still say how much is inside it.

**Why shrink-to-fit width, and why this is the same wrinkle as decision 3.** A
collapsed column that still claimed its full 240–380px slot would leave a
*hole* rather than buy space, because its line's height comes from its tallest
member. Collapsed columns therefore switch to shrink-to-fit and reflow their
neighbours around them; the only residue is a thin strip of empty page under
the header, for the height of that line.

**Rejected: moving collapsed columns into a separate "folded" chip strip above
the board.** Tried first, and rejected by the operator in those terms: *"I want
to be able to collapse the context headers, but I don't want them to fold into
the header."* A collapsed column is still one of the columns; relocating it
makes the reader re-find it, and makes the board's order a function of what
happens to be shut.

## Decision 5 — view preferences are device-local, and never reach `settings`

The chosen axis and the set of collapsed columns persist **per device**, in
`localStorage`, behind the injectable-`storage` idiom
`client/web/src/screens/triage-collapse.ts` already establishes (and
`shell/rail-collapse.ts` and `screens/questions/collapse.ts` after it):
`hb.<screen>.<thing>` keys, defaults encoded as **key absence** rather than as
a stored default value, and every call tolerating absent or throwing storage —
a preference that cannot persist still applies for the session.

*Amended 2026-08-13: `triage-collapse.ts` is deleted with the section it was a
preference for (see the amendment to the Consequences below). The idiom is
unchanged and `shell/rail-collapse.ts` still shows it; the `StorageLike` this
decision's modules inject now comes from `client/web/src/screens/storage.ts`.*

*Amended 2026-08-21: with the board on two surfaces (decision 1's own
amendment), `<screen>` in those keys is now load-bearing rather than
decorative — the project board reads and writes `hb.projects.*`, Now
`hb.now.*`, so an axis chosen on a project cannot re-group Now. It stops
there deliberately: the projects keys are shared by every project, not one
pair per project, because the prune-against-live-columns write means
per-project collapse entries would be exactly the unbounded key accretion
this decision keeps out of `settings`. An axis is the preference worth
remembering; a collapsed column on another project is not.*

**Why never the `settings` table.** [ADR-0015](0015-the-standing-question-read-contract.md)
and [ADR-0017](0017-the-standing-question-surface-axis.md) both rejected it for
exactly this shape: it has no DELETE and it syncs everywhere, so a view
preference would accrete keys forever and would follow the reader onto devices
whose widths make it wrong. A grouping axis is a fact about one screen on one
device, not about the person.

**Two deliberate non-persistences**, because "remember everything" is the wrong
default here:

- **The filter selection is not persisted.** You must never open Now to a
  filtered set of columns and misread it as an empty frontier. A remembered
  filter is a remembered lie about what you have to do.
- **Collapse state is cleared when the axis changes**, because it is keyed by
  column label and those labels no longer exist. Same instinct as ADR-0015
  discarding a pane override when its computed band changes: an override whose
  subject is gone is not a preference, it is a stale key.

## Decision 6 — the names, and what each one already meant

Every name the prototype invented was checked against `CONTEXT.md` and the ADRs
before it could reach a ticket, because names end up in module names, test
names and this file. Most were already spent.

| Prototype word | What it already means | Decision |
| --- | --- | --- |
| "lane" | [ADR-0009](0009-the-owned-schema-and-context-lanes.md)'s transport lanes; [ADR-0012](0012-the-notification-lane.md)'s notification lane | **dropped** with the partition it named (decision 2) |
| "board" | `screens/StatusScreen.tsx` and ADR-0017's ranked region | **columns** |
| "band" | ADR-0015's pane **salience**, which `CONTEXT.md` deliberately separates from urgency | not used |
| "pressing" | would be a fifth synonym beside urgency / band / salience / severity | **urgency** |
| "Context" | `CONTEXT.md` defines *Context source* and *Context snapshot*, never bare "Context"; the live clash is with Now's aside, labelled `Context` | **Context** for the axis — and the aside is renamed, below |

**Now's aside is renamed to "Standing questions".** It is labelled `Context`
today, and that label has been stale since ADR-0015 replaced the calendar
context tile, the demo standing-question card and the snapshot tiles with the
ranked region: the panel holds standing questions and nothing called context.
It is also the only aside carrying an inaccurate name — the other three are
"Core and calendar status", "Steps and notes", "Alert rules".

So this is a correction the repo owes anyway, not a concession extracted by the
axis. But it is *also* what frees the word: without it the screen says
"Context" twice, six inches apart, meaning two unrelated things — an item's
`@computer` in the centre column, context *sources* in the aside. #401 carries
the rename; this ADR is where the decision lives, which is why ADR-0015's
Status header points here rather than being edited.

## Decision 7 — selecting a card is not a takeover

`RealFrontier` today returns `ItemDetailPanel` *instead of* the frontier.
Instead: the selected item expands **above** the columns, and the columns stay
standing under it.

**Why.** Picking one action should not cost you the view of everything you
might have picked instead — which is the whole point of a surface whose job is
choosing among what is startable. The operator's verdict, on the first attempt
at this: *"I don't want it to take over the whole screen. I want it to expand
the pill and go to the top, but leave the rest of the board visible."*

Three consequences, all of them things that would otherwise be discovered late:

- **The panel is the existing `ItemDetailPanel`, never a second
  implementation.** It is threaded the app's own act callback, the item's steps,
  the last-act error and the microtask affordance. This is the point rather than
  an economy: whatever lands on item detail next — Grill me (#359) among them —
  arrives with no parallel code path to reconcile.

  *Amended 2026-08-14 (#TODO): the same argument was then made one level
  up. `ItemDetailPanel` and `TriageRow`'s expanded body were themselves two
  implementations of one thing, and only one of them could edit an item — so a
  minted action's own description, deadline and project were reachable nowhere
  in the app. They are now one component, `components/domain/ItemPanel.tsx`,
  parameterised by mode: `"detail"` keeps exactly the behaviour above and adds
  an **Edit** button revealing the identical fields, saving through
  `Core::triage` with `destination: null` (the stage-agnostic edit #122 already
  allowed). `"triage"` is the row's expanded body, unchanged. No new mutation,
  and the slot below still holds exactly one editor. `ItemDetailPanel` is
  therefore gone as a component: every mention of that name in this ADR —
  above, and twice in the Consequences below — names what detail mode of
  `ItemPanel` now is, and is left standing as the record of what was decided
  then.*
- **The source card stays marked while its item is expanded.** With the columns
  still on screen, the reader has to be able to see where the thing at the top
  came from and what it was sitting next to.
- **The panel is scrolled into view on select.** A card near the bottom of a
  long board would otherwise expand off-screen, which makes "it goes to the
  top" true of the DOM and false for the reader.

The slot above the frontier is Now's own. ADR-0015 gives the **aside** to the
ranked region, and its "standing questions never take the banner" is a claim
about the aside's contents — not about the centre column. The aside survives
selection, which is the property #359 calls "the one thing this surface has
that Triage does not".

**The prototype's one deliberate omission does not carry forward.**
`RealFrontier`'s optimistic post-act fallback — the frozen item that keeps the
panel open, and correctly enabled, when an act moves the item out of every live
query (a block sets a stage neither the frontier nor the blocked query reads) —
was skipped in the prototype because it is not what the prototype was asking
about. Production must keep it: two review rounds on PR #207 produced that
machinery, including the stale-`false` pending window it bridges.

## Decision 8 — the visual-coverage disposition, settled here

`?demo` **never mounts `RealFrontier`** — `NowScreen` branches to it only when
demo is off, and that branch exists to keep the two from entangling. So the
columns, the filter panel, the collapse states and the urgency colours are
**unphotographed by default** at every width and theme, and the surfaces
registry would be silent about it unless something said so.

This takes **#273's disposition**: component tests for everything decidable,
plus a recorded hand review on a device with real items for the rest.
`docs/SURFACES.md` states it explicitly rather than leaving the gap to be
discovered mid-slice. #359 makes the same demand of the Grill surface for the
same reason.

**Each UI slice updates the registry row it invalidates**, so the registry is
never stale between slices. `docs/SURFACES.md`'s "Now's centre column" row
asserts four things this batch touches — that `RealFrontier` is "the frontier
grouped by project" (#402 makes it false), the triage scroll cap, and that that
cap is *"the only independent scroll container in the centre column"* (a live
constraint, per decision 3, not merely a description).

**Rejected: entangling `?demo` with `RealFrontier` to get the screenshots.**
That branch is deliberate, and widening the demo path to photograph production
code would trade a documented coverage gap for an undocumented behavioural one.
If it is ever done it is a decided change with its reasoning written down, not
a side effect of wanting a picture.

*Amended 2026-08-13 (#420): the gap is closed, and the rejection above still
stands — this is the decided change it asked for, not the entangling it
refused. `?demo` keeps its exact present meaning and still never mounts
`RealFrontier`. What is added is a **second, mutually exclusive demo world**,
`?demo=board`, which seeds a real `TaskState` — the shape the sync engine
publishes — and returns `null` for `DemoData`. A null `demo` prop is precisely
what makes `NowScreen` take its `RealFrontier` branch, so the two paths stay as
separate as this decision wanted them: nothing is widened, and no production
component learns that a fixture exists.*

*Amended 2026-08-20 (#456): `NowScreen` deleted its `demo` prop and the
branch above with it — it renders `RealFrontier` unconditionally now, on
every world. The separation the amendment above wanted is preserved by a
different mechanism: `App.tsx`'s `task = demoTask ?? liveTask` (`demoTask`
from `demoTaskState()`, seeded only under the board world) is what still
keeps a fixture out of `RealFrontier` on every other world, not a prop
`NowScreen` branches on.*

*Why a seeded state rather than richer kit fixtures: `DemoItem` carries no
`context` and no `energy`, having been written before either was an axis, so
the kit world could not express this decision's own grouping in principle.*

*The fixture (`client/web/src/fixtures/demo-task-state.ts`) mirrors
**production's measured shape and none of its content** — 29 board cards, the
context/size/energy/source spreads read once from `GET /api/changes?since=0` on
2026-08-13, no projects, no blocked edges, priority flat at zero. Mirroring the
awkward parts is the point, and photographing them turned three of this ADR's
own decisions into findings worth their own issues: **grouping by Project
yields exactly one column**, because production has no projects at all
(decision 1 licenses four axes; one of them currently does nothing);
**the no-value bucket is the biggest column on every axis**, which decision 1
pins always-last, so the largest column sits past the fold; and **`n more` is
the normal case rather than an edge one**, two context columns being over
`COLUMN_CAP` on day one. Two documented departures keep the gate covering
states production is not in today: three deadlines, one per urgency band, so
decision 2's colour ladder is photographed at all, and a seeded `lastTriage`
failure so #418's alert is too.*

*What this does not change: the disposition above still holds for everything a
photograph cannot decide. The board world is read-only — no mutation is rewired
to it — so it is a camera, not a second writable app.*

*Amended 2026-08-20 (#455): bare `?demo` now means the board world, and the
kit world moves to the one exact spelling `?demo=kit` — `demo-mode.ts`'s
fallback for every unrecognised spelling flips from the kit to the board, and
the nine-screen loop, the capture popover and the brand-token pass all move
with it, so the board pass #420 added above is now the primary one rather
than a second pass alongside a kit one. **The mutual exclusion between the two
worlds is preserved, not relaxed** — that is what keeps the rejection above
intact: this is still the "decided change with its reasoning written down",
now applied to which spelling is the default, not to widening `?demo` itself.
It holds because Routes and Alerts — the two screens the board world's own
`TaskState` still cannot reach, since neither ever reads `task` — take a
`DemoData | null` prop that is `null` under the default `?demo` now exactly as
it was `null` under `?demo=board` before: nothing changed at either screen,
only which query string reaches them with `null`. No `DemoData` is served
inside the board world by this amendment; #457 is where Routes and Alerts are
expected to gain their own board-gated fixtures, and until then their
populated render is reachable only at `?demo=kit` and reviewed by hand
(`docs/SURFACES.md`). The standing-question fixture world this decision's own
`?demo` reference depended on, `demo-questions.ts`, is deleted with the flip:
#452 had already folded its content into the board seed's `bindings` /
`paneReads`, on the explicit condition that the kit world's own Status capture
still worked off it until the flip — this is that flip, so the module and the
`demo ? demoQuestionInputs(...) : …` branches it fed (`NowScreen.tsx`,
`StatusScreen.tsx`) are gone; both screens now read
`realQuestionInputs(task, …)` unconditionally, `task` being whichever of the
live store or the board seed `App.tsx` resolved.*

*Amended 2026-08-20 (#457): the amendment above's forecast — Routes and
Alerts gaining "their own board-gated fixtures" and a `DemoData | null` prop
— is not what landed. Both screens stayed kit-fed: each now calls its own
dev-gated `demoData()` accessor directly (`fixtures/demo-data.ts`), taking no
`demo` prop from `App.tsx` at all, so their populated render is still
reachable only at `?demo=kit`, unchanged from the amendment above. What #457
actually added was `RoutesScreen.test.tsx` and `AlertsScreen.test.tsx`,
closing the component-test gap the board-world flip left open
(`docs/SURFACES.md`).*

## Decision 9 — a card dropped into a column takes that column's value

*Added 2026-09-08 (#801). This is the decision the rest of the ADR is not: the
board's first write.*

Dragging a card from one column to another **sets the field the board is
grouped by**, to the value of the column it lands in. One gesture, one field,
on every axis:

| Axis | A drop into a column writes | Into the no-value column |
| --- | --- | --- |
| Context | `items.context` = the column | clears it |
| Project | `items.project_id` = the column | clears it |
| Size | `items.size` = the column | clears it |
| Energy | `items.energy` = the column | clears it |
| Urgency | `items.deadline`, per band — see below | *(no such column; urgency is total)* |

A card released over its **own** column writes nothing — the same-column test
is the grouping's own reader, so "already there" cannot drift from where the
board actually drew the card. Captures (`triage` and `grilling` cards, which
share these columns since decision 1's amendment) drag exactly like actions,
and the write's `destination` is always `null`: a drag edits a field and never
promotes a capture into `ready`.

**The one exception to "one field": a drop into a project column also copies
that project's `default_context` onto an item that names no context** —
[ADR-0030](0030-the-project-lane.md) decision 3's rule, reached from the board
rather than from `/to-actions`. An item that already names a context keeps it;
the reader's own answer outranks the project's default.

### Urgency writes a deadline, because it groups by a reading of one

The other four axes group by a field and set that field. `urgency` groups by a
*reading of* `items.deadline` against the clock, so a drop there has to invent
a deadline that reads back as the band it landed in:

- **`now` → today.** A day-grained deadline resolves to `23:59`, so today is
  inside the 24-hour `now` window from any moment of the day.
- **`soon` → today + 2 civil days.** *Not* today + 1: by the same end-of-day
  resolution, tomorrow is under 24 hours away for anything after midnight and
  would read back as `now` for most of the day. +2 is the smallest shift that
  reads `soon` at every hour.
- **`calm` → the deadline is cleared.** Every deadline-less item is calm, and
  clearing is the only edit that reads `calm` for certain — a distant deadline
  would too, but the board must not invent one.
- **`overdue` → refused.** It is the one band that is a fact about the *past*,
  and there is no reason to let a reader manufacture one.

Refusal is **physical, not chromatic**: the `overdue` column simply never
lights, and a card released over it springs home. Nothing is painted red —
decision 2 keeps status colour on this board meaning urgency, and a second
meaning for it would cost more than the refusal is worth.

### Where the rule lives

**In the core** (`hummingbird_core::decisions::frontier::drop_edits`), reached
through both seams — [ADR-0025](0025-the-decisions-carve-out.md). It is the
inverse of `group_frontier` and belongs beside it: same axis vocabulary, same
blank-folding, same urgency arithmetic, and the same-column test *is*
`group_frontier`'s own `axis_value`. A round-trip gate applies a drop's edits
and re-groups the item, on every axis and every column the board can draw; a
rule stated twice would pass every other test and still put the card back
where it came from.

What stays per client is the **gesture** — the physics, the hit-testing and
the edge auto-scroll all consume measured boxes, which is `frontier-lanes`'s
own argument for staying TS.

### The gesture: a weighted spring drag on pointer events

The card is not attached to the pointer; it is attached to the pointer **by a
spring**. It lags, it tilts into its own travel, it overshoots and settles.
The **whole column** is the target and tints (`--surface-quiet` fill,
`--border-strong` outline — the highlight is on the column, never on the card,
decision 2 again). The card is released *over* its target.

That shape was **tried, not reasoned to** — the same method decision 1 and
[the "Where this decision came from" section](#where-this-decision-came-from)
record for the board itself. A second throwaway prototype
(`client/web/src/screens/now-prototype/`, again deleted once this ADR carried
its verdict) mounted four affordances inside the real Now screen behind
`?demo&variant=`, and the operator flipped between them. **The four rejected
shapes, which are the expensive part of what was learned:**

- **Native HTML5 drag-and-drop.** The cheap baseline, and what this issue's
  own plan assumed before the prototype ran. It gives the ghost image, the
  drop cursor and edge auto-scroll for free — and it **does not fire for a
  finger at all**. Rejected on touch alone; everything it gave away had to be
  rebuilt by hand.
- **A pointer drag whose target is the column's header strip, previewing the
  landing with an opening slot.** Two disagreements with the winner, both
  rejected: a header-strip target is a small target for a gesture whose whole
  point is that a column is a place, and the opening slot promises a *position*
  the board does not honour — `orderFrontier` decides where the card actually
  lands, so the preview would be a lie about everything but the column.
- **Pick, then place** — a key or a long press picks the card up, every column
  that would accept it grows a `Move here` button. The only variant native to
  every input the product has, and the only one a screen reader gets for free.
  Rejected as the *primary* affordance because it is two decisions where the
  reader has one intention; its accessibility argument is answered instead by
  leaving the item panel exactly where it is (below).
- **The same spring, thrown.** Release with speed and the card flies,
  decelerates under friction, and lands in whichever column its *projected*
  resting point falls in. Rejected: the aim is taken before the flight starts,
  so the reader is committing to a column they are not yet pointing at. The
  magnetic variant — columns pulling the card off the cursor — was rejected
  with it, and for the same reason.

So the winner keeps the **weight** and drops the **ballistics**.

### What the gesture must not cost

- **Touch scrolling.** The prototype set `touch-action: none` on every card,
  which would trade the board's own scrolling for the gesture. A finger
  therefore **arms on a 350ms hold**; movement before that scrolls the board
  as it always did, and only once armed does the gesture swallow `touchmove`.
  A mouse or pen arms on 5px of travel, as it always would.
- **The item panel.** A drag never opens it — the click a gesture ends with is
  swallowed — which is decision 7's "selecting a card is not a takeover" seen
  from the other side. A **keyboard reader keeps the panel and gets no
  gesture**: the panel already edits every one of these fields, so the drag is
  a faster path to an existing write rather than the only path to it.
- **The board's layout.** The column rects are frozen at gesture start and
  hit-tested against that snapshot, for the same reason decision 4 freezes the
  lanes' resting room: anything that reflowed mid-drag would move the target
  out from under the pointer. The one thing that legitimately moves is the
  scroll container, and the gesture corrects for exactly that.

### The tablet, and not the phone

The gesture ports to Android's **wide-window** lane board and to nothing else.
The phone stacks the columns down one list, where a drag between them would be
a scroll through the board rather than a movement across it — there is no
second column on screen to aim at. The phone keeps the item detail panel,
which is where it already edits these fields.

The tablet reaches the same core rule through **two** seam doors rather than
one, and the second is what keeps "a refused column never lights" true on a
surface that cannot ask per frame: `droppable_columns` answers, once when a
press becomes a drag, which of the board's columns this card could land in,
and `move_item_to_column` commits once when the finger lifts. The alternative
— letting every column but the card's own light, and refusing on release —
was rejected: it would make `overdue` invite a drop it always declines, which
is the affordance lying about itself. The web asks the same question per frame
because it can (`dropEdits` is a synchronous wasm call); Android cannot, and
buys the same behaviour with one crossing per gesture instead of a rule
restated in Kotlin.

Two differences from the web are accepted rather than overlooked. Android
**does not FLIP**: the write and the board's re-read happen on the gesture the
finger just ended, so the recomposed card is simply drawn at rest in its new
column — the web's held-card FLIP exists to pay for a re-read that arrives
several renders later, and there is nothing here to pay for. And there is **no
edge auto-scroll**: four axes against a cap of six columns means the tablet's
board fits, so there is no edge to drag toward.

## Consequences

- `screens/frontier-groups.ts` and its test are deleted with the project
  sections they served (#402). Project grouping survives as one axis of the new
  module.
- Now's centre column gains a compact **card** distinct from
  `components/domain/ItemRow.tsx`, which stays as it is for Triage, Done,
  Ledger and Now's own Blocked section. Two components because they have
  genuinely different densities and affordances — not a variant flag.
- Everything decidable is a **pure sibling module** the `.tsx` only threads
  state through, which is the split every `screens/*` module keeps: `readonly`
  items in, a fresh data shape out, display text left to the caller, and the
  clock a parameter where one is needed rather than read inside.
- The Blocked section and the triage section stay below the columns, unchanged.

  *Amended 2026-08-13: the triage section is **dissolved into the columns**.
  The unsorted captures are cards among the startable actions — grouped by the
  same live axis, ordered under that column's actions, and marked with the
  `triage` `StageBadge` rather than a badge invented for this surface.
  `screens/NowTriageSection.tsx` and `screens/triage-collapse.ts` are deleted
  with it; `StorageLike` moved to `screens/storage.ts`, which is what the rest
  of the preference modules import now.*

  *Three things this leaves standing rather than reopening. **Ordering:** one
  concatenation before grouping (`orderFrontier`'s output, then `orderTriage`'s)
  is the whole implementation, because `groupFrontier` preserves input order
  inside every bucket — so decision 1's "one ordering rule, one spelling"
  survives, and there is still no second ordering function. **Colour:** stage is
  one of the three things the design system lets a coloured pill encode, so the
  chip is not decision 2's fourth meaning. **Selection:** a capture fills
  decision 7's slot with `TriageRow` forced open — never `ItemDetailPanel`,
  whose act vocabulary offers a pre-action item nothing — so S13/#111's "two
  editors are never open at once" now holds by construction rather than by
  withholding a section, and the captures' cards stay on the board whichever
  kind is open.*

  *Why at all: the section was a second place to look for work on a screen whose
  question is "what do I do next". An unsorted capture **is** a candidate answer
  — sorting it is the doing — and stacking the captures outside the axis meant
  the one thing the board could not tell you was which of them belonged to the
  context you are actually in.*

  *Amended 2026-08-13 (#418): the amendment above cost a **failure** its home,
  and Now grows one line to give it back. `TriageRow` renders its failure
  outside its expanded block precisely so a late result still lands on a
  collapsed row — true where the rows stand in a list, false the moment the row
  became the slot, because closing the slot unmounts the component. So a triage
  that failed after the reader closed the panel was displayed nowhere: the
  capture returned to the board (correctly — a failed triage leaves the item in
  `triageInbox`) saying nothing. Now states it itself, above the columns, as a
  `role="alert"` paragraph **naming the capture**, and stays silent while that
  capture is the open one so the two surfaces never both speak for one result.
  Both sentences come from one pure module, `screens/triage-failure.ts`.*

  *Three shapes were on the table and two are rejected here, because they are
  the expensive knowledge. **On the card** — error text under the failing
  capture's title — is the most precise about where a failure belongs, and it
  loses to this decision's own furniture: a column caps at a measured number of
  cards (six when nothing can be measured — see decision 3's 2026-09-08
  amendment), so a capture behind `n more` would wear a message nobody can see,
  which is the
  original bug one layer down. (Decision 2 rules out saying it in colour
  instead: what a card's colour encodes is urgency and nothing else.)
  **Holding the slot open on failure** is the smallest change and the closest to
  the old behaviour, and it takes the panel away from the reader at the exact
  moment they asked to close it — a surface whose decision 7 is "selecting a
  card is not a takeover" should not answer a failed write with one.*

  *What this does not do is fix the general case. `TaskState.lastTriage` holds
  the most recent result and not a map, so exactly one failure exists at a time
  and this line is honest about precisely that. A per-item error surface — for
  triage or for `lastAct`, which has the same shape and the same limit — is a
  store change, and a bigger decision than the bug that prompted this one.*

  *Amended 2026-08-13 (the general case, taken up): **the store does not grow a
  map, and Now grows a second line instead.** `lastTriage`/`lastAct` were read
  above as "the most recent result, so failures are lost" — they are not lost,
  because they are not that lane. Both are posted synchronously, one per
  request, off the task host's own serial queue (`worker/task-worker.ts`), and
  a `"failed"` there means the mutation never reached the outbound queue at
  all. Nothing about a sync cycle writes either field. So "one at a time, per
  kind" is what is actually true of them, and a `Record<itemId, result>` would
  have been a map with at most one live key, plus an eviction problem
  (`deadLetters`' own, which "only ever grows") bought for nothing.*

  *The lane that genuinely holds many failures at once is the **dead-letter
  journal** — where a write the authority rejected lands, several per drained
  cycle. Merging the two was considered and rejected: `CONTEXT.md` distinguishes
  them, and rightly. "Your write never queued" and "the change's effect is
  abandoned" are different facts, need different words, and only one of them is
  re-appliable by hand.*

  *So this decision is two small ones. **On Now:** `lastAct` gets exactly the
  treatment `lastTriage` got here — a second `role="alert"` line above the
  columns, naming its item. It is a second line and not a second use of the
  first, because the two results coexist in the store and a shared slot would
  make one failure hide the other. Both sentences now come from one pure
  module, `screens/write-failure.ts` (renamed from `triage-failure.ts`, which
  this amendment's parent named; one algorithm, the subject as its parameter).
  The act line stays silent while the failing item's `ItemDetailPanel` is open
  — that panel's `actError` owns the message then — but **not** while the
  failing item is an open capture, because a capture in the slot gets
  `TriageRow`, whose checkmark issues an act and which renders no act failure at
  all. That was a third stranding, found by asking which editor actually wears
  each result.*

  *Amended 2026-08-14 (#TODO): the **triage** line's suppression widened
  on the same principle. It used to fall silent only for an open capture,
  because only `TriageRow` could wear a triage failure; now that detail mode
  edits through the same mutation, `ItemPanel` says its own, and
  `strandedTriageFailure` takes whichever item has an editor open on it. The
  question is unchanged — which editor actually wears each result — and the
  answer moved because a second editor appeared.*

  *Meanwhile the dead-letter journal could not say **which** item an abandoned
  change was about: its entries carried the queue entry's id, which names the
  attempt. `MutationIntent::subject` (derived from the queued intent, so every
  already-durable entry answers it with no schema bump) now carries the entity
  and row onto `DeadLetterEntryDTO`, and Settings names the item by title.
  `Core::ledger`'s own badge derivation was re-expressed in terms of it rather
  than keeping a second reading of the same question.*
- `CONTEXT.md` gains **Size**, **Energy** and the item's **Context**. All three
  existed only inside other definitions and in ADR-0009's DDL; this decision
  makes all three UI vocabulary, and the glossary adjudicates design questions.

## Two corrections found in the same naming pass

Cheap to carry here, and both were live wrong answers:

- `.claude/skills/hummingbird-design/README.md` said size is
  `quick / normal / deep`. The schema says
  `size TEXT CHECK (size IN ('quick','short','deep'))`
  (`server/authority/src/schema.rs`). **The schema wins**; the mirror is
  corrected.

  *That conclusion did not survive: the DDL quoted here is no longer the DDL
  that runs, and the mirror's spelling was the right one — see the amendment
  pointer in the Status header. The reasoning stays with the ADR that
  reversed it and is not copied back.*
- The glossary term is **Deadline**, not "Due date" —
  [ADR-0013](0013-the-rule-condition-vocabulary.md) renamed it. The retired name
  *had* been reintroduced, in two places and one of them load-bearing: the design
  mirror's own **"Domain words are exact… do not paraphrase them"** roster listed
  "Due date" as a term of art, and its icon legend glossed `flag` the same way.
  Both corrected here, and that roster now also carries **Size**, **Energy** and
  **Context**.

  It also survives in **code**, which this slice deliberately does not touch:
  `screens/TriageRow.tsx` labels its deadline field `"Due date"` — a
  *user-facing* string, asserted by six `TriageScreen.test.tsx` expectations. So
  the app says the retired word out loud. Recorded rather than fixed, because a
  docs-only slice changing a UI label and six tests is the wrong shape; it gets
  its own issue.
