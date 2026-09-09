// THROWAWAY (#801, Phase 1). Variant B — pointer drag with a landing preview,
// and the header strip alone is the target.
//
// Two disagreements with A, both deliberate:
//
//  - **Pointer events, not HTML5 DnD.** The same gesture then works with a
//    finger, which is the whole of what A cannot do. The cost is that
//    everything A gets free — the ghost image, the drop cursor, the
//    auto-scroll — has to be drawn here.
//  - **The target is the column's header strip, not the whole column.** A
//    tall column is a large target that also happens to be full of cards you
//    might have meant to press. Aiming at the name is unambiguous. Whether
//    that reads as precise or as fussy is exactly what the operator is being
//    asked.
//
// The preview is the second idea: rather than tinting the destination, the
// destination *opens a slot* — the cards under the header push down and an
// accent-bordered space appears where the card is about to be. Nothing is
// sprung; the slot opens on the design system's own `--dur-base`.

import { cardAttrs, columnAttrs, freeze, hitPart, type FrozenPart } from "./spring";
import { CARRIED, DROPPED, PART_TRANSITION, VACATED, LANDING } from "./styles";
import type { DropHost, VariantImpl } from "./seam";

/** The header strip's own height — `--row-height`, the system's 44px row that
 * doubles as the minimum touch target. The heading sits in exactly that. */
const HEADER_BAND = 44;

/** Below this the gesture was a click, and the card should open rather than
 * have moved a pixel and swallowed the press. */
const DRAG_SLOP = 5;

function el(name: string): HTMLElement | null {
  return document.querySelector<HTMLElement>(`[data-proto-${name}]`);
}

export function variantB(host: DropHost): VariantImpl {
  const g = host.gesture;

  /** The part whose HEADER the pointer is over — not whose body. */
  const headerUnder = (x: number, y: number): FrozenPart | null => {
    const part = hitPart(g.frozen, { x, y }, 0);
    if (!part) return null;
    return y <= part.rect.top + HEADER_BAND ? part : null;
  };

  const cardsIn = (key: string): HTMLElement[] =>
    [...document.querySelectorAll<HTMLElement>(`[data-proto-col="${CSS.escape(key)}"] [data-proto-card]`)];

  const openSlot = (key: string | null, height: number) => {
    if (g.over === key) return;
    if (g.over !== null) {
      for (const card of cardsIn(g.over)) card.style.transform = "";
    }
    const landing = el("landing");
    if (key === null) {
      if (landing) landing.style.display = "none";
      g.over = key;
      return;
    }
    for (const card of cardsIn(key)) {
      card.style.transition = PART_TRANSITION;
      card.style.transform = `translateY(${height + 12}px)`;
    }
    const part = g.frozen.find((p) => p.key === key);
    if (landing && part) {
      Object.assign(landing.style, {
        display: "block",
        left: `${part.rect.left}px`,
        top: `${part.rect.top + HEADER_BAND + 12}px`,
        width: `${part.rect.width}px`,
        height: `${height}px`,
      });
    }
    g.over = key;
  };

  const reset = (card: HTMLElement | null) => {
    openSlot(null, 0);
    if (card) Object.assign(card.style, DROPPED);
    const ghost = el("ghost");
    if (ghost) ghost.style.display = "none";
    g.itemId = null;
    g.moved = false;
  };

  return {
    cardProps: (item, ctx) => ({
      ...cardAttrs(item.id),
      // A finger dragging this must not also scroll the page.
      style: { touchAction: "none" },
      onPointerDown: (event) => {
        if (item.pending || event.button !== 0) return;
        const card = event.currentTarget as HTMLElement;
        const rect = card.getBoundingClientRect();
        g.itemId = item.id;
        g.moved = false;
        g.frozen = freeze();
        g.origin = { x: event.clientX, y: event.clientY };
        g.cardOrigin = { x: rect.left, y: rect.top };
        card.setPointerCapture(event.pointerId);

        const ghost = el("ghost");
        if (ghost) {
          Object.assign(ghost.style, {
            display: "block",
            left: `${rect.left}px`,
            top: `${rect.top}px`,
            width: `${rect.width}px`,
            height: `${rect.height}px`,
          });
        }

        const move = (moveEvent: PointerEvent) => {
          const dx = moveEvent.clientX - g.origin.x;
          const dy = moveEvent.clientY - g.origin.y;
          if (!g.moved && Math.hypot(dx, dy) < DRAG_SLOP) return;
          if (!g.moved) {
            g.moved = true;
            Object.assign(card.style, CARRIED);
          }
          card.style.transform = `translate(${dx}px, ${dy}px)`;
          const over = headerUnder(moveEvent.clientX, moveEvent.clientY);
          const key = over && host.allows(item.id, over.key, ctx) ? over.key : null;
          openSlot(key, rect.height);
        };

        const up = () => {
          card.removeEventListener("pointermove", move);
          card.removeEventListener("pointerup", up);
          card.removeEventListener("pointercancel", up);
          if (g.moved && g.over !== null) host.drop(item.id, g.over, ctx);
          reset(card);
        };

        card.addEventListener("pointermove", move);
        card.addEventListener("pointerup", up);
        card.addEventListener("pointercancel", up);
      },
      // The press that ended a drag must not also open the item panel.
      onClickCapture: (event) => {
        if (g.moved) {
          event.preventDefault();
          event.stopPropagation();
        }
      },
    }),

    columnProps: (column, part) => columnAttrs(column.value ?? "", column.value, part),
    columnFooter: () => null,

    // Both floating pieces are drawn once and then moved imperatively — a
    // React re-render per pointer event would re-run `groupFrontier`, which
    // `spring.ts`'s header explains is not affordable.
    overlay: () => (
      <>
        <div
          data-proto-ghost=""
          aria-hidden="true"
          style={{ ...VACATED, position: "fixed", display: "none", zIndex: 30, borderRadius: "var(--radius-card)", border: "1px solid var(--border-subtle)" }}
        />
        <div
          data-proto-landing=""
          aria-hidden="true"
          style={{ ...LANDING, position: "fixed", display: "none", zIndex: 30 }}
        />
      </>
    ),
  };
}
