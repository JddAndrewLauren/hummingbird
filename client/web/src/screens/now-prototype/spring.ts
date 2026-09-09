// THROWAWAY (#801, Phase 1). The physics and the geometry the drag variants
// share. Deleted with the rest of `now-prototype/` before the build.
//
// Two rules this file exists to keep:
//
// 1. **A gesture never re-renders the board.** `FrontierColumns` calls
//    `groupFrontier` — a wasm crossing — on every render, and the board holds
//    29 cards. Driving a drag through React state would do both 60 times a
//    second. So a gesture writes `element.style.transform` directly through
//    the registry below, and React re-renders exactly once, when a drop
//    changes the override map.
// 2. **A gesture never changes layout.** `FrontierColumns.tsx:668-712`
//    deliberately freezes the lanes' resting room so a collapse cannot move a
//    column; anything that reflowed mid-drag would move the target out from
//    under the pointer. Transforms only, and the column rects are read ONCE
//    at gesture start (`freeze`) and hit-tested against that snapshot.

export interface Point {
  x: number;
  y: number;
}

export function clamp(n: number, lo: number, hi: number): number {
  return n < lo ? lo : n > hi ? hi : n;
}

/** True when the reader has asked for less motion. The design system collapses
 * every duration token to 1ms there; an integrator has no token to collapse,
 * so each variant asks this and places instantly instead. */
export function reducedMotion(): boolean {
  return typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;
}

/** One axis of a damped spring, integrated semi-implicitly (velocity first,
 * then position — stable at the step sizes a browser actually delivers, which
 * explicit Euler is not).
 *
 * `k` is stiffness and `c` damping at unit mass: `a = -k(x - target) - cv`.
 * The defaults settle in ~250ms with a small overshoot. Critical damping is
 * `c = 2*sqrt(k)` ≈ 45 at `k = 520`, so 32 is deliberately *under*-damped —
 * the overshoot is the point of the variant. */
export class Spring {
  x: number;
  v = 0;
  target: number;
  constructor(x: number, readonly k = 520, readonly c = 32) {
    this.x = x;
    this.target = x;
  }
  step(dt: number): void {
    // Seconds, and capped: a backgrounded tab hands back one enormous frame,
    // and a spring integrated across it explodes.
    const h = Math.min(dt, 32) / 1000;
    this.v += (-this.k * (this.x - this.target) - this.c * this.v) * h;
    this.x += this.v * h;
  }
  snap(x: number): void {
    this.x = x;
    this.target = x;
    this.v = 0;
  }
  get resting(): boolean {
    return Math.abs(this.x - this.target) < 0.35 && Math.abs(this.v) < 12;
  }
}

/** Pointer velocity in px/ms, from the last few samples rather than the last
 * one: a single frame's delta is dominated by whatever jitter the last event
 * carried, and a throw aimed by that is not the throw the hand made. */
export class VelocityTracker {
  private samples: { x: number; y: number; t: number }[] = [];
  sample(x: number, y: number, t: number): void {
    this.samples.push({ x, y, t });
    if (this.samples.length > 5) this.samples.shift();
  }
  reset(): void {
    this.samples = [];
  }
  velocity(): Point {
    const first = this.samples[0];
    const last = this.samples[this.samples.length - 1];
    if (!first || !last) return { x: 0, y: 0 };
    const dt = last.t - first.t;
    if (dt <= 0) return { x: 0, y: 0 };
    return { x: (last.x - first.x) / dt, y: (last.y - first.y) / dt };
  }
}

/** Where a throw would come to rest, under per-frame friction `f`: the sum of
 * a geometric series, `v * dt / (1 - f)`. Used to pick the column BEFORE the
 * flight starts, so the card is already aimed at a real slot and the flight
 * and the settle can be one spring rather than two animations spliced. */
export function projectThrow(from: Point, v: Point, f = 0.92, frame = 16): Point {
  const reach = frame / (1 - f);
  return { x: from.x + v.x * reach, y: from.y + v.y * reach };
}

/** One drawn part of one column: the main chunk, or one of the continuation
 * parts a column that runs on into the spare lanes draws to its right. A drop
 * target must cover all of them — they are one column.
 *
 * Elements are found by `data-` attribute rather than through a ref registry.
 * `Card` is not a `forwardRef` and its props type has no `ref`, and the
 * prototype attaches to it through a spread — a data attribute costs one
 * string, works identically on the column `div`s, and is the kind of shortcut
 * a throwaway is allowed. */
export interface FrozenPart {
  /** `column.value ?? ""` — the per-column key the board itself uses. */
  key: string;
  value: string | null;
  el: HTMLElement;
  rect: DOMRect;
}

/** `null` is a real column value (the "No context" column), so the attribute
 * carries a sentinel rather than an empty string, which is also `key`. */
const NO_VALUE = "\u0000none";

export function columnAttrs(key: string, value: string | null, part: number) {
  return { "data-proto-col": key, "data-proto-value": value ?? NO_VALUE, "data-proto-part": part };
}

export function cardAttrs(id: string) {
  return { "data-proto-card": id };
}

export function cardElement(id: string): HTMLElement | null {
  return document.querySelector<HTMLElement>(`[data-proto-card="${CSS.escape(id)}"]`);
}

/** Every drawn element of one column, so a tint lands on all of it — the
 * chunk and every continuation part. */
export function columnElements(key: string): HTMLElement[] {
  return [...document.querySelectorAll<HTMLElement>(`[data-proto-col="${CSS.escape(key)}"]`)];
}

/** The geometry snapshot a gesture hit-tests against, taken once at its start
 * (rule 2 in this file's header). */
export function freeze(): FrozenPart[] {
  return [...document.querySelectorAll<HTMLElement>("[data-proto-col]")].map((el) => {
    const raw = el.dataset.protoValue ?? NO_VALUE;
    return {
      key: el.dataset.protoCol ?? "",
      value: raw === NO_VALUE ? null : raw,
      el,
      rect: el.getBoundingClientRect(),
    };
  });
}

/** The part under a point, or the nearest one within `slack` px if the point
 * is between lanes (the board's `--space-6` gutters are dead ground, and a
 * throw that lands in one should still resolve). */
export function hitPart(frozen: readonly FrozenPart[], at: Point, slack = 48): FrozenPart | null {
  let best: FrozenPart | null = null;
  let bestDistance = Infinity;
  for (const p of frozen) {
    const r = p.rect;
    const dx = Math.max(r.left - at.x, 0, at.x - r.right);
    const dy = Math.max(r.top - at.y, 0, at.y - r.bottom);
    const d = Math.hypot(dx, dy);
    if (d < bestDistance) {
      bestDistance = d;
      best = p;
    }
  }
  return bestDistance <= slack ? best : null;
}

/** A rAF loop that runs while `step` returns true. Returns its own canceller,
 * so a gesture that is abandoned mid-flight (a re-render, a second drag) can
 * stop the previous one rather than leaving two loops writing the same
 * element. */
export function loop(step: (dt: number) => boolean): () => void {
  let last = performance.now();
  let live = true;
  let handle = requestAnimationFrame(function tick(now) {
    if (!live) return;
    const dt = now - last;
    last = now;
    if (step(dt)) handle = requestAnimationFrame(tick);
    else live = false;
  });
  return () => {
    live = false;
    cancelAnimationFrame(handle);
  };
}
