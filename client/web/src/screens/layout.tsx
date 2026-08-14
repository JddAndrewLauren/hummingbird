import type { ReactNode } from "react";

// The screen skeletons every surface shares. The design README fixes the
// rail and the right-hand panel and lets only the centre column scroll; the
// shell owns the scroll container, and `Aside` sticks to the top of it so
// the panel stays put while the column moves under it.

export function TwoColumn({ children }: { children: ReactNode }) {
  return (
    <div
      style={{
        display: "flex",
        gap: "var(--space-8)",
        alignItems: "flex-start",
        flexWrap: "wrap",
      }}
    >
      {children}
    </div>
  );
}

export function Column({ children }: { children: ReactNode }) {
  return (
    <div
      style={{
        flex: 1,
        // `min()` rather than a bare 380: a fixed minimum cannot be honoured
        // below 380px of container, so the column overflowed the page
        // horizontally instead of shrinking. Desktop behaviour is unchanged —
        // at every width the three existing visual projects cover, the
        // container already exceeds 380, so this resolves to 380px exactly.
        minWidth: "min(380px, 100%)",
        display: "flex",
        flexDirection: "column",
        gap: "var(--space-7)",
      }}
    >
      {children}
    </div>
  );
}

// `label` names the panel for assistive tech: a complementary landmark with
// no accessible name is just "complementary", and there is one on four
// different screens.
export function Aside({ label, children }: { label: string; children: ReactNode }) {
  return (
    <aside
      aria-label={label}
      style={{
        width: "var(--panel-width)",
        flex: "0 0 auto",
        position: "sticky",
        top: 0,
        alignSelf: "flex-start",
        // The panel is sticky, so its height is whatever its content is —
        // and once Now's aside holds a ranked region that grows with the
        // number of standing questions (#245, ADR-0015), that content can
        // exceed the viewport and simply be unreachable: the shell's one
        // scroll container scrolls the *page*, past a panel that is stuck to
        // the top. Capping it at the viewport and letting it scroll itself is
        // what keeps the bottom of the panel reachable on every screen that
        // has one (Now, Settings, Alerts, Routes).
        maxHeight: "100dvh",
        overflowY: "auto",
        display: "flex",
        flexDirection: "column",
        gap: "var(--space-6)",
      }}
    >
      {children}
    </aside>
  );
}

/** A single-column surface capped at the content measure. */
export function SingleColumn({ children }: { children: ReactNode }) {
  return (
    <div
      style={{
        maxWidth: "var(--content-max)",
        display: "flex",
        flexDirection: "column",
        gap: "var(--space-7)",
      }}
    >
      {children}
    </div>
  );
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
