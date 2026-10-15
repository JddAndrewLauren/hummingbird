import { useState } from "react";
import { Badge } from "../core/Badge";
import { Button } from "../core/Button";
import type { FetchTranscript, TranscriptFailure } from "../../grill/transcript-client";
import type { GrillDTO } from "../../store/protocol";
import { relativeAge } from "../../shell/sync-status";
import { StageBadge } from "./StageBadge";

// **The Grill history** (#358, ADR-0023): an item's completed grills,
// newest first — the order is `Core::grills_for`'s, never re-sorted here.
// Drawn in two places, the Grill takeover and item detail, from one
// component so the two never disagree about what a grill looked like.
//
// **Collapsed by default, and the collapsed list costs nothing.** Every row
// it draws comes from the mirror (`TaskState.grillsByItem`), which the sweep
// already carries; the one thing the sweep does not carry is the transcript
// (ADR-0023 decision 4), and that is fetched from `GET /api/grills/:id` only
// when a reader expands that one entry's transcript. Nothing here asks for a
// sync.
//
// **What was proposed and what was applied are two facts.** The model's
// proposal and the patch the reader confirmed are drawn as two separately
// labelled blocks and never merged, because the whole point of keeping a
// history is judging whether grilling is working — and that needs "what it
// suggested" and "what I accepted" side by side.
//
// Backend/model is not drawn: nothing persists it yet (#358's scope gate).

export interface GrillHistoryProps {
  /** `TaskState.grillsByItem[itemId]` — `undefined` until the mirror read
   * answers, which draws nothing rather than claiming "never grilled". */
  grills: GrillDTO[] | undefined;
  /** `grill/transcript-client.ts`'s fetch, the only network this makes. */
  fetchTranscript: FetchTranscript;
  /** For the relative "completed" age; injectable so a test is stable. */
  nowMs?: number;
}

type TranscriptState =
  | { phase: "pending" }
  | { phase: "ok"; transcript: string }
  | { phase: "failed"; reason: TranscriptFailure };

/** What a failed transcript read says — the reason in the product's own
 * register, and never the device token. */
export const TRANSCRIPT_FAILURE_COPY: Record<TranscriptFailure, string> = {
  no_device_token: "No device token is stored — paste one in Settings to read transcripts.",
  unreachable: "The transcript didn't load — the authority isn't answering.",
  rejected: "The authority refused this device's token.",
  not_found: "The authority has no record of this grill.",
  bad_response: "The transcript didn't load — the authority's answer was unreadable.",
};

const VERDICT = {
  resolved: { label: "Resolved", tone: "success" },
  fog_remains: { label: "Fog remains", tone: "warn" },
} as const;

/** A stored patch is JSON text; indent it for reading, or show it verbatim
 * when it is not JSON (the review card lets a reader type anything). */
function readablePatch(text: string): string {
  try {
    return JSON.stringify(JSON.parse(text), null, 2);
  } catch {
    return text;
  }
}

/** Whether the reader confirmed exactly what the model proposed. Compared as
 * parsed JSON so re-indentation alone never reads as an edit. */
function appliedAsProposed(grill: GrillDTO): boolean {
  return readablePatch(grill.modelProposal) === readablePatch(grill.appliedPatch);
}

const PRE_STYLE = {
  font: "var(--type-meta)",
  textTransform: "none" as const,
  letterSpacing: "normal",
  whiteSpace: "pre-wrap" as const,
  overflowWrap: "anywhere" as const,
  margin: 0,
  padding: "var(--space-4)",
  borderRadius: "var(--radius-sm)",
};

const MUTED = { font: "var(--type-body-sm)", color: "var(--text-muted)", margin: 0 };

function PatchBlock({ label, text, tone }: { label: string; text: string; tone: "proposed" | "applied" }) {
  return (
    <section aria-label={label} style={{ display: "flex", flexDirection: "column", gap: "var(--space-2)" }}>
      <span className="hb-meta">{label}</span>
      <pre
        style={{
          ...PRE_STYLE,
          color: tone === "proposed" ? "var(--text-secondary)" : "var(--text-primary)",
          background: tone === "proposed" ? "var(--surface-quiet)" : "transparent",
          border: tone === "proposed" ? "1px solid transparent" : "1px solid var(--border-default)",
        }}
      >
        {readablePatch(text)}
      </pre>
    </section>
  );
}

function GrillEntry({
  grill,
  nowMs,
  transcript,
  onToggleTranscript,
  transcriptOpen,
  onRetry,
}: {
  grill: GrillDTO;
  nowMs: number;
  transcript: TranscriptState | undefined;
  transcriptOpen: boolean;
  onToggleTranscript: () => void;
  onRetry: () => void;
}) {
  const verdict = VERDICT[grill.verdict];
  const same = appliedAsProposed(grill);
  return (
    <li
      style={{
        display: "flex",
        flexDirection: "column",
        gap: "var(--space-4)",
        paddingTop: "var(--space-4)",
        borderTop: "1px solid var(--border-subtle)",
      }}
    >
      <div style={{ display: "flex", alignItems: "center", gap: "var(--space-3)", flexWrap: "wrap" }}>
        <Badge tone={verdict.tone}>{verdict.label}</Badge>
        <StageBadge stage={grill.resultingStage} compact />
        <span className="hb-meta">completed {relativeAge(Math.max(0, nowMs - grill.completedAt))}</span>
      </div>
      <p style={{ font: "var(--type-body)", color: "var(--text-primary)", margin: 0 }}>{grill.summary}</p>
      <PatchBlock label="Proposed by the model" text={grill.modelProposal} tone="proposed" />
      {same ? (
        <p style={MUTED}>Applied as proposed.</p>
      ) : (
        <PatchBlock label="Applied" text={grill.appliedPatch} tone="applied" />
      )}
      <div>
        <Button
          variant="ghost"
          size="sm"
          iconLeft="chevron-down"
          aria-expanded={transcriptOpen}
          onClick={onToggleTranscript}
        >
          {transcriptOpen ? "Hide transcript" : "Show transcript"}
        </Button>
      </div>
      {transcriptOpen ? (
        transcript === undefined || transcript.phase === "pending" ? (
          <p role="status" style={MUTED}>
            Loading transcript…
          </p>
        ) : transcript.phase === "failed" ? (
          <div style={{ display: "flex", alignItems: "center", gap: "var(--space-4)", flexWrap: "wrap" }}>
            <p role="alert" style={{ ...MUTED, color: "var(--status-danger-fg)" }}>
              {TRANSCRIPT_FAILURE_COPY[transcript.reason]}
            </p>
            <Button variant="secondary" size="sm" iconLeft="rotate-ccw" onClick={onRetry}>
              Try again
            </Button>
          </div>
        ) : (
          <pre aria-label="Transcript" style={{ ...PRE_STYLE, color: "var(--text-secondary)", background: "var(--surface-quiet)" }}>
            {transcript.transcript}
          </pre>
        )
      ) : null}
    </li>
  );
}

export function GrillHistory({ grills, fetchTranscript, nowMs }: GrillHistoryProps) {
  // Read once at mount: a completed grill's age is a label, not a clock,
  // and re-reading time on every render is what `react-hooks/purity` bans.
  const [mountedAtMs] = useState(() => Date.now());
  const now = nowMs ?? mountedAtMs;
  const [open, setOpen] = useState(false);
  const [openTranscripts, setOpenTranscripts] = useState<Record<string, boolean>>({});
  // A grill is immutable (ADR-0023 decision 2), so a transcript once read
  // stays correct for the life of this view — kept, never re-fetched on a
  // second expand. Only a failure is ever asked for again, and only on the
  // reader's "Try again".
  const [transcripts, setTranscripts] = useState<Record<string, TranscriptState>>({});

  if (grills === undefined) return null;

  const load = (grillId: string) => {
    setTranscripts((current) => ({ ...current, [grillId]: { phase: "pending" } }));
    void fetchTranscript(grillId).then((result) => {
      setTranscripts((current) => ({
        ...current,
        [grillId]: result.kind === "ok" ? { phase: "ok", transcript: result.transcript } : { phase: "failed", reason: result.reason },
      }));
    });
  };

  const toggleTranscript = (grillId: string) => {
    const opening = !openTranscripts[grillId];
    setOpenTranscripts((current) => ({ ...current, [grillId]: opening }));
    if (opening && transcripts[grillId] === undefined) load(grillId);
  };

  return (
    <section aria-label="Grill history">
      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          gap: "var(--space-4)",
          flexWrap: "wrap",
        }}
      >
        <span className="hb-meta">grill history</span>
        {grills.length > 0 ? (
          <Button variant="ghost" size="sm" iconLeft="chevron-down" aria-expanded={open} onClick={() => setOpen(!open)}>
            {open ? "Hide" : `Show ${grills.length} ${grills.length === 1 ? "grill" : "grills"}`}
          </Button>
        ) : null}
      </div>
      {grills.length === 0 ? (
        <p style={{ ...MUTED, marginTop: "var(--space-3)" }}>Never grilled — no completed grills on this item.</p>
      ) : open ? (
        <ol
          style={{
            listStyle: "none",
            margin: "var(--space-3) 0 0",
            padding: 0,
            display: "flex",
            flexDirection: "column",
            gap: "var(--space-4)",
          }}
        >
          {grills.map((grill) => (
            <GrillEntry
              key={grill.id}
              grill={grill}
              nowMs={now}
              transcript={transcripts[grill.id]}
              transcriptOpen={openTranscripts[grill.id] === true}
              onToggleTranscript={() => toggleTranscript(grill.id)}
              onRetry={() => load(grill.id)}
            />
          ))}
        </ol>
      ) : null}
    </section>
  );
}
