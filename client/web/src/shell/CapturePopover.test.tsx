// @vitest-environment jsdom

// The regression suite for the shell's capture popover and the box inside it.
// It inherited what `TriageScreen.test.tsx` used to hold for the capture box
// (#110 / PR #206, then #208's Energy/Size/Context and #222's clear-on-ok
// rule) and adds the two things the move into the shell introduced: the
// second destination, and the overlay's own keyboard contract.
//
// The deciding logic is unit-tested elsewhere — `canSubmitCapture` refuses an
// empty draft, `capture-destination.ts` names the two stages,
// `capture-meta.ts` resolves the sliders onto the wire vocabulary. What no
// node test can reach is whether the buttons actually consult them, whether
// Enter still means Triage and not the mint, and whether the overlay's ways
// out reach `onClose`. That thread is what these mount. (Escape is not among
// them any more — the shell owns it, `escape-claimants.ts`.)

import { describe, expect, it, vi } from "vitest";
import { CapturePopover } from "./CapturePopover";
import { CONTEXTS } from "../screens/field-vocabulary";
import { VAULT_PATH_PROBLEM } from "../screens/triage-form";
import { FILE_PATH_PROBLEM } from "../dropbox/file-link";
import type { ProjectDTO } from "../store/protocol";
import type { TaskCaptureResult } from "../store/store";
import { fireEvent, render, screen } from "../test/component";

function renderPopover(
  options: {
    open?: boolean;
    demo?: boolean;
    lastCapture?: TaskCaptureResult | null;
    projects?: ProjectDTO[];
    contextSuggestions?: readonly string[];
    /** #771: `null` (the default) is an unbound vault, which draws no note
     * disclosure at all. */
    vaultName?: string | null;
    /** ADR-0036: absent (the default) is a render with no file-link wiring
     * behind it, which draws no file disclosure. */
    fileLinks?: { localRoot: string | null };
    attachmentFailure?: string | null;
  } = {},
) {
  const onSubmit = vi.fn();
  const onClose = vi.fn();
  const props = (lastCapture: TaskCaptureResult | null) => ({
    open: options.open ?? true,
    focusRequestId: 1,
    onClose,
    onSubmit,
    projects: options.projects ?? [],
    contextSuggestions: options.contextSuggestions ?? CONTEXTS,
    demo: options.demo ?? false,
    vaultName: options.vaultName ?? null,
    fileLinks: options.fileLinks,
    attachmentFailure: options.attachmentFailure ?? null,
    lastCapture,
  });
  const view = render(<CapturePopover {...props(options.lastCapture ?? null)} />);
  const rerender = (lastCapture: TaskCaptureResult | null) =>
    view.rerender(<CapturePopover {...props(lastCapture)} />);
  return { onSubmit, onClose, rerender };
}

function field(): HTMLInputElement {
  return screen.getByLabelText("Capture") as HTMLInputElement;
}

/** Every optional field left at rest — what `resolveCaptureFields` hands
 * `onSubmit` when nothing beside the title was touched. */
/** The fourth `onSubmit` argument: a capture that asked for neither a
 * note nor a file, which is every capture in this file. */
const NO_ATTACHMENTS = { vaultPath: null, filePath: null };

const NO_FIELDS = {
  size: null,
  energy: null,
  context: null,
  description: null,
  projectId: null,
  priority: null,
  deadline: null,
  scheduledDate: null,
  linkUrl: null,
  linkLabel: null,
};

describe("CapturePopover — the overlay", () => {
  it("renders nothing at all while closed", () => {
    renderPopover({ open: false });
    expect(screen.queryByLabelText("Capture")).toBeNull();
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("opens as a dialog with the capture field already focused", () => {
    renderPopover();
    expect(screen.getByRole("dialog").getAttribute("aria-modal")).toBe("true");
    expect(document.activeElement).toBe(field());
  });

  // #380: the popover holds no dictation state of its own, only plumbing —
  // `CaptureBox` is mounted here with no speech API (this file's header),
  // which is the resting "not dictating" arm, so the only thing provable at
  // this level is that the report actually reaches through. The cancel and
  // restore mechanics themselves are `CaptureBox.dictation.test.tsx`'s, with
  // the seam mocked, exactly where that file's own header says it must live.
  it("forwards CaptureBox's dictating report straight through, with no opinion of its own", () => {
    const onDictatingChange = vi.fn();
    render(
      <CapturePopover
        open={true}
        focusRequestId={1}
        onClose={vi.fn()}
        onSubmit={vi.fn()}
        projects={[]}
        contextSuggestions={CONTEXTS}
        demo={false}
        lastCapture={null}
        cancelDictationRequestId={0}
        onDictatingChange={onDictatingChange}
      />,
    );
    expect(onDictatingChange).toHaveBeenCalledWith(false);
  });

  it("closes on the close button and on the scrim, and claims no key of its own", () => {
    // Escape is the shell's now (`escape-claimants.ts`) — this popover is its
    // shallowest claimant, but it binds nothing itself, so a stray keydown
    // here must do nothing at all.
    const first = renderPopover();
    fireEvent.keyDown(field(), { key: "Escape" });
    expect(first.onClose).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    expect(first.onClose).toHaveBeenCalledTimes(1);

    // The scrim is the dialog's parent element; a press on the card itself
    // must not close (a drag that ends outside is not a request to close).
    const dialog = screen.getByRole("dialog");
    fireEvent.mouseDown(dialog);
    expect(first.onClose).toHaveBeenCalledTimes(1);
    fireEvent.mouseDown(dialog.parentElement as HTMLElement);
    expect(first.onClose).toHaveBeenCalledTimes(2);
  });
});

describe("CapturePopover — the capture box", () => {
  it("refuses an empty or whitespace-only draft on all three buttons", () => {
    renderPopover();
    const add = screen.getByRole("button", { name: "Triage" });
    const mint = screen.getByRole("button", { name: "Mint action" });
    const today = screen.getByRole("button", { name: "Mint for today" });

    expect(add.hasAttribute("disabled")).toBe(true);
    expect(mint.hasAttribute("disabled")).toBe(true);
    expect(today.hasAttribute("disabled")).toBe(true);

    fireEvent.change(field(), { target: { value: "   " } });
    expect(add.hasAttribute("disabled")).toBe(true);
    expect(mint.hasAttribute("disabled")).toBe(true);
    expect(today.hasAttribute("disabled")).toBe(true);

    fireEvent.change(field(), { target: { value: "Call the plumber" } });
    expect(add.hasAttribute("disabled")).toBe(false);
    expect(mint.hasAttribute("disabled")).toBe(false);
    expect(today.hasAttribute("disabled")).toBe(false);
  });

  it("sends the raw draft to Triage", () => {
    const { onSubmit } = renderPopover();
    fireEvent.change(field(), { target: { value: "  Buy   OAT milk  " } });
    fireEvent.click(screen.getByRole("button", { name: "Triage" }));

    expect(onSubmit).toHaveBeenCalledTimes(1);
    expect(onSubmit).toHaveBeenCalledWith("  Buy   OAT milk  ", "triage", NO_FIELDS, NO_ATTACHMENTS);
  });

  it("sends the skip — the mint button captures straight into Ready", () => {
    const { onSubmit } = renderPopover();
    fireEvent.change(field(), { target: { value: "Order the worktop" } });
    fireEvent.click(screen.getByRole("button", { name: "Mint action" }));

    expect(onSubmit).toHaveBeenCalledTimes(1);
    expect(onSubmit).toHaveBeenCalledWith("Order the worktop", "ready", NO_FIELDS, NO_ATTACHMENTS);
  });

  it("mints for today: straight into Ready with a date-only deadline", () => {
    vi.useFakeTimers();
    try {
      vi.setSystemTime(new Date(2026, 8, 3, 14, 30));
      const { onSubmit } = renderPopover();
      fireEvent.change(field(), { target: { value: "Pay the water bill" } });
      fireEvent.click(screen.getByRole("button", { name: "Mint for today" }));

      expect(onSubmit).toHaveBeenCalledTimes(1);
      expect(onSubmit).toHaveBeenCalledWith("Pay the water bill", "ready", {
        ...NO_FIELDS,
        deadline: "2026-09-03",
      }, NO_ATTACHMENTS);
    } finally {
      vi.useRealTimers();
    }
  });

  it("mints for today over a deadline picked under More details", () => {
    vi.useFakeTimers();
    try {
      vi.setSystemTime(new Date(2026, 8, 3, 14, 30));
      const { onSubmit } = renderPopover();
      fireEvent.change(field(), { target: { value: "Pay the water bill" } });
      fireEvent.click(screen.getByRole("button", { name: "More details" }));
      fireEvent.change(screen.getByLabelText("Deadline"), { target: { value: "2026-10-20" } });
      fireEvent.click(screen.getByRole("button", { name: "Mint for today" }));

      expect(onSubmit).toHaveBeenCalledWith("Pay the water bill", "ready", {
        ...NO_FIELDS,
        deadline: "2026-09-03",
      }, NO_ATTACHMENTS);
    } finally {
      vi.useRealTimers();
    }
  });

  it("submits to Triage on Enter, never to Ready", () => {
    const { onSubmit } = renderPopover();
    fireEvent.change(field(), { target: { value: "Call the plumber" } });
    fireEvent.keyDown(field(), { key: "Enter" });
    expect(onSubmit).toHaveBeenCalledWith("Call the plumber", "triage", NO_FIELDS, NO_ATTACHMENTS);
  });

  it("does not submit on the Enter that commits an IME composition", () => {
    const { onSubmit } = renderPopover();
    fireEvent.change(field(), { target: { value: "植物に水をやる" } });
    fireEvent.keyDown(field(), { key: "Enter", isComposing: true });
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it("stays open and says where the capture went once the result comes back ok", () => {
    const { onSubmit, onClose, rerender } = renderPopover();
    fireEvent.change(field(), { target: { value: "Call the plumber" } });
    fireEvent.click(screen.getByRole("button", { name: "Triage" }));
    expect(onClose).not.toHaveBeenCalled();

    rerender({ seed: "s1", kind: "ok", id: "item-9", error: null });
    expect(screen.getByText(/Added to Triage — Call the plumber/)).toBeDefined();
    expect(field().value).toBe("");
    expect(document.activeElement).toBe(field());

    fireEvent.change(field(), { target: { value: "Order the worktop" } });
    fireEvent.click(screen.getByRole("button", { name: "Mint action" }));
    rerender({ seed: "s2", kind: "ok", id: "item-10", error: null });
    expect(screen.getByText(/Minted into Ready — Order the worktop/)).toBeDefined();
    expect(onSubmit).toHaveBeenCalledTimes(2);
  });
});

// #208's headline acceptance, proved from the rendered controls themselves —
// not just that `resolveCaptureFields` (the pure layer) accepts a
// `CaptureMeta`. The Energy/Size sliders are `role="slider"` elements moved
// with the keyboard (`End` jumps to the last stop, per `Slider.tsx`'s own
// `onKeyDown`), never a plain `<input>`.
describe("CapturePopover — the full field set behind More details", () => {
  const openDetails = () =>
    fireEvent.click(screen.getByRole("button", { name: /more details/i }));

  it("keeps every mint-time field shut until asked", () => {
    // One line and Enter is the fastest thing on the screen, and a form that
    // opens to seven fields taxes every capture for a decision most of them
    // do not make.
    renderPopover();

    expect(screen.queryByLabelText("Description")).toBeNull();
    expect(screen.queryByLabelText("Project")).toBeNull();
    expect(screen.queryByLabelText("Priority")).toBeNull();
    expect(screen.queryByLabelText("Deadline")).toBeNull();
    expect(screen.queryByLabelText("Scheduled date")).toBeNull();
    // The three attachment toggles are behind the same chevron: three more
    // controls in the resting state would tax every capture for a decision
    // almost none of them make.
    expect(screen.queryByRole("button", { name: "Add a link" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Add a note" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Add a file" })).toBeNull();
    expect(
      screen.getByRole("button", { name: /more details/i }).getAttribute("aria-expanded"),
    ).toBe("false");
  });

  it("carries every revealed field onto the submit, in one capture", () => {
    const { onSubmit } = renderPopover({
      projects: [
        {
          id: "proj-1",
          name: "Kitchen",
          description: null,
          githubRepo: null,
          defaultContext: null,
          archivedAt: null,
          createdAt: 0,
          updatedAt: 0,
          version: 0,
        } as ProjectDTO,
      ],
    });
    fireEvent.change(field(), { target: { value: "Order the worktop" } });
    openDetails();

    fireEvent.change(screen.getByLabelText("Description"), {
      target: { value: "the oak one" },
    });
    fireEvent.change(screen.getByLabelText("Project"), { target: { value: "proj-1" } });
    fireEvent.change(screen.getByLabelText("Priority"), { target: { value: "2" } });
    fireEvent.change(screen.getByLabelText("Deadline"), { target: { value: "2026-09-01" } });
    fireEvent.change(screen.getByLabelText("Scheduled date"), {
      target: { value: "2026-08-30" },
    });

    fireEvent.click(screen.getByRole("button", { name: "Triage" }));

    expect(onSubmit).toHaveBeenCalledTimes(1);
    expect(onSubmit).toHaveBeenCalledWith("Order the worktop", "triage", {
      ...NO_FIELDS,
      description: "the oak one",
      projectId: "proj-1",
      priority: 2,
      deadline: "2026-09-01",
      scheduledDate: "2026-08-30",
      linkUrl: null,
      linkLabel: null,
    }, NO_ATTACHMENTS);
  });

  it("names an hour through the deadline field's own second gesture", () => {
    const { onSubmit } = renderPopover();
    fireEvent.change(field(), { target: { value: "Call the vet" } });
    openDetails();
    fireEvent.change(screen.getByLabelText("Deadline"), { target: { value: "2026-09-01" } });
    fireEvent.click(screen.getByRole("button", { name: "Add time" }));
    fireEvent.change(screen.getByLabelText("Time"), { target: { value: "09:30" } });

    fireEvent.click(screen.getByRole("button", { name: "Triage" }));

    expect(onSubmit).toHaveBeenCalledWith("Call the vet", "triage", {
      ...NO_FIELDS,
      deadline: "2026-09-01T09:30",
    }, NO_ATTACHMENTS);
  });

  it("cannot be given an impossible date at all — the controls are pickers", () => {
    // Both date fields are `input[type=date]`, which refuses to hold
    // `2026-02-30`; the value never reaches the form's state, so there is
    // nothing for the submit gate to catch. That gate still runs
    // (`captureMetaProblems`, unit-tested against the same rules triage
    // uses) — this is the pin on WHY it never fires here, so a later change
    // back to a free-text field is a visibly different test rather than a
    // silently unguarded form.
    renderPopover();
    fireEvent.change(field(), { target: { value: "Call the vet" } });
    openDetails();
    fireEvent.change(screen.getByLabelText("Scheduled date"), {
      target: { value: "2026-02-30" },
    });

    expect((screen.getByLabelText("Scheduled date") as HTMLInputElement).value).toBe("");
    expect(screen.getByRole("button", { name: "Triage" }).hasAttribute("disabled")).toBe(false);
    expect(screen.queryByText("Use YYYY-MM-DD")).toBeNull();
  });

  it("keeps the revealed fields while a capture is in flight, and shuts them on ok", () => {
    // #222's rule, now covering five more fields than it did: a failed write
    // must not take the reader's typing with it.
    const { rerender } = renderPopover();
    fireEvent.change(field(), { target: { value: "Call the vet" } });
    openDetails();
    fireEvent.change(screen.getByLabelText("Description"), { target: { value: "about Bess" } });

    rerender({ kind: "failed", seed: "seed-1", id: null, error: "boom" });
    expect((screen.getByLabelText("Description") as HTMLTextAreaElement).value).toBe("about Bess");

    rerender({ kind: "ok", seed: "seed-2", id: "item-1", error: null });
    // Shut again, and empty behind the disclosure — the next capture starts
    // clean rather than inheriting this one's decisions.
    expect(screen.queryByLabelText("Description")).toBeNull();
    openDetails();
    expect((screen.getByLabelText("Description") as HTMLTextAreaElement).value).toBe("");
  });
});

describe("CapturePopover — the capture meta (#208)", () => {
  it("carries the Energy/Size/Context selections onto the submit", () => {
    const { onSubmit } = renderPopover();
    fireEvent.change(field(), { target: { value: "Buy soil" } });
    fireEvent.keyDown(screen.getByRole("slider", { name: "Energy" }), { key: "End" });
    fireEvent.keyDown(screen.getByRole("slider", { name: "Size" }), { key: "End" });
    fireEvent.change(screen.getByLabelText("Context"), { target: { value: "@garden" } });

    fireEvent.click(screen.getByRole("button", { name: "Triage" }));

    expect(onSubmit).toHaveBeenCalledWith("Buy soil", "triage", {
      ...NO_FIELDS,
      size: "deep",
      energy: "high",
      context: "@garden",
    }, NO_ATTACHMENTS);
  });

  it("sends only the one field the reader set, leaving the other two absent", () => {
    const { onSubmit } = renderPopover();
    fireEvent.change(field(), { target: { value: "Buy soil" } });
    fireEvent.keyDown(screen.getByRole("slider", { name: "Energy" }), { key: "End" });

    fireEvent.click(screen.getByRole("button", { name: "Triage" }));

    expect(onSubmit).toHaveBeenCalledWith("Buy soil", "triage", {
      ...NO_FIELDS,
      energy: "high",
    }, NO_ATTACHMENTS);
  });

  // The clear-on-ok rule, and its one carve-out. Energy and Size are
  // per-item judgements and go back to rest; the context does not, because
  // adding three things for the same place is what the popover staying open
  // is FOR, and re-picking `@garden` each time taxes every capture after the
  // first for a decision already made. Both halves in one test on purpose —
  // "context survived" only means anything beside a control that cleared.
  it("clears Energy and Size on an ok result but keeps the context", () => {
    const { rerender } = renderPopover();
    fireEvent.change(field(), { target: { value: "Buy soil" } });
    fireEvent.keyDown(screen.getByRole("slider", { name: "Energy" }), { key: "End" });
    fireEvent.change(screen.getByLabelText("Context"), { target: { value: "@garden" } });

    fireEvent.click(screen.getByRole("button", { name: "Triage" }));
    rerender({ seed: "s1", kind: "ok", id: "item-9", error: null });

    expect(screen.getByRole("slider", { name: "Energy" }).getAttribute("aria-valuenow")).toBe("-1");
    expect((screen.getByLabelText("Context") as HTMLInputElement).value).toBe("@garden");
    expect((field() as HTMLInputElement).value).toBe("");
  });

  // And the sticky context is genuinely carried INTO the next capture, not
  // merely left painted on a control nobody reads: the second submit sends
  // it without the reader touching the combobox again.
  it("sends the kept context with the next capture, untouched", () => {
    const { onSubmit, rerender } = renderPopover();
    fireEvent.change(field(), { target: { value: "Buy soil" } });
    fireEvent.change(screen.getByLabelText("Context"), { target: { value: "@garden" } });
    fireEvent.click(screen.getByRole("button", { name: "Triage" }));
    rerender({ seed: "s1", kind: "ok", id: "item-9", error: null });

    fireEvent.change(field(), { target: { value: "Buy compost" } });
    fireEvent.click(screen.getByRole("button", { name: "Triage" }));

    expect(onSubmit).toHaveBeenLastCalledWith("Buy compost", "triage", {
      ...NO_FIELDS,
      context: "@garden",
    }, NO_ATTACHMENTS);
  });

  // The suggestions are the caller's, not a list this component holds: a
  // context minted since the app loaded reaches the popup by being passed in,
  // which is the whole of what `App.tsx` wires `contextSuggestions` for.
  //
  // Opened from the chevron, and with a context ALREADY IN THE BOX, because
  // that is the state #641's sticky field leaves behind and the state the
  // native `<datalist>` this replaced could not show a full list in
  // (`components/forms/Combobox.tsx`'s header).
  it("offers exactly the contexts it was handed", () => {
    renderPopover({ contextSuggestions: [...CONTEXTS, "@calls"] });
    fireEvent.change(screen.getByLabelText("Context"), { target: { value: "@errands" } });
    fireEvent.click(screen.getByRole("button", { name: "Show context suggestions" }));
    expect(screen.getAllByRole("option").map((option) => option.textContent)).toEqual([
      ...CONTEXTS,
      "@calls",
    ]);
  });

  // The caption that used to sit under these controls said stage and dates
  // were "decided at mint time". They can be decided here now — the "More
  // details" disclosure is where — so the sentence was deleted rather than
  // reworded, and this is the pin against it coming back.
  it("makes no claim about what capture cannot set", () => {
    renderPopover();
    expect(screen.queryByText(/decided at mint time/i)).toBeNull();
    expect(screen.queryByText(/not yet stored/i)).toBeNull();
  });
});

// The capture half of issue #222's rule, which that issue deliberately scoped
// out: a write that has not been acknowledged yet must not take the reader's
// work with it. Since #208 that work is the title PLUS three selections.
describe("CapturePopover — the clear-on-ok rule (#222)", () => {
  it("keeps the whole draft while the capture is still in flight", () => {
    renderPopover();
    fireEvent.change(field(), { target: { value: "Buy soil" } });
    fireEvent.keyDown(screen.getByRole("slider", { name: "Energy" }), { key: "End" });
    fireEvent.keyDown(screen.getByRole("slider", { name: "Size" }), { key: "End" });
    fireEvent.change(screen.getByLabelText("Context"), { target: { value: "@garden" } });

    fireEvent.click(screen.getByRole("button", { name: "Triage" }));

    // No result has come back — neither ok nor failed.
    expect(field().value).toBe("Buy soil");
    expect(screen.getByRole("slider", { name: "Energy" }).getAttribute("aria-valuenow")).toBe("2");
    expect(screen.getByRole("slider", { name: "Size" }).getAttribute("aria-valuenow")).toBe("2");
    expect((screen.getByLabelText("Context") as HTMLInputElement).value).toBe("@garden");
  });

  it("keeps the title and all three meta fields when the capture comes back failed", () => {
    const { rerender } = renderPopover();
    fireEvent.change(field(), { target: { value: "Buy soil" } });
    fireEvent.keyDown(screen.getByRole("slider", { name: "Energy" }), { key: "End" });
    fireEvent.keyDown(screen.getByRole("slider", { name: "Size" }), { key: "Home" });
    fireEvent.change(screen.getByLabelText("Context"), { target: { value: "@garden" } });
    fireEvent.click(screen.getByRole("button", { name: "Triage" }));

    rerender({ seed: "s1", kind: "failed", id: null, error: "Offline." });

    expect(screen.getByText("Offline.")).toBeDefined();
    expect(field().value).toBe("Buy soil");
    expect(screen.getByRole("slider", { name: "Energy" }).getAttribute("aria-valuenow")).toBe("2");
    expect(screen.getByRole("slider", { name: "Size" }).getAttribute("aria-valuenow")).toBe("0");
    expect((screen.getByLabelText("Context") as HTMLInputElement).value).toBe("@garden");
  });

  // The seed guard, from the other side: a result already processed must not
  // clear a draft the reader has started since. Without it, every unrelated
  // re-render carrying the same stale `lastCapture` would wipe the box.
  it("does not re-clear a draft typed after an already-processed ok result", () => {
    const ok: TaskCaptureResult = { seed: "s1", kind: "ok", id: "item-9", error: null };
    const { rerender } = renderPopover({ lastCapture: ok });

    fireEvent.change(field(), { target: { value: "Second thought" } });
    // Same seed, an unrelated re-render (a sync outcome, say).
    rerender(ok);

    expect(field().value).toBe("Second thought");
  });

  it("announces a failed capture to a screen reader", () => {
    renderPopover({ lastCapture: { seed: "s1", kind: "failed", id: null, error: "Offline." } });
    expect(screen.getByRole("alert").textContent).toBe("Offline.");
  });

  it("says nothing after a capture that reads as ok", () => {
    renderPopover({ lastCapture: { seed: "s1", kind: "ok", id: "item-9", error: null } });
    expect(screen.queryByText(/didn't go through/i)).toBeNull();
  });

  it("clears a stale capture failure once a later capture succeeds", () => {
    const { rerender } = renderPopover({
      lastCapture: { seed: "s1", kind: "failed", id: null, error: "Nope." },
    });
    expect(screen.getByText("Nope.")).toBeDefined();

    rerender({ seed: "s2", kind: "ok", id: "item-9", error: null });
    expect(screen.queryByText("Nope.")).toBeNull();
  });

  it("clears and reports right away in demo mode, where no result is coming", () => {
    const { onSubmit } = renderPopover({ demo: true });
    fireEvent.change(field(), { target: { value: "Call the plumber" } });
    fireEvent.click(screen.getByRole("button", { name: "Triage" }));

    expect(onSubmit).toHaveBeenCalledTimes(1);
    expect(field().value).toBe("");
    expect(screen.getByText(/Added to Triage — Call the plumber/)).toBeDefined();
  });

  it("never wears a stale failure in demo mode", () => {
    renderPopover({
      demo: true,
      lastCapture: { seed: "s1", kind: "failed", id: null, error: "Nope." },
    });
    expect(screen.queryByText("Nope.")).toBeNull();
  });
});

/** The three things a capture can point at, as one row behind "More details".
 * The link is a pair of columns on the item and rides on the capture itself;
 * the note and the file cannot (`useCaptureAttachments.ts` says why) and
 * leave through `onSubmit`'s fourth argument instead. */
describe("CapturePopover — the attachment row", () => {
  const VAULT = { vaultName: "JDD" };
  const DROPBOX = { fileLinks: { localRoot: null } };

  function openDetails() {
    fireEvent.click(screen.getByRole("button", { name: /more details/i }));
  }

  it("draws all three once the details are open", () => {
    renderPopover({ ...VAULT, ...DROPBOX });
    openDetails();
    expect(screen.getByRole("button", { name: "Add a link" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Add a note" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Add a file" })).toBeTruthy();
  });

  /** A control that cannot be written is not drawn — the same rule the item
   * panel's note affordance applies. Nothing announces a vault that isn't
   * there, and a render with no file-link wiring cannot create a file link. */
  it("draws only the link when there is no vault and no file wiring", () => {
    renderPopover();
    openDetails();
    expect(screen.getByRole("button", { name: "Add a link" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Add a note" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Add a file" })).toBeNull();
  });

  /** The path is *proposed* from what has been typed so far and editable
   * before it is ever stored — the same `derivePath` the item panel's note
   * editor proposes from the item's title. */
  it("proposes a vault path from the draft, and carries the edited one", () => {
    const { onSubmit } = renderPopover(VAULT);
    fireEvent.change(field(), { target: { value: "Knee rehab" } });
    openDetails();
    fireEvent.click(screen.getByRole("button", { name: "Add a note" }));

    const path = screen.getByLabelText("Vault path") as HTMLInputElement;
    expect(path.value).toBe("Hummingbird/Knee rehab.md");

    fireEvent.change(path, { target: { value: "Reading/Knee.md" } });
    fireEvent.click(screen.getByRole("button", { name: "Triage" }));

    expect(onSubmit).toHaveBeenLastCalledWith("Knee rehab", "triage", NO_FIELDS, {
      vaultPath: "Reading/Knee.md",
      filePath: null,
    });
  });

  /** Pasting a path out of a file manager — absolute, quoted, backslashed —
   * is how one usually arrives, and `normalizePastedPath` is the same
   * function the item panel's own add row uses. */
  it("normalizes a pasted file path against this device's root", () => {
    const { onSubmit } = renderPopover({ fileLinks: { localRoot: "C:\\Dropbox" } });
    fireEvent.change(field(), { target: { value: "Pay the invoice" } });
    openDetails();
    fireEvent.click(screen.getByRole("button", { name: "Add a file" }));
    fireEvent.change(screen.getByLabelText("File path"), {
      target: { value: '"C:\\Dropbox\\Finance\\2026\\receipt.pdf"' },
    });
    fireEvent.click(screen.getByRole("button", { name: "Triage" }));

    expect(onSubmit).toHaveBeenLastCalledWith("Pay the invoice", "triage", NO_FIELDS, {
      vaultPath: null,
      filePath: "Finance/2026/receipt.pdf",
    });
  });

  it("carries all three at once, the link on the capture and the other two beside it", () => {
    const { onSubmit } = renderPopover({ ...VAULT, ...DROPBOX });
    fireEvent.change(field(), { target: { value: "Fit the washer" } });
    openDetails();
    fireEvent.click(screen.getByRole("button", { name: "Add a link" }));
    fireEvent.change(screen.getByLabelText("URL"), { target: { value: "https://example.test/x" } });
    fireEvent.click(screen.getByRole("button", { name: "Add a note" }));
    fireEvent.change(screen.getByLabelText("Vault path"), { target: { value: "Reading/Tap.md" } });
    fireEvent.click(screen.getByRole("button", { name: "Add a file" }));
    fireEvent.change(screen.getByLabelText("File path"), { target: { value: "House/tap.pdf" } });
    fireEvent.click(screen.getByRole("button", { name: "Triage" }));

    expect(onSubmit).toHaveBeenLastCalledWith(
      "Fit the washer",
      "triage",
      { ...NO_FIELDS, linkUrl: "https://example.test/x" },
      { vaultPath: "Reading/Tap.md", filePath: "House/tap.pdf" },
    );
  });

  /** A bad path blocks the capture rather than being silently dropped from
   * it — the same answer #782's "a link name needs a URL" already gives.
   * Submitting and discarding what someone typed would be found out by
   * opening the item. */
  it("refuses the capture while a path is bad, and says so in the shared words", () => {
    const { onSubmit } = renderPopover({ ...VAULT, ...DROPBOX });
    fireEvent.change(field(), { target: { value: "Knee rehab" } });
    openDetails();
    fireEvent.click(screen.getByRole("button", { name: "Add a note" }));
    fireEvent.change(screen.getByLabelText("Vault path"), { target: { value: "../outside.md" } });

    expect(screen.getByText(VAULT_PATH_PROBLEM)).toBeTruthy();
    expect((screen.getByRole("button", { name: "Triage" }) as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "Triage" }));
    expect(onSubmit).not.toHaveBeenCalled();

    fireEvent.change(screen.getByLabelText("Vault path"), { target: { value: "Reading/Knee.md" } });
    fireEvent.click(screen.getByRole("button", { name: "Add a file" }));
    fireEvent.change(screen.getByLabelText("File path"), { target: { value: "../../etc/passwd" } });
    expect(screen.getByText(FILE_PATH_PROBLEM)).toBeTruthy();
    expect((screen.getByRole("button", { name: "Triage" }) as HTMLButtonElement).disabled).toBe(true);
  });

  /** A proposal is shown so it can be judged, not so it can be attached
   * behind the reader's back. Opening the note toggle to see what a note
   * WOULD be called, then closing it, must leave the item pointing at
   * nothing. */
  it("throws away an untouched derived path, and re-derives it from the current draft", () => {
    const { onSubmit } = renderPopover(VAULT);
    fireEvent.change(field(), { target: { value: "Knee rehab" } });
    openDetails();

    const toggle = () => screen.getByRole("button", { name: "Add a note" });
    fireEvent.click(toggle());
    expect((screen.getByLabelText("Vault path") as HTMLInputElement).value).toBe(
      "Hummingbird/Knee rehab.md",
    );
    fireEvent.click(toggle());

    // Retitled while the proposal was shut: reopening must not offer a note
    // named for the title the capture used to have.
    fireEvent.change(field(), { target: { value: "Shoulder rehab" } });
    fireEvent.click(toggle());
    expect((screen.getByLabelText("Vault path") as HTMLInputElement).value).toBe(
      "Hummingbird/Shoulder rehab.md",
    );
    fireEvent.click(toggle());

    fireEvent.click(screen.getByRole("button", { name: "Triage" }));
    expect(onSubmit).toHaveBeenLastCalledWith("Shoulder rehab", "triage", NO_FIELDS, {
      vaultPath: null,
      filePath: null,
    });
  });

  it("keeps an edited path when the disclosure is closed", () => {
    const { onSubmit } = renderPopover(VAULT);
    fireEvent.change(field(), { target: { value: "Knee rehab" } });
    openDetails();
    fireEvent.click(screen.getByRole("button", { name: "Add a note" }));
    fireEvent.change(screen.getByLabelText("Vault path"), { target: { value: "Reading/Knee.md" } });
    fireEvent.click(screen.getByRole("button", { name: "Add a note" }));

    fireEvent.click(screen.getByRole("button", { name: "Triage" }));
    expect(onSubmit).toHaveBeenLastCalledWith("Knee rehab", "triage", NO_FIELDS, {
      vaultPath: "Reading/Knee.md",
      filePath: null,
    });
  });

  /** A problem that blocks every submit must never be hideable. Closing the
   * disclosure over a bad path would otherwise disable all three buttons and
   * silence Enter with nothing on screen saying why. */
  it("will not let a blocking path be closed away", () => {
    renderPopover(VAULT);
    fireEvent.change(field(), { target: { value: "Knee rehab" } });
    openDetails();
    fireEvent.click(screen.getByRole("button", { name: "Add a note" }));
    fireEvent.change(screen.getByLabelText("Vault path"), { target: { value: "../outside.md" } });

    // Shut the field, then the whole details block. Both refuse.
    fireEvent.click(screen.getByRole("button", { name: "Add a note" }));
    expect(screen.getByLabelText("Vault path")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: /more details/i }));
    expect(screen.getByText(VAULT_PATH_PROBLEM)).toBeTruthy();
    expect((screen.getByRole("button", { name: "Triage" }) as HTMLButtonElement).disabled).toBe(true);

    // Repaired, both disclosures fall shut on their own — back to the state
    // the reader had actually left them in, since nothing is being forced
    // open any more. The path they typed is still carried.
    fireEvent.change(screen.getByLabelText("Vault path"), { target: { value: "Reading/Knee.md" } });
    expect(screen.queryByLabelText("Vault path")).toBeNull();
    expect(screen.queryByRole("button", { name: "Add a note" })).toBeNull();
    expect((screen.getByRole("button", { name: "Triage" }) as HTMLButtonElement).disabled).toBe(false);
  });

  /** #222, extended to the two paths: a failed capture keeps everything the
   * reader typed, and an ok clears it along with the disclosures. */
  it("keeps the paths through a failure and clears them on an ok", () => {
    const { rerender } = renderPopover({ ...VAULT, ...DROPBOX });
    fireEvent.change(field(), { target: { value: "Knee rehab" } });
    openDetails();
    fireEvent.click(screen.getByRole("button", { name: "Add a note" }));
    fireEvent.change(screen.getByLabelText("Vault path"), { target: { value: "Reading/Knee.md" } });
    fireEvent.click(screen.getByRole("button", { name: "Triage" }));

    rerender({ seed: "s1", kind: "failed", id: null, error: "nope" });
    expect((screen.getByLabelText("Vault path") as HTMLInputElement).value).toBe("Reading/Knee.md");

    rerender({ seed: "s2", kind: "ok", id: "item-9", error: null });
    expect(screen.queryByLabelText("Vault path")).toBeNull();
    expect(screen.queryByRole("button", { name: "Add a note" })).toBeNull();
  });

  /** The capture landed and the thing that rode with it did not — a separate
   * report from `captureError`, because a reader who took this for a failed
   * capture would go looking for an item that is already there. */
  it("renders the attachment failure its own way", () => {
    renderPopover({ attachmentFailure: "The item was captured, but its note link didn't go through." });
    expect(
      screen.getByText("The item was captured, but its note link didn't go through."),
    ).toBeTruthy();
  });
});
