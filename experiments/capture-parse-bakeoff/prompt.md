# Capture-parse prompt

This exact wording is fed to **both** parsers (Gemini Nano on-device and the hosted
baseline). Identical prompt is the point of the bake-off — a difference in output is a
difference in the model, not the instructions. Do not tune it per parser.

The runner interpolates the JSON Schema and the raw capture where marked, then requests
schema-constrained output against that schema.

---

You turn one raw personal-task capture — dictated or typed, often messy — into a single
structured task, conforming exactly to the provided JSON Schema.

Rules:

1. **Find the actionable line and make it the `title`.** In clean typed input the title is
   the whole thing. In run-on dictation you must locate the one action the person means;
   strip filler ("okay so", "uh", "I need to", "maybe"). Keep the title short and imperative.
2. **Never lose content.** Anything in the raw that isn't the title goes into `notes`.
   If there's no remainder, omit `notes` — don't emit an empty string.
3. **Do not invent.** Never add a fact, name, date, or field the raw doesn't contain.
4. **`due` is optional and conservative.** Set it only when the raw carries an explicit
   temporal phrase. Resolve to `YYYY-MM-DD` if you can; otherwise keep the phrase verbatim.
   No temporal phrase → `null`. Never guess a date.
5. **`label` is optional and conservative.** Add a `context`/`energy`/`size` hint only when
   the raw clearly implies it. Omit any dimension you'd be guessing.
6. **Multiple actions in one capture:** put the clearest single action in `title` and keep
   the rest verbatim in `notes`. Do not silently drop the others. (The schema holds one
   task; multi-item captures are a known hard case being measured — surface them, don't
   hide them.)
7. **If you cannot parse it,** put the entire raw text verbatim in `title` and leave the
   rest unset. A kept-but-unparsed capture beats a lost or hallucinated one.

Output only JSON conforming to the schema. No prose, no code fences.

Schema:

```json
{{SCHEMA}}
```

Raw capture:

```
{{RAW}}
```
