import { useEffect, useRef, useState } from "react";
import { Badge } from "../components/core/Badge";
import { Button } from "../components/core/Button";
import { Card } from "../components/core/Card";
import { IconButton } from "../components/core/IconButton";
import { Checkbox } from "../components/forms/Checkbox";
import { Input } from "../components/forms/Input";
import { Textarea } from "../components/forms/Textarea";
import type { GrillProposal, GrillQuestion } from "../skills/envelope";
import type { GrillTurnState } from "../skills/grill-turn-state";
import { planReplacementLabel, wouldStrandPlan } from "./grill-review";
import type { GrillCompletion } from "../store/worker-client";
import type { StepDTO, TaskItemDTO } from "../store/protocol";
import { formatGrillTranscript, type GrillTurn } from "../skills/grill-args";

/** The Triage row's center-column takeover (#355, ADR-0023): the interview,
 * one typed turn at a time (ADR-0023 decision 1), ending in the editable
 * review card. Leaving mid-interview (Back, or closing the tab) never loses
 * the conversation: it is saved automatically through the core, device-local
 * (#356) — `Discard`, below, is the one EXPLICIT, confirmed way to actually
 * drop it. A decline is NOT a dead end either: "Try again" (below) re-asks
 * with the transcript already threaded, which is what makes the transcript
 * "resumable" a real property rather than a claim. This component renders
 * the current `GrillTurnState` and nothing else; every decision about turns,
 * transcripts, the draft, the Steps snapshot and the Confirm mutation
 * belongs to `shell/useGrillTakeoverWiring.ts`. */
export interface GrillTakeoverProps {
  item: TaskItemDTO;
  /** This session's frozen Steps snapshot (`useGrillTakeoverWiring.ts`'s
   * `sessionSteps`) — `null` until it lands, which the review card and its
   * Confirm button both read as "not ready yet", never as "no Steps". */
  steps: StepDTO[] | null;
  turn: GrillTurnState;
  /** Every completed round so far — the review card's own transcript
   * (`skills/grill-args.ts`'s `formatGrillTranscript`). */
  turns: GrillTurn[];
  onAnswer: (text: string) => void;
  onKeepGrilling: () => void;
  /** "Try again", from a decline — re-asks with the transcript already
   * threaded (`useGrillWiring.ts`'s `onRetry`). */
  onRetry: () => void;
  onConfirm: (completion: GrillCompletion) => void;
  onBack: () => void;
  /** #356's explicit, confirmed "Discard" gesture — `useGrillTakeoverWiring
   * .ts`'s `discard`, only ever called after this component confirms with
   * the human first (below). */
  onDiscard: () => void;
  /** `write-failure.ts`'s `grillCompletionFailureFor`, read by the caller —
   * this component only renders it. */
  completionError: string | null;
}

const CARD_STYLE = { display: "flex", flexDirection: "column" as const, gap: "var(--space-5)" };

function BackRow({
  title,
  onBack,
  onDiscard,
}: {
  title: string;
  onBack: () => void;
  onDiscard: () => void;
}) {
  return (
    <div style={{ display: "flex", alignItems: "center", gap: "var(--space-4)" }}>
      <IconButton icon="x" label="Back to Triage" onClick={onBack} />
      <span className="hb-meta" style={{ flex: "1 1 auto" }}>
        grilling — {title}
      </span>
      {/* #356's explicit, confirmed "Discard": a native confirm dialog is
          the whole of "confirmed" here — the simplest thing that actually
          stops an accidental tap, and this app has no modal system to
          build a second one on top of for a single, rare button. */}
      <Button
        size="sm"
        variant="ghost"
        iconLeft="trash-2"
        onClick={() => {
          if (window.confirm("Discard this grill? The interview so far will be lost.")) {
            onDiscard();
          }
        }}
      >
        Discard
      </Button>
    </div>
  );
}

function Narration({ messages }: { messages: string[] }) {
  if (messages.length === 0) return null;
  return (
    <div role="status" style={{ display: "flex", flexDirection: "column", gap: "var(--space-2)" }}>
      {messages.map((message, index) => (
        <span key={`${index}-${message}`} style={{ font: "var(--type-body-sm)", color: "var(--text-muted)" }}>
          {message}
        </span>
      ))}
    </div>
  );
}

function QuestionCard({
  question,
  onAnswer,
}: {
  question: GrillQuestion;
  onAnswer: (text: string) => void;
}) {
  const [freeText, setFreeText] = useState("");
  return (
    <Card padding="var(--space-6)" style={CARD_STYLE}>
      <p style={{ font: "var(--type-h3)", color: "var(--text-primary)" }}>{question.prompt}</p>
      <p style={{ font: "var(--type-body-sm)", color: "var(--text-secondary)" }}>
        Recommended: {question.recommendedAnswer}
      </p>
      <div style={{ display: "flex", gap: "var(--space-4)", flexWrap: "wrap" }}>
        {question.choices.map((choice) => (
          <Button key={choice} size="lg" variant="secondary" onClick={() => onAnswer(choice)}>
            {choice}
          </Button>
        ))}
      </div>
      <form
        style={{ display: "flex", gap: "var(--space-4)", alignItems: "flex-end", flexWrap: "wrap" }}
        onSubmit={(event) => {
          event.preventDefault();
          if (freeText.trim().length === 0) return;
          onAnswer(freeText);
          setFreeText("");
        }}
      >
        <Input
          label="Or answer in your own words"
          size="lg"
          value={freeText}
          onChange={(event) => setFreeText(event.target.value)}
          style={{ flex: "1 1 240px" }}
        />
        <Button type="submit" size="lg" variant="primary">
          Answer
        </Button>
      </form>
    </Card>
  );
}

function ReviewCard({
  item,
  steps,
  proposal,
  turns,
  onKeepGrilling,
  onConfirm,
  completionError,
}: {
  item: TaskItemDTO;
  steps: StepDTO[] | null;
  proposal: GrillProposal;
  turns: GrillTurn[];
  onKeepGrilling: () => void;
  onConfirm: (completion: GrillCompletion) => void;
  completionError: string | null;
}) {
  const [summary, setSummary] = useState(proposal.summary);
  const [patchText, setPatchText] = useState(() => JSON.stringify(proposal.patch, null, 2));
  const [deleteUntickedPlan, setDeleteUntickedPlan] = useState(false);
  // Which proposal object the editable fields above were last seeded
  // from — `TriageRow.tsx`'s own "adjusting state when a prop changes"
  // pattern (compared during render, not inside an effect), used here for
  // the identical reason: a fresh proposal (a new `Keep grilling` round)
  // must reseed the editable fields and reset the tick to its default-off
  // resting state, and an edit made against a superseded proposal must
  // never survive onto this one.
  const [seenProposal, setSeenProposal] = useState(proposal);
  if (seenProposal !== proposal) {
    setSeenProposal(proposal);
    setSummary(proposal.summary);
    setPatchText(JSON.stringify(proposal.patch, null, 2));
    setDeleteUntickedPlan(false);
  }

  // `null` reads as "nothing to protect yet", the same honest default the
  // Confirm button below also disables on — the snapshot has not landed,
  // not "this item carries no plan".
  const offerPlanReplacement = steps !== null && wouldStrandPlan(proposal.verdict, steps);
  const snapshotReady = steps !== null;

  return (
    <Card padding="var(--space-6)" style={CARD_STYLE}>
      <div style={{ display: "flex", alignItems: "center", gap: "var(--space-4)" }}>
        <Badge tone={proposal.verdict === "resolved" ? "success" : "warn"}>
          {proposal.verdict === "resolved" ? "Resolved" : "Fog remains"}
        </Badge>
      </div>
      <Textarea
        label="Summary"
        rows={3}
        value={summary}
        onChange={(event) => setSummary(event.target.value)}
      />
      <Textarea
        label="Proposed edit"
        hint="Recorded on the Grill — never applied to the item automatically."
        rows={4}
        value={patchText}
        onChange={(event) => setPatchText(event.target.value)}
      />
      {offerPlanReplacement ? (
        <Checkbox
          checked={deleteUntickedPlan}
          onChange={(event) => setDeleteUntickedPlan(event.target.checked)}
          label={planReplacementLabel(steps ?? [])}
        />
      ) : null}
      {completionError ? (
        <p role="alert" style={{ font: "var(--type-body-sm)", color: "var(--status-danger-fg)", margin: 0 }}>
          {completionError}
        </p>
      ) : null}
      <div style={{ display: "flex", gap: "var(--space-4)", justifyContent: "flex-end", flexWrap: "wrap" }}>
        <Button size="lg" variant="secondary" iconLeft="help-circle" disabled={item.pending} onClick={onKeepGrilling}>
          Keep grilling
        </Button>
        <Button
          size="lg"
          variant="primary"
          iconLeft="check"
          disabled={item.pending || !snapshotReady}
          onClick={() =>
            onConfirm({
              transcript: formatGrillTranscript(turns),
              summary,
              verdict: proposal.verdict,
              modelProposal: JSON.stringify(proposal.patch),
              appliedPatch: patchText,
              deleteUntickedPlan,
            })
          }
        >
          Confirm
        </Button>
      </div>
    </Card>
  );
}

export function GrillTakeover({
  item,
  steps,
  turn,
  turns,
  onAnswer,
  onKeepGrilling,
  onRetry,
  onConfirm,
  onBack,
  onDiscard,
  completionError,
}: GrillTakeoverProps) {
  // Focus moves into the takeover the moment it mounts — otherwise it opens
  // with focus left wherever the "Grill me" button was, which is no longer
  // on screen at all (the whole row list is gone). `tabIndex={-1}`: the
  // container is a landmark to land keyboard focus on, never a tab stop of
  // its own.
  const containerRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    containerRef.current?.focus();
  }, []);

  return (
    <div
      ref={containerRef}
      tabIndex={-1}
      style={{ display: "flex", flexDirection: "column", gap: "var(--space-5)", outline: "none" }}
    >
      <BackRow title={item.title} onBack={onBack} onDiscard={onDiscard} />

      {turn.phase === "asking" ? (
        <Card padding="var(--space-6)" style={CARD_STYLE}>
          <Narration messages={turn.messages} />
        </Card>
      ) : null}

      {turn.phase === "question" ? <QuestionCard question={turn.question} onAnswer={onAnswer} /> : null}

      {turn.phase === "proposal" ? (
        <ReviewCard
          item={item}
          steps={steps}
          proposal={turn.proposal}
          turns={turns}
          onKeepGrilling={onKeepGrilling}
          onConfirm={onConfirm}
          completionError={completionError}
        />
      ) : null}

      {turn.phase === "declined" ? (
        <Card padding="var(--space-6)" style={CARD_STYLE}>
          <Narration messages={turn.messages} />
          <p role="alert" style={{ font: "var(--type-body)", color: "var(--status-danger-fg)", margin: 0 }}>
            {turn.reason}
          </p>
          {/* The transcript threaded so far survives a decline — `onRetry`
              re-asks with it intact, never a fresh empty interview. A
              single control, per the brief's "no comforts": no automatic
              retry, one deliberate button. */}
          <div style={{ display: "flex", justifyContent: "flex-end" }}>
            <Button size="lg" variant="secondary" iconLeft="rotate-ccw" onClick={onRetry}>
              Try again
            </Button>
          </div>
        </Card>
      ) : null}
    </div>
  );
}
