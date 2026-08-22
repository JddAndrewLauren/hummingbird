import { Badge } from "../../components/core/Badge";
import { Card } from "../../components/core/Card";
import { EmptyState } from "../../components/feedback/EmptyState";
import type { QuestionInputs } from "../questions/contract";
import {
  NEVER_POLLED_SUBJECT,
  githubBand,
  githubGapReason,
  githubView,
  observedAtMs,
} from "./github";

// The GitHub workflow pane's own expanded rendering — `KimiPaneExpanded`'s
// shape, carried over: a headline first, supporting detail below it,
// freshness last. There is no dormant rendering here, on the same pane's
// own reasoning — dormant IS the collapsed row, which the shell owns. A
// `distant` (cadence-unreadable) pane is different: it is non-`dormant`, so
// `collapse.ts`'s default rule opens it, and this card is the view the
// reader actually gets — it must carry the same "cadence unreadable" fact
// the collapsed headline does, or the unjudged overdue-ness reads as an
// ordinary healthy card (#314 review round 2).

function ageLabel(ageMs: number): string {
  const hours = Math.floor(ageMs / 3_600_000);
  if (hours < 1) {
    return "under an hour ago";
  }
  if (hours < 48) {
    return `${hours}h ago`;
  }
  return `${Math.floor(hours / 24)}d ago`;
}

/** The pane's content, with no `Card` of its own — what the Status board's
 * expanded tile draws inside the one card it already is. `*PaneExpanded`
 * below is this plus Now's card. */
/** Whether this body draws its own headline.
 *
 * Now's card does (it is the pane's whole rendering there). The Status
 * board's expanded tile does not: that tile's header row already carries the
 * pane's decided `collapsedHeadline`, so a body that drew its own would say
 * the same thing twice, two lines apart. */
export function GithubPaneBody({
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
        <EmptyState
          compact
          icon="cloud-fog"
          headingLevel={3}
          title="No answer yet"
          body="Nothing has polled this yet."
        />
      </>
    );
  }

  const view = githubView(subjectKey, inputs);
  if (view === null) {
    return (
      <>
        <EmptyState
          compact
          icon="cloud-fog"
          headingLevel={3}
          title="No answer yet"
          body={githubGapReason(subjectKey, inputs)}
        />
      </>
    );
  }

  const { body, freshness, stale } = view;
  // The band judges overdue-ness as of the poller's own observation, never
  // this render's clock — `github.rs`'s `observed_at_ms` states why.
  const band = githubBand(body, observedAtMs(inputs.nowMs, freshness));
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
              band === "live" || band === "imminent"
                ? "var(--status-danger-fg)"
                : band === "near" || band === "distant"
                  ? "var(--status-warn-fg)"
                  : "var(--text-primary)",
          }}
        >
          {body.displayName}
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
        <span className="hb-meta">
          {body.lastRunAtMs === null
            ? "never run"
            : `last run ${body.lastRunConclusion ?? "in progress"} (${body.lastRunEvent}), ${ageLabel(inputs.nowMs - body.lastRunAtMs)}`}
        </span>
        <span className="hb-meta">
          {body.lastScheduledSuccessAtMs === null
            ? "no scheduled success on record"
            : `last scheduled success ${ageLabel(inputs.nowMs - body.lastScheduledSuccessAtMs)}`}
        </span>
        {band === "live" || band === "imminent" ? (
          <Badge tone="danger" mono>
            cron stalled
          </Badge>
        ) : band === "distant" ? (
          // The overdue judgement could not be made — `githubBand` bands
          // this `distant`, and this card must say so, not present two
          // green facts unwarned. Two ways in: an unreadable cadence, or a
          // row whose observation instant could not be located.
          <Badge tone="warn" mono>
            {body.declaredCadenceMs === null ? "cadence unreadable" : "observed when unknown"}
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

export function GithubPaneExpanded({
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
    githubView(subjectKey, inputs) === null;
  return (
    <Card padding={gap ? "var(--space-3)" : "var(--space-5)"}>
      <GithubPaneBody subjectKey={subjectKey} inputs={inputs} />
    </Card>
  );
}
