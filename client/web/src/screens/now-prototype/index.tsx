// PROTOTYPE — throwaway. Delete this whole directory when the question is
// answered; see `NOTES.md` for the question and the verdict.
//
// Three variations of the Now screen's frontier rendering, switchable at
// `?variant=A|B|C`, mounted inside the real Now screen so they are judged
// against the real header, nav rail, aside and density — not in a vacuum.
//
// Gated exactly as `?demo` is (`fixtures/demo.ts`): `import.meta.env.DEV` is
// substituted with the literal `false` at build time, so the whole lane —
// variants, fixture and switcher — drops out of a production bundle with the
// dead branch, and no query string can bring it back.

import { useEffect, useRef, useState } from "react";
import { ItemDetailPanel } from "../../components/domain/ItemDetailPanel";
import type { ProjectDTO, StepDTO, TaskActionName, TaskItemDTO } from "../../store/protocol";
import type { MicrotaskWiring } from "../../shell/useMicrotaskWiring";
import { PrototypeSwitcher, type SwitcherEntry } from "./PrototypeSwitcher";
import { PROTOTYPE_PROJECTS, prototypeItems } from "./fixture";
import { VariantA } from "./VariantA";
import { VariantB } from "./VariantB";
import { VariantC } from "./VariantC";

const ENTRIES: SwitcherEntry[] = [
  { key: "A", name: "Narrow it down" },
  { key: "B", name: "Lanes" },
  { key: "C", name: "Columns + filters + urgency" },
];

/** The variant asked for, or `null` when this is an ordinary Now render.
 * Pure over the query string so the caller stays testable. */
export function prototypeVariant(search: string): string | null {
  if (!import.meta.env.DEV) {
    return null;
  }
  const asked = new URLSearchParams(search).get("variant");
  if (asked === null) {
    return null;
  }
  const key = asked.toUpperCase();
  return ENTRIES.some((entry) => entry.key === key) ? key : ENTRIES[0].key;
}

/** Everything the real detail panel needs, threaded from `NowScreen` so item
 * detail is never re-prototyped: the panel mounted here IS
 * `ItemDetailPanel`, with the app's own act callback, steps, error and
 * microtask affordance. `undefined` over the fixture, whose ids exist in no
 * query — that path mounts the same panel with inert handlers, `?demo`'s
 * precedent for an affordance that must not issue a real request. */
export interface PrototypeDetail {
  selectedItemId: string | null;
  steps: StepDTO[];
  actError: string | null;
  onAct: (itemId: string, action: TaskActionName) => void;
  onClose: () => void;
  microtask?: MicrotaskWiring;
}

export function NowPrototype({
  initialVariant,
  frontier,
  projects,
  nowMs,
  onOpenItem,
  detail,
}: {
  initialVariant: string;
  frontier: readonly TaskItemDTO[];
  projects: readonly ProjectDTO[];
  nowMs: number;
  onOpenItem: (itemId: string) => void;
  detail?: PrototypeDetail;
}) {
  const [variant, setVariant] = useState(initialVariant);
  const [fixtureSelectedId, setFixtureSelectedId] = useState<string | null>(null);

  // Real frontier when there is one, the fixture otherwise — a variant judged
  // against three items tells you nothing about how it groups thirty.
  const useFixture = frontier.length < 8;
  const items = useFixture ? prototypeItems(nowMs) : frontier;
  const resolvedProjects = useFixture ? PROTOTYPE_PROJECTS : projects;

  const selectedId = useFixture ? fixtureSelectedId : (detail?.selectedItemId ?? null);
  const selectedItem = selectedId
    ? (items.find((item) => item.id === selectedId) ?? null)
    : null;
  const open = useFixture ? setFixtureSelectedId : onOpenItem;
  const close = useFixture ? () => setFixtureSelectedId(null) : (detail?.onClose ?? (() => {}));

  // The panel opens ABOVE the board rather than in place of it, so a card
  // deep in a long board would otherwise expand off-screen. Scrolling the
  // panel into view is what makes "it goes to the top" true for the reader
  // and not just for the DOM.
  const panelRef = useRef<HTMLDivElement | null>(null);
  useEffect(() => {
    if (selectedId) {
      panelRef.current?.scrollIntoView({ block: "start", behavior: "smooth" });
    }
  }, [selectedId]);

  const pick = (key: string) => {
    const url = new URL(window.location.href);
    url.searchParams.set("variant", key);
    window.history.replaceState(null, "", url);
    setVariant(key);
  };

  const props = {
    items,
    projects: resolvedProjects,
    nowMs,
    onOpenItem: open,
    selectedId,
  };

  return (
    <>
      {/* The expanded item, at the top of the column, with the board left
          standing under it — picking one action should not cost you the view
          of everything else you might have picked instead. (ADR-0015 reserves
          Now's *aside* for the ranked region and says standing questions never
          take the banner; the banner slot above the frontier is Now's own.) */}
      {selectedItem ? (
        <div ref={panelRef} style={{ scrollMarginTop: "var(--space-4)" }}>
          <ItemDetailPanel
            // Remounts per item so the panel's own selects reset with it —
            // `RealFrontier` keys it the same way and for the same reason.
            key={selectedItem.id}
            item={selectedItem}
            steps={useFixture ? [] : (detail?.steps ?? [])}
            onClose={close}
            onAct={
              useFixture
                ? () => {}
                : (action) => detail?.onAct(selectedItem.id, action)
            }
            actError={useFixture ? null : detail?.actError}
            microtask={useFixture ? undefined : detail?.microtask}
          />
        </div>
      ) : null}

      {variant === "A" ? <VariantA {...props} /> : null}
      {variant === "B" ? <VariantB {...props} /> : null}
      {variant === "C" ? <VariantC {...props} /> : null}

      <PrototypeSwitcher
        entries={ENTRIES}
        current={variant}
        onPick={pick}
        source={useFixture ? `fixture · ${items.length} actions` : `live · ${items.length} actions`}
      />
    </>
  );
}
