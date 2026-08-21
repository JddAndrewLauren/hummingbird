import type { ReactNode } from "react";
import { Button } from "../../components/core/Button";
import { Card } from "../../components/core/Card";
import { Icon } from "../../components/core/Icon";
import { EmptyState } from "../../components/feedback/EmptyState";
import type { HomeworkItemCore } from "../../decisions/seam";
import type { QuestionInputs } from "../questions/contract";
import { homeworkHeadline, homeworkLink, homeworkView } from "./homework";

// The homework pane's own expanded rendering (#675) — the winning item, the
// notes written on it, and whatever else is still open beneath.
//
// **Read-only, deliberately.** Every affordance this pane could grow (mark
// done, open the item, edit the notes) already exists on the frontier and
// on the item detail panel; adding a second one here would be a second
// write path onto the same item for no reason but proximity, and #675's own
// decision table is flat that the body is a read. The pane's job is that
// the notes are *reachable* without hunting for the item, which a rendering
// alone satisfies. The standing session link does not breach that: a link
// is a navigation, not a second write path onto the item — and it is not
// attached to the winning item at all, which is the whole point of
// "standing". It is drawn in *every* arm below, the empty one and the
// broken one included, for the same reason.
//
// **Not `ItemRow`, which is what this was first built on.** The visual gate
// caught it: `ItemRow` lays a title beside a stage badge, an urgency dot
// and its meta chips on one flexible row, and in the aside's 320px it gave
// the whole width to the chips and ellipsised "Prep for Thursday's session"
// down to `P.` — the same failure mode `docs/SURFACES.md` already records
// finding once before. The chips are not what this pane is answering with
// anyway: the stage and the urgency are the frontier's business, and the
// deadline is already said, in the reader's own terms, by the headline
// above. So the item lines here are a title and (for the ones below the
// winner) their date, set in the aside's own type.
//
// **Nothing here decides.** The band, the answer state and `daysAway`
// arrive from `homework.rs` through `homework.ts`; the words are
// `homeworkHeadline`'s, reused rather than rewritten so the collapsed row
// and this card can never say different things about the same item.

/** One open item below the winner: its title, and its deadline when it has
 * one. Wraps rather than ellipsises — the aside is narrow and a title cut
 * to two characters names nothing. */
function OtherItem({ item }: { item: HomeworkItemCore }) {
  return (
    <div style={{ display: "flex", gap: "var(--space-3)", alignItems: "baseline" }}>
      <Icon name="scroll-text" size={13} style={{ position: "relative", top: 2 }} />
      <span style={{ font: "var(--type-body-sm)", color: "var(--text-secondary)" }}>
        {item.title}
        {item.deadline === null ? null : (
          <span className="hb-meta" style={{ marginLeft: "var(--space-3)" }}>
            {item.deadline}
          </span>
        )}
      </span>
    </div>
  );
}

/** The one shell every arm renders inside, so the standing link is appended
 * to all of them rather than to the answered one alone.
 *
 * The link is a `Button` (the `AlertCard` "Open source" idiom) rather than a
 * bare anchor: `window.open` with `noopener,noreferrer`, matching the one
 * other place this app leaves itself for an operator-supplied URL. Watch the
 * 320px aside — a labelled control there is exactly what ellipsised a title
 * down to `P.` before (see above); only the visual gate's PNG proves this
 * one reads. */
function HomeworkCard({
  link,
  padding,
  children,
}: {
  link: string | null;
  padding: string;
  children: ReactNode;
}) {
  return (
    <Card
      padding={padding}
      style={{ display: "flex", flexDirection: "column", gap: "var(--space-3)" }}
    >
      {children}
      {link === null ? null : (
        <div>
          <Button
            size="sm"
            variant="ghost"
            iconRight="arrow-up-right"
            onClick={() => window.open(link, "_blank", "noopener,noreferrer")}
          >
            Join the session
          </Button>
        </div>
      )}
    </Card>
  );
}

export function HomeworkPaneExpanded({ inputs }: { subjectKey: string; inputs: QuestionInputs }) {
  const facts = homeworkView(inputs);
  const link = homeworkLink(inputs);
  if (facts === null) {
    // Visibly broken, never quietly empty — and it says which thing is
    // missing rather than showing an empty list that reads as "nothing due".
    return (
      <HomeworkCard link={link} padding="var(--space-3)">
        <EmptyState
          compact
          icon="cloud-fog"
          headingLevel={3}
          title="Can't read this device's time zone"
          body="Without it there is no way to say which day a deadline falls on."
        />
      </HomeworkCard>
    );
  }

  const winner = facts.winner;
  if (winner === null) {
    // An empty homework list is good news, reported as a fact — the brand's
    // own rule about empty states.
    return (
      <HomeworkCard link={link} padding="var(--space-3)">
        <EmptyState
          compact
          icon="circle-check"
          headingLevel={3}
          title="No open homework"
          body="Capture one with the @homework context and it shows up here."
        />
      </HomeworkCard>
    );
  }

  return (
    <HomeworkCard link={link} padding="var(--space-5)">
      <span className="hb-meta">{homeworkHeadline(facts)}</span>

      <h3
        style={{
          font: "var(--type-h3)",
          color: "var(--text-primary)",
          margin: 0,
        }}
      >
        {winner.title}
      </h3>

      {/* The whole point of the pane: the preparation notes, in the reader's
          own words, without going to find the item. `pre-wrap` because they
          were typed with their own line breaks and this is the one place
          they are read. */}
      {winner.description === null ? null : (
        <p
          style={{
            font: "var(--type-body-sm)",
            color: "var(--text-secondary)",
            whiteSpace: "pre-wrap",
            margin: 0,
          }}
        >
          {winner.description}
        </p>
      )}

      {facts.others.length === 0 ? null : (
        <>
          <span className="hb-meta" style={{ color: "var(--text-muted)" }}>
            {facts.others.length === 1 ? "1 more open" : `${facts.others.length} more open`}
          </span>
          {facts.others.map((item) => (
            <OtherItem key={item.id} item={item} />
          ))}
        </>
      )}
    </HomeworkCard>
  );
}
