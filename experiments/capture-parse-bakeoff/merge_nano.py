#!/usr/bin/env python3
"""Merge the phone's nano_results.jsonl into scoring.md's `nano` column.

Stdlib only (matches the sweeper ethos). The Nano-side counterpart of
`run_hosted.py --merge` and `merge_worksheet.py`.

    ./merge_nano.py                          # reads nano_results.jsonl
    ./merge_nano.py --results other.jsonl
    ./merge_nano.py --check                  # validate only, write nothing

The runner app (`nano-runner/`) writes two row shapes, and BOTH are results:

    {"id": "ph-01", "parse": {...}}                       # model emitted strict JSON
    {"id": "ph-01", "error": "...", "raw_output": "..."}  # API error, or not JSON

An error is never retried into a success and never dropped -- how often on-device
inference fails is one of the things the bake-off is measuring. Invalid parses are
rendered too (prefixed `**INVALID:**`), because "produced JSON that misses the schema"
is a different failure from "produced no JSON at all", and the scorer needs to see which.

CODE FENCES. Gemini Nano wraps its answer in ```json fences on every capture, though the
shared prompt says "No prose, no code fences"; the hosted baseline never does. Scoring
that as a total miss would collapse two separate findings into one and throw away the
whole parse-quality comparison, so a whole-output fence around well-formed JSON is
unwrapped here, counted separately in the summary, and marked `**FENCED:**` in the cell
so the envelope failure stays visible in the sheet. This happens ONLY in scoring, and
only for an exact whole-output fence -- the phone still records output verbatim, and
prose outside the fence is never repaired.

Every cell is sanitised (`|` escaped, newlines -> `<br>`) before it goes into the
markdown table. Unlike the ground_truth and hosted columns, this text is model output
arriving from a device -- untrusted enough that a stray pipe should not silently
re-shape the table.

Exit code is 1 if anything is off (missing ids, invalid parses, error rows), while
still writing whatever valid state it has -- same convention as run_hosted.py --merge.
Stdout is a pasteable markdown summary for the issue comment; diagnostics go to stderr.
"""

import argparse
import json
import os
import re
import sys

from run_hosted import read_jsonl, validate
from merge_worksheet import cell

HERE = os.path.dirname(os.path.abspath(__file__))
CORPUS = os.path.join(HERE, "corpus.jsonl")
SCHEMA = os.path.join(HERE, "schema.json")
SCORING = os.path.join(HERE, "scoring.md")
RESULTS = os.path.join(HERE, "nano_results.jsonl")

TODO = "_TODO_"

# The runner's wording for "the model answered, but not with one strict JSON object".
NOT_JSON = "model output is not a strict JSON object"


# A ```json ... ``` wrapper around the whole output, and nothing else outside it.
FENCE = re.compile(r"\A\s*```[A-Za-z0-9_-]*\s*\n(?P<body>.*?)\n?\s*```\s*\Z", re.S)


def unfence(text):
    """-> the fenced body, or None if `text` isn't exactly one fenced block.

    Gemini Nano wraps its output in ```json fences even though the shared prompt says
    "No prose, no code fences" -- observed on every capture of the 2026-08-08 run. The
    hosted baseline obeyed. That difference is a real finding and is counted separately.

    But scoring it as a total miss would throw away the substantive comparison: the
    JSON inside is well-formed, and stripping a fence in a production integration is a
    deterministic envelope unwrap, not a semantic repair. So the envelope failure and
    the parse quality are measured as two different things.

    Strictly one whole-output fence: prose outside the fence, or a fence around only
    part of the answer, is NOT unwrapped -- that would be repairing content.
    """
    m = FENCE.match(text)
    return m.group("body") if m else None


def sanitize(text):
    """Model text -> one safe markdown table cell."""
    return text.replace("|", r"\|").replace("\r\n", "\n").replace("\r", "\n").replace("\n", "<br>")


def trunc(s, n=160):
    return s if len(s) <= n else s[:n].rstrip() + "…"


def classify(rows, corpus_ids, schema):
    """-> (cells_by_id, stats, problems).

    cells_by_id holds the rendered markdown for every id the phone reported on.
    """
    cells = {}
    stats = {"valid": 0, "invalid": 0, "error": 0, "fenced": 0}
    problems = []
    seen = set()

    for row in rows:
        cid = row.get("id")
        if cid is None:
            problems.append("a result row has no 'id'")
            continue
        if cid in seen:
            problems.append(f"{cid}: reported twice -- the runner should write each id once")
            continue
        seen.add(cid)

        # A strict-parse failure whose output is just a fenced JSON object is really two
        # facts: "didn't obey the envelope instruction" and "the parse itself was X".
        # Recover the second without losing the first.
        marker = ""
        if "error" in row and row["error"] == NOT_JSON and row.get("raw_output"):
            body = unfence(row["raw_output"])
            if body is not None:
                try:
                    inner = json.loads(body)
                except (ValueError, TypeError):
                    inner = None
                if isinstance(inner, dict):
                    stats["fenced"] += 1
                    problems.append(f"{cid}: output was valid JSON wrapped in a code fence "
                                    "(prompt says no fences) -- scored on the unwrapped parse")
                    row = {"id": cid, "parse": inner}
                    marker = "**FENCED:** "

        if "parse" in row:
            errs = []
            validate(row["parse"], schema, cid, errs)
            if errs:
                # Show BOTH the schema complaint and the content: "emitted JSON that
                # misses the schema" is a different failure from "emitted no JSON", and
                # the scorer has to be able to tell them apart at a glance.
                stats["invalid"] += 1
                problems.extend(errs)
                why = trunc(errs[0].split(": ", 1)[-1], 90)
                cells[cid] = sanitize(
                    marker + "**INVALID:** " + why + "\n" + cell(_titled(row["parse"]))
                )
            else:
                stats["valid"] += 1
                cells[cid] = sanitize(marker + cell(row["parse"]))
        elif "error" in row:
            stats["error"] += 1
            problems.append(f"{cid}: {row['error']}")
            raw = row.get("raw_output")
            detail = trunc(row["error"], 90)
            if raw:
                detail += " · out: " + trunc(raw, 90)
            cells[cid] = sanitize("**ERR:** " + detail)
        else:
            problems.append(f"{cid}: row is neither a parse nor an error")

    for cid in corpus_ids:
        if cid not in seen:
            problems.append(f"{cid}: no nano result")
    for extra in seen - set(corpus_ids):
        problems.append(f"{extra}: result has no matching corpus capture")

    return cells, stats, problems


def _titled(parse):
    """`cell()` assumes a title; an invalid parse may not have one."""
    if isinstance(parse, dict) and isinstance(parse.get("title"), str):
        return parse
    shown = dict(parse) if isinstance(parse, dict) else {"raw": parse}
    shown["title"] = "(no valid title) " + json.dumps(parse, ensure_ascii=False)[:120]
    return shown


def write_scoring(cells, scoring_path):
    """Replace the nano cell (column 3) for each id we have a result for."""
    with open(scoring_path, encoding="utf-8") as fh:
        lines = fh.readlines()
    filled = 0
    for i, line in enumerate(lines):
        m = re.match(r"^\| (\S+) \|", line)
        if not m or m.group(1) not in cells:
            continue
        # split(" | ") folds the leading pipe into cols[0], so the columns are
        # 0:id 1:raw 2:ground_truth 3:nano 4:hosted -- nano is [3]. The totals
        # footer has a `| nano |` row, but "nano" is not a corpus id so it can
        # never match here.
        cols = line.rstrip("\n").split(" | ")
        if len(cols) < 5 or cols[3].strip() != TODO:
            continue  # already filled or an unexpected shape -- don't clobber
        cols[3] = cells[m.group(1)]
        lines[i] = " | ".join(cols) + "\n"
        filled += 1
    with open(scoring_path, "w", encoding="utf-8") as fh:
        fh.writelines(lines)
    return filled


def summary(stats, corpus_ids, problems, filled):
    """A pasteable markdown block for the issue comment."""
    total = len(corpus_ids)
    reported = stats["valid"] + stats["invalid"] + stats["error"]
    out = [
        "**Nano column merged into `scoring.md`.**",
        "",
        f"- captures: {total}, reported by the phone: {reported}",
        f"- schema-valid parses: {stats['valid']}",
        f"- parses that missed the schema: {stats['invalid']}",
        f"- rows the model/API failed outright: {stats['error']}",
        f"- arrived wrapped in a code fence: {stats['fenced']}"
        f" (prompt forbids fences; the hosted baseline emitted none) — unwrapped for"
        f" scoring and marked `**FENCED:**`",
        f"- cells filled this run: {filled}",
    ]
    if problems:
        out += ["", f"{len(problems)} issue(s) -- see stderr for the full list."]
    return "\n".join(out) + "\n"


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--results", default=RESULTS, help="nano_results.jsonl from the phone")
    ap.add_argument("--scoring", default=SCORING, help="scoring sheet to fill")
    ap.add_argument("--check", action="store_true", help="validate only; write nothing")
    args = ap.parse_args()

    with open(SCHEMA, encoding="utf-8") as fh:
        schema = json.load(fh)
    corpus_ids = [c["id"] for c in read_jsonl(CORPUS)]

    if not os.path.isfile(args.results):
        sys.exit(f"{args.results}: not found -- pull it off the phone first (see "
                 "nano-runner/README.md)")

    cells, stats, problems = classify(read_jsonl(args.results), corpus_ids, schema)

    filled = 0
    if args.check:
        sys.stderr.write("--check: nothing written\n")
    else:
        filled = write_scoring(cells, args.scoring)

    sys.stdout.write(summary(stats, corpus_ids, problems, filled))

    sys.stderr.write(
        f"{stats['valid']} valid, {stats['invalid']} invalid, {stats['error']} error row(s); "
        f"filled {filled} cell(s)\n"
    )
    if problems:
        sys.stderr.write(f"{len(problems)} issue(s):\n")
        for p in problems:
            sys.stderr.write(f"  - {p}\n")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
