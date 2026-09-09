// THROWAWAY (#801, Phase 1). Variant C — pick, then place. No drag at all.
//
// The zero-motion pole, and the only variant that is native to every input the
// product actually has: `m` (or Enter on the card's own move control) picks a
// card up; every column that would accept it lights and grows a `Move here`
// button in its footer; Escape puts it down. Nothing follows a pointer,
// nothing is thrown, and a screen reader gets the whole gesture for free
// because it is two ordinary button presses.
//
// It is here to make the drag variants earn their complexity. If moving a card
// twice a week is what actually happens, this is enough — and it is the only
// one of the four that works identically on the phone web form.

import { Button } from "../../components/core/Button";
import { cardAttrs, columnAttrs } from "./spring";
import { LANDING, LIT_STYLE } from "./styles";
import type { DropHost, VariantImpl } from "./seam";

/** How long a press has to last to read as "pick this up" rather than as a
 * tap that opens the item. Long enough not to fire on a click, short enough
 * not to feel broken. */
const LONG_PRESS_MS = 350;

export function variantC(host: DropHost): VariantImpl {
  const g = host.gesture;
  let timer: ReturnType<typeof setTimeout> | null = null;

  const pick = (id: string | null) => {
    g.picked = id;
    host.bump();
  };

  const clearTimer = () => {
    if (timer !== null) clearTimeout(timer);
    timer = null;
  };

  return {
    cardProps: (item) => ({
      ...cardAttrs(item.id),
      style: g.picked === item.id ? LANDING : undefined,
      "aria-grabbed": g.picked === item.id ? true : undefined,
      onKeyDown: (event) => {
        if (event.key === "m" || event.key === "M") {
          event.preventDefault();
          pick(g.picked === item.id ? null : item.id);
        }
        if (event.key === "Escape" && g.picked !== null) {
          event.preventDefault();
          pick(null);
        }
      },
      // Touch's half of the same gesture. `pointerup` before the timer fires
      // is an ordinary tap and still opens the item, which is what makes this
      // additive rather than a takeover of the card.
      onPointerDown: () => {
        if (item.pending) return;
        clearTimer();
        timer = setTimeout(() => pick(item.id), LONG_PRESS_MS);
      },
      onPointerUp: clearTimer,
      onPointerLeave: clearTimer,
    }),

    columnProps: (column, part, ctx) => {
      const key = column.value ?? "";
      const legal = g.picked !== null && host.allows(g.picked, key, ctx);
      return {
        ...columnAttrs(key, column.value, part),
        // The board does not recede; the legal columns come forward. Same
        // result, and it never dims something the reader is still reading.
        style: legal ? LIT_STYLE : undefined,
      };
    },

    // The control sits at the foot of the column, where the reveal control
    // already lives — the one place on a column that is not a card.
    columnFooter: (column, part, ctx) => {
      const key = column.value ?? "";
      if (g.picked === null || !host.allows(g.picked, key, ctx)) return null;
      // A column drawn across several lanes would otherwise grow one button
      // per part.
      if (part !== 0) return null;
      const picked = g.picked;
      return (
        <Button
          variant="quiet"
          size="sm"
          fullWidth
          onClick={() => {
            host.drop(picked, key, ctx);
            pick(null);
          }}
        >
          Move here
        </Button>
      );
    },

    overlay: () => {
      if (g.picked === null) return null;
      const item = g.items.get(g.picked);
      return (
        <div
          role="status"
          style={{
            position: "fixed",
            left: "50%",
            bottom: "calc(var(--space-16) + var(--row-height))",
            transform: "translateX(-50%)",
            zIndex: 60,
            display: "flex",
            alignItems: "center",
            gap: "var(--space-4)",
            padding: "var(--space-3) var(--space-5)",
            background: "var(--surface-card)",
            border: "1px solid var(--border-strong)",
            borderRadius: "var(--radius-pill)",
            boxShadow: "var(--shadow-3)",
          }}
        >
          <span style={{ font: "var(--type-body)" }}>Moving {item ? item.title : "an item"}</span>
          <Button variant="ghost" size="sm" onClick={() => pick(null)}>
            Cancel
          </Button>
        </div>
      );
    },
  };
}
