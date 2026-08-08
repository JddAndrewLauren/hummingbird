# Runbook — the human's path through the bake-off

Everything you personally have to do to resolve
[#42](https://github.com/JddAndrewLauren/hummingbird/issues/42), in order, with the agent
handoffs marked. Total hands-on time: roughly **5–10 min talking, 30–45 min labelling,
one app run, 10 min reviewing**. Agents do everything between those.

Rule of thumb throughout: **you produce inputs (voice, judgment); agents do bookkeeping.**

---

## Phase 1 — Dictate the messy batch (~5–10 min, phone in hand)

**Why you:** the corpus needs real speech-recognition + Gemini task-extraction noise.
Typed text — anyone's — is exactly the placeholder data this replaces.

**How:** capture via voice to Gemini exactly as you normally would ("remind me to…" /
"add a task…"), so each lands in Google Tasks and the sweeper carries it to Triage.

1. **Note the clock time before you start.** The batch is identified afterwards by its
   `createdAt` window — no prefixes to dictate, no cleanup bookkeeping for you.
2. Work through the scenario list below — **improvise in your own words, don't read
   lines verbatim.** Scripted-then-read text is half-fake; the skeletons only show the
   *shape* of mess wanted. Real errands from your actual life beat invented ones.
3. Don't self-edit. Filler, trailing off, changing your mind mid-sentence — that's the
   payload, not a mistake. If Gemini mangles one, **leave it**; mangling is data.
4. Do a few from the **watch** too, if convenient (different mic, different noise).
5. Note the clock time when done.

### Scenario list (~20–24 captures; counts are targets, not law)

| # | shape | skeleton to riff on (don't read verbatim) |
| --- | --- | --- |
| 4× | run-on with filler | "okay so I need to ⟨errand⟩ and also ⟨second thing⟩ and maybe ⟨third⟩ while I'm at it" |
| 4× | multi-item, one utterance | "⟨thing one⟩, ⟨thing two⟩, and I still haven't ⟨thing three⟩" |
| 3× | half-sentence / no verb | "the thing with the ⟨noun phrase⟩" · "⟨person⟩'s ⟨object⟩" |
| 3× | "remind me to…" | "remind me to ⟨small chore⟩ ⟨when-phrase⟩" |
| 2× | self-correction | "schedule the — no wait — reschedule ⟨meeting⟩ to ⟨day⟩" |
| 3× | explicit temporal (due-bait) | "…by Friday" · "…the fifteenth" · "…tonight" |
| 2× | trailing off / truncated | "book the flights for" ⟨stop talking⟩ |
| 2–3× | clean one-liner (control) | a normal, tidy capture — the easy case must stay represented |

### Hand off (one message, any session)

> "Dictation batch is in Triage, window ⟨start⟩–⟨end⟩."

The agent then: pulls the window from Linear, imports as `real-*` rows with `origin`
pointers, re-runs the hosted column (`run_hosted.py` steps in `README.md`), retires
redundant `ph-*` rows, updates `scoring.md`, and cancels the batch issues out of Triage
so they don't pollute `/next-up-personal`. Wait ~15 min after your last capture (sweeper
cadence) before sending it.

---

## Phase 2 — Blind ground truth (~30–45 min, no phone needed)

**Why you:** the hosted column was written by Claude; if an agent also writes the
"correct" answers, ground truth correlates with one contestant and the bake-off is
rigged. **Why blind:** reading a model's parse first anchors your label to it.

1. Ask any session: **"emit the ground-truth worksheet."** It produces a file with only
   `id` + `raw` — no model columns anywhere on screen. (Equivalent one-liner:
   `python3 -c "import json;[print(f'\n== {r[\"id\"]} ==\n{r[\"raw\"]}\nGT: ') for r in map(json.loads, open('corpus.jsonl'))]" > ground_truth_worksheet.txt`)
2. **Do not open `scoring.md` or `hosted_results.jsonl`** until the worksheet is done.
3. For each raw, write the parse *you* would want in the local store:
   - **title** — the one actionable line, filler stripped, short and imperative.
   - **notes** — everything else worth keeping; blank if none.
   - **due** — only if the raw carries an explicit temporal phrase; date if resolvable,
     else the phrase verbatim; otherwise leave unset. Never guess.
   - **label** — `context`/`energy`/`size` only where clearly implied; skip freely.
   - **multi-item** — clearest single action as title, the rest verbatim in notes.
   - **unparseable** — whole raw as title, nothing else. Kept beats clever.
4. Hand the worksheet back: the agent merges it into `scoring.md`'s `ground_truth`
   column. After this point you may look at anything.

---

## Phase 3 — The Nano run (phone, airplane mode)

**Why you:** the phone is in your pocket, not mine.

**Preferred path — the runner app** (ask an agent to build it if not yet built: a
minimal Android app that loads `corpus.jsonl`, runs each capture through Gemini Nano
via the ML Kit GenAI Prompt API, schema-constrained with `prompt.md` + `schema.json`,
and writes `nano_results.jsonl`):

1. Install the APK on the **Pixel 10 Pro Fold**.
2. **Airplane mode on. Wi-Fi off too.** Leave it on for the whole run — offline-ness is
   the claim under test.
3. Run once end-to-end; if a capture errors, let the app record the error as that row's
   result (an on-device failure is a result, not a retry candidate).
4. Airplane mode off, share `nano_results.jsonl` back (any channel; paste into a #42
   comment works).

**Fallback path (no app):** AI Studio's on-device playground or a scratch project with
`com.google.mlkit:genai-prompt`, model nano-v3; paste each prompt from
`run_hosted.py --emit-prompts` by hand. Tedious but honest — same rules about airplane
mode and recording failures verbatim.

The agent then merges the Nano column into `scoring.md`.

---

## Phase 4 — Score and call it (~10 min review)

Mechanical comparison is agent work now that ground truth exists; **the verdict is
yours.**

1. Ask: **"score the bake-off."** The agent fills the ✓ columns (`Y`/`~`/`N` per
   `scoring.md`'s rules), tags failure modes, and tallies the footer — flagging any row
   where the comparison was a judgment call rather than mechanical.
2. Review the flagged rows; overrule anywhere you disagree.
3. Write the **verdict** — one of the four postures, weighing the `real-*` rows above
   the `ph-*` rows:
   - **trusted alone** — Nano ≈ hosted on title/notes, zero hallucinated fields on real rows.
   - **guard-railed** — usable but fallible: parsed fields marked provisional for triage review.
   - **hosted-preferred** — hosted clearly better; Nano only as the offline fallback, re-parse on sync.
   - **dropped** — Nano misses badly; offline captures stay raw text until sync. A legitimate outcome.

   All four are safe under the locked invariant: parse is additive, never destructive —
   the raw string always survives.

### Hand off — resolution

> "Verdict is ⟨posture⟩, resolve the ticket."

The agent posts the resolution comment on #42, closes it, appends the pointer to map
[#35](https://github.com/JddAndrewLauren/hummingbird/issues/35)'s Decisions-so-far, and
records the layer-2 outcome back into
[#41](https://github.com/JddAndrewLauren/hummingbird/issues/41). Nano-specific
integration work unblocks (or evaporates) accordingly.

---

## State at a glance

| step | owner | status |
| --- | --- | --- |
| Harness (schema/prompt/corpus/runner/sheet) | agent | ✅ built |
| Real Triage history seeded (5 captures) | agent | ✅ done 2026-08-07 |
| Hosted baseline column | agent | ✅ filled (re-runs cheap; later via the runner endpoint per #41) |
| Messy dictation batch | **you** | ⬜ Phase 1 |
| Corpus import + hosted re-run | agent | ⬜ after Phase 1 |
| Blind ground truth | **you** | ⬜ Phase 2 |
| Nano runner app | agent | ⬜ on request |
| Nano run on the Pixel | **you** | ⬜ Phase 3 |
| Mechanical scoring + tally | agent | ⬜ after Phases 2–3 |
| Verdict + resolution | **you** → agent | ⬜ Phase 4 |
