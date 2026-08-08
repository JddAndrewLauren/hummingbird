#!/usr/bin/env python3
"""Hosted-baseline runner for the capture-parse bake-off.

Stdlib only (matches the sweeper ethos: no third-party deps, one-shot, readable).

The hosted runner service from #41 does not exist yet, so this script does NOT
hard-wire an Anthropic key or make a network call. It brackets that missing seam
from both sides:

  1. `--emit-prompts`  builds, for every corpus capture, the exact prompt to send
     to the hosted model (prompt.md with the schema and raw capture interpolated,
     schema-constrained output requested against schema.json). One JSONL record
     per capture on stdout (or --out).  This is the contract for what goes to the
     hosted parser — feed these to the same hosted Claude the layer-1 runner will
     use, or paste them by hand.

  2. `--merge`         reads the model's replies back (from --responses FILE or
     stdin), validates each against schema.json, and writes hosted_results.jsonl.

Response line format (JSONL), either shape accepted:
    {"id": "ph-01", "parse": {"title": "...", ...}}
    {"id": "ph-01", "title": "...", "notes": "..."}      # bare fields, wrapped
The output is always the first shape: {"id": ..., "parse": {...}}.

Examples:
    ./run_hosted.py --emit-prompts > prompts.jsonl
    ./run_hosted.py --merge --responses replies.jsonl        # -> hosted_results.jsonl
    cat replies.jsonl | ./run_hosted.py --merge              # stdin also works

hosted_results.jsonl in this directory was produced by a hosted Claude parsing
the corpus directly; re-running --merge on fresh replies overwrites it.
"""

import argparse
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
CORPUS = os.path.join(HERE, "corpus.jsonl")
PROMPT = os.path.join(HERE, "prompt.md")
SCHEMA = os.path.join(HERE, "schema.json")
RESULTS = os.path.join(HERE, "hosted_results.jsonl")


def read_jsonl(path):
    rows = []
    with open(path, encoding="utf-8") as fh:
        for n, line in enumerate(fh, 1):
            line = line.strip()
            if not line:
                continue
            try:
                rows.append(json.loads(line))
            except json.JSONDecodeError as e:
                sys.exit(f"{path}:{n}: bad JSON: {e}")
    return rows


# --- minimal JSON Schema check ------------------------------------------------
# Covers exactly the keywords schema.json uses: type (incl. union list), required,
# properties, additionalProperties:false, enum, minLength, and for arrays minItems
# plus a single `items` subschema. Not a general validator.

def validate(node, schema, path, errs):
    t = schema.get("type")
    if t is not None:
        types = t if isinstance(t, list) else [t]
        if not any(_is_type(node, x) for x in types):
            errs.append(f"{path}: expected type {t}, got {type(node).__name__}")
            return  # type wrong -> deeper checks are noise
    if "enum" in schema and node not in schema["enum"]:
        errs.append(f"{path}: {node!r} not in enum {schema['enum']}")
    if isinstance(node, str) and "minLength" in schema:
        if len(node) < schema["minLength"]:
            errs.append(f"{path}: shorter than minLength {schema['minLength']}")
    if isinstance(node, list):
        if len(node) < schema.get("minItems", 0):
            errs.append(f"{path}: fewer than minItems {schema['minItems']}")
        if isinstance(schema.get("items"), dict):
            for i, el in enumerate(node):
                validate(el, schema["items"], f"{path}[{i}]", errs)
    if isinstance(node, dict) and schema.get("type") == "object":
        for req in schema.get("required", []):
            if req not in node:
                errs.append(f"{path}: missing required '{req}'")
        props = schema.get("properties", {})
        if schema.get("additionalProperties") is False:
            for k in node:
                if k not in props:
                    errs.append(f"{path}: unexpected property '{k}'")
        for k, v in node.items():
            if k in props:
                validate(v, props[k], f"{path}.{k}", errs)


def _is_type(node, name):
    return {
        "object": isinstance(node, dict),
        "array": isinstance(node, list),
        "string": isinstance(node, str),
        "number": isinstance(node, (int, float)) and not isinstance(node, bool),
        "integer": isinstance(node, int) and not isinstance(node, bool),
        "boolean": isinstance(node, bool),
        "null": node is None,
    }.get(name, False)


# --- modes --------------------------------------------------------------------

def emit_prompts(out):
    corpus = read_jsonl(CORPUS)
    with open(PROMPT, encoding="utf-8") as fh:
        template = fh.read()
    with open(SCHEMA, encoding="utf-8") as fh:
        schema_text = fh.read().rstrip()
    for cap in corpus:
        prompt = template.replace("{{SCHEMA}}", schema_text).replace("{{RAW}}", cap["raw"])
        rec = {"id": cap["id"], "source": cap.get("source"), "raw": cap["raw"], "prompt": prompt}
        out.write(json.dumps(rec, ensure_ascii=False) + "\n")


def merge(responses_path, out_path):
    with open(SCHEMA, encoding="utf-8") as fh:
        schema = json.load(fh)
    corpus_ids = [c["id"] for c in read_jsonl(CORPUS)]

    if responses_path:
        replies = read_jsonl(responses_path)
    else:
        replies = [json.loads(l) for l in sys.stdin if l.strip()]

    by_id = {}
    for r in replies:
        rid = r.get("id")
        if rid is None:
            sys.exit("response record missing 'id'")
        parse = r["parse"] if "parse" in r else {k: v for k, v in r.items() if k != "id"}
        by_id[rid] = parse

    errors, written = [], []
    for cid in corpus_ids:
        if cid not in by_id:
            errors.append(f"{cid}: no hosted response")
            continue
        errs = []
        validate(by_id[cid], schema, cid, errs)
        errors.extend(errs)
        written.append({"id": cid, "parse": by_id[cid]})

    for extra in set(by_id) - set(corpus_ids):
        errors.append(f"{extra}: response has no matching corpus capture")

    with open(out_path, "w", encoding="utf-8") as fh:
        for rec in written:
            fh.write(json.dumps(rec, ensure_ascii=False) + "\n")

    sys.stderr.write(f"wrote {len(written)} parses to {out_path}\n")
    if errors:
        sys.stderr.write(f"{len(errors)} schema/coverage issue(s):\n")
        for e in errors:
            sys.stderr.write(f"  - {e}\n")
        return 1
    sys.stderr.write("all parses valid against schema.json\n")
    return 0


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    g = ap.add_mutually_exclusive_group(required=True)
    g.add_argument("--emit-prompts", action="store_true", help="write per-capture prompts (JSONL) for the hosted model")
    g.add_argument("--merge", action="store_true", help="validate hosted replies and write hosted_results.jsonl")
    ap.add_argument("--responses", help="JSONL of hosted replies (default: stdin) for --merge")
    ap.add_argument("--out", help="output path (default: stdout for --emit-prompts, hosted_results.jsonl for --merge)")
    args = ap.parse_args()

    if args.emit_prompts:
        if args.out:
            with open(args.out, "w", encoding="utf-8") as fh:
                emit_prompts(fh)
        else:
            emit_prompts(sys.stdout)
        return 0
    return merge(args.responses, args.out or RESULTS)


if __name__ == "__main__":
    sys.exit(main())
