import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

import { DEMO_DATA } from "./demo-data";

// The drift gate on `DEMO_DATA.ruleKindRegistry.sources`.
//
// The rules editor's `source eq` control is a pick from the frozen source
// registry rather than a text box, precisely so a value the authority does
// not declare cannot be authored. The demo world has to offer that same
// vocabulary — it is what the visual gate photographs — but `demo-data.ts`
// is a static TS module and the real registry crosses at runtime through
// wasm, so the list there is hand-written. A hand-written copy of a frozen
// vocabulary is the exact drift `RulesScreenStructuralTest` bans Kotlin
// from committing, and it went wrong the first time it was written: the
// list shipped `anthropic-usage/v1`, which `sources.rs`'s own header calls
// **unenrolled** and which exists there only inside a test fixture. It
// rendered as an ordinary pickable option the authority would have refused.
//
// So the copy is pinned to the Rust side rather than trusted. Reading Rust
// source text from vitest is the house idiom for a cross-language pin
// (`shared-fixtures.test.ts` reads `client/core`'s fixtures, the race pane's
// suite reads `server/race-poll`'s golden body), and `client.yml` covers
// `client/**`.
//
// **What this reads is the frozen ADR-0014 table, not `REGISTRY` itself.**
// `REGISTRY`'s entries name `&'static str` constants (`GMAIL_V1`), so
// scraping it would mean resolving those; the frozen table in
// `sources::tests::registry_matches_the_frozen_adr_0014_table` spells every
// source as a literal, in registration order, and a Rust test pins it to
// `REGISTRY` verbatim — including `retired_as`. Two hops, both gated, and
// neither is a hand copy.
//
// **The one gap, stated rather than hidden:** a change to `sources.rs`
// alone touches no `client/**` path, so `client.yml`'s filter can leave
// this unrun until the next web change. It catches the drift, just not
// always in the PR that causes it — which is still strictly better than the
// nothing that let the phantom entry through.

const SOURCES_RS = new URL(
  "../../../../server/domain/src/sources.rs",
  import.meta.url,
);

/** Every `(source, ..., retired_as)` tuple of the frozen ADR-0014 table, in
 * registration order. The opening `("<source>", Shape::` shape is what
 * identifies a tuple's first element — no other string literal in the block
 * is followed by `, Shape::`, so `Expiry::Always("…")` and
 * `Some("city-waste/v2")` cannot be mistaken for one. */
function frozenTable(): { source: string; retiredAs: string | null }[] {
  const src = readFileSync(SOURCES_RS, "utf8");
  const start = src.indexOf("let expected: &[(&str, Shape, Writes, Expiry, Option<&str>)] = &[");
  expect(start, "the frozen ADR-0014 table moved or was renamed").toBeGreaterThan(-1);
  const end = src.indexOf("\n        ];", start);
  expect(end, "the frozen table's closing bracket moved").toBeGreaterThan(start);
  const block = src.slice(start, end);

  const entries: { source: string; retiredAs: string | null }[] = [];
  const tuple = /\(\s*"([^"]+)"\s*,\s*Shape::/g;
  const bounds: { source: string; at: number }[] = [];
  for (let m = tuple.exec(block); m !== null; m = tuple.exec(block)) {
    bounds.push({ source: m[1], at: m.index });
  }
  bounds.forEach(({ source, at }, i) => {
    const body = block.slice(at, i + 1 < bounds.length ? bounds[i + 1].at : block.length);
    const retired = /Some\("([^"]+)"\)/.exec(body);
    entries.push({ source, retiredAs: retired ? retired[1] : null });
  });
  return entries;
}

describe("the demo world's source vocabulary", () => {
  it("is the frozen registry, in registration order, with nothing invented", () => {
    expect(DEMO_DATA.ruleKindRegistry.sources).toEqual(frozenTable());
  });

  it("marks retirement from the registry's own answer rather than a second list", () => {
    // The one real retired entry (ADR-0014 bumped `city-waste`). If this
    // ever reads `null`, the editor stops greying an option the authority
    // will 400 — the failure the whole `retiredAs` field exists to prevent.
    const cityWasteV1 = DEMO_DATA.ruleKindRegistry.sources.find(
      (s) => s.source === "city-waste/v1",
    );
    expect(cityWasteV1?.retiredAs).toBe("city-waste/v2");
  });

  it("offers every source a demo rule actually names, so no fixture rule renders an unknown value", () => {
    const known = new Set(DEMO_DATA.ruleKindRegistry.sources.map((s) => s.source));
    // `ruleDetails`, not `rules` — the latter is the three-line marketing
    // list the landing copy renders; the wire-shaped fixtures the Rules
    // screen actually reads are these.
    for (const rule of DEMO_DATA.ruleDetails) {
      for (const condition of rule.conditions) {
        if (condition.field === "source" && condition.op === "eq") {
          expect(known, `${rule.id} names a source the registry does not declare`).toContain(
            condition.value,
          );
        }
      }
    }
  });
});
