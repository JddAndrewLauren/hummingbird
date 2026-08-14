import type { ReactNode } from "react";

// The screen skeletons every surface shares.
//
// All four style through a class rather than a `style={{}}` object, and their
// rules live in `shell/responsive.css`. That is not a preference: below 640px
// the aside stops being a fixed, sticky, self-scrolling panel and becomes a
// stacked block, and at equal importance a stylesheet rule loses to an
// element's own `style` attribute. `!important` is the one thing that outranks
// it, and undoing four properties on the aside would have meant four of them —
// so the inline objects were deleted rather than supplemented and shouted over.
// That file's header carries the full argument, and its `.hb-aside` rule
// carries the ADR-0015 reachability note this comment used to hold.
//
// `Section` keeps its inline styles: it is not a layout skeleton, nothing about
// it changes with width, and moving it would put non-responsive styling in the
// responsive stylesheet.

export function TwoColumn({ children }: { children: ReactNode }) {
  return <div className="hb-two-column">{children}</div>;
}

export function Column({ children }: { children: ReactNode }) {
  return <div className="hb-column">{children}</div>;
}

// `label` names the panel for assistive tech: a complementary landmark with
// no accessible name is just "complementary", and there is one on four
// different screens.
export function Aside({ label, children }: { label: string; children: ReactNode }) {
  return (
    <aside aria-label={label} className="hb-aside">
      {children}
    </aside>
  );
}

/** A single-column surface capped at the content measure. */
export function SingleColumn({ children }: { children: ReactNode }) {
  return <div className="hb-single-column">{children}</div>;
}

export function Section({
  title,
  meta,
  children,
  id,
}: {
  title: string;
  meta?: string;
  children: ReactNode;
  /** An in-page anchor target, so one part of a screen can route the reader
   * to another (#121: the calendar picker's locked row points at the
   * bindings editor further down Settings). */
  id?: string;
}) {
  return (
    <div id={id}>
      <div
        style={{
          display: "flex",
          alignItems: "baseline",
          justifyContent: "space-between",
          marginBottom: "var(--space-4)",
        }}
      >
        {/* h2 under the header's h1: `--type-h3` is the size token, not the
            level — heading levels must not skip. */}
        <h2 style={{ font: "var(--type-h3)", color: "var(--text-primary)" }}>{title}</h2>
        {meta ? <span className="hb-meta">{meta}</span> : null}
      </div>
      {children}
    </div>
  );
}
