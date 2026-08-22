import { Badge } from "../../components/core/Badge";
import { Card } from "../../components/core/Card";
import { PaneGap } from "../questions/PaneGap";
import type { QuestionInputs } from "../questions/contract";
import {
  NEVER_POLLED_SUBJECT,
  uptimeBand,
  uptimeGapReason,
  uptimeView,
} from "./uptime";

// The uptime pane's own expanded rendering — `GithubPaneExpanded`'s shape,
// carried over: a headline first, the raw observation below it, freshness
// last. There is no dormant rendering here, on the same pane's own
// reasoning — dormant IS the collapsed row, which the shell owns. Every
// non-dormant band opens on its own (`collapse.ts`'s default rule), and
// this card must carry the same fact its collapsed headline does — an
// unreachable host's own error text, a wrong status code, or an
// unexpectedly-answering service that should be off — never a silently
// green card behind a red collapsed row.

/** The pane's content, with no `Card` of its own — what the Status board's
 * expanded tile draws inside the one card it already is. `*PaneExpanded`
 * below is this plus Now's card. */
/** Whether this body draws its own headline.
 *
 * Now's card does (it is the pane's whole rendering there). The Status
 * board's expanded tile does not: that tile's header row already carries the
 * pane's decided `collapsedHeadline`, so a body that drew its own would say
 * the same thing twice, two lines apart. */
export function UptimePaneBody({
  subjectKey,
  inputs,
  headline = true,
}: {
  subjectKey: string;
  inputs: QuestionInputs;
  headline?: boolean;
}) {
  if (subjectKey === NEVER_POLLED_SUBJECT) {
    return (
      <>
        <PaneGap
          headline={headline}
          title="No answer yet"
          body="Nothing has polled this yet."
        />
      </>
    );
  }

  const view = uptimeView(subjectKey, inputs);
  if (view === null) {
    return (
      <>
        <PaneGap
          headline={headline}
          title="No answer yet"
          body={uptimeGapReason(subjectKey, inputs)}
        />
      </>
    );
  }

  const { serviceId, body, freshness, stale } = view;
  const band = uptimeBand(body);
  const ageMs = freshness.kind === "age" ? freshness.ageMs : null;

  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        gap: "var(--space-4)",
      }}
    >
      {headline ? (
        <p
          style={{
            font: "var(--weight-bold) 20px/1.1 var(--font-display)",
            letterSpacing: "var(--tracking-heading)",
            color:
              band === "live"
                ? "var(--status-danger-fg)"
                : band === "near"
                  ? "var(--status-warn-fg)"
                  : "var(--text-primary)",
          }}
        >
          {serviceId}
        </p>
      ) : null}

      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: "var(--space-4)",
          flexWrap: "wrap",
        }}
      >
        <span className="hb-meta">expected {body.expected}</span>
        <span className="hb-meta">
          {body.error !== null
            ? `unreachable — ${body.error}`
            : `answered ${body.observedStatus} (wanted ${body.expectStatus})`}
        </span>
        {body.expected === "off" && body.error === null ? (
          <Badge tone="danger" mono>
            reachable when it should be off
          </Badge>
        ) : body.expected === "on" && body.error !== null ? (
          <Badge tone="danger" mono>
            unreachable
          </Badge>
        ) : body.expected === "on" &&
          body.observedStatus !== body.expectStatus ? (
          <Badge tone="warn" mono>
            unexpected status
          </Badge>
        ) : null}
      </div>

      {stale ? (
        <span className="hb-meta" style={{ color: "var(--status-warn-fg)" }}>
          {ageMs === null
            ? "stale — age unknown"
            : `stale — as of ${Math.floor(ageMs / 3_600_000)}h ago`}
        </span>
      ) : null}
    </div>
  );
}

export function UptimePaneExpanded({
  subjectKey,
  inputs,
}: {
  subjectKey: string;
  inputs: QuestionInputs;
}) {
  // The gap arm sits tighter than the answered one, exactly as it did when
  // this component held both trees itself. Re-reading the (pure) view to
  // pick the padding costs a call and keeps the two arms' chrome unchanged.
  const gap =
    subjectKey === NEVER_POLLED_SUBJECT ||
    uptimeView(subjectKey, inputs) === null;
  return (
    <Card padding={gap ? "var(--space-3)" : "var(--space-5)"}>
      <UptimePaneBody subjectKey={subjectKey} inputs={inputs} />
    </Card>
  );
}
