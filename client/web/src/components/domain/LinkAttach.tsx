// #782's Link, as an item panel draws it: the whole of what an operator can
// do to `items.link_url`/`items.link_label` without opening the fields.
//
// **The same three states `NoteLink` has**, deliberately — an item pointing
// at nothing offers `Add` and the chain glyph, which opens an editor; one
// already pointing somewhere shows where it goes and offers an `…` over that
// same editor, with **Remove link** beside the save. The two components sit
// side by side in one row, so a shape that differed between them would read
// as a difference in meaning that isn't there.
//
// **It replaced the panel's own `Edit link` button.** That button opened the
// *whole* Edit form to reach two fields, which made the Edit form and this
// row two live editors over one column — the thing `NoteLink`'s header
// already refuses for `vault_path`. It also absorbed the always-visible
// anchor that used to sit above the badges: this button carries the same
// `linkDisplayLabel` text, the same target and the same "leaves the app"
// glyph, so keeping both would have been the link drawn twice in one read
// mode. #782's "always visible wherever the item is opened" still holds —
// this row is read mode's, and read mode is where an opened item lands.
//
// **Never a second copy of the rules.** Whether a stored URL may be followed
// at all is `decisions/seam.ts`'s `linkIsFollowable`, the core's rule shared
// with Android's `ACTION_VIEW`; what a link is called is the same file's
// `linkDisplayLabel`; and "a name needs a URL" is `linkLabelProblem`, which
// the capture box, the Edit form and the authority all read too.
//
// **What an unfollowable stored URL still gets.** The column is plain text
// the authority checks only for non-blankness, so a writer that is not this
// client can leave a `mailto:` or a bare word in it. This draws no anchor
// for one — an anchor to a scheme this client would not vouch for is a click
// it should not offer — but it still draws the `…`, because the operator
// needs a way to repair it and the Edit form is no longer the door.

import { useState } from "react";
import { Button } from "../core/Button";
import { IconButton } from "../core/IconButton";
import { Input } from "../forms/Input";
import { linkDisplayLabel, linkIsFollowable, linkLabelProblem } from "../../decisions/seam";
import type { TaskItemDTO } from "../../store/protocol";
import type { TriageEdits } from "../../store/worker-client";

export interface LinkAttachProps {
  item: TaskItemDTO;
  /** The panel's own triage save. Absent for a render with no worker behind
   * it (demo mode), which leaves following the link — the one gesture that
   * writes nothing — as all this draws. */
  onTriage?: (itemId: string, destination: null, edits: TriageEdits) => void;
}

/** Whether `LinkAttach` draws anything. Unlike `NoteLink`'s twin this is
 * always `true` when there is anywhere to write: a link needs no binding and
 * no device-local fact, so "no link yet" is an offer to make one rather than
 * a reason to draw nothing. It exists so the row's three gates read alike at
 * the call site. */
export function linkAttachVisible({ item, onTriage }: LinkAttachProps): boolean {
  if (onTriage !== undefined) {
    // Something can always be written: an offer to add one, or an offer to
    // repair whatever is stored.
    return true;
  }
  // Read-only, so the only thing left worth drawing is a link that can
  // actually be followed. A stored `mailto:` with nothing to edit it with is
  // an empty row.
  return item.linkUrl !== null && linkIsFollowable(item.linkUrl);
}

export function LinkAttach({ item, onTriage }: LinkAttachProps) {
  const [editing, setEditing] = useState(false);
  const [url, setUrl] = useState("");
  const [label, setLabel] = useState("");

  if (!linkAttachVisible({ item, onTriage })) {
    return null;
  }
  const stored = item.linkUrl;

  const write = (nextUrl: string | null, nextLabel: string | null) => {
    // `destination: null` (#122): pointing an item at a URL moves it through
    // no stage, exactly as detail mode's own Edit save does.
    onTriage?.(item.id, null, { linkUrl: nextUrl, linkLabel: nextLabel });
    setEditing(false);
  };

  const startEditing = () => {
    setUrl(stored ?? "");
    setLabel(item.linkLabel ?? "");
    setEditing(true);
  };

  if (editing) {
    const typedUrl = url.trim();
    const typedLabel = label.trim();
    const problem = linkLabelProblem(typedUrl, typedLabel);
    return (
      <div
        style={{
          display: "flex",
          flexDirection: "column",
          gap: "var(--space-4)",
          // The row this sits in is a wrapping flex of buttons; a URL is a
          // sentence-length value, so the editor takes the whole line.
          flex: "1 1 100%",
        }}
      >
        <div style={{ display: "flex", alignItems: "flex-start", gap: "var(--space-4)", flexWrap: "wrap" }}>
          <Input
            label="URL"
            size="sm"
            type="url"
            inputMode="url"
            icon="link"
            value={url}
            placeholder="https://"
            style={{ flex: "2 1 200px", minWidth: 0 }}
            onChange={(event) => {
              setUrl(event.target.value);
              // Emptying the URL is a clear, and a clear takes the name with
              // it — here in the form, not only in the patch, so what is left
              // behind cannot read as "a name beside no URL".
              if (event.target.value.trim().length === 0) {
                setLabel("");
              }
            }}
          />
          <Input
            label="Link name"
            size="sm"
            value={label}
            error={problem}
            placeholder="Shown as the host when empty"
            style={{ flex: "1 1 160px", minWidth: 0 }}
            onChange={(event) => setLabel(event.target.value)}
          />
        </div>
        <div style={{ display: "flex", gap: "var(--space-4)", flexWrap: "wrap" }}>
          <Button
            size="sm"
            iconLeft="check"
            // Same rule as the Edit form's Save: an unconfirmed mutation on
            // this item blocks the next one.
            disabled={problem !== undefined || typedUrl.length === 0 || item.pending}
            onClick={() => write(typedUrl, typedLabel === "" ? null : typedLabel)}
          >
            {stored === null ? "Link" : "Save"}
          </Button>
          <Button size="sm" variant="ghost" onClick={() => setEditing(false)}>
            Cancel
          </Button>
          {stored !== null ? (
            // `danger`, and it clears BOTH halves: a name beside no URL is
            // the one state this pair may never be in. It throws away a
            // stored value and nothing else, which is why it asks for no
            // confirmation.
            <Button
              size="sm"
              variant="danger"
              iconLeft="trash-2"
              disabled={item.pending}
              onClick={() => write(null, null)}
            >
              Remove link
            </Button>
          ) : null}
        </div>
      </div>
    );
  }

  if (stored === null) {
    return (
      <Button
        size="sm"
        variant="secondary"
        iconRight="link"
        aria-label="Add a link"
        title="Add a link"
        onClick={startEditing}
      >
        Add
      </Button>
    );
  }

  return (
    <div style={{ display: "inline-flex", alignItems: "center", gap: "var(--space-3)" }}>
      {linkIsFollowable(stored) ? (
        // **A real anchor**, through `Button`'s `href` — not a button that
        // calls `window.open`. This control's whole job is to go somewhere,
        // and only an anchor is announced as a link, listed in a screen
        // reader's link rotor, and carries middle-click, modifier-click,
        // "Copy link address" and the status-bar preview. The panel drew an
        // `<a>` before this row existed and must not have lost that in the
        // move. Never disabled by a pending mutation either: following a
        // link the item already carries writes nothing.
        <Button
          size="sm"
          variant="secondary"
          href={stored}
          target="_blank"
          iconLeft="link"
          // The same "this leaves the app" glyph `AlertCard`'s "Open source"
          // carries, and for the same reason.
          iconRight="arrow-up-right"
        >
          {linkDisplayLabel(stored, item.linkLabel)}
        </Button>
      ) : null}
      {onTriage ? (
        <IconButton
          size="sm"
          icon="ellipsis"
          label="Edit or remove the link"
          onClick={startEditing}
        />
      ) : null}
    </div>
  );
}
