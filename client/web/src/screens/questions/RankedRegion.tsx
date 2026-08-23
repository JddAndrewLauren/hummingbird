import { useState } from "react";
import { Icon } from "../../components/core/Icon";
import type { Screen } from "../../shell/screens";
import {
  boundedGlyphs,
  type PaneAnswer,
  type PaneGlyph,
  type QuestionInputs,
  type RankedPane,
  type Surface,
} from "./contract";
import {
  readCollapseMap,
  resolveCollapsed,
  writeCollapseOverride,
  type CollapseMap,
  type StorageLike,
} from "./collapse";
import { QUESTIONS, rankPanes } from "./registry";
import { questionLabel } from "./roster";
import { samePaneIdentity } from "./sort";

// ADR-0015's **ranked region** (#245): every standing question, ordered by
// how much it deserves the eye right now, taking over Now's Context aside.
//
// **Order is captured in state; content is read fresh.** This is the one
// mechanism worth reading the code for. The bands move with the clock, which
// ticks every 30 seconds — and a region that re-sorted on every tick would
// slide a pane out from under the cursor mid-reach. So one ranking is
// captured, and the captured sample decides position, band chrome, the
// collapsed sentence, the glyphs and the collapse resolution (all from the
// same sample, so an override and a position can never disagree about which
// band the pane was in). The *expanded* content renders from the live inputs
// on every render, so an optimistic write is instant even while the order
// stands still.
//
// The sample is re-taken on exactly two signals:
//
//   * a completed sync cycle (`syncOutcomeSeq`) — new facts arrived, and the
//     order they imply is the honest one;
//   * `samePaneIdentity` failing — a pane appeared, disappeared, or crossed
//     between an answer and a gap. Without this a freshly mounted region
//     whose data lands a moment later would sit in a whole wrong answer
//     state for up to a minute.
//
// And on nothing else. Band and `withinBand` movement deliberately does NOT
// re-open the order: that comparison is exactly the mid-interaction move the
// captured sample exists to prevent.
//
// The setState-during-render below is this repo's sanctioned idiom for
// "adjust state when a prop changes" (`FrontierBoard`,
// `SettingsScreen`'s `BindingRow`), comparison-guarded and never a ref:
// `react-hooks/refs` forbids reading a ref during render and
// `react-hooks/set-state-in-effect` forbids the effect version.

interface Sample {
  seq: number;
  panes: RankedPane[];
}

export interface RankedRegionProps {
  /** ADR-0017's surface axis: which ranked region this instance is —
   * `NowScreen`'s aside or the Status screen. Threaded straight into
   * `rankPanes`, which is the one place the filter is actually applied; the
   * sort, `samePaneIdentity` sampling and the collapse map below are
   * unaware a second surface exists. */
  surface: Surface;
  /** Everything but the clock — the region supplies `nowMs` itself so a tick
   * cannot be forgotten at one call site and not another. */
  inputs: Omit<QuestionInputs, "nowMs">;
  nowMs: number;
  /** `TaskState.syncOutcomeSeq` — the "a cycle happened" signal, for the same
   * reason every `use*Wiring` hook keys on it rather than on an outcome's
   * `kind`. */
  syncOutcomeSeq: number;
  /** Injectable for tests, the `calendar/persistence.ts` idiom. */
  storage?: StorageLike;
  onScreen: (screen: Screen) => void;
  /** #122's do-date write affordance, forwarded verbatim to whichever
   * pane's `Expanded` component asks for it — see `contract.ts`'s
   * `QuestionDef.Expanded` doc. `App.tsx` passes `handleSetScheduledDate`
   * unconditionally since #456; optional only so a caller with nothing to
   * send it can omit it. */
  onSetScheduledDate?: (itemId: string, date: string | null) => void;
}

function Glyph({ glyph }: { glyph: PaneGlyph }) {
  if (glyph.kind === "icon") {
    return <Icon name={glyph.name} size={13} title={glyph.label} color="var(--text-muted)" />;
  }
  return (
    <span
      role="img"
      aria-label={glyph.label}
      style={{
        width: 8,
        height: 8,
        borderRadius: "50%",
        background: glyph.fill,
        border: `1px solid ${glyph.edge}`,
        flex: "0 0 auto",
      }}
    />
  );
}

/** The collapsed row, drawn **entirely by the shell** — no pane ships a
 * compact form of its own, which is what keeps every quiet question in the
 * region reading as one list rather than N private ones. One line: the
 * question, its glyphs, its answer. */
function CollapsedRow({
  label,
  answer,
  onToggle,
}: {
  label: string;
  answer: PaneAnswer;
  onToggle: () => void;
}) {
  const glyphs = boundedGlyphs(answer.icon);
  return (
    <button
      type="button"
      onClick={onToggle}
      aria-expanded={false}
      style={{
        display: "flex",
        alignItems: "center",
        gap: "var(--space-3)",
        width: "100%",
        // The system's row height, which is also the minimum touch target.
        minHeight: 44,
        padding: "var(--space-2) var(--space-3)",
        background: "transparent",
        border: "1px solid transparent",
        borderRadius: "var(--radius-md)",
        cursor: "pointer",
        textAlign: "left",
        color: "var(--text-secondary)",
      }}
    >
      <span className="hb-meta">{label}</span>
      {glyphs.map((glyph, index) => (
        <Glyph key={`${glyph.label}-${index}`} glyph={glyph} />
      ))}
      <span
        style={{
          marginLeft: "auto",
          font: "var(--type-body-sm)",
          color: "var(--text-primary)",
        }}
      >
        {answer.collapsedHeadline}
      </span>
    </button>
  );
}

/** Rewrites every key a `StorageLike` sees, so `collapse.ts` — which always
 * reads/writes its own one fixed key — actually lands in a different real
 * slot per surface. `"now"` is left un-suffixed on purpose: it is the key
 * every device before this surface existed already wrote its overrides
 * under. */
function namespacedStorage(base: StorageLike, surface: Surface): StorageLike {
  const suffix = surface === "now" ? "" : `.${surface}`;
  return {
    getItem: (key) => base.getItem(`${key}${suffix}`),
    setItem: (key, value) => base.setItem(`${key}${suffix}`, value),
    removeItem: (key) => base.removeItem(`${key}${suffix}`),
  };
}

export function RankedRegion({
  surface,
  inputs,
  nowMs,
  syncOutcomeSeq,
  storage,
  onScreen,
  onSetScheduledDate,
}: RankedRegionProps) {
  const live = rankPanes({ ...inputs, nowMs }, surface);

  const [sample, setSample] = useState<Sample>(() => ({ seq: syncOutcomeSeq, panes: live }));
  if (sample.seq !== syncOutcomeSeq || !samePaneIdentity(sample.panes, live)) {
    setSample({ seq: syncOutcomeSeq, panes: live });
  }

  // `collapse.ts` prunes its map down to exactly the pane keys ITS caller
  // currently has live (`writeCollapseOverride`'s own doc) — correct for one
  // surface's region, but two surfaces sharing one `localStorage` key would
  // each read the other's panes as "no longer ranked" and prune them away on
  // every toggle. `collapse.ts` stays surface-blind (it is a pure module with
  // no reason to know ADR-0017 exists, and `sort.test.ts`/`collapse.test.ts`
  // are the tripwire for that leaking in) — so the isolation happens here
  // instead, by giving each surface's region its own key inside the same
  // storage rather than a second storage-layer concept. `"now"` keeps the
  // bare key so an existing device's Now overrides survive this surface
  // shipping.
  const scopedStorage = storage ? namespacedStorage(storage, surface) : undefined;

  const [collapseMap, setCollapseMap] = useState<CollapseMap>(() =>
    scopedStorage ? readCollapseMap(scopedStorage) : {},
  );

  function toggle(paneKey: string, answer: PaneAnswer, collapsed: boolean): void {
    const override = { band: answer.band, collapsed: !collapsed };
    if (!scopedStorage) {
      setCollapseMap({ ...collapseMap, [paneKey]: override });
      return;
    }
    setCollapseMap(
      writeCollapseOverride(
        scopedStorage,
        collapseMap,
        paneKey,
        override,
        sample.panes.map((pane) => pane.paneKey),
      ),
    );
  }

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-5)" }}>
      {sample.panes.map((pane) => {
        const definition = QUESTIONS[pane.question];
        const collapsed = resolveCollapsed(collapseMap, pane.paneKey, pane.answer);
        if (collapsed) {
          return (
            <CollapsedRow
              key={pane.paneKey}
              label={questionLabel(pane.question)}
              answer={pane.answer}
              onToggle={() => toggle(pane.paneKey, pane.answer, collapsed)}
            />
          );
        }
        const Expanded = definition.Expanded;
        return (
          <div key={pane.paneKey}>
            <button
              type="button"
              onClick={() => toggle(pane.paneKey, pane.answer, collapsed)}
              aria-expanded
              style={{
                display: "flex",
                alignItems: "center",
                gap: "var(--space-2)",
                padding: 0,
                background: "transparent",
                border: "none",
                cursor: "pointer",
              }}
            >
              <span className="hb-meta">{questionLabel(pane.question)}</span>
              <Icon name="chevron-down" size={13} color="var(--text-muted)" />
            </button>
            <div style={{ marginTop: "var(--space-4)" }}>
              {/* The LIVE inputs, not the sample's — an expanded pane must
                  never be a frame behind what the device already knows. */}
              <Expanded
                subjectKey={pane.subjectKey}
                inputs={{ ...inputs, nowMs }}
                onSetupNavigate={() => onScreen("settings")}
                onSetScheduledDate={onSetScheduledDate}
              />
            </div>
          </div>
        );
      })}
    </div>
  );
}
