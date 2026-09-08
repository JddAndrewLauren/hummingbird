// ADR-0036's file links, as an item panel offers a new one: `Add` and the
// Dropbox glyph, opening one path field.
//
// **Only two of the three states its neighbours have.** `LinkAttach` and
// `NoteLink` each own one column, so each can show what it points at and
// offer to change it. A file link is neither — an item points at *many*
// files, they are rows in `file_links` rather than a field, and ADR-0036
// makes them add-and-remove-only: a row is never re-pointed. So what an item
// already has is the list the panel draws below, where each row carries its
// own Open and its own remove, and this control is only ever the offer to
// add one more. The button is the same shape as the other two so the row
// reads as three of one thing; what it opens onto is different because the
// thing behind it is.
//
// **It judges the path and writes nothing.** `normalizePastedPath` and
// `isValidFilePath` are `dropbox/file-link.ts`'s — the whole of this repo's
// Dropbox vendor knowledge, shared with the capture box's own file field —
// and the message for a bad one is that module's `FILE_PATH_PROBLEM`, so the
// two surfaces cannot invent separate accounts of what is wrong.
//
// **The write, and the one-at-a-time rule, stay with the panel.** `onAdd`
// and `disabled` come from the caller because the panel's remove buttons
// share the same outstanding-seed gate: a second write while one is in
// flight would mint a seed for a row the first has already changed, and the
// authority's answer to it would flash a failure after a success. One owner
// for that state, and it is the component that draws both halves.

import { useState } from "react";
import { Button } from "../core/Button";
import { Input } from "../forms/Input";
import { FILE_PATH_PROBLEM, isValidFilePath, normalizePastedPath } from "../../dropbox/file-link";

export interface FileAttachProps {
  /** Called with a normalized, shape-checked path. The caller mints the
   * seed and owns the result — see the header. */
  onAdd: (path: string) => void;
  /** This device's Dropbox folder, or `null` when Settings has none:
   * `normalizePastedPath`'s second argument, nothing more. */
  localRoot: string | null;
  /** True while this panel has a file-link write in flight. */
  disabled: boolean;
}

export function FileAttach({ onAdd, localRoot, disabled }: FileAttachProps) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState("");

  if (!editing) {
    return (
      <Button
        size="sm"
        variant="secondary"
        iconRight="dropbox"
        aria-label="Add a file"
        title="Add a file"
        onClick={() => setEditing(true)}
      >
        Add
      </Button>
    );
  }

  // Normalized before it is judged, because pasting a path out of a file
  // manager — absolute, quoted, backslashed — is how one usually arrives.
  const path = normalizePastedPath(draft, localRoot);
  const invalid = path !== "" && !isValidFilePath(path);
  const submit = () => {
    if (path === "" || invalid || disabled) {
      return;
    }
    onAdd(path);
    close();
  };

  /** Every exit empties the field. A path abandoned by Cancel or Escape is
   * abandoned — resurrecting it the next time Add is pressed would offer a
   * value nobody asked for twice. */
  function close(): void {
    setDraft("");
    setEditing(false);
  }

  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        gap: "var(--space-4)",
        // Same reason as `NoteLink`'s editor: the row is a wrapping flex of
        // buttons and a path is a sentence-length value.
        flex: "1 1 100%",
      }}
    >
      <div style={{ display: "flex", alignItems: "flex-start", gap: "var(--space-4)" }}>
        <Input
          label="File path"
          size="sm"
          icon="dropbox"
          value={draft}
          error={invalid ? FILE_PATH_PROBLEM : undefined}
          placeholder="Finance/2026/receipt.pdf"
          style={{ flex: 1, minWidth: 0 }}
          onChange={(event) => setDraft(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              submit();
            }
            if (event.key === "Escape") {
              close();
            }
          }}
        />
        <Button size="sm" iconLeft="check" disabled={path === "" || invalid || disabled} onClick={submit}>
          Add
        </Button>
        <Button size="sm" variant="ghost" onClick={close}>
          Cancel
        </Button>
      </div>
    </div>
  );
}
