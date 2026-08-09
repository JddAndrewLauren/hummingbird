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

1. **`./emit_worksheet.py`** — writes `ground_truth_worksheet.txt`: `id` + `raw` plus four
   blank fields per capture, no model column anywhere on screen. It reads `corpus.jsonl`
   and nothing else, so it can't leak one. (Already emitted for the current 42-row corpus;
   re-run it only if the corpus changes. It refuses to overwrite a worksheet with answers
   in it.) Each entry carries a `spoken:` date — resolve relative phrases against *that*,
   not today.
2. **Do not open `scoring.md` or `hosted_results.jsonl`** until the worksheet is done.
3. For each raw, write the parse *you* would want in the local store:
   - **title** — the one actionable line, filler stripped, short and imperative. On a
     multi-action capture it's the **first** action stated, not the most important.
   - **items** — only when the capture holds several actions: one `- ` bullet per action,
     in the order spoken, the first repeating the title's. Blank otherwise; never a
     one-item list. This doesn't split the capture into three tasks — it stays one task,
     and splitting is a Triage call you make later.
   - **notes** — everything else worth keeping that isn't an action; blank if none.
   - **due** — only if the raw carries an explicit temporal phrase; date if resolvable,
     else the phrase verbatim; otherwise leave unset. Never guess.
   - **label** — `context`/`energy`/`size` only where clearly implied; skip freely.
   - **multi-item** — every action in `items`, first one also the title. Nothing lost.
   - **unparseable** — whole raw as title, nothing else. Kept beats clever.
4. **`./merge_worksheet.py`** (or hand the worksheet back to any session) — validates every
   label against `schema.json`, writes `ground_truth.jsonl`, and fills `scoring.md`'s
   `ground_truth` column. `--check` validates without writing. Partial worksheets are fine:
   unlabelled rows are listed and left as `_TODO_`, and re-running never clobbers a cell
   you've already filled — so you can label in several sittings. After this point you may
   look at anything.

---

## Phase 3 — The Nano run (phone, airplane mode)

**Why you:** the phone is in your pocket, not mine.

**Preferred path — the runner app**, `nano-runner/` (built; full operator runbook in
`nano-runner/README.md`). It bundles `corpus.jsonl`, assembles each prompt byte-identically
to `run_hosted.py --emit-prompts` from `prompt.md` + `schema.json`, runs it through Gemini
Nano via the ML Kit GenAI Prompt API, and writes `nano_results.jsonl`:

1. Install the APK on the **Pixel 10 Pro Fold** (`./gradlew :app:assembleDebug`).
2. **On Wi-Fi**, open it once and let the model download finish if it asks — AICore
   provisioning is the one step that can't happen offline.
3. **Airplane mode on. Wi-Fi off too.** Leave it on for the whole run — offline-ness is
   the claim under test, and the app records the system's airplane flag at run start.
4. Tap Start. The screen shows `n / 42` and an error count and **nothing else** — no
   parses, no model text — so this is safe to do before Phase 2. If a capture errors the
   app records the error as that row's result (an on-device failure is a result, not a
   retry candidate) and moves on; finished captures are never re-run if you relaunch.
   If it stops early saying the failures look systemic, or a run gets spoiled by
   something that isn't the model, use **Discard this run and start over**.
5. Airplane mode off, tap Share and send back all three files (`nano_results.jsonl`, the
   verbatim `nano_raw.jsonl` sidecar, and `nano_run_meta.json`).

Note the API asymmetry, recorded on #42: the Prompt API's structured-output path can't
express this schema and would change the prompt, so Nano runs **unconstrained** with the
identical prompt and its output is validated afterwards. The hosted side was not weakened
to match.

**Fallback path (no app):** AI Studio's on-device playground or a scratch project with
`com.google.mlkit:genai-prompt`, model nano-v3; paste each prompt from
`run_hosted.py --emit-prompts` by hand. Tedious but honest — same rules about airplane
mode and recording failures verbatim.

The agent then merges the Nano column into `scoring.md` with `./merge_nano.py` (or you
can: drop `nano_results.jsonl` in this directory and run it).

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
| Messy dictation batch | **you** | ✅ done 2026-08-08 (23 captures, ION-36…ION-58) |
| Corpus import + hosted re-run | agent | ✅ done 2026-08-08 (`real-06…28`; 10 dictated `ph-*` retired; Triage cleared) |
| Blind ground truth | **you** | ⬜ Phase 2 — **the only thing still blocking the verdict** |
| Nano runner app | agent | ✅ built 2026-08-08 (`nano-runner/`, + `merge_nano.py`); v1 run failed on a config bug, fixed and re-armed same day |
| Nano run on the Pixel | **you** | ✅ done 2026-08-08, 42/42 offline on nano-v3 (3 attempts; ~3.6 s median per capture) |
| Mechanical scoring + tally | agent | ⬜ after Phases 2–3 |
| Verdict + resolution | **you** → agent | ⬜ Phase 4 |
