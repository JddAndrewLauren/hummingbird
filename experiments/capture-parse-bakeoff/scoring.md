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
3. **Mark the four ✓ columns per parser** — score `nano` and `hosted` each against
   `ground_truth`. Use `Y` (match), `~` (partial), `N` (miss); write it as `nano/hosted`
   (e.g. `~/Y`). `stretch✓` covers `due` **and** `label` together — mark `N/A` when the
   ground truth has neither. `items✓` is the multi-item metric and the one place scoring is
   a count rather than a judgment: `Y` = same actions, same order; `~` = all actions found
   but split or ordered differently; `N` = an action missing, an action invented, or
   `items` absent on a multi-action capture. Mark `N/A` when the ground truth has no
   `items` — but if a parser emitted `items` anyway on a single-action capture, that's `N`,
   not `N/A`.
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
| `MULTI` | multi-item capture collapsed — `items` absent or short on a capture holding several actions, so the 2nd and 3rd action are lost. An action *invented* by over-splitting ("milk and eggs" → two entries) is `HALL`, not `MULTI`. |

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

**Multi-item is a quarter of real dictation** — 7 of the 28 `real-*` rows — which is why
`items` exists in the schema and `items✓` in this sheet. Every run-on shape produced it. A
parser that handles single-action captures perfectly and collapses multi-item ones is
wrong about 25% of real input, so `items✓` on the `real-*` subset deserves as much weight
as `title✓` when the verdict gets written.

**Due-date resolution is relative to the run date.** The hosted column for `ph-*` /
`real-01…05` was produced on 2026-08-07 and for `real-06…28` on 2026-08-08. Resolve the
ground truth (and the Nano run) against the same per-row date, or relative phrases will
mis-score by a day.

## Sheet

| id | raw | ground_truth | nano | hosted | title✓ | notes✓ | items✓ | stretch✓ | failure_modes |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| ph-01 | Email Dana the Q3 roadmap draft before Thursday standup | _TODO_ | **FENCED:** **INVALID:** fewer than minItems 2<br>**t:** Email Dana the Q3 roadmap draft before Thursday standup | **t:** Email Dana the Q3 roadmap draft<br>**n:** Before Thursday standup.<br>**due:** 2026-08-13<br>**lbl:** context=@computer, size=quick |  |  |  |  |  |
| ph-03 | the thing with the fly deploy token | _TODO_ | **FENCED:** **t:** the thing with the fly deploy token | **t:** The thing with the Fly deploy token<br>**n:** Fragment — no action stated; kept verbatim.<br>**lbl:** context=@computer |  |  |  |  |  |
| ph-04 | remind me to water the plants when I get home | _TODO_ | **FENCED:** **INVALID:** fewer than minItems 2<br>**t:** remind me to water the plants<br>**n:** when I get home | **t:** Water the plants<br>**lbl:** context=@home, energy=low, size=quick |  |  |  |  |  |
| ph-05 | buy a birthday card for mom, book the oil change, and cancel the gym … | _TODO_ | **FENCED:** **t:** buy a birthday card for mom<br>**items:** 1. book the oil change · 2. cancel the gym trial before it charges m…<br>**n:** book the oil change, and cancel the gym trial before it char… | **t:** Buy a birthday card for mom<br>**items:** 1. Buy a birthday card for mom · 2. Book the oil change · 3. Cancel the gym trial<br>**n:** Cancel the gym trial before it charges me.<br>**lbl:** context=@errands, size=quick |  |  |  |  |  |
| ph-06 | quarterly taxes | _TODO_ | **FENCED:** **t:** quarterly taxes | **t:** Quarterly taxes |  |  |  |  |  |
| ph-08 | Draft the ADR for the runner endpoint — separate Fly app, not on the … | _TODO_ | **FENCED:** **t:** Draft the ADR for the runner endpoint<br>**n:** separate Fly app, not on the sweeper, single bearer token, r… | **t:** Draft the ADR for the runner endpoint<br>**n:** Separate Fly app, not on the sweeper; single bearer token; r…<br>**lbl:** context=@computer, size=deep |  |  |  |  |  |
| ph-11 | dentist | _TODO_ | **FENCED:** **t:** dentist | **t:** Dentist |  |  |  |  |  |
| ph-13 | Read the Foundation Models guided-generation docs before starting the… | _TODO_ | **FENCED:** **INVALID:** fewer than minItems 2<br>**t:** Read the Foundation Models guided-generation docs<br>**n:** before starting the iPad prototype | **t:** Read the Foundation Models guided-generation docs<br>**n:** Before starting the iPad prototype.<br>**lbl:** context=@computer, size=medium |  |  |  |  |  |
| ph-14 | book flights for | _TODO_ | **FENCED:** **INVALID:** fewer than minItems 2<br>**t:** book flights for | **t:** Book flights for<br>**n:** Truncated capture — destination missing; kept verbatim. |  |  |  |  |  |
| ph-17 | leak under the bathroom sink getting worse | _TODO_ | **FENCED:** **INVALID:** fewer than minItems 2<br>**t:** leak under the bathroom sink<br>**n:** getting worse | **t:** Leak under the bathroom sink getting worse<br>**n:** No verb stated; kept close to raw rather than inventing an a…<br>**lbl:** context=@home |  |  |  |  |  |
| ph-18 | Submit the reimbursement for the conference hotel by end of month | _TODO_ | **FENCED:** **INVALID:** fewer than minItems 2<br>**t:** Submit the reimbursement for the conference hotel | **t:** Submit the reimbursement for the conference hotel<br>**n:** By end of month.<br>**due:** 2026-08-31<br>**lbl:** context=@computer, size=quick |  |  |  |  |  |
| ph-20 | milk eggs bread coffee filters | _TODO_ | **FENCED:** **t:** Buy milk<br>**items:** 1. Buy eggs · 2. Buy bread · 3. Buy coffee filters | **t:** Milk, eggs, bread, coffee filters<br>**n:** Shopping list — no verb; items kept in the title.<br>**lbl:** context=@errands, size=quick |  |  |  |  |  |
| ph-22 | car registration and also the smog check | _TODO_ | **FENCED:** **t:** car registration and smog check<br>**items:** 1. car registration · 2. smog check | **t:** Car registration<br>**items:** 1. Car registration · 2. Smog check<br>**lbl:** context=@errands |  |  |  |  |  |
| ph-23 | Idea: a raw-text fallback for capture-parse that never loses the utte… | _TODO_ | **FENCED:** **t:** Idea: a raw-text fallback for capture-parse that never loses the utt… | **t:** Idea: raw-text fallback for capture-parse that never loses the utter…<br>**n:** "Couldn't parse, kept verbatim."<br>**lbl:** context=@computer, size=medium |  |  |  |  |  |
| real-01 | Testing with voice | _TODO_ | **FENCED:** **t:** Testing with voice | **t:** Testing with voice |  |  |  |  |  |
| real-02 | Call the dentist for new aligners | _TODO_ | **FENCED:** **t:** Call the dentist for new aligners | **t:** Call the dentist for new aligners<br>**lbl:** context=@phone, size=quick |  |  |  |  |  |
| real-03 | order clothes | _TODO_ | **FENCED:** **t:** order clothes | **t:** Order clothes<br>**lbl:** context=@computer |  |  |  |  |  |
| real-04 | golf practice | _TODO_ | **FENCED:** **t:** golf practice | **t:** Golf practice |  |  |  |  |  |
| real-05 | Setup hermes agent | _TODO_ | **FENCED:** **t:** Setup hermes agent | **t:** Setup hermes agent<br>**lbl:** context=@computer |  |  |  |  |  |
| real-06 | I need to return that Amazon package and also buy printer paper. May… | _TODO_ | **FENCED:** **INVALID:** fewer than minItems 2<br>**t:** return Amazon package<br>**items:** 1. buy printer paper<br>**n:** also buy printer paper | **t:** Return the Amazon package<br>**items:** 1. Return the Amazon package · 2. Buy printer paper · 3. Stop at the pharmacy<br>**n:** The pharmacy stop was hedged ("maybe").<br>**lbl:** context=@errands, size=quick |  |  |  |  |  |
| real-07 | Okay so I need to pick up the dry cleaning and also get dog food and… | _TODO_ | **FENCED:** **t:** pick up the dry cleaning<br>**items:** 1. get dog food · 2. grab some coffee<br>**n:** and also get dog food and maybe grab some coffee while I'm a… | **t:** Pick up the dry cleaning<br>**items:** 1. Pick up the dry cleaning · 2. Get dog food · 3. Grab some coffee<br>**n:** The coffee was hedged ("maybe ... while I'm at it").<br>**lbl:** context=@errands, size=quick |  |  |  |  |  |
| real-08 | Review the draft contract for the meeting | _TODO_ | **FENCED:** **t:** Review the draft contract for the meeting | **t:** Review the draft contract for the meeting<br>**lbl:** context=@computer, size=medium |  |  |  |  |  |
| real-09 | Buy a new USBC cable | _TODO_ | **FENCED:** **t:** Buy a new USBC cable | **t:** Buy a new USB-C cable<br>**lbl:** context=@errands, size=quick |  |  |  |  |  |
| real-10 | Schedule the oil change for the car | _TODO_ | **FENCED:** **t:** Schedule the oil change for the car | **t:** Schedule the oil change for the car<br>**lbl:** context=@phone, size=quick |  |  |  |  |  |
| real-11 | Email Sarah about the | _TODO_ | **FENCED:** **t:** Email Sarah about the | **t:** Email Sarah about the<br>**n:** Truncated capture — subject missing; kept verbatim.<br>**lbl:** context=@computer |  |  |  |  |  |
| real-12 | Book the the flights for | _TODO_ | **FENCED:** **t:** Book the flights | **t:** Book the flights for<br>**n:** Truncated capture — destination missing; dictation stutter (… |  |  |  |  |  |
| real-13 | Pick up the prescription tonight | _TODO_ | **FENCED:** **INVALID:** fewer than minItems 2<br>**t:** Pick up the prescription | **t:** Pick up the prescription<br>**n:** Tonight.<br>**due:** 2026-08-08<br>**lbl:** context=@errands, size=quick |  |  |  |  |  |
| real-14 | We're new the car registration by the 15th | _TODO_ | **FENCED:** **INVALID:** fewer than minItems 2<br>**t:** We're new the car registration<br>**n:** by the 15th | **t:** Renew the car registration<br>**n:** By the 15th. Raw reads "We're new the car registration" — "W…<br>**due:** 2026-08-15<br>**lbl:** context=@computer |  |  |  |  |  |
| real-15 | Send the revised proposal to James by Friday | _TODO_ | **FENCED:** **INVALID:** fewer than minItems 2<br>**t:** Send the revised proposal to James | **t:** Send the revised proposal to James<br>**n:** By Friday.<br>**due:** 2026-08-14<br>**lbl:** context=@computer, size=quick |  |  |  |  |  |
| real-16 | Move the dentist appointment. Actually no reschedule the cleaning to… | _TODO_ | **FENCED:** **INVALID:** fewer than minItems 2<br>**t:** Reschedule the cleaning to the 21st<br>**items:** 1. Reschedule the cleaning to the 21st<br>**n:** Move the dentist appointment.<br>**due:** 2024-01-21 | **t:** Reschedule the cleaning to the 21st<br>**n:** Self-correction in the raw: started as "move the dentist app…<br>**due:** 2026-08-21<br>**lbl:** context=@phone, size=quick |  |  |  |  |  |
| real-17 | Schedule the actually no it reschedule that budget meeting to Tuesda… | _TODO_ | **FENCED:** **INVALID:** fewer than minItems 2<br>**t:** Schedule the budget meeting to Tuesday<br>**items:** 1. Schedule the budget meeting to Tuesday<br>**n:** actually no it reschedule | **t:** Reschedule the budget meeting<br>**n:** To Tuesday. Self-correction in the raw: "Schedule the — actu…<br>**due:** 2026-08-11<br>**lbl:** context=@computer, size=quick |  |  |  |  |  |
| real-18 | Remind me to cancel that free trial next Thursday | _TODO_ | **FENCED:** **INVALID:** fewer than minItems 2<br>**t:** Remind me to cancel that free trial<br>**n:** next Thursday | **t:** Cancel the free trial<br>**n:** Next Thursday.<br>**due:** next Thursday<br>**lbl:** context=@computer, size=quick |  |  |  |  |  |
| real-19 | Remind me to water the plants when I get home | _TODO_ | **FENCED:** **INVALID:** fewer than minItems 2<br>**t:** Remind me to water the plants | **t:** Water the plants<br>**lbl:** context=@home, energy=low, size=quick |  |  |  |  |  |
| real-20 | Remind me to tackle the take the trash. Take the trash cans out tomo… | _TODO_ | **FENCED:** **INVALID:** fewer than minItems 2<br>**t:** Take the trash<br>**items:** 1. Take the trash<br>**n:** Remind me to tackle the take the trash cans out tomorrow mor…<br>**due:** 2024-03-11<br>**lbl:** context=@home, energy=low, size=quick | **t:** Take the trash cans out<br>**n:** Tomorrow morning. Raw contained a false start: "remind me to…<br>**due:** 2026-08-09<br>**lbl:** context=@home, energy=low, size=quick |  |  |  |  |  |
| real-21 | To deal with the weird charge on the credit card | _TODO_ | **FENCED:** **INVALID:** expected type string, got NoneType<br>**t:** Deal with the weird charge on the credit card | **t:** Deal with the weird charge on the credit card<br>**lbl:** context=@phone |  |  |  |  |  |
| real-22 | The thing with the the hotel reservation in Chicago | _TODO_ | **FENCED:** **t:** The thing with the hotel reservation in Chicago | **t:** The thing with the hotel reservation in Chicago<br>**n:** Fragment — no action stated; dictation stutter ("the the") c… |  |  |  |  |  |
| real-23 | The thing with the garage door sensor | _TODO_ | **FENCED:** **t:** The thing with the garage door sensor | **t:** The thing with the garage door sensor<br>**n:** Fragment — no action stated; kept verbatim.<br>**lbl:** context=@home |  |  |  |  |  |
| real-24 | Finish the quarterly notes update the server inventory and I still h… | _TODO_ | **FENCED:** **t:** Finish the quarterly notes update<br>**items:** 1. Finish the quarterly notes update · 2. the server inventory<br>**n:** the server inventory and I still haven't replied to mark | **t:** Finish the quarterly notes<br>**items:** 1. Finish the quarterly notes · 2. Update the server inventory · 3. Reply to Mark<br>**lbl:** context=@computer |  |  |  |  |  |
| real-25 | Email the photographer confirm the dinner reservation and I haven't… | _TODO_ | **FENCED:** **t:** Email photographer confirm dinner reservation<br>**n:** and I haven't yet about a wedding gift | **t:** Email the photographer<br>**items:** 1. Email the photographer · 2. Confirm the dinner reservation<br>**n:** "I haven't yet about a wedding gift" — clause incomplete in…<br>**lbl:** context=@computer |  |  |  |  |  |
| real-26 | Pick up milk. Replace the air filter and I haven't called the plumbe… | _TODO_ | **FENCED:** **t:** Pick up milk<br>**n:** Replace the air filter and I haven't called the plumber yet | **t:** Pick up milk<br>**items:** 1. Pick up milk · 2. Replace the air filter · 3. Call the plumber<br>**lbl:** context=@errands, size=quick |  |  |  |  |  |
| real-27 | Order more toner send Rachel the updated spreadsheet and I I still h… | _TODO_ | **FENCED:** **t:** Order more toner<br>**items:** 1. send Rachel the updated spreadsheet · 2. submit the expense report<br>**n:** send Rachel the updated spreadsheet and I I still haven't su… | **t:** Order more toner<br>**items:** 1. Order more toner · 2. Send Rachel the updated spreadsheet · 3. Submit the expense report<br>**n:** Raw had a stutter ("I I").<br>**lbl:** context=@computer |  |  |  |  |  |
| real-28 | I should go to the bank and drop off the library books and fill up t… | _TODO_ | **FENCED:** **t:** go to the bank<br>**items:** 1. drop off the library books · 2. fill up the car with gas<br>**n:** and drop off the library books and fill up the car with gas… | **t:** Go to the bank<br>**items:** 1. Go to the bank · 2. Drop off the library books · 3. Fill up the car with gas<br>**n:** All three while out.<br>**lbl:** context=@errands |  |  |  |  |  |

## Totals (fill after scoring)

| parser | title✓ | notes✓ | items✓ | stretch✓ | DROP | HALL | GARB | SPLIT | MULTI |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| nano | _TODO_ | _TODO_ | _TODO_ | _TODO_ |  |  |  |  |  |
| hosted | _TODO_ | _TODO_ | _TODO_ | _TODO_ |  |  |  |  |  |

**Verdict (feeds #41 layer-2 decision):** _TODO_ — one of: trustworthy alone offline ·
needs guardrails (confidence gate / raw-text fallback) · prefer-hosted-when-online, Nano
as offline fallback · dropped (offline capture stays raw-text-only until sync).
