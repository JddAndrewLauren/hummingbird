// THROWAWAY (#801, Phase 1). The prototype's whole attachment to the real
// board, and the state the four variants share.
//
// The question: **what does moving a card between columns feel like, and
// which affordance is right** — not whether the write works. Nothing here
// reaches the worker; a drop writes an in-memory override that is applied to
// the items BEFORE `groupFrontier`, so the card moves through the real wasm
// grouping and the real lane packing, and a reload forgets everything.
//
// **Why the board is not forked.** `FrontierColumns.tsx` is one 1168-line
// file whose own header rejects variant flags on its components, and the
// variants must be judged against the real screen at real density (ADR-0021's
// "Where this decision came from" is that argument, and #405 the precedent).
// So the board gains ONE optional prop and five no-op-when-absent call sites,
// and each variant is a different implementation of the interface below. The
// shared surface is a spread-props seam, not a shared layout — variants stay
// free to disagree about everything they draw.
//
// **Why the variants are plain functions, not hooks.** Only this file may own
// React state: a variant that owned its own would have to be called
// conditionally. Variants mutate `Gesture` (a ref) and the DOM directly, and
// call `bump()` when something they draw actually changed. That is also what
// keeps a drag off React entirely — see `spring.ts`'s header for why a
// re-render per frame is not affordable here.
//
// **The gate is behavioural, not a bundle proof.** `import.meta.env.DEV` plus
// the board demo world plus a `variant` param, all checked in `useNowPrototype`
// — so bare `?demo` renders exactly what it renders today, which is what the
// visual gate shoots. Unlike `fixtures/demo.ts` this is NOT claimed to be
// tree-shaken: `FrontierBoard` calls the hook unconditionally, so the module
// is reachable from the graph. It carries no fixture text, so
// `assert-no-fixtures` is unaffected, and the whole directory is deleted
// before the build.

import { useState, type CSSProperties, type HTMLAttributes, type ReactNode } from "react";
import type { FrontierAxis, FrontierColumn } from "../../decisions/seam";
import type { ProjectDTO, TaskItemDTO, TriageEdits } from "../../store/protocol";
import { demoMode } from "../../fixtures/demo-mode";
import { describeMove, dropEdits } from "./drop-edits";
import type { FrozenPart, Point } from "./spring";

export const VARIANTS = ["A", "B", "C", "D"] as const;
export type VariantKey = (typeof VARIANTS)[number];

export const VARIANT_LABEL: Record<VariantKey, string> = {
  A: "Native drag, column is the target",
  B: "Pointer drag, landing preview",
  C: "Pick, then place",
  D: "Physics",
};

/** The three physics readings, all one integrator (`spring.ts`) — the
 * operator asked to try all of them rather than pick one up front. */
export const PHYSICS_MODES = ["throw", "weighted", "magnetic"] as const;
export type PhysicsMode = (typeof PHYSICS_MODES)[number];

export const PHYSICS_LABEL: Record<PhysicsMode, string> = {
  throw: "throw — release with speed, it flies and lands where aimed",
  weighted: "weighted — lag and settle, but no flight",
  magnetic: "magnetic — columns pull the card off the cursor",
};

/** What the board knows and a drop needs: the live axis (state inside
 * `FrontierColumns`, not a prop), and the two arguments `dropEdits` takes
 * beside the item. */
export interface PrototypeCtx {
  axis: FrontierAxis;
  projects: readonly ProjectDTO[];
  nowMs: number;
}

export type CardExtra = HTMLAttributes<HTMLElement> & {
  style?: CSSProperties;
  "data-proto-card"?: string;
};

export type ColumnExtra = HTMLAttributes<HTMLDivElement> & {
  style?: CSSProperties;
  "data-proto-col"?: string;
  "data-proto-value"?: string;
  "data-proto-part"?: number;
};

/** The five seams `FrontierColumns` calls, each a no-op when the prop is
 * absent. `bar` is rendered by `FrontierBoard`, outside the board's own box. */
export interface FrontierPrototype {
  applyOverrides(items: readonly TaskItemDTO[]): readonly TaskItemDTO[];
  cardProps(item: TaskItemDTO, ctx: PrototypeCtx): CardExtra;
  columnProps(column: FrontierColumn, part: number, ctx: PrototypeCtx): ColumnExtra;
  columnFooter(column: FrontierColumn, part: number, ctx: PrototypeCtx): ReactNode;
  overlay(ctx: PrototypeCtx): ReactNode;
  bar: ReactNode;
}

/** The four members a variant implements; `bar` is the host's. */
export type VariantImpl = Omit<FrontierPrototype, "bar" | "applyOverrides">;

/** Mutable gesture state, shared by every variant because only one gesture
 * runs at a time. Deliberately one flat shape rather than a slice per variant:
 * four near-identical little state objects would be four places to look. */
export interface Gesture {
  /** The item being dragged, or carried in variant C's "moving" state. */
  itemId: string | null;
  /** Column geometry as it was when the gesture started (`spring.ts` rule 2). */
  frozen: FrozenPart[];
  /** The column key currently lit, so a tint is cleared exactly once. */
  over: string | null;
  /** Pointer and card positions at gesture start, in client coordinates. */
  origin: Point;
  cardOrigin: Point;
  /** The live rAF canceller, if a flight or a settle is running. */
  cancel: (() => void) | null;
  /** Set once the pointer has travelled far enough to be a drag, so the click
   * that would otherwise open the item panel can be swallowed. */
  moved: boolean;
  /** Variant C's picked item — a separate field because C's "moving" state
   * outlives any pointer gesture. */
  picked: string | null;
  /** Filled from `columnProps`/`cardProps` on every render, so a gesture that
   * only has a DOM key can find the objects a drop needs. */
  columns: Map<string, FrontierColumn>;
  items: Map<string, TaskItemDTO>;
}

/** What a variant may do to the world: ask whether a drop would write
 * anything, and make it write. Nothing else — no variant touches the override
 * map or the last-move line directly. */
export interface DropHost {
  gesture: Gesture;
  allows(itemId: string, columnKey: string, ctx: PrototypeCtx): boolean;
  drop(itemId: string, columnKey: string, ctx: PrototypeCtx): boolean;
  /** Re-render the board — for the two things a variant draws through React
   * (C's `Move here` footers, B's placeholder) rather than through the DOM. */
  bump(): void;
  mode: PhysicsMode;
}

function readVariant(search: string): VariantKey | null {
  const raw = new URLSearchParams(search).get("variant");
  if (raw === null) return null;
  const upper = raw.toUpperCase() as VariantKey;
  return VARIANTS.includes(upper) ? upper : "A";
}

/** The three-way gate, all of it here so there is one place to read it: a dev
 * build, the board demo world, and an explicit `variant` param. Bare `?demo`
 * — the URL `visual/surfaces.spec.ts` shoots — returns `null` and the board
 * is untouched. */
export function prototypeVariant(search = window.location.search): VariantKey | null {
  if (!import.meta.env.DEV) return null;
  if (demoMode(search) !== "board") return null;
  return readVariant(search);
}

const NO_EXTRA: CardExtra = {};

export function useNowPrototype(
  build: (host: DropHost, ctxVariant: VariantKey) => VariantImpl,
  renderBar: (state: BarState) => ReactNode,
): FrontierPrototype | undefined {
  const [variant, setVariant] = useState<VariantKey | null>(() => prototypeVariant());
  const [mode, setMode] = useState<PhysicsMode>("throw");
  const [overrides, setOverrides] = useState<ReadonlyMap<string, TriageEdits>>(() => new Map());
  const [lastMove, setLastMove] = useState<string | null>(null);
  const [, setTick] = useState(0);
  // A `useState` initialiser rather than a `useRef`, for one reason: this
  // object is read while rendering (`allows` decides which columns C draws a
  // `Move here` in), and `react-hooks/refs` correctly refuses that of a ref.
  // What is wanted is a stable mutable box that is not ref-shaped, which is
  // exactly what a state initialiser returns.
  const [gesture] = useState<Gesture>(() => ({
    itemId: null,
    frozen: [],
    over: null,
    origin: { x: 0, y: 0 },
    cardOrigin: { x: 0, y: 0 },
    cancel: null,
    moved: false,
    picked: null,
    columns: new Map(),
    items: new Map(),
  }));

  if (variant === null) return undefined;

  const host: DropHost = {
    gesture: gesture,
    mode,
    bump: () => setTick((n) => n + 1),
    allows: (itemId, columnKey, ctx) => editsFor(gesture, itemId, columnKey, ctx) !== null,
    drop: (itemId, columnKey, ctx) => {
      const edits = editsFor(gesture, itemId, columnKey, ctx);
      const item = gesture.items.get(itemId);
      if (!edits || !item) return false;
      setOverrides((prev) => {
        const next = new Map(prev);
        next.set(itemId, { ...prev.get(itemId), ...edits });
        return next;
      });
      setLastMove(describeMove(item, ctx.axis, edits));
      return true;
    },
  };

  const impl = build(host, variant);

  return {
    applyOverrides: (items) =>
      overrides.size === 0
        ? items
        : items.map((item) => {
            const edit = overrides.get(item.id);
            return edit ? ({ ...item, ...edit } as TaskItemDTO) : item;
          }),
    cardProps: (item, ctx) => {
      gesture.items.set(item.id, item);
      return impl.cardProps(item, ctx) ?? NO_EXTRA;
    },
    columnProps: (column, part, ctx) => {
      gesture.columns.set(column.value ?? "", column);
      return impl.columnProps(column, part, ctx);
    },
    columnFooter: impl.columnFooter,
    overlay: impl.overlay,
    bar: renderBar({
      variant,
      mode,
      lastMove,
      moves: overrides.size,
      pick: (next) => {
        abandon(gesture);
        setVariant(next);
        const url = new URL(window.location.href);
        url.searchParams.set("variant", next);
        window.history.replaceState(null, "", url.toString());
      },
      pickMode: setMode,
      reset: () => {
        setOverrides(new Map());
        setLastMove(null);
      },
    }),
  };
}

export interface BarState {
  variant: VariantKey;
  mode: PhysicsMode;
  lastMove: string | null;
  moves: number;
  pick(next: VariantKey): void;
  pickMode(next: PhysicsMode): void;
  reset(): void;
}

/** Drop whatever the outgoing variant was holding. A function rather than two
 * assignments at the call site: `react-hooks/immutability` reads a write to a
 * `useState` value as a bug, and it is right about every case but this one. */
function abandon(gesture: Gesture): void {
  gesture.cancel?.();
  gesture.cancel = null;
  gesture.picked = null;
  gesture.itemId = null;
  gesture.over = null;
  gesture.moved = false;
}

function editsFor(
  gesture: Gesture,
  itemId: string,
  columnKey: string,
  ctx: PrototypeCtx,
): TriageEdits | null {
  const item = gesture.items.get(itemId);
  const column = gesture.columns.get(columnKey);
  if (!item || !column) return null;
  return dropEdits(ctx.axis, column, item, ctx.projects, ctx.nowMs);
}
