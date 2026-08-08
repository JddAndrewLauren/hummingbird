#!/usr/bin/env python3
"""Merge the filled ground-truth worksheet into scoring.md's ground_truth column.

Stdlib only (matches the sweeper ethos). The other half of `emit_worksheet.py`.

    ./merge_worksheet.py                             # reads ground_truth_worksheet.txt
    ./merge_worksheet.py --worksheet other.txt
    ./merge_worksheet.py --check                     # validate only, write nothing

Parses the `== id ==` blocks, validates each label against `schema.json` (so a
typo'd enum is caught here rather than during scoring), writes
`ground_truth.jsonl`, and replaces the `_TODO_` in each row's ground_truth cell
in `scoring.md`.

Blank fields mean unset, which is a legitimate label -- a capture with no
temporal phrase SHOULD have a blank `due`. But a blank `title` is not a label
at all, so unlabelled rows are reported and left as `_TODO_`; re-running after
filling them in is safe and idempotent.
"""

import argparse
import json
import os
import re
import sys

from run_hosted import validate  # same minimal validator the hosted side uses

HERE = os.path.dirname(os.path.abspath(__file__))
CORPUS = os.path.join(HERE, "corpus.jsonl")
SCHEMA = os.path.join(HERE, "schema.json")
SCORING = os.path.join(HERE, "scoring.md")
WORKSHEET = os.path.join(HERE, "ground_truth_worksheet.txt")
OUT = os.path.join(HERE, "ground_truth.jsonl")

FIELDS = ("title", "notes", "due", "label")


def parse_worksheet(path):
    """-> {id: {field: text}}. Continuation lines are indented by two spaces."""
    blocks, cur, field = {}, None, None
    with open(path, encoding="utf-8") as fh:
        for n, line in enumerate(fh, 1):
            # `== id ==` with real whitespace and a `=`-free id, so the header's
            # ==== rule line isn't mistaken for a block.
            head = re.match(r"^== +([^=\s]+) +==\s*$", line)
            if head:
                cur, field = head.group(1), None
                blocks[cur] = {}
                continue
            if cur is None:
                continue  # preamble
            kv = re.match(r"^(%s):[ \t]*(.*)$" % "|".join(FIELDS), line)
            if kv:
                field = kv.group(1)
                blocks[cur][field] = kv.group(2).strip()
                continue
            if field and line.startswith("  ") and line.strip():
                blocks[cur][field] = (blocks[cur][field] + " " + line.strip()).strip()
                continue
            if not line.strip():
                field = None
    return blocks


def parse_label(text, cid, errs):
    label = {}
    for part in (p.strip() for p in text.split(",") if p.strip()):
        if "=" not in part:
            errs.append(f"{cid}: label fragment {part!r} is not key=value")
            continue
        k, v = (s.strip() for s in part.split("=", 1))
        if k not in ("context", "energy", "size"):
            errs.append(f"{cid}: unknown label key {k!r}")
            continue
        label[k] = v
    return label


def build(blocks, corpus_ids, errs):
    parses, unlabelled = {}, []
    for cid in corpus_ids:
        b = blocks.get(cid)
        if b is None:
            unlabelled.append(cid)
            continue
        title = b.get("title", "")
        if not title:
            unlabelled.append(cid)
            continue
        p = {"title": title}
        if b.get("notes"):
            p["notes"] = b["notes"]
        if b.get("due"):
            p["due"] = b["due"]
        if b.get("label"):
            label = parse_label(b["label"], cid, errs)
            if label:
                p["label"] = label
        parses[cid] = p

    for extra in set(blocks) - set(corpus_ids):
        errs.append(f"{extra}: worksheet block has no matching corpus capture")
    return parses, unlabelled


def cell(p):
    def trunc(s, n):
        return s if len(s) <= n else s[:n].rstrip() + "…"
    bits = ["**t:** " + trunc(p["title"], 68)]
    if p.get("notes"):
        bits.append("**n:** " + trunc(p["notes"], 60))
    if p.get("due"):
        bits.append("**due:** " + p["due"])
    if p.get("label"):
        bits.append("**lbl:** " + ", ".join("%s=%s" % kv for kv in p["label"].items()))
    return "<br>".join(bits)


def write_scoring(parses):
    """Replace the ground_truth cell (column 3) for each labelled row."""
    with open(SCORING, encoding="utf-8") as fh:
        lines = fh.readlines()
    filled = 0
    for i, line in enumerate(lines):
        m = re.match(r"^\| (\S+) \|", line)
        if not m or m.group(1) not in parses:
            continue
        # split(" | ") folds the leading pipe into cols[0], so the columns are
        # 0:id 1:raw 2:ground_truth 3:nano 4:hosted -- ground_truth is [2].
        cols = line.rstrip("\n").split(" | ")
        if len(cols) < 5 or cols[2].strip() != "_TODO_":
            continue  # already filled or an unexpected shape -- don't clobber
        cols[2] = cell(parses[m.group(1)])
        lines[i] = " | ".join(cols) + "\n"
        filled += 1
    with open(SCORING, "w", encoding="utf-8") as fh:
        fh.writelines(lines)
    return filled


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--worksheet", default=WORKSHEET)
    ap.add_argument("--check", action="store_true", help="validate only; write nothing")
    args = ap.parse_args()

    with open(SCHEMA, encoding="utf-8") as fh:
        schema = json.load(fh)
    with open(CORPUS, encoding="utf-8") as fh:
        corpus_ids = [json.loads(l)["id"] for l in fh if l.strip()]

    errs = []
    parses, unlabelled = build(parse_worksheet(args.worksheet), corpus_ids, errs)
    for cid, p in parses.items():
        validate(p, schema, cid, errs)

    sys.stderr.write(f"{len(parses)}/{len(corpus_ids)} captures labelled\n")
    if unlabelled:
        sys.stderr.write(f"{len(unlabelled)} still unlabelled (left as _TODO_): "
                         + ", ".join(unlabelled) + "\n")
    if errs:
        sys.stderr.write(f"{len(errs)} problem(s):\n")
        for e in errs:
            sys.stderr.write(f"  - {e}\n")
        sys.stderr.write("Nothing written. Fix the worksheet and re-run.\n")
        return 1
    if args.check:
        sys.stderr.write("worksheet is valid (--check: nothing written)\n")
        return 0

    with open(OUT, "w", encoding="utf-8") as fh:
        for cid in corpus_ids:
            if cid in parses:
                fh.write(json.dumps({"id": cid, "parse": parses[cid]}, ensure_ascii=False) + "\n")
    filled = write_scoring(parses)
    sys.stderr.write(f"wrote {len(parses)} labels to {OUT}; filled {filled} cell(s) in scoring.md\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
