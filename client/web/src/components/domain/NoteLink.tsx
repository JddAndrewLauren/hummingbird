// #771's note affordance, as an item panel draws it: the whole of what an
// operator can do to `items.vault_path` without opening the fields.
//
// **Three states, one control cluster.** An item pointing at nothing offers
// `Add` and the note glyph, which opens an editor prefilled with
// `derivePath(title)` —
// the path is *proposed* and editable before it is stored, never written by
// the click that offers it. An item already pointing somewhere offers "Open
// note" plus an `…` reopening that same editor over the stored value, with
// "Remove link" beside the save. So the three gestures the operator has are
// link, open, and edit-or-unlink, and none of them needs the Edit form.
//
// *This replaced #771's original one-button affordance, where "Start a note"
// silently wrote the derived path on its first click and there was no way to
// choose the path, point at an existing note, or unlink at all. Prototyped as
// three variants (action row / property strip / metadata chip) before it was
// built; the action row won because linking is a thing you do, not a fact the
// panel states, and it keeps the panel's read mode free of a control that is
// empty on almost every item.*
// *That resting label was the words "Link a note" until this component joined
// the link and file affordances in one row: all three say `Add`, and the
// glyph is what names which. The three states themselves are unchanged.*
//
// **Never a second copy of the path rules.** What a path may look like is
// `obsidian/vault-uri.ts`'s `isValidVaultPath`, and the message when it does
// not is `triage-form.ts`'s — the field in the Edit form and the editor here
// are two doors onto one column and must not disagree about it.
//
// **What the invalid stored path still cannot do here.** `vault_path` is a
// plain column the authority only checks for non-blankness, so a writer that
// is not this client — `sweep.py`, a skill, the agent — can leave a path
// this module refuses to send. Read mode then draws nothing at all, exactly
// as it did before: repairing such a path stays the Edit form's job, and a
// bare `…` beside no label is not an affordance anyone would find.

import { useState } from "react";
import { Button } from "../core/Button";
import { IconButton } from "../core/IconButton";
import { Input } from "../forms/Input";
import { buildUri, derivePath, isValidVaultPath } from "../../obsidian/vault-uri";
import { VAULT_PATH_PROBLEM } from "../../screens/triage-form";
import type { TaskItemDTO } from "../../store/protocol";
import type { TriageEdits } from "../../store/worker-client";

export interface NoteLinkProps {
  item: TaskItemDTO;
  /** The operator's vault name, off the `obsidian-vault` binding. `null`
   * (unbound) draws nothing: the affordance announces a vault that isn't
   * there. */
  vaultName: string | null;
  /** The panel's own triage save. Absent for a render with no worker behind
   * it (demo mode), which leaves opening — the one gesture that writes
   * nothing — as all this draws. */
  onTriage?: (itemId: string, destination: null, edits: TriageEdits) => void;
}

/** Whether `NoteLink` draws anything, for a caller deciding whether to open
 * the row it would sit in. Same conditions the component itself branches on,
 * named once so the two cannot drift. */
export function noteLinkVisible({ item, vaultName, onTriage }: NoteLinkProps): boolean {
  if (vaultName === null) {
    return false;
  }
  if (item.vaultPath !== null) {
    return isValidVaultPath(item.vaultPath);
  }
  return derivePath(item.title) !== null && onTriage !== undefined;
}

export function NoteLink({ item, vaultName, onTriage }: NoteLinkProps) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState("");

  if (!noteLinkVisible({ item, vaultName, onTriage })) {
    return null;
  }
  const stored = item.vaultPath;

  const open = (path: string) => {
    // Optimistic and deliberately so (#771): there is no `x-success` round
    // trip and the web has no router to receive one.
    // `obsidian://new?…&append` opens the note when it is there and creates
    // it when it is not, so re-clicking is always safe — which is what makes
    // a confirmation unnecessary rather than merely omitted.
    window.open(buildUri(vaultName ?? "", path), "_blank", "noopener,noreferrer");
  };

  const write = (path: string | null) => {
    // `destination: null` (#122): pointing an item at a note moves it
    // through no stage, exactly as detail mode's own Edit save does.
    onTriage?.(item.id, null, { vaultPath: path });
    setEditing(false);
  };

  const startEditing = () => {
    setDraft(stored ?? derivePath(item.title) ?? "");
    setEditing(true);
  };

  if (editing) {
    const typed = draft.trim();
    const invalid = typed.length > 0 && !isValidVaultPath(typed);
    return (
      <div
        style={{
          display: "flex",
          flexDirection: "column",
          gap: "var(--space-4)",
          // The row this sits in is a wrapping flex of buttons; a path is a
          // sentence-length value, so the editor takes the whole line rather
          // than the width one button leaves it.
          flex: "1 1 100%",
        }}
      >
        <div style={{ display: "flex", alignItems: "flex-start", gap: "var(--space-4)" }}>
          <Input
            size="sm"
            icon="notebook-text"
            value={draft}
            error={invalid ? VAULT_PATH_PROBLEM : undefined}
            placeholder="Hummingbird/Knee rehab.md"
            style={{ flex: 1, minWidth: 0 }}
            onChange={(event) => setDraft(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter" && !invalid && typed.length > 0 && !item.pending) {
                write(typed);
              }
              if (event.key === "Escape") {
                setEditing(false);
              }
            }}
          />
          <Button
            size="sm"
            iconLeft="check"
            // Same rule as the Edit form's Save: an unconfirmed mutation on
            // this item blocks the next one.
            disabled={invalid || typed.length === 0 || item.pending}
            onClick={() => write(typed)}
          >
            {stored === null ? "Link" : "Save"}
          </Button>
          <Button size="sm" variant="ghost" onClick={() => setEditing(false)}>
            Cancel
          </Button>
        </div>
        {stored !== null ? (
          <div>
            {/* Its own line, and `danger`: unlinking is the one gesture here
                that throws a stored value away. It never touches the note —
                only the pointer — which is why it asks for no confirmation. */}
            <Button
              size="sm"
              variant="danger"
              iconLeft="trash-2"
              disabled={item.pending}
              onClick={() => write(null)}
            >
              Remove link
            </Button>
          </div>
        ) : null}
      </div>
    );
  }

  if (stored === null) {
    return (
      <Button
        size="sm"
        variant="secondary"
        iconRight="notebook-text"
        // The glyph is the noun, so the label has to do the naming the glyph
        // cannot for a screen reader — and it is the hover tooltip too, the
        // same contract the capture box's matching toggle carries.
        aria-label="Add a note"
        title="Add a note"
        onClick={startEditing}
      >
        Add
      </Button>
    );
  }

  return (
    <div style={{ display: "inline-flex", alignItems: "center", gap: "var(--space-3)" }}>
      <Button
        size="sm"
        variant="secondary"
        iconLeft="notebook-text"
        // The same "this leaves the app" glyph `AlertCard`'s "Open source"
        // carries, and for the same reason.
        iconRight="arrow-up-right"
        // Never blocked by a pending mutation: reopening a note the item
        // already points at writes nothing.
        onClick={() => open(stored)}
      >
        Open note
      </Button>
      {onTriage ? (
        <IconButton
          size="sm"
          icon="ellipsis"
          label="Edit or remove the note link"
          onClick={startEditing}
        />
      ) : null}
    </div>
  );
}
