import { Button } from "../components/core/Button";
import { EmptyState } from "../components/feedback/EmptyState";

export interface SeamFailureProps {
  /** What `initDecisions()` rejected with, already reduced to a string. Shown
   * verbatim in the mono meta style — it is the one fact worth showing, and
   * paraphrasing it would cost the reader the only clue they have. */
  detail: string;
  /** Injected so a test can prove the control without a real navigation. */
  onReload?: () => void;
}

// The surface a failed decision seam renders INSTEAD of `App` (ADR-0025,
// #141/M1-1 review).
//
// `main.tsx` awaits `initDecisions()` as the loading gate, and the gate's
// whole point is that no component renders against a not-ready seam — the
// wrappers in `decisions/seam.ts` throw rather than fall back to a stale TS
// copy. Rendering `App` anyway on a rejection would defeat exactly that:
// there is no error boundary in this app, so the first synchronous wrapper
// call (`CaptureBox`'s `canSubmitCapture`, on the first keystroke-less
// render of the capture popover) would unmount the whole tree into a blank
// page. So a failure keeps the gate CLOSED and renders this instead — no
// decision consumer is mounted at all.
//
// Deliberately not routed through `coreStore`: that store belongs to the
// SharedWorker handshake, whose `ready` message clears `error`
// (`store/worker-client.ts`), so a worker connecting after this failure
// would erase the report. This surface owns its own text and cannot be
// overwritten.
export function SeamFailure({ detail, onReload = () => window.location.reload() }: SeamFailureProps) {
  return (
    <div
      role="alert"
      style={{
        minHeight: "100dvh",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        padding: "var(--gutter-page)",
        background: "var(--surface-page)",
      }}
    >
      <EmptyState
        icon="info"
        headingLevel={1}
        title="hummingbird can't start"
        body="The decision module failed to load, so nothing here can be trusted to answer. Reloading is the only fix from this side."
        action={
          <Button variant="primary" iconLeft="refresh-cw" onClick={onReload}>
            Reload
          </Button>
        }
      />
      <p
        style={{
          position: "absolute",
          bottom: "var(--space-8)",
          font: "var(--type-meta)",
          letterSpacing: "var(--tracking-meta)",
          textTransform: "uppercase",
          color: "var(--text-muted)",
          maxWidth: "60ch",
          textAlign: "center",
        }}
      >
        {detail}
      </p>
    </div>
  );
}
