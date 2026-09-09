// THROWAWAY (#801, Phase 1). Variant A — native drag, the whole column is the
// target.
//
// The cheap baseline, and the shape Phase 2 currently assumes: HTML5
// drag-and-drop, the browser's own ghost image, no dependency, no custom
// motion at all. The whole column lights — header included, collapsed or not,
// every continuation part — because a column is one thing however many lanes
// it is drawn across.
//
// What it cannot do, which is half of why the other three exist: touch. HTML5
// DnD does not fire for a finger, so on the phone web form this affordance is
// simply absent.

import { cardAttrs, columnAttrs, columnElements, freeze } from "./spring";
import { paint } from "./styles";
import type { DropHost, VariantImpl } from "./seam";

export function variantA(host: DropHost): VariantImpl {
  const g = host.gesture;

  const light = (key: string | null) => {
    if (g.over === key) return;
    if (g.over !== null) paint(columnElements(g.over), false);
    if (key !== null) paint(columnElements(key), true);
    g.over = key;
  };

  const end = () => {
    light(null);
    g.itemId = null;
  };

  return {
    cardProps: (item) => ({
      // A card whose write is still in flight is not a card to move again.
      draggable: !item.pending,
      ...cardAttrs(item.id),
      onDragStart: (event) => {
        g.itemId = item.id;
        g.frozen = freeze();
        event.dataTransfer.setData("text/plain", item.id);
        event.dataTransfer.effectAllowed = "move";
      },
      onDragEnd: end,
    }),

    columnProps: (column, part, ctx) => {
      const key = column.value ?? "";
      return {
        ...columnAttrs(key, column.value, part),
        onDragOver: (event) => {
          if (g.itemId === null) return;
          if (!host.allows(g.itemId, key, ctx)) {
            // Not preventing the default IS the refusal: the browser then
            // shows its own "you cannot drop here" cursor and no drop event
            // is ever raised. Nothing is painted red.
            event.dataTransfer.dropEffect = "none";
            light(null);
            return;
          }
          event.preventDefault();
          event.dataTransfer.dropEffect = "move";
          light(key);
        },
        onDragLeave: (event) => {
          // `dragleave` also fires crossing into a child element, which would
          // strobe the tint over every card in the column.
          const to = event.relatedTarget;
          if (to instanceof Node && event.currentTarget.contains(to)) return;
          if (g.over === key) light(null);
        },
        onDrop: (event) => {
          event.preventDefault();
          const id = event.dataTransfer.getData("text/plain") || g.itemId;
          if (id) host.drop(id, key, ctx);
          end();
        },
      };
    },

    columnFooter: () => null,
    overlay: () => null,
  };
}
