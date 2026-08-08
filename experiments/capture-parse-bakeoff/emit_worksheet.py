#!/usr/bin/env python3
"""Emit the blind ground-truth worksheet for the capture-parse bake-off.

Stdlib only (matches the sweeper ethos).

Phase 2 of RUNBOOK.md: the human labels the correct parse for every capture
*from the raw text alone*, before seeing either model's output. This script
builds the worksheet that makes that possible -- it reads `corpus.jsonl` and
NOTHING ELSE. It cannot show you a model parse because it never opens
`hosted_results.jsonl`, `nano_results.jsonl`, or `scoring.md`.

    ./emit_worksheet.py                  # -> ground_truth_worksheet.txt
    ./emit_worksheet.py --out -          # -> stdout

Fill in the four fields under each capture, then hand the file back to any
session ("ground truth is done") and it merges into `scoring.md`'s
`ground_truth` column via `merge_worksheet.py`.

Leave a field blank to mean "unset" -- that is a real answer, not a skipped
one. `notes:` blank means the whole capture was the title; `due:` blank means
no temporal phrase; `label:` blank means nothing was clearly implied.

The `spoken:` line on each capture is the date the capture was actually made.
Resolve relative phrases ("tonight", "by Friday") against THAT date, not
today's -- otherwise the labels disagree with the model columns by a day.
"""

import argparse
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
CORPUS = os.path.join(HERE, "corpus.jsonl")
WORKSHEET = os.path.join(HERE, "ground_truth_worksheet.txt")

HEADER = """\
GROUND-TRUTH WORKSHEET -- capture-parse bake-off (#42)
======================================================================

Blind labelling. Do NOT open scoring.md or hosted_results.jsonl until this
file is finished -- reading a model's parse first anchors your label to it,
and the whole bake-off rests on these labels being independent.

For each capture, write the parse YOU would want in the local store:

  title  -- the one actionable line, filler stripped, short and imperative.
            If the capture holds SEVERAL actions, the title is the FIRST one
            stated -- not the most important. First is a fact; "clearest" is
            an opinion, and no two labellers agree on it.
  items  -- ONLY for captures with more than one action. One action per line,
            in the order spoken, each indented by two spaces and starting
            with "- ". Repeat the title's own action as the first entry --
            the redundancy is deliberate. Blank for single-action captures;
            never write a one-item list. Example:

              items:
                - Pick up milk
                - Replace the air filter
                - Call the plumber

  notes  -- everything else worth keeping that ISN'T an action: qualifiers,
            hedges ("maybe"), incomplete clauses, a note about a suspected
            mis-transcription. Blank if there is no remainder.
  due    -- only if the raw carries an explicit temporal phrase. A date
            (YYYY-MM-DD) if you can resolve it against that row's `spoken`
            date, else the phrase verbatim. Blank otherwise. Never guess.
  label  -- comma-separated, any subset, only where CLEARLY implied:
              context=@home|@errands|@phone|@computer|@work|@anywhere
              energy=low|medium|high
              size=quick|medium|deep
            Blank is a perfectly good answer. Skip freely.

Two rules that decide the hard rows:

  multi-item   -- every action goes in `items`, in the order spoken; the
                  first one is also the title. Nothing may be lost. This
                  does NOT mean the capture becomes three tasks -- it stays
                  one task, and whether to split it is a Triage decision you
                  make later, not a labelling decision you make now.
  unparseable  -- put the whole raw text in title, leave the rest blank.
                  Kept beats clever.

Blank means "unset" and is a real answer. Write your label even when it feels
obvious -- the easy rows are the control.

Keep the `== id ==` lines exactly as they are; the merge script keys on them.
Multi-line values are fine: indent continuation lines by two spaces.

{count} captures below.
======================================================================
"""

ENTRY = """
== {id} ==
raw:    {raw}
source: {source}   spoken: {spoken}

title:
items:
notes:
due:
label:
"""


# The `ph-*` rows are synthetic and have no capture date. Their hosted parses were
# produced on 2026-08-07 and resolved relative phrases against it, so ground truth
# must use the same anchor -- otherwise every placeholder due-bait row scores as a
# hosted hallucination for no reason but a missing date.
PLACEHOLDER_ANCHOR = ("2026-08-07 (synthetic row -- no real capture date; this is the "
                      "anchor the hosted column used, so resolve against it)")


def spoken_date(cap):
    """The date to resolve this row's relative phrases against."""
    m = re.search(r"\d{4}-\d{2}-\d{2}", cap.get("origin", "") or "")
    return m.group(0) if m else PLACEHOLDER_ANCHOR


def emit(out):
    with open(CORPUS, encoding="utf-8") as fh:
        corpus = [json.loads(line) for line in fh if line.strip()]
    out.write(HEADER.format(count=len(corpus)))
    for cap in corpus:
        out.write(ENTRY.format(id=cap["id"], raw=cap["raw"],
                               source=cap.get("source", "unknown"),
                               spoken=spoken_date(cap)))
    return len(corpus)


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--out", default=WORKSHEET,
                    help="output path, or '-' for stdout (default: ground_truth_worksheet.txt)")
    args = ap.parse_args()

    if args.out == "-":
        return 0 if emit(sys.stdout) else 1

    if os.path.exists(args.out):
        with open(args.out, encoding="utf-8") as fh:
            if re.search(r"^(title|items|notes|due|label):\s*\S|^ +- \S", fh.read(), re.M):
                sys.exit(f"{args.out} already has answers in it -- refusing to overwrite. "
                         "Move it aside first if you really want a fresh worksheet.")

    with open(args.out, "w", encoding="utf-8") as fh:
        n = emit(fh)
    sys.stderr.write(f"wrote {n} captures to {args.out}\n")
    sys.stderr.write("Blind phase: do not open scoring.md or hosted_results.jsonl until it's filled.\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
