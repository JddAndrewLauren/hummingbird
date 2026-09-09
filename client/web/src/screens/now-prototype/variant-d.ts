// THROWAWAY (#801, Phase 1). Variant D — physics.
//
// The card is not attached to the pointer; it is attached to the pointer by a
// spring. That one sentence is the whole variant, and everything below is a
// consequence of it: the card lags, it tilts into the direction it is
// travelling, it overshoots and settles, and — in `throw` — it can be let go
// and keep going.
//
// **Three modes, one integrator**, because the operator asked to try all
// three rather than pick one blind:
//
//  - `throw`     release with speed and the card flies, decelerates under
//                friction, and lands in whichever column its PROJECTED
//                resting point falls in. The aim is taken before the flight
//                starts, so the flight and the settle are one spring rather
//                than two animations spliced together.
//  - `weighted`  the same weight and the same settle, but no ballistics: the
//                card must be released over its target. The calm reading.
//  - `magnetic`  the card tracks the pointer until a column's pull takes it
//                off the cursor and into the slot, and then resists leaving.
//                Physics as guidance rather than as momentum.
//
// **This variant deliberately breaks the design system.** Motion there is
// 90–320ms, "nothing drifts or floats", and overshoot is allowed on hover and
// nowhere else. A thrown card drifts by definition. That conflict is the
// point: if D wins, the win is an argument to amend the motion section, and
// `NOTES.md` says so. It is not smuggled in as a detail.
//
// Under `prefers-reduced-motion` the engine does not run at all — the card
// follows the pointer 1:1 and is placed instantly. A reader who has asked for
// less motion is not the reader this variant is arguing with.

import {
  cardAttrs,
  cardElement,
  clamp,
  columnAttrs,
  columnElements,
  freeze,
  hitPart,
  loop,
  projectThrow,
  reducedMotion,
  Spring,
  VelocityTracker,
  type FrozenPart,
  type Point,
} from "./spring";
import { CARRIED, DROPPED, PART_TRANSITION, paint } from "./styles";
import type { DropHost, VariantImpl } from "./seam";

const HEADER_BAND = 44;
const DRAG_SLOP = 5;
/** How far out a column starts pulling, in `magnetic`. Roughly one column's
 * own width — any further and every column pulls at once. */
const ATTRACT_RADIUS = 260;
/** The column already held pulls harder than the one being approached, so the
 * card does not flicker between two of them along the boundary. */
const HYSTERESIS = 1.3;

export function variantD(host: DropHost): VariantImpl {
  const g = host.gesture;

  const light = (key: string | null) => {
    if (g.over === key) return;
    if (g.over !== null) {
      paint(columnElements(g.over), false);
      for (const card of cardsIn(g.over)) card.style.transform = "";
    }
    if (key !== null) paint(columnElements(key), true);
    g.over = key;
  };

  const cardsIn = (key: string): HTMLElement[] => [
    ...document.querySelectorAll<HTMLElement>(`[data-proto-col="${CSS.escape(key)}"] [data-proto-card]`),
  ];

  /** The slot a column would put the card in, as a translate offset from
   * where the card is actually laid out — under the header, at the column's
   * own width. */
  const slotOffset = (part: FrozenPart, size: { w: number; h: number }): Point => ({
    x: part.rect.left + part.rect.width / 2 - (g.cardOrigin.x + size.w / 2),
    y: part.rect.top + HEADER_BAND + 12 + size.h / 2 - (g.cardOrigin.y + size.h / 2),
  });

  return {
    cardProps: (item, ctx) => ({
      ...cardAttrs(item.id),
      style: { touchAction: "none" },
      onPointerDown: (event) => {
        if (item.pending || event.button !== 0) return;
        const card = event.currentTarget as HTMLElement;
        const rect = card.getBoundingClientRect();
        const size = { w: rect.width, h: rect.height };
        const gentle = reducedMotion();

        g.cancel?.();
        g.itemId = item.id;
        g.moved = false;
        g.frozen = freeze();
        g.origin = { x: event.clientX, y: event.clientY };
        g.cardOrigin = { x: rect.left, y: rect.top };
        card.setPointerCapture(event.pointerId);

        const sx = new Spring(0);
        const sy = new Spring(0);
        const tracker = new VelocityTracker();
        // Where the pointer wants the card, in translate space.
        const wanted: Point = { x: 0, y: 0 };
        let pointer: Point = { x: event.clientX, y: event.clientY };
        let flying = false;
        let landing: FrozenPart | null = null;

        const centre = (): Point => ({
          x: g.cardOrigin.x + sx.x + size.w / 2,
          y: g.cardOrigin.y + sy.x + size.h / 2,
        });

        /** In `magnetic`, blend the pointer's target toward the nearest
         * column that would accept the card, by how close it is. */
        const attracted = (): { target: Point; part: FrozenPart | null } => {
          const at = centre();
          let best: FrozenPart | null = null;
          let bestPull = 0;
          for (const part of g.frozen) {
            if (!host.allows(item.id, part.key, ctx)) continue;
            const r = part.rect;
            const dx = Math.max(r.left - at.x, 0, at.x - r.right);
            const dy = Math.max(r.top - at.y, 0, at.y - r.bottom);
            let pull = 1 - clamp(Math.hypot(dx, dy) / ATTRACT_RADIUS, 0, 1);
            if (part.key === g.over) pull = clamp(pull * HYSTERESIS, 0, 1);
            if (pull > bestPull) {
              bestPull = pull;
              best = part;
            }
          }
          if (!best || bestPull <= 0) return { target: wanted, part: null };
          const slot = slotOffset(best, size);
          return {
            target: {
              x: wanted.x + (slot.x - wanted.x) * bestPull,
              y: wanted.y + (slot.y - wanted.y) * bestPull,
            },
            part: best,
          };
        };

        const paintFrame = () => {
          // Tilt reads velocity, so the card leans into its own travel and
          // rights itself as it settles. Capped so it never reads as spin.
          const tilt = clamp(sx.v / 90, -8, 8);
          card.style.transform = `translate(${sx.x}px, ${sy.x}px) rotate(${tilt}deg)`;
        };

        const openSlot = (key: string | null) => {
          light(key);
          if (key === null) return;
          for (const c of cardsIn(key)) {
            if (c === card) continue;
            c.style.transition = PART_TRANSITION;
            c.style.transform = `translateY(${size.h + 12}px)`;
          }
        };

        const step = (dt: number): boolean => {
          if (!flying) {
            const { target, part } = host.mode === "magnetic" ? attracted() : { target: wanted, part: null };
            sx.target = target.x;
            sy.target = target.y;
            sx.step(dt);
            sy.step(dt);
            paintFrame();
            const over =
              host.mode === "magnetic"
                ? part
                : hitPart(g.frozen, host.mode === "weighted" ? pointer : centre(), 0);
            openSlot(over && host.allows(item.id, over.key, ctx) ? over.key : null);
            return true;
          }
          sx.step(dt);
          sy.step(dt);
          paintFrame();
          if (!sx.resting || !sy.resting) return true;
          settle();
          return false;
        };

        /** The flight is over. Commit if it ended somewhere legal, then hand
         * the card back to the layout — with one FLIP frame, so the throw
         * reads as continuous rather than as a jump into a new column. */
        const settle = () => {
          const from = card.getBoundingClientRect();
          Object.assign(card.style, DROPPED);
          light(null);
          g.itemId = null;
          if (!landing) return;
          const key = landing.key;
          landing = null;
          if (!host.drop(item.id, key, ctx)) return;
          requestAnimationFrame(() => {
            const moved = cardElement(item.id);
            if (!moved) return;
            const to = moved.getBoundingClientRect();
            const fx = new Spring(from.left - to.left);
            const fy = new Spring(from.top - to.top);
            moved.style.transition = "none";
            g.cancel = loop((dt) => {
              fx.step(dt);
              fy.step(dt);
              moved.style.transform = `translate(${fx.x}px, ${fy.x}px)`;
              if (fx.resting && fy.resting) {
                moved.style.transform = "";
                moved.style.transition = "";
                return false;
              }
              return true;
            });
          });
        };

        const move = (moveEvent: PointerEvent) => {
          pointer = { x: moveEvent.clientX, y: moveEvent.clientY };
          const dx = pointer.x - g.origin.x;
          const dy = pointer.y - g.origin.y;
          if (!g.moved && Math.hypot(dx, dy) < DRAG_SLOP) return;
          if (!g.moved) {
            g.moved = true;
            Object.assign(card.style, CARRIED);
            if (!gentle) g.cancel = loop(step);
          }
          wanted.x = dx;
          wanted.y = dy;
          tracker.sample(pointer.x, pointer.y, moveEvent.timeStamp);
          if (gentle) {
            // No engine: the card is the pointer, and the target column is
            // whatever is under it.
            sx.snap(dx);
            sy.snap(dy);
            card.style.transform = `translate(${dx}px, ${dy}px)`;
            const over = hitPart(g.frozen, pointer, 0);
            light(over && host.allows(item.id, over.key, ctx) ? over.key : null);
          }
        };

        const up = () => {
          card.removeEventListener("pointermove", move);
          card.removeEventListener("pointerup", up);
          card.removeEventListener("pointercancel", up);
          if (!g.moved) {
            Object.assign(card.style, DROPPED);
            g.itemId = null;
            return;
          }
          // A reader who asked for less motion gets no ballistics either:
          // there is no flight to project, so the card lands where the
          // pointer let go of it whatever the mode says.
          const target = gentle
            ? hitPart(g.frozen, pointer, 0)
            : host.mode === "throw"
              ? hitPart(g.frozen, projectThrow(centre(), tracker.velocity()), 80)
              : host.mode === "weighted"
                ? hitPart(g.frozen, pointer, 0)
                : hitPart(g.frozen, centre(), 0);
          landing = target && host.allows(item.id, target.key, ctx) ? target : null;

          if (gentle) {
            g.cancel?.();
            settle();
            return;
          }
          flying = true;
          if (landing) {
            const slot = slotOffset(landing, size);
            sx.target = slot.x;
            sy.target = slot.y;
          } else {
            // Refused, or thrown at nothing: the card springs home. No red,
            // no shake — it simply does not stay where it was put.
            sx.target = 0;
            sy.target = 0;
          }
          if (host.mode === "throw") {
            // Hand the hand's own speed to the spring, so the flight starts
            // at the velocity the throw actually had. px/ms -> px/s.
            const v = tracker.velocity();
            sx.v = v.x * 1000;
            sy.v = v.y * 1000;
          }
        };

        card.addEventListener("pointermove", move);
        card.addEventListener("pointerup", up);
        card.addEventListener("pointercancel", up);
      },
      onClickCapture: (event) => {
        if (g.moved) {
          event.preventDefault();
          event.stopPropagation();
        }
      },
    }),

    columnProps: (column, part) => columnAttrs(column.value ?? "", column.value, part),
    columnFooter: () => null,
    overlay: () => null,
  };
}
