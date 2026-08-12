# ADR conventions

The ADRs in this directory are the decision record. This file holds the
rules about the record itself; everything else is in the ADRs.

## Amending an accepted ADR

**An accepted ADR records what was decided when it was accepted. It is not
kept current in place.** Later thinking is recorded by pointer, so each ADR
stays a readable account of its own decision rather than a document that
silently mutates.

1. **An amendment made by a later ADR lives in that later ADR.** The amended
   ADR gains one entry in its Status header — `**amended YYYY-MM-DD by
   [ADR-00xx](00xx-….md):**` followed by a sentence or two saying what
   changed. The amending text itself — the new table row, the DDL, the
   reworded rule — stays in the ADR that decided it, and is not copied back.

2. **An amendment no other ADR owns is written inline**, in the ADR it
   amends, marked with its date and its issue: `*Amended 2026-08-10 (#120):
   …*`. A grilling, a review round, or an issue that changes one paragraph
   has nowhere else to live. This is the narrow exception, not a second
   convention — if a change is large enough to want a home of its own, give
   it an ADR and use rule 1.

3. **A table, list or DDL block that later ADRs amend carries a line saying
   so**, immediately below it, pointing at the Status header. Completeness
   is not something a reader should have to infer: a four-row table that is
   really five rows must say where the fifth is.

The worked example is
[ADR-0009](0009-the-owned-schema-and-context-lanes.md): its lane table and
its schema DDL both carry rule 3's line, its header carries rule 1's
pointers to ADR-0011 through ADR-0015, and the #120 and #266 notes in its
body are rule 2's exception.
