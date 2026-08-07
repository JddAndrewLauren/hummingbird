# Capture-parse bake-off

Prototype for [#42](https://github.com/JddAndrewLauren/hummingbird/issues/42) (under
[#41](https://github.com/JddAndrewLauren/hummingbird/issues/41) layer 2). **Decides one
thing:** whether on-device Gemini Nano can parse real captures well enough to be trusted
**offline** — or whether offline capture should stay raw-text-only until sync.

"Capture-parse" = turn one raw capture (dictated or typed) into the local store's
task-field schema, deterministically, so it can queue offline and sync later. The sweeper
already does the trivial version of this (title verbatim, notes→description, due dropped —
`docs/sweeper.md`); a real parser has to *find* the title inside messy dictation and decide
whether to infer more.

## What's measured

Same corpus, same `prompt.md`, same `schema.json`, two parsers — scored against a blind
human ground truth on:

- **Structured-field accuracy** — `title` fidelity, `notes` capture, and (stretch)
  inferred `due` / `label` correctness, per field.
- **Failure-mode tally** — dropped content, hallucinated fields, garbled dictation, wrong
  title/notes split, multi-item captures collapsed. Tags defined in `scoring.md`.

A few dozen captures, one table a human reads in a minute. A prototype, not a benchmark.

## Files

| file | what it is |
| --- | --- |
| `schema.json` | The parse target. Core `title`+`notes`; `due` and `label` are clearly-optional **stretch** probes — the places a small model is most likely to fail. Real JSON Schema, usable for schema-constrained output. |
| `prompt.md` | The single shared parse prompt given to **both** parsers verbatim. Identical wording is the point. |
| `corpus.jsonl` | The captures, one JSON object per line: `{"id","raw","source"}`. **⚠ Currently placeholder** — see below. |
| `run_hosted.py` | Stdlib-only runner. `--emit-prompts` builds the per-capture prompts to send the hosted model; `--merge` validates the replies against `schema.json` and writes `hosted_results.jsonl`. No API key, no network — the hosted-runner service from #41 doesn't exist yet, so it reads replies from a file/stdin. |
| `hosted_results.jsonl` | The hosted-baseline parses, `{"id","parse"}` per line. Produced by a hosted Claude parsing the corpus directly with `prompt.md` + `schema.json` — a genuine hosted-baseline sample. |
| `scoring.md` | The scoring sheet: one row per capture, `hosted` pre-filled, `ground_truth` and `nano` left as `_TODO_`, plus how-to-score and the totals footer. |

## Corpus status — placeholder, must be replaced

No Linear API key was present at `~/.config/linear/api-key` when this harness was built, so
the corpus was **not** seeded from real Triage history. `corpus.jsonl` holds 24 realistic
**placeholder** captures instead — deliberately including the messy cases the ticket calls
out: run-on dictation, half-sentences, "remind me to…", multiple items in one utterance,
no-verb fragments, garbled dictation.

**Before the real run, replace or augment the corpus with real data:**

- Pull recent Triage / recent issue titles as real seed captures. With a key present, the
  existing `.claude/skills/next-up-personal/scripts/linear.sh survey` (endpoint
  `https://api.linear.app/graphql`, `Authorization: <key>` raw — no `Bearer`) already
  returns issue titles; take a few dozen recent ones.
- Add a **fresh batch of dictation** through the real phone→Gemini→Tasks path, so the
  corpus reflects actual transcription noise, not typed approximations of it.

Keep the `{"id","raw","source"}` shape. Then re-run both sides.

## How to run

### Hosted side (no phone, no human)

```sh
# 1. build the prompts to send the hosted model
./run_hosted.py --emit-prompts > prompts.jsonl

# 2. send each record's `prompt` to the hosted model (the same hosted Claude the
#    layer-1 runner uses), requesting schema-constrained output against schema.json.
#    Collect replies as JSONL: {"id": "...", "parse": {...}}  (bare fields also accepted).

# 3. validate + write hosted_results.jsonl
./run_hosted.py --merge --responses replies.jsonl
```

`hosted_results.jsonl` is already populated from a hosted-Claude pass over the placeholder
corpus, so the baseline column in `scoring.md` is filled and the scoring flow is
demonstrable today. Re-run steps 1–3 after the corpus is swapped for real captures.

### On-device side (needs the phone — human only)

Run Gemini Nano (nano-v3) via the ML Kit GenAI **Prompt API** on the **Pixel 10 Pro Fold**,
schema-constrained against `schema.json`, with the **identical** `prompt.md`. **Airplane
mode on** for the whole run, to prove it's genuinely offline. Paste each parse into the
`nano` column of `scoring.md`.

## What the human still has to do

Everything that needs the physical phone or human judgment — left as explicit `_TODO_`s,
**not** invented here:

1. **Swap the corpus for real captures** (see *Corpus status* above) and re-run the hosted
   side. Placeholder data must not decide a real trust question.
2. **Blind ground truth.** For each capture, hand-write the correct parse in
   `scoring.md`'s `ground_truth` column **from the raw text alone, before looking at either
   model's output.** Blind-first or the score anchors to whatever the models produced.
3. **The Nano run.** Pixel 10 Pro Fold, airplane mode, identical prompt + schema; fill the
   `nano` column.
4. **Score and tally.** Mark the ✓ columns and failure-mode tags, fill the totals footer,
   and write the verdict — which feeds the #41 layer-2 decision (trusted alone / guard-railed
   / prefer-hosted-when-online / dropped).

## Constraints (kept)

Stdlib-only Python, no new deps (sweeper ethos). Nothing here touches `sweep.py` or any
sweeper invariant. The Nano column and the blind ground-truth column are TODOs by design —
no Nano outputs and no ground-truth labels were invented.
