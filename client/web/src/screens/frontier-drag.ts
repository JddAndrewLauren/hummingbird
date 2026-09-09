// The frontier board's drag gesture (#801, ADR-0021 decision 9): a card is
// carried between columns and takes the column it lands in.
//
// **The card is not attached to the pointer; it is attached to the pointer
// by a spring.** It lags, it tilts into its own travel, it overshoots and
// settles. That is the shape the Phase 1 prototype's operator verdict
// picked (variant D, mode "weighted") over native HTML5 drag, a
// header-strip target with a landing slot, pick-then-place, and the same
// spring with ballistics — ADR-0021 decision 9 records all five and why the
// other four lost. What is kept from the winner is the weight; what is
// dropped is the throw: the card is released *over* its target, never
// aimed, so there is no velocity projection here and no magnetic pull.
//
// Three rules this file exists to keep.
//
// 1. **A gesture never re-renders the board.** `FrontierColumns` calls
//    `groupFrontier` — a wasm crossing — on every render. Driving a drag
//    through React state would do that sixty times a second. So a gesture
//    writes `element.style.transform` directly and React re-renders exactly
//    once, when the drop's write comes back.
// 2. **A gesture never changes layout.** `FrontierColumns` freezes the
//    lanes' resting room precisely so a collapse cannot move a column;
//    anything reflowing mid-drag would move the target out from under the
//    pointer. Transforms only, and the column rects are read ONCE at
//    gesture start (`freeze`) and hit-tested against that snapshot. The one
//    thing that legitimately moves under a drag is the scroll container,
//    and `beginDrag` corrects for exactly that and nothing else.
// 3. **Refusal is physical.** A column that would not accept the card never
//    lights, and a card released over one springs home. Nothing is painted
//    red: status colour on this board means urgency (ADR-0021 decision 2),
//    and a second meaning for it would cost more than the refusal is worth.
//
// The *decision* a drop makes — which field it writes — is not here. It is
// `hummingbird_core::decisions::frontier::drop_edits`, reached through
// `dropEdits` in the seam (ADR-0025): the physics, the hit-testing and the
// auto-scroll consume measured boxes and stay per client, the mapping does
// not.

export interface Point {
  x: number;
  y: number;
}

export function clamp(n: number, lo: number, hi: number): number {
  return n < lo ? lo : n > hi ? hi : n;
}

/** True when the reader has asked for less motion. The design system
 * collapses every duration token to 1ms there; a spring has no token to
 * collapse, so it is not run at all — the card follows the pointer 1:1 and
 * is placed instantly. */
export function reducedMotion(): boolean {
  return typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;
}

/** One axis of a damped spring, integrated semi-implicitly (velocity first,
 * then position — stable at the step sizes a browser actually delivers,
 * which explicit Euler is not).
 *
 * `k` is stiffness and `c` damping at unit mass: `a = -k(x - target) - cv`.
 * Critical damping at `k = 520` is `c = 2*sqrt(k)` ~= 45, so 32 is
 * deliberately *under*-damped: the settle overshoots once and comes back
 * inside ~250ms. That overshoot is what the design system's motion
 * paragraph was amended for — it is the gesture's whole character, not a
 * detail. */
export class Spring {
  x: number;
  v = 0;
  target: number;
  constructor(
    x: number,
    readonly k = 520,
    readonly c = 32,
  ) {
    this.x = x;
    this.target = x;
  }
  step(dt: number): void {
    // Seconds, and capped: a backgrounded tab hands back one enormous
    // frame, and a spring integrated across it explodes.
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

/** A rAF loop that runs while `step` returns true, returning its own
 * canceller so a gesture abandoned mid-flight (a second drag, an unmount)
 * can stop the previous one rather than leaving two loops writing the same
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

// ------------------------------------------------------------- geometry

/** `null` is a real column value — the no-value column, the one a drop
 * *clears* the field for — so the attribute carries a sentinel rather than
 * an empty string, which is already the column's key. */
const NO_VALUE = "\u0000none";

/** Attributes every drawn part of a column carries: the main chunk and each
 * continuation part a column that runs on into the spare lanes draws to its
 * right. A drop target covers all of them, because they are one column.
 *
 * Elements are found by attribute rather than through a ref registry:
 * `Card` is not a `forwardRef`, so the board attaches to it through a props
 * spread, and one data attribute works identically on the card and on the
 * column `div`s. */
export function columnAttrs(key: string, value: string | null) {
  return { "data-hb-column": key, "data-hb-column-value": value ?? NO_VALUE };
}

export function cardAttrs(id: string) {
  return { "data-hb-card": id };
}

export function cardElement(id: string): HTMLElement | null {
  return document.querySelector<HTMLElement>(`[data-hb-card="${CSS.escape(id)}"]`);
}

/** Every drawn element of one column, so a tint lands on all of it. */
export function columnElements(key: string): HTMLElement[] {
  return [...document.querySelectorAll<HTMLElement>(`[data-hb-column="${CSS.escape(key)}"]`)];
}

/** One drawn part of one column, with the geometry the gesture hit-tests
 * against. */
export interface FrozenPart {
  /** `column.value ?? ""` — the per-column key the board itself uses. */
  key: string;
  value: string | null;
  rect: DOMRect;
}

/** The geometry snapshot a gesture hit-tests against, taken once at its
 * start (rule 2 in this file's header). */
export function freeze(): FrozenPart[] {
  return [...document.querySelectorAll<HTMLElement>("[data-hb-column]")].map((el) => {
    const raw = el.dataset.hbColumnValue ?? NO_VALUE;
    return {
      key: el.dataset.hbColumn ?? "",
      value: raw === NO_VALUE ? null : raw,
      rect: el.getBoundingClientRect(),
    };
  });
}

/** The part under a point, or the nearest one within `slack` px if the
 * point is between lanes — the board's gutters are dead ground, and a hand
 * that releases in one is pointing at a column, not at nothing. Beyond the
 * slack the answer is `null`, and the card springs home. */
export function hitPart(frozen: readonly FrozenPart[], at: Point, slack = 16): FrozenPart | null {
  let best: FrozenPart | null = null;
  let bestDistance = Infinity;
  for (const part of frozen) {
    const r = part.rect;
    const dx = Math.max(r.left - at.x, 0, at.x - r.right);
    const dy = Math.max(r.top - at.y, 0, at.y - r.bottom);
    const d = Math.hypot(dx, dy);
    if (d < bestDistance) {
      bestDistance = d;
      best = part;
    }
  }
  return bestDistance <= slack ? best : null;
}

// --------------------------------------------------------------- styling

/** A column that would accept the card being carried. Written straight onto
 * the element during a gesture, so it must be a flat property bag.
 *
 * The design system's rules that bite here: the highlight goes on the
 * *column*, never on the card (a card's own colour means urgency), and a
 * surface being pointed at gets more solid rather than less — a
 * `--surface-quiet` fill and a `--border-strong` outline, never an opacity
 * fade. */
const LIT: Record<string, string> = {
  background: "var(--surface-quiet)",
  outline: "1px solid var(--border-strong)",
  outlineOffset: "var(--space-2)",
  borderRadius: "var(--radius-card)",
  transition: "background var(--dur-fast) var(--ease-flit)",
};

const UNLIT: Record<string, string> = {
  background: "",
  outline: "",
  outlineOffset: "",
  borderRadius: "",
  transition: "",
};

/** A card being carried. Elevation 3 is the system's floating step — what a
 * dialog gets, which is exactly what a card held above the board is.
 * `transition: none` because the rAF loop owns the transform now, and
 * `Card`'s own 200ms transform transition would fight it every frame. */
const CARRIED: Record<string, string> = {
  position: "relative",
  zIndex: "40",
  boxShadow: "var(--shadow-3)",
  transition: "none",
  cursor: "grabbing",
  willChange: "transform",
};

const DROPPED: Record<string, string> = {
  position: "",
  zIndex: "",
  boxShadow: "",
  transition: "",
  cursor: "",
  willChange: "",
  transform: "",
};

function paint(elements: readonly HTMLElement[], on: boolean): void {
  for (const el of elements) Object.assign(el.style, on ? LIT : UNLIT);
}

// ---------------------------------------------------------- the gesture

/** How far the pointer must travel before a mouse or pen gesture is a drag
 * rather than a click. */
const DRAG_SLOP = 5;

/** How long a finger must rest before a touch gesture is a drag. Under it,
 * movement scrolls the board — which is why the prototype's blanket
 * `touch-action: none` on every card cannot ship: it would trade the
 * board's own scrolling for the gesture. */
const HOLD_MS = 350;

/** Within this many px of the scroll container's edge, a live drag scrolls
 * it — the one thing native HTML5 drag gave the prototype for free. */
const EDGE_BAND = 48;
const EDGE_SPEED = 12;

/** Where a column's first card sits below its heading — the slot a landing
 * card aims at. A measured constant rather than a read: the heading is one
 * 44px row plus the column's own gap, and reading it would mean a second
 * layout pass in the middle of a gesture. */
const HEADER_BAND = 56;

/** A card held at the slot it landed in, waiting for the board to re-render
 * under it. The write is asynchronous — the drop calls `onTriage`, the
 * worker applies it, and the re-read arrives some renders later — so the
 * gesture cannot FLIP synchronously the way the prototype could. It holds
 * the card where the hand left it and hands the board this record instead;
 * `settleLanding` finishes the motion on the next render. */
export interface Landing {
  itemId: string;
  fromRect: DOMRect;
}

export interface DragHost {
  /** Would the column keyed `key` accept this card? It lights if so. */
  allows(key: string): boolean;
  /** Commit a drop into `key`. */
  drop(key: string): void;
  /** The scroll container to auto-scroll near the edges of, and to correct
   * hit-testing for when it moves. `.hb-scroll` on every real mount. */
  scroller(): HTMLElement | null;
  /** The gesture became a drag: the click it ends with must not open the
   * item panel. */
  onArm(): void;
  /** The card has settled and is being held at `landing.fromRect`. */
  onLand(landing: Landing): void;
}

/** Carry one card. Called from the card's own `pointerdown`; everything
 * after that is listeners on the card itself, so the gesture survives the
 * pointer leaving the board. A gesture owns itself and reports through
 * `host`. */
export function beginDrag(
  card: HTMLElement,
  itemId: string,
  event: PointerEvent,
  host: DragHost,
): void {
  const gentle = reducedMotion();
  const touch = event.pointerType === "touch";
  const frozen = freeze();
  const origin: Point = { x: event.clientX, y: event.clientY };
  const scroller = host.scroller();
  const scrollAtStart = scroller?.scrollTop ?? 0;
  const pointerId = event.pointerId;

  const sx = new Spring(0);
  const sy = new Spring(0);
  let pointer: Point = { ...origin };
  let armed = false;
  let flying = false;
  let lit: string | null = null;
  let landing: FrozenPart | null = null;
  let cancelLoop: (() => void) | null = null;
  let holdTimer: ReturnType<typeof setTimeout> | null = null;

  /** How far the container has scrolled since `freeze`. Both the pointer
   * and the frozen rects live in the viewport frame, so a scroll moves the
   * columns out from under a snapshot that cannot be retaken (rule 2). One
   * correction fixes both readings: the pointer is compared in the frame
   * the rects were taken in. */
  const scrolled = (): number => (scroller ? scroller.scrollTop - scrollAtStart : 0);
  const frozenPointer = (): Point => ({ x: pointer.x, y: pointer.y + scrolled() });

  const light = (key: string | null) => {
    if (lit === key) return;
    if (lit !== null) paint(columnElements(lit), false);
    if (key !== null) paint(columnElements(key), true);
    lit = key;
  };

  const target = (): FrozenPart | null => {
    const part = hitPart(frozen, frozenPointer());
    return part && host.allows(part.key) ? part : null;
  };

  /** The slot a column would put the card in, as a translate offset from
   * where the card is drawn right now: centred under the column's heading. */
  const slotOffset = (part: FrozenPart, box: DOMRect): Point => ({
    x: part.rect.left + part.rect.width / 2 - (box.left + box.width / 2),
    y: part.rect.top + HEADER_BAND + box.height / 2 - (box.top + box.height / 2),
  });

  const paintFrame = () => {
    // Tilt reads velocity, so the card leans into its own travel and rights
    // itself as it settles. Capped so it never reads as spin.
    const tilt = clamp(sx.v / 90, -8, 8);
    card.style.transform = `translate(${sx.x}px, ${sy.x}px) rotate(${tilt}deg)`;
  };

  const autoScroll = () => {
    if (!scroller) return;
    const r = scroller.getBoundingClientRect();
    if (pointer.y < r.top + EDGE_BAND) scroller.scrollTop -= EDGE_SPEED;
    else if (pointer.y > r.bottom - EDGE_BAND) scroller.scrollTop += EDGE_SPEED;
  };

  const step = (dt: number): boolean => {
    if (!flying) {
      autoScroll();
      const wanted = frozenPointer();
      sx.target = wanted.x - origin.x;
      sy.target = wanted.y - origin.y;
      sx.step(dt);
      sy.step(dt);
      paintFrame();
      light(target()?.key ?? null);
      return true;
    }
    sx.step(dt);
    sy.step(dt);
    paintFrame();
    if (!sx.resting || !sy.resting) return true;
    settle();
    return false;
  };

  /** The card has stopped moving. Commit if it stopped somewhere legal,
   * then hold it exactly where it is and hand the board a `Landing` — see
   * that type for why this cannot finish the motion itself. */
  const settle = () => {
    light(null);
    card.style.cursor = "";
    card.style.willChange = "";
    const key = landing?.key;
    landing = null;
    if (key === undefined) {
      // Refused, or released over nothing: the card has already sprung back
      // to where the layout puts it and nothing was written, so there is no
      // render to wait for — hand it straight back.
      Object.assign(card.style, DROPPED);
      return;
    }
    host.onLand({ itemId, fromRect: card.getBoundingClientRect() });
    host.drop(key);
  };

  function end() {
    card.removeEventListener("pointermove", move);
    card.removeEventListener("pointerup", end);
    card.removeEventListener("pointercancel", end);
    card.removeEventListener("touchmove", swallowTouch);
    if (holdTimer !== null) clearTimeout(holdTimer);
    if (!armed) {
      Object.assign(card.style, DROPPED);
      return;
    }
    landing = target();
    if (gentle) {
      cancelLoop?.();
      settle();
      return;
    }
    flying = true;
    if (landing) {
      // The card's own box already carries the live transform, so the slot
      // offset is relative to where it is *now* — added to where the spring
      // already is rather than replacing it.
      const slot = slotOffset(landing, card.getBoundingClientRect());
      sx.target = sx.x + slot.x;
      sy.target = sy.x + slot.y;
    } else {
      // Refused, or released over nothing: the card springs home.
      sx.target = 0;
      sy.target = 0;
    }
  }

  function arm() {
    if (armed) return;
    armed = true;
    host.onArm();
    Object.assign(card.style, CARRIED);
    // jsdom has no pointer capture, and a browser that has lost the pointer
    // already will throw rather than answer.
    try {
      card.setPointerCapture?.(pointerId);
    } catch {
      // The gesture works without it; only a pointer that leaves the card
      // mid-drag is lost, and that ends the gesture cleanly anyway.
    }
    // Non-passive, and only once armed: before this the browser owns the
    // finger and the board scrolls normally.
    if (touch) card.addEventListener("touchmove", swallowTouch, { passive: false });
    if (!gentle) cancelLoop = loop(step);
  }

  function swallowTouch(touchEvent: TouchEvent) {
    touchEvent.preventDefault();
  }

  function move(moveEvent: PointerEvent) {
    pointer = { x: moveEvent.clientX, y: moveEvent.clientY };
    const far = Math.hypot(pointer.x - origin.x, pointer.y - origin.y) >= DRAG_SLOP;
    if (!armed) {
      if (!far) return;
      // A finger that moves before the hold is over is scrolling, not
      // dragging, and the gesture gets out of its way for good.
      if (touch) {
        end();
        return;
      }
      arm();
    }
    if (gentle) {
      const wanted = frozenPointer();
      sx.snap(wanted.x - origin.x);
      sy.snap(wanted.y - origin.y);
      card.style.transform = `translate(${sx.x}px, ${sy.x}px)`;
      light(target()?.key ?? null);
    }
  }

  card.addEventListener("pointermove", move);
  card.addEventListener("pointerup", end);
  card.addEventListener("pointercancel", end);
  if (touch) holdTimer = setTimeout(arm, HOLD_MS);
}

/** Finish a `Landing`: the board has re-rendered, so the card is wherever
 * the new grouping put it — the target column after a write lands, its own
 * column after one fails. Either way the motion is the same, and it is a
 * plain FLIP: put the card back where the hand left it, then spring it to
 * where it now belongs.
 *
 * Returns a canceller, or `null` when there is nothing to animate. */
export function settleLanding(landing: Landing): (() => void) | null {
  const el = cardElement(landing.itemId);
  if (!el) return null;
  Object.assign(el.style, DROPPED);
  const to = el.getBoundingClientRect();
  const dx = landing.fromRect.left - to.left;
  const dy = landing.fromRect.top - to.top;
  if (reducedMotion() || (Math.abs(dx) < 0.5 && Math.abs(dy) < 0.5)) return null;

  const fx = new Spring(dx);
  const fy = new Spring(dy);
  el.style.transition = "none";
  return loop((dt) => {
    fx.step(dt);
    fy.step(dt);
    el.style.transform = `translate(${fx.x}px, ${fy.x}px)`;
    if (fx.resting && fy.resting) {
      el.style.transform = "";
      el.style.transition = "";
      return false;
    }
    return true;
  });
}
