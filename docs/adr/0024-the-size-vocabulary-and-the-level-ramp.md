# ADR-0024: Size is quick/normal/deep, and the level ramp is a licensed meaning for colour

**Status:** accepted · 2026-08-14
**Context:** #446, from a design handoff proposing custom glyph families for
the two metadata dimensions the app has always stored and barely drawn.
Amends [ADR-0009](0009-the-owned-schema-and-context-lanes.md) (the DDL's
`size` `CHECK`, `SCHEMA_VERSION` 6 → 7) and narrows
[ADR-0021](0021-the-frontier-in-columns.md) decision 2 (what a colour on a
frontier card is allowed to mean). Numbered 0024 because 0023 was taken by
[ADR-0023](0023-the-grill-interview-is-a-native-typed-turn-contract.md) the
day before.

Two decisions that arrived together and are recorded together because the
second is what made the first worth its cost: the word only had to be fixed
in the database *because* the word was about to become visible everywhere.

## Decision 1 — the middle size is `normal`, all the way to the store

**Size is `quick`, `normal`, `deep`.** It was `quick`, `short`, `deep` in the
DDL, in `hummingbird_domain::Size`, in the ranker and in the glossary — and
`normal` on the capture slider, and `Short` in the triage select. Three
spellings of one value, which is one more than a system can carry.

**The glossary was the thing that was wrong, not the slider.** `CONTEXT.md`
said `short`; the operator's ruling was that the word people actually use for
the middle of that scale is `normal`, and that the glossary had simply
recorded the wrong one. That is worth stating plainly because the glossary
normally adjudicates: this is the exception where the code was closer to the
domain than the document was, and the document moved.

**The rename went to the wire rather than stopping at a display label.** A
display-only fix was available and cheaper: keep `short` on the wire, print
"normal". It was rejected because a display label that disagrees with the
stored value is exactly the state this change is unwinding — the capture box
had carried a second array of display words for precisely that reason, guarded
by a length assertion because nothing mechanical kept the two aligned. Fixing
the symptom by adding a fourth spelling would have been the same mistake with
a better excuse. With one word, the display array *is* the wire array and the
guard has nothing left to guard.

**The cost is a real migration.** `hummingbird-authority` has been live since
2026-08-10 (#237), so the re-freeze doctrine that covered `due_date` →
`deadline` (ADR-0013, #153) — rewrite the frozen fixture, no migration,
nothing deployed to care — is no longer available. `size` is inside a `CHECK`
constraint, which SQLite cannot alter in place, so `SCHEMA_VERSION` 6 → 7 is
the **first growth that is not additive at all**: `add_missing_columns` is
joined by a table rebuild. Three traps, all measured rather than assumed, and
all recorded at `rebuild_items_for_size_vocabulary` in
`server/authority/src/schema.rs`:

1. **`ALTER TABLE … RENAME TO` rewrites the stored DDL.** The textbook rebuild
   (create `items_new`, copy, drop, rename) leaves `sqlite_master` holding
   `CREATE TABLE "items" (…)` — name quoted, `IF NOT EXISTS` gone. Three
   growth tests assert a migrated store and a fresh one hold *byte-identical*
   DDL, and that invariant is the thing most likely to catch a migration that
   produces a nearly-right shape. So the rebuild drops `items` and recreates it
   from the `CREATE_ITEMS` constant itself: it cannot drift from the fresh
   path, because it is the fresh path.
2. **Foreign keys.** `steps`, `blocked_by` and `grills` all reference
   `items(id)`, and with enforcement on, `DROP TABLE items` is an implicit
   delete of every parent row. `PRAGMA defer_foreign_keys` does *not* rescue
   it — the deferred counter is not decremented by a reinsert into a table the
   violating statement never saw, so the commit fails anyway. The rebuild
   stands the children aside instead (copy out, empty, rebuild the parent, put
   back), which needs no pragma, no transaction, and no assumption about
   whether the Durable Object enforces foreign keys at all. The test rig turns
   enforcement **on**, so the growth test exercises the strict case.
3. **The children's clauses follow a rename.** Renaming `items` out of the way
   first would rewrite `REFERENCES items(id)` to point at the scratch name and
   leave it there. Nothing is ever renamed; the parent is dropped and rebuilt
   under its own name.

**A device may still send the old word, and is not punished for it.**
`Size::Normal` carries `#[serde(alias = "short")]` and `Size::parse` accepts
`"short"`. The client sync engine persists an outbound queue
(`client/core/src/sync/`), so a `CreateItem` or `ItemPatch` minted before the
deploy can drain after it; without the alias those writes land in the
dead-letter journal instead of the store. It is one-way — `as_str` emits
`normal` and nothing writes the old word — so this is a doorway, not a second
spelling.

## Decision 2 — size and energy are drawn, and the ramp may use colour

**Both dimensions render as a glyph and its word**: size as depth rings (a
centre dot gaining rings as the work goes deeper), energy as three ascending
bars. The level is carried twice over — by the glyph's fill (earned elements
solid, unearned ghosted, unset a flat wash) and by colour from a four-step
ramp that reuses tokens the system already has: `--text-muted`,
`--status-done-fg`, `--urgency-soon`, `--urgency-now`. The ramp maps by
**position on the scale**, not by name, which is what lets one table serve
both dimensions instead of two that can drift.

**Icon and label always share a colour.** The design system's rule (README,
ICONOGRAPHY) is why `levelColor` returns one answer applied to both. A
coloured mark beside an uncoloured word reads as decoration; together they
read as one statement about the item.

**This narrows ADR-0021 decision 2, and the cost is real.** That ADR said a
frontier card's colour "encodes urgency and nothing else". Size badges render
on frontier cards, so an amber mark on a card can now mean *due soon* (the
leading edge) or *normal size* (the badge), and a reader has to look at which
element is amber to know which. That is accepted, not hidden. The claim that
survives is the one ADR-0021 already fell back to: **the card's own colour**
still means urgency and nothing else. These badges are `ItemRow`'s vocabulary
inherited unchanged, exactly as `StageBadge` and the priority label already
are — the card renders the row's meta chips, and always did.

**Rows gain energy.** Energy was rendered in exactly one place, the detail
panel, which made two dimensions of the same kind feel like different kinds of
fact. They are now drawn the same way wherever either is drawn.

**Unset is a resting state, never a warning.** Both families have an unset
variant — every element at 45%, paired with an em dash — and it is muted, not
escalated. CONTEXT.md is explicit that an absent size is an unmade judgement
rather than a claim of smallness, and the drawing says the same thing.

Where that ghost renders was a judgement this ADR makes explicitly, because
the surfaces disagree about what silence means:

- **`ItemDetailPanel` always draws both**, ghost and em dash included. It is
  the one surface that describes a single item in full, so "nobody has judged
  this yet" is information the panel owes its reader.
- **`ItemRow` and the frontier card omit an absent dimension entirely**,
  keeping the row's existing contract for every optional chip — nothing to
  say, nothing rendered, the rule `priority`, `steps` and `blockedBy` already
  follow. A dense column is not the place to draw an unmade judgement on every
  line.
- **The capture slider and the triage select always draw it**, because there
  the value is being chosen and its absence is the current answer.

**No colour is added to the system**, and the visual gate needs no change:
per `docs/SURFACES.md` the visual spec asserts that tokens *resolve*, never
that they equal a hex, and all seven tokens already exist at both themes.

## What this does not decide

The design-system mirror at `.claude/skills/hummingbird-design/` still says
`quick / short / deep` — its `github.md` records that the upstream
claude.ai/design project was deliberately corrected *to* that spelling. The
mirror is a copy, so editing it here would be undone by the next re-pull. The
reversal is recorded in `github.md` as a pending push; correcting the design
project itself is a separate gesture.
