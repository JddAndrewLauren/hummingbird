// @vitest-environment jsdom

// #379's wiring test: the capture box with the speech seam replaced wholesale.
// A file of its own rather than more cases in `CapturePopover.test.tsx`, for a
// reason worth stating — that file mounts the box under a jsdom with NO speech
// API, which is the `unsupported` arm and must stay untouched (it is also the
// standing proof that no microphone renders there and no probe runs, so no
// `act()` warning appears). Mocking the seam there would destroy that.
//
// The seam is mocked, not faked at the `globalThis` level, because that is the
// contract this test exists to check: `speech/local-dictation.ts` is
// browser-only, is unit-tested on its own, and the box must depend on nothing
// about a recognizer beyond the three callbacks. The mock hands the test the
// handlers, so "a callback arriving after the session ends changes nothing" is
// asserted from OUTSIDE the component, as the issue requires.

import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, render, screen } from "../test/component";
import type {
  DictationCapability,
  DictationHandlers,
  DictationSession,
} from "../speech/local-dictation";

const seam = vi.hoisted(() => {
  return {
    capability: { kind: "ready" } as DictationCapability,
    present: true,
    /** The handlers the component passed to the live session — the test's
     * stand-in for a person speaking. */
    handlers: null as DictationHandlers | null,
    stops: 0,
    aborts: 0,
  };
});

vi.mock("../speech/local-dictation", () => ({
  isDictationApiPresent: () => seam.present,
  probeDictationCapability: () => Promise.resolve(seam.capability),
  startLocalDictation: (handlers: DictationHandlers): DictationSession => {
    seam.handlers = handlers;
    return {
      stop: () => {
        seam.stops += 1;
        handlers.onEnd();
      },
      abort: () => {
        seam.aborts += 1;
        handlers.onEnd();
      },
    };
  },
}));

const { CaptureBox } = await import("./CaptureBox");

beforeEach(() => {
  seam.capability = { kind: "ready" };
  seam.present = true;
  seam.handlers = null;
  seam.stops = 0;
  seam.aborts = 0;
});

function renderBox() {
  const onSubmit = vi.fn();
  const view = render(
    <CaptureBox onSubmit={onSubmit} demo={false} focusRequestId={1} lastCapture={null} />,
  );
  return { onSubmit, view };
}

function field(): HTMLInputElement {
  return screen.getByLabelText("Capture") as HTMLInputElement;
}

/** Lets the capability probe resolve — one microtask, inside `act`, so the
 * `setState` it lands is flushed the way React wants it. */
async function settleProbe(): Promise<void> {
  await act(async () => {});
}

function mic(): HTMLElement {
  return screen.getByRole("button", { name: "Dictate" });
}

function stopMic(): HTMLElement {
  return screen.getByRole("button", { name: "Stop dictating" });
}

/** One recognition result, as the seam would deliver it — cumulative, always. */
function hear(transcript: string): void {
  act(() => {
    seam.handlers?.onTranscript(transcript);
  });
}

async function startListening(): Promise<void> {
  await settleProbe();
  fireEvent.click(mic());
}

describe("CaptureBox — dictation", () => {
  it("renders no microphone at all when the capability is unsupported", async () => {
    seam.capability = { kind: "unsupported", reason: "nope" };
    renderBox();
    await settleProbe();
    expect(screen.queryByRole("button", { name: "Dictate" })).toBeNull();
  });

  it("renders no microphone for setup-required — that arm is the setup slice's", async () => {
    seam.capability = { kind: "setup-required" };
    renderBox();
    await settleProbe();
    expect(screen.queryByRole("button", { name: "Dictate" })).toBeNull();
  });

  it("never probes when the API is absent, so nothing renders and no state settles", () => {
    seam.present = false;
    renderBox();
    // No `act` wrapper and no await: if the box had probed, this assertion
    // would be racing a promise and React would warn about the update.
    expect(screen.queryByRole("button", { name: "Dictate" })).toBeNull();
  });

  it("puts spoken words in the field, and a second tap commits them", async () => {
    const { onSubmit } = renderBox();
    await startListening();
    hear("call the vet");
    expect(field().value).toBe("call the vet");

    fireEvent.click(stopMic());
    expect(seam.stops).toBe(1);
    // Back to idle, and the words are still there.
    expect(mic()).toBeTruthy();
    expect(field().value).toBe("call the vet");

    fireEvent.click(screen.getByRole("button", { name: "Add to Triage" }));
    // #110: the raw spliced string, unmodified, through the same path a typed
    // capture takes — and the metadata untouched by the session.
    expect(onSubmit).toHaveBeenCalledWith("call the vet", "triage", {
      size: null,
      energy: null,
      context: null,
    });
  });

  it("lands the transcript at the caret with the suffix intact", async () => {
    renderBox();
    await settleProbe();
    fireEvent.change(field(), { target: { value: "call today" } });
    act(() => {
      field().setSelectionRange(5, 5);
    });
    fireEvent.click(mic());
    hear("the vet");
    expect(field().value).toBe("call the vet today");
    // The caret sits at the end of the inserted text, not at the end of the
    // rewritten value.
    expect(field().selectionStart).toBe("call the vet".length);
  });

  it("replaces a selection, the way typing over one does", async () => {
    renderBox();
    await settleProbe();
    fireEvent.change(field(), { target: { value: "call the vet" } });
    act(() => {
      field().setSelectionRange(5, 12);
    });
    fireEvent.click(mic());
    hear("the dentist");
    expect(field().value).toBe("call the dentist");
  });

  it("replaces an earlier interim result rather than concatenating it", async () => {
    renderBox();
    await startListening();
    hear("call");
    hear("call the");
    hear("call the vet");
    expect(field().value).toBe("call the vet");
  });

  it("makes the field readOnly while listening, and editable again after", async () => {
    renderBox();
    await startListening();
    expect(field().readOnly).toBe(true);
    fireEvent.click(stopMic());
    expect(field().readOnly).toBe(false);
  });

  it("stops the session on Enter and does not submit", async () => {
    const { onSubmit } = renderBox();
    await startListening();
    hear("call the vet");
    fireEvent.keyDown(field(), { key: "Enter" });
    expect(seam.stops).toBe(1);
    expect(onSubmit).not.toHaveBeenCalled();
    // And the very next Enter, no longer listening, submits as it always did.
    fireEvent.keyDown(field(), { key: "Enter" });
    expect(onSubmit).toHaveBeenCalledTimes(1);
  });

  it("cancels the session when the page is hidden, keeping what was heard", async () => {
    // #379's open decision, taken as cancel — see `CaptureBox.tsx`'s header.
    // What a backgrounded session loses is the undelivered tail, never the
    // words already in the field.
    renderBox();
    await startListening();
    hear("call the vet");
    const hidden = vi.spyOn(document, "hidden", "get").mockReturnValue(true);
    act(() => {
      document.dispatchEvent(new Event("visibilitychange"));
    });
    expect(seam.aborts).toBe(1);
    expect(field().value).toBe("call the vet");
    expect(field().readOnly).toBe(false);
    expect(mic()).toBeTruthy();
    hidden.mockRestore();
  });

  it("aborts the session when the box unmounts, releasing the microphone", async () => {
    // Load-bearing: `CapturePopover` returns null when closed, which unmounts
    // this box while the recognizer is still running.
    const { view } = renderBox();
    await startListening();
    view.unmount();
    expect(seam.aborts).toBe(1);
  });

  it("ignores a callback that arrives after the session ended", async () => {
    renderBox();
    await startListening();
    hear("call the vet");
    fireEvent.click(stopMic());
    // The recognizer keeps talking. Asserted from outside the component: the
    // handlers the box handed over are still callable, and calling them must
    // change nothing at all.
    hear("something else entirely");
    act(() => {
      seam.handlers?.onError({ code: "network", message: "late error" });
    });
    expect(field().value).toBe("call the vet");
    expect(screen.queryByText("late error")).toBeNull();
  });

  it("states a dictation error and ends the session", async () => {
    renderBox();
    await startListening();
    act(() => {
      seam.handlers?.onError({ code: "no-speech", message: "Nothing was heard." });
      seam.handlers?.onEnd();
    });
    expect(screen.getByRole("alert").textContent).toBe("Nothing was heard.");
    expect(field().readOnly).toBe(false);
    // Cleared by the next attempt rather than lingering over a working session.
    fireEvent.click(mic());
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("ends the session on a deliberate submit, so a later result cannot resurrect it", async () => {
    // The clear-on-ok rule (#222) rewrites `draft` from a capture result, so a
    // session still splicing onto halves frozen before that clear would put
    // the submitted capture back in the field.
    const { onSubmit, view } = renderBox();
    await startListening();
    hear("call the vet");
    fireEvent.click(screen.getByRole("button", { name: "Add to Triage" }));
    expect(onSubmit).toHaveBeenCalledWith("call the vet", "triage", {
      size: null,
      energy: null,
      context: null,
    });
    expect(seam.aborts).toBe(1);
    view.rerender(
      <CaptureBox
        onSubmit={onSubmit}
        demo={false}
        focusRequestId={1}
        lastCapture={{ kind: "ok", seed: "seed-1", id: "item-1", error: null }}
      />,
    );
    expect(field().value).toBe("");
    hear("call the vet");
    expect(field().value).toBe("");
  });

  it("clears a dictation error when a capture succeeds, so it cannot outlive its draft", async () => {
    const { onSubmit, view } = renderBox();
    await startListening();
    act(() => {
      seam.handlers?.onError({ code: "no-speech", message: "Nothing was heard." });
      seam.handlers?.onEnd();
    });
    fireEvent.change(field(), { target: { value: "call the vet" } });
    fireEvent.click(screen.getByRole("button", { name: "Add to Triage" }));
    expect(screen.getByRole("alert").textContent).toBe("Nothing was heard.");
    view.rerender(
      <CaptureBox
        onSubmit={onSubmit}
        demo={false}
        focusRequestId={1}
        lastCapture={{ kind: "ok", seed: "seed-1", id: "item-1", error: null }}
      />,
    );
    // The box is empty again, and wears no report of a session two captures ago.
    expect(field().value).toBe("");
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("leaves the metadata controls alone across a session", async () => {
    const { onSubmit } = renderBox();
    await settleProbe();
    fireEvent.change(screen.getByLabelText("Context"), { target: { value: "@phone" } });
    fireEvent.click(mic());
    hear("call the vet");
    fireEvent.click(stopMic());
    fireEvent.click(screen.getByRole("button", { name: "Add to Triage" }));
    expect(onSubmit).toHaveBeenCalledWith("call the vet", "triage", {
      size: null,
      energy: null,
      context: "@phone",
    });
  });
});
