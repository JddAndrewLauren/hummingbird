import { useLayoutEffect, useRef } from "react";
import type { CSSProperties, ReactNode } from "react";

// A headline that is always exactly one line, shrunk to whatever size that
// takes (#245's vacation and race panes).
//
// **Why this exists.** Both panes interpolate a name nobody here controls —
// a calendar event's title, a race's official name — into a display-sized
// headline, inside the 320px aside. "Lisbon in 12 days" already overruns
// 34px there, so the choice is wrap, cut, or shrink, and the brand's own
// habit settles it: the product does not hide data it has. Nothing is ever
// truncated; the type gives way instead.
//
// The cost is stated plainly because it is a real one: a fitted headline is
// off the design system's eight-size ladder, and two panes showing names of
// different lengths will show different display sizes. That is the trade the
// single-line rule buys.
//
// **How.** Measured after layout and written straight onto the node — a
// measurement is not application state, and `react-hooks/set-state-in-effect`
// is an error in this repo anyway. `CapturePopover`'s anchor is the same
// idiom, for the same reasons. The fit itself is iterated rather than
// solved — see the loop below for why one ratio is not enough.
//
// The effect deliberately takes no dependency array. The content is a
// countdown — it changes on the region's 30-second tick without any prop
// here changing identity — and re-measuring two nodes per render is cheaper
// than being wrong for 30 seconds.

/** The smallest a headline is allowed to get, below which it overruns its
 * card rather than shrinking further — still one line, still uncut, and
 * visibly wrong in a way that says the *content* is the problem.
 *
 * 10px, measured rather than picked: in a 320px aside the hostile ends of
 * both feeds — "Grandma's 90th birthday in Saskatchewan in 12 days" and
 * "12 days before Formula 1 Louis Vuitton Australian GP" — fit at 11.5px and
 * 11.1px. A 12px floor left both of them overflowing by 12–23px for the sake
 * of half a pixel of type. Anything that still cannot fit at 10px is longer
 * than either feed has ever produced. */
const FLOOR_PX = 10;

export interface FitTextProps {
  /** The size this headline wants, when the content allows it. */
  basePx: number;
  /** The outer block's styling. Use longhand `fontFamily` / `fontWeight` /
   * `lineHeight` rather than the `font` shorthand — the shorthand would
   * reset the very `fontSize` this component is here to set. */
  style?: CSSProperties;
  children: ReactNode;
}

export function FitText({ basePx, style, children }: FitTextProps) {
  const block = useRef<HTMLParagraphElement>(null);
  const line = useRef<HTMLSpanElement>(null);

  useLayoutEffect(() => {
    function fit() {
      const outer = block.current;
      const inner = line.current;
      if (!outer || !inner) {
        return;
      }
      // Back to base first: without this, a headline that got shorter would
      // stay at the size the longer one needed.
      outer.style.fontSize = `${basePx}px`;
      const available = outer.clientWidth;
      if (available === 0) {
        // No layout to measure — jsdom, or a pane inside a hidden subtree.
        // The base size stands, which is the right answer for a card nobody
        // is looking at.
        return;
      }
      // Iterated, not solved in one ratio. Text width is linear in font size,
      // but a line is not only text: the vacation headline puts a fixed
      // `--space-3` between its spans, and that gap does not shrink with the
      // type. One pass therefore lands slightly too big on exactly the
      // content that needed the shrink most. Each pass measures what it
      // actually got, so the fixed part is paid for by the second lap; three
      // laps is far more than any measured case has used.
      let size = basePx;
      for (let pass = 0; pass < 3; pass++) {
        const needed = inner.getBoundingClientRect().width;
        if (needed <= available) {
          return;
        }
        const next = Math.max(FLOOR_PX, Math.floor((size * available) / needed));
        if (next === size) {
          // At the floor, or rounding has stopped moving it: this is as small
          // as this headline gets, and it overruns rather than being cut.
          return;
        }
        size = next;
        outer.style.fontSize = `${size}px`;
      }
    }

    fit();
    window.addEventListener("resize", fit);

    // The display face loads with `font-display: swap`, so the first
    // measurement can be of the fallback face. Re-fit once the real one is in.
    let live = true;
    document.fonts?.ready.then(() => {
      if (live) {
        fit();
      }
    });

    return () => {
      live = false;
      window.removeEventListener("resize", fit);
    };
  });

  return (
    <p ref={block} style={{ fontSize: basePx, margin: 0, ...style }}>
      {/* Inline-block, so its rect is the width the line actually wants
          rather than the width the block was given. */}
      <span ref={line} style={{ display: "inline-block", whiteSpace: "nowrap" }}>
        {children}
      </span>
    </p>
  );
}
