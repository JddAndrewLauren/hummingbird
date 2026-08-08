# Scoring sheet

One row per capture. The **hosted** column is pre-filled from `hosted_results.jsonl`.
The **ground_truth** and **nano** columns are `_TODO_` — they need, respectively, a human
and the phone (see `README.md`). Do not fill `ground_truth` after reading the model
columns: label blind first, or the score anchors to what the models happened to produce.

## How to score

1. **Blind ground truth first.** For each `raw`, write the correct parse in `ground_truth`
   from the raw text alone, without looking at the `hosted` or `nano` cells. Use the same
   schema (`title` / `notes` / optional `due` / optional `label`).
2. **Run Nano** on the Pixel (airplane mode) with the identical `prompt.md` + `schema.json`,
   and paste each parse into the `nano` cell.
3. **Mark the three ✓ columns per parser** — score `nano` and `hosted` each against
   `ground_truth`. Use `Y` (match), `~` (partial), `N` (miss); write it as `nano/hosted`
   (e.g. `~/Y`). `stretch✓` covers `due` **and** `label` together — mark `N/A` when the
   ground truth has neither.
4. **Tag failure modes** with the codes below, prefixed by which parser, e.g.
   `nano:MULTI, hosted:—`. `—` = clean.
5. **Totals go in the footer** — per-parser ✓ counts and a failure-mode tally. That table
   is the whole result; keep it to something a human reads in a minute.

## Failure-mode tags

| code | meaning |
| --- | --- |
| `DROP` | dropped content — something in the raw is missing from the parse entirely |
| `HALL` | hallucinated field — a fact, date, name, or label not present in the raw |
| `GARB` | garbled dictation — filler / stutters / mis-transcription left uncleaned in a field |
| `SPLIT` | wrong title/notes split — the actionable line and the remainder divided wrongly |
| `MULTI` | multi-item capture collapsed — several actions flattened into one, others lost |

**The real captures carry the baiting now.** `real-01…05` are the original Triage history
(2026-08-07) — all short and clean. `real-06…28` are the fresh dictation batch
(2026-08-08), spoken through the real phone→Gemini→Tasks path and deliberately messy:
multi-item (`real-06, 07, 24, 25, 26, 27, 28`), truncated / no-verb fragment
(`real-11, 12, 21, 22, 23`), garbled or self-correcting dictation
(`real-14, 16, 17, 20, 25, 27`), inferred-due bait
(`real-13, 14, 15, 16, 17, 18, 20`). `real-19` is a verbatim duplicate of `ph-04` — a free
consistency check on each parser.

The ten **dictated** placeholders were retired when the real batch landed (2026-08-08):
fake dictation noise has no business standing in for the real thing once the real thing
exists. What remains is 14 `ph-*` rows, all **typed** captures — a class the dictation
batch by definition can't cover — baiting multi-item (`ph-05, 22`), no-verb / fragment
(`ph-03, 06, 11, 14, 17, 20`) and inferred-due (`ph-01, 18`).

Read the totals with the `real-*` subset in view: 28 of 42 rows, covering every failure
mode, so it can carry the verdict on its own.

**Due-date resolution is relative to the run date.** The hosted column for `ph-*` /
`real-01…05` was produced on 2026-08-07 and for `real-06…28` on 2026-08-08. Resolve the
ground truth (and the Nano run) against the same per-row date, or relative phrases will
mis-score by a day.

## Sheet

| id | raw | ground_truth | nano | hosted | title✓ | notes✓ | stretch✓ | failure_modes |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| ph-01 | Email Dana the Q3 roadmap draft before Thursday standup | _TODO_ | _TODO_ | **t:** Email Dana the Q3 roadmap draft<br>**n:** Before Thursday standup.<br>**due:** 2026-08-13<br>**lbl:** context=@computer, size=quick |  |  |  |  |
| ph-03 | the thing with the fly deploy token | _TODO_ | _TODO_ | **t:** The thing with the Fly deploy token<br>**n:** Fragment — no action stated; kept verbatim.<br>**lbl:** context=@computer | | | | |
| ph-04 | remind me to water the plants when I get home | _TODO_ | _TODO_ | **t:** Water the plants<br>**lbl:** context=@home, energy=low, size=quick | | | | |
| ph-05 | buy a birthday card for mom, book the oil change, and cancel the gym … | _TODO_ | _TODO_ | **t:** Buy a birthday card for mom<br>**n:** Also: book the oil change; cancel the gym trial before it c… | | | | |
| ph-06 | quarterly taxes | _TODO_ | _TODO_ | **t:** Quarterly taxes | | | | |
| ph-08 | Draft the ADR for the runner endpoint — separate Fly app, not on the … | _TODO_ | _TODO_ | **t:** Draft the ADR for the runner endpoint<br>**n:** Separate Fly app, not on the sweeper; single bearer token; …<br>**lbl:** context=@computer, size=deep | | | | |
| ph-11 | dentist | _TODO_ | _TODO_ | **t:** Dentist | | | | |
| ph-13 | Read the Foundation Models guided-generation docs before starting the… | _TODO_ | _TODO_ | **t:** Read the Foundation Models guided-generation docs<br>**n:** Before starting the iPad prototype.<br>**lbl:** context=@computer, size=medium | | | | |
| ph-14 | book flights for | _TODO_ | _TODO_ | **t:** Book flights for<br>**n:** Truncated capture — destination missing; kept verbatim. | | | | |
| ph-17 | leak under the bathroom sink getting worse | _TODO_ | _TODO_ | **t:** Leak under the bathroom sink getting worse<br>**n:** No verb stated; kept close to raw rather than inventing an …<br>**lbl:** context=@home | | | | |
| ph-18 | Submit the reimbursement for the conference hotel by end of month | _TODO_ | _TODO_ | **t:** Submit the reimbursement for the conference hotel<br>**n:** By end of month.<br>**due:** 2026-08-31<br>**lbl:** context=@computer, size=quick | | | | |
| ph-20 | milk eggs bread coffee filters | _TODO_ | _TODO_ | **t:** Milk, eggs, bread, coffee filters<br>**n:** Shopping list — no verb; items kept in the title.<br>**lbl:** context=@errands, size=quick | | | | |
| ph-22 | car registration and also the smog check | _TODO_ | _TODO_ | **t:** Car registration<br>**n:** Also: the smog check (likely a prerequisite).<br>**lbl:** context=@errands | | | | |
| ph-23 | Idea: a raw-text fallback for capture-parse that never loses the utte… | _TODO_ | _TODO_ | **t:** Idea: raw-text fallback for capture-parse that never loses the utterance<br>**n:** "Couldn't parse, kept verbatim."<br>**lbl:** context=@computer, size=medium | | | | |
| real-01 | Testing with voice | _TODO_ | _TODO_ | **t:** Testing with voice | | | | |
| real-02 | Call the dentist for new aligners | _TODO_ | _TODO_ | **t:** Call the dentist for new aligners<br>**lbl:** context=@phone, size=quick | | | | |
| real-03 | order clothes | _TODO_ | _TODO_ | **t:** Order clothes<br>**lbl:** context=@computer | | | | |
| real-04 | golf practice | _TODO_ | _TODO_ | **t:** Golf practice | | | | |
| real-05 | Setup hermes agent | _TODO_ | _TODO_ | **t:** Setup hermes agent<br>**lbl:** context=@computer | | | | |
| real-06 | I need to return that Amazon package and also buy printer paper. May… | _TODO_ | _TODO_ | **t:** Return the Amazon package<br>**n:** Also: buy printer paper; maybe stop at the pharmacy.<br>**lbl:** context=@errands, size=quick | | | | |
| real-07 | Okay so I need to pick up the dry cleaning and also get dog food and… | _TODO_ | _TODO_ | **t:** Pick up the dry cleaning<br>**n:** Also: get dog food; maybe grab some coffee while I'm at it.<br>**lbl:** context=@errands, size=quick | | | | |
| real-08 | Review the draft contract for the meeting | _TODO_ | _TODO_ | **t:** Review the draft contract for the meeting<br>**lbl:** context=@computer, size=medium | | | | |
| real-09 | Buy a new USBC cable | _TODO_ | _TODO_ | **t:** Buy a new USB-C cable<br>**lbl:** context=@errands, size=quick | | | | |
| real-10 | Schedule the oil change for the car | _TODO_ | _TODO_ | **t:** Schedule the oil change for the car<br>**lbl:** context=@phone, size=quick | | | | |
| real-11 | Email Sarah about the | _TODO_ | _TODO_ | **t:** Email Sarah about the<br>**n:** Truncated capture — subject missing; kept verbatim.<br>**lbl:** context=@computer | | | | |
| real-12 | Book the the flights for | _TODO_ | _TODO_ | **t:** Book the flights for<br>**n:** Truncated capture — destination missing; dictation stutter (… | | | | |
| real-13 | Pick up the prescription tonight | _TODO_ | _TODO_ | **t:** Pick up the prescription<br>**n:** Tonight.<br>**due:** 2026-08-08<br>**lbl:** context=@errands, size=quick | | | | |
| real-14 | We're new the car registration by the 15th | _TODO_ | _TODO_ | **t:** Renew the car registration<br>**n:** By the 15th. Raw reads "We're new the car registration" — "W…<br>**due:** 2026-08-15<br>**lbl:** context=@computer | | | | |
| real-15 | Send the revised proposal to James by Friday | _TODO_ | _TODO_ | **t:** Send the revised proposal to James<br>**n:** By Friday.<br>**due:** 2026-08-14<br>**lbl:** context=@computer, size=quick | | | | |
| real-16 | Move the dentist appointment. Actually no reschedule the cleaning to… | _TODO_ | _TODO_ | **t:** Reschedule the cleaning to the 21st<br>**n:** Self-correction in the raw: started as "move the dentist app…<br>**due:** 2026-08-21<br>**lbl:** context=@phone, size=quick | | | | |
| real-17 | Schedule the actually no it reschedule that budget meeting to Tuesda… | _TODO_ | _TODO_ | **t:** Reschedule the budget meeting<br>**n:** To Tuesday. Self-correction in the raw: "Schedule the — actu…<br>**due:** 2026-08-11<br>**lbl:** context=@computer, size=quick | | | | |
| real-18 | Remind me to cancel that free trial next Thursday | _TODO_ | _TODO_ | **t:** Cancel the free trial<br>**n:** Next Thursday.<br>**due:** next Thursday<br>**lbl:** context=@computer, size=quick | | | | |
| real-19 | Remind me to water the plants when I get home | _TODO_ | _TODO_ | **t:** Water the plants<br>**lbl:** context=@home, energy=low, size=quick | | | | |
| real-20 | Remind me to tackle the take the trash. Take the trash cans out tomo… | _TODO_ | _TODO_ | **t:** Take the trash cans out<br>**n:** Tomorrow morning. Raw contained a false start: "remind me to…<br>**due:** 2026-08-09<br>**lbl:** context=@home, energy=low, size=quick | | | | |
| real-21 | To deal with the weird charge on the credit card | _TODO_ | _TODO_ | **t:** Deal with the weird charge on the credit card<br>**lbl:** context=@phone | | | | |
| real-22 | The thing with the the hotel reservation in Chicago | _TODO_ | _TODO_ | **t:** The thing with the hotel reservation in Chicago<br>**n:** Fragment — no action stated; dictation stutter ("the the") c… | | | | |
| real-23 | The thing with the garage door sensor | _TODO_ | _TODO_ | **t:** The thing with the garage door sensor<br>**n:** Fragment — no action stated; kept verbatim.<br>**lbl:** context=@home | | | | |
| real-24 | Finish the quarterly notes update the server inventory and I still h… | _TODO_ | _TODO_ | **t:** Finish the quarterly notes<br>**n:** Also: update the server inventory; still haven't replied to…<br>**lbl:** context=@computer | | | | |
| real-25 | Email the photographer confirm the dinner reservation and I haven't… | _TODO_ | _TODO_ | **t:** Email the photographer<br>**n:** Also: confirm the dinner reservation; "I haven't yet about a…<br>**lbl:** context=@computer | | | | |
| real-26 | Pick up milk. Replace the air filter and I haven't called the plumbe… | _TODO_ | _TODO_ | **t:** Pick up milk<br>**n:** Also: replace the air filter; haven't called the plumber yet…<br>**lbl:** context=@errands, size=quick | | | | |
| real-27 | Order more toner send Rachel the updated spreadsheet and I I still h… | _TODO_ | _TODO_ | **t:** Order more toner<br>**n:** Also: send Rachel the updated spreadsheet; still haven't sub…<br>**lbl:** context=@computer | | | | |
| real-28 | I should go to the bank and drop off the library books and fill up t… | _TODO_ | _TODO_ | **t:** Go to the bank<br>**n:** Also: drop off the library books; fill up the car with gas w…<br>**lbl:** context=@errands | | | | |

## Totals (fill after scoring)

| parser | title✓ | notes✓ | stretch✓ | DROP | HALL | GARB | SPLIT | MULTI |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| nano | _TODO_ | _TODO_ | _TODO_ | | | | | |
| hosted | _TODO_ | _TODO_ | _TODO_ | | | | | |

**Verdict (feeds #41 layer-2 decision):** _TODO_ — one of: trustworthy alone offline ·
needs guardrails (confidence gate / raw-text fallback) · prefer-hosted-when-online, Nano
as offline fallback · dropped (offline capture stays raw-text-only until sync).
