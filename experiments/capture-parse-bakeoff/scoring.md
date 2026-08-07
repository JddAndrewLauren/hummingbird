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

The messy corpus captures deliberately bait these: multi-item (`ph-02, 05, 12, 19, 24`),
no-verb / fragment (`ph-03, 06, 11, 14, 17, 20`), garbled dictation (`ph-07, 15`),
inferred-due bait (`ph-01, 09, 15, 18, 21, 24`).

## Sheet

| id | raw | ground_truth | nano | hosted | title✓ | notes✓ | stretch✓ | failure_modes |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| ph-01 | Email Dana the Q3 roadmap draft before Thursday standup | _TODO_ | _TODO_ | **t:** Email Dana the Q3 roadmap draft<br>**n:** Before Thursday standup.<br>**due:** 2026-08-13<br>**lbl:** context=@computer, size=quick |  |  |  |  |
| ph-02 | okay so I need to call the dentist about rescheduling the cleaning an… | _TODO_ | _TODO_ | **t:** Call the dentist to reschedule the cleaning<br>**n:** Also: grab the prescription while out that way; maybe pick … | | | | |
| ph-03 | the thing with the fly deploy token | _TODO_ | _TODO_ | **t:** The thing with the Fly deploy token<br>**n:** Fragment — no action stated; kept verbatim.<br>**lbl:** context=@computer | | | | |
| ph-04 | remind me to water the plants when I get home | _TODO_ | _TODO_ | **t:** Water the plants<br>**lbl:** context=@home, energy=low, size=quick | | | | |
| ph-05 | buy a birthday card for mom, book the oil change, and cancel the gym … | _TODO_ | _TODO_ | **t:** Buy a birthday card for mom<br>**n:** Also: book the oil change; cancel the gym trial before it c… | | | | |
| ph-06 | quarterly taxes | _TODO_ | _TODO_ | **t:** Quarterly taxes | | | | |
| ph-07 | send the uh the invoice to the to the landscaping people the one from… | _TODO_ | _TODO_ | **t:** Send the invoice to the landscaping people<br>**n:** The one from last month.<br>**lbl:** context=@computer, size=quick | | | | |
| ph-08 | Draft the ADR for the runner endpoint — separate Fly app, not on the … | _TODO_ | _TODO_ | **t:** Draft the ADR for the runner endpoint<br>**n:** Separate Fly app, not on the sweeper; single bearer token; …<br>**lbl:** context=@computer, size=deep | | | | |
| ph-09 | remind me to take the trash out tonight it's recycling week | _TODO_ | _TODO_ | **t:** Take the trash out<br>**n:** It's recycling week.<br>**due:** 2026-08-07<br>**lbl:** context=@home, energy=low, size=quick | | | | |
| ph-10 | yeah if you could just make sure the passport gets renewed we leave i… | _TODO_ | _TODO_ | **t:** Renew the passport<br>**n:** We leave in October, so there's a window.<br>**lbl:** context=@errands, size=deep | | | | |
| ph-11 | dentist | _TODO_ | _TODO_ | **t:** Dentist | | | | |
| ph-12 | pick up the dry cleaning call the plumber about the leak under the si… | _TODO_ | _TODO_ | **t:** Pick up the dry cleaning<br>**n:** Also: call the plumber about the leak under the sink; we're… | | | | |
| ph-13 | Read the Foundation Models guided-generation docs before starting the… | _TODO_ | _TODO_ | **t:** Read the Foundation Models guided-generation docs<br>**n:** Before starting the iPad prototype.<br>**lbl:** context=@computer, size=medium | | | | |
| ph-14 | book flights for | _TODO_ | _TODO_ | **t:** Book flights for<br>**n:** Truncated capture — destination missing; kept verbatim. | | | | |
| ph-15 | schedule the — no wait — reschedule the one on one with Sam to Friday… | _TODO_ | _TODO_ | **t:** Reschedule the 1:1 with Sam<br>**n:** To Friday afternoon.<br>**due:** 2026-08-14<br>**lbl:** context=@computer, size=quick | | | | |
| ph-16 | remind me to send Grandma a thank you note for the sweater | _TODO_ | _TODO_ | **t:** Send Grandma a thank-you note for the sweater<br>**lbl:** context=@home, energy=low, size=quick | | | | |
| ph-17 | leak under the bathroom sink getting worse | _TODO_ | _TODO_ | **t:** Leak under the bathroom sink getting worse<br>**n:** No verb stated; kept close to raw rather than inventing an …<br>**lbl:** context=@home | | | | |
| ph-18 | Submit the reimbursement for the conference hotel by end of month | _TODO_ | _TODO_ | **t:** Submit the reimbursement for the conference hotel<br>**n:** By end of month.<br>**due:** 2026-08-31<br>**lbl:** context=@computer, size=quick | | | | |
| ph-19 | transfer five hundred to savings and set up the autopay for the elect… | _TODO_ | _TODO_ | **t:** Transfer $500 to savings<br>**n:** Also: set up autopay for the electric bill.<br>**lbl:** context=@phone, size=quick | | | | |
| ph-20 | milk eggs bread coffee filters | _TODO_ | _TODO_ | **t:** Milk, eggs, bread, coffee filters<br>**n:** Shopping list — no verb; items kept in the title.<br>**lbl:** context=@errands, size=quick | | | | |
| ph-21 | follow up with the recruiter she said she'd have an update by Wednesd… | _TODO_ | _TODO_ | **t:** Follow up with the recruiter<br>**n:** She said she'd have an update by Wednesday.<br>**lbl:** context=@phone, size=quick | | | | |
| ph-22 | car registration and also the smog check | _TODO_ | _TODO_ | **t:** Car registration<br>**n:** Also: the smog check (likely a prerequisite).<br>**lbl:** context=@errands | | | | |
| ph-23 | Idea: a raw-text fallback for capture-parse that never loses the utte… | _TODO_ | _TODO_ | **t:** Idea: raw-text fallback for capture-parse that never loses the utterance<br>**n:** "Couldn't parse, kept verbatim."<br>**lbl:** context=@computer, size=medium | | | | |
| ph-24 | so for the trip we still need to book the dog sitter confirm the rent… | _TODO_ | _TODO_ | **t:** Book the dog sitter for the trip<br>**n:** Also: confirm the rental car; pay the cabin deposit (due th… | | | | |

## Totals (fill after scoring)

| parser | title✓ | notes✓ | stretch✓ | DROP | HALL | GARB | SPLIT | MULTI |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| nano | _TODO_ | _TODO_ | _TODO_ | | | | | |
| hosted | _TODO_ | _TODO_ | _TODO_ | | | | | |

**Verdict (feeds #41 layer-2 decision):** _TODO_ — one of: trustworthy alone offline ·
needs guardrails (confidence gate / raw-text fallback) · prefer-hosted-when-online, Nano
as offline fallback · dropped (offline capture stays raw-text-only until sync).
