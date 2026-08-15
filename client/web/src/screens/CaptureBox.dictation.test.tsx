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
    /** #381: how many times the install seam was called, and what it
     * resolves to next — the test's stand-in for the browser's installer. */
    installs: 0,
    installResult: Promise.resolve(true) as Promise<boolean>,
  };
});

vi.mock("../speech/local-dictation", () => ({
  isDictationApiPresent: () => seam.present,
  probeDictationCapability: () => Promise.resolve(seam.capability),
  installDictationModel: () => {
    seam.installs += 1;
    return seam.installResult;
  },
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
  seam.installs = 0;
  seam.installResult = Promise.resolve(true);
});

/** Every optional field left at rest — what `resolveCaptureFields` hands
 * `onSubmit` when nothing beside the title was touched. */
const NO_FIELDS = {
  size: null,
  energy: null,
  context: null,
  description: null,
  projectId: null,
  priority: null,
  deadline: null,
  scheduledDate: null,
};

function renderBox(options: { onDictatingChange?: (dictating: boolean) => void } = {}) {
  const onSubmit = vi.fn();
  const onDictatingChange = options.onDictatingChange ?? vi.fn();
  const view = render(
    <CaptureBox
      onSubmit={onSubmit}
      projects={[]}
      demo={false}
      focusRequestId={1}
      lastCapture={null}
      cancelDictationRequestId={0}
      onDictatingChange={onDictatingChange}
    />,
  );
  const bumpCancel = (id: number) =>
    view.rerender(
      <CaptureBox
        onSubmit={onSubmit}
        projects={[]}
        demo={false}
        focusRequestId={1}
        lastCapture={null}
        cancelDictationRequestId={id}
        onDictatingChange={onDictatingChange}
      />,
    );
  return { onSubmit, view, onDictatingChange, bumpCancel };
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

/** #381: the setup-state mic, distinct from the ordinary "Dictate" control. */
function setupMic(): HTMLElement {
  return screen.getByRole("button", { name: "Set up dictation" });
}

function downloadControl(): HTMLElement {
  return screen.getByRole("button", { name: "Download speech model" });
}

/** Resolves or rejects the seam's `installDictationModel` on demand, so a
 * test can assert what renders WHILE the promise is still pending. */
function deferredInstall(): { resolve: (ok: boolean) => void; reject: (error: unknown) => void } {
  let resolve!: (ok: boolean) => void;
  let reject!: (error: unknown) => void;
  seam.installResult = new Promise<boolean>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { resolve, reject };
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

  it("renders the setup mic, not the ordinary Dictate control, for setup-required", async () => {
    // #381: `setup-required` is actionable, through its OWN control — never
    // through the ordinary "Dictate" mic `canDictate` renders.
    seam.capability = { kind: "setup-required" };
    renderBox();
    await settleProbe();
    expect(screen.queryByRole("button", { name: "Dictate" })).toBeNull();
    expect(screen.getByRole("button", { name: "Set up dictation" })).toBeTruthy();
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

    fireEvent.click(screen.getByRole("button", { name: "Triage" }));
    // #110: the raw spliced string, unmodified, through the same path a typed
    // capture takes — and the metadata untouched by the session.
    expect(onSubmit).toHaveBeenCalledWith("call the vet", "triage", NO_FIELDS);
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
    fireEvent.click(screen.getByRole("button", { name: "Triage" }));
    expect(onSubmit).toHaveBeenCalledWith("call the vet", "triage", NO_FIELDS);
    expect(seam.aborts).toBe(1);
    view.rerender(
      <CaptureBox
        onSubmit={onSubmit}
        projects={[]}
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
    fireEvent.click(screen.getByRole("button", { name: "Triage" }));
    expect(screen.getByRole("alert").textContent).toBe("Nothing was heard.");
    view.rerender(
      <CaptureBox
        onSubmit={onSubmit}
        projects={[]}
        demo={false}
        focusRequestId={1}
        lastCapture={{ kind: "ok", seed: "seed-1", id: "item-1", error: null }}
      />,
    );
    // The box is empty again, and wears no report of a session two captures ago.
    expect(field().value).toBe("");
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("ends a still-live session and drops its snapshot when a capture succeeds out from under it (#367)", async () => {
    // Not the deliberate-submit case above: here the session belongs to a
    // NEXT capture, still listening, when an EARLIER submit's result lands.
    // The clear-on-ok rule rewrites `draft` to "" regardless, so the session's
    // frozen halves (built from whatever the field said before that clear)
    // would resurrect that text if a later transcript spliced onto them.
    const { onSubmit, view, onDictatingChange, bumpCancel } = renderBox();
    await settleProbe();
    // A draft already sitting in the field when the NEXT session starts — if
    // the snapshot were wrongly restored instead of dropped, this is what
    // would come back, distinguishing this from a cancel.
    fireEvent.change(field(), { target: { value: "buy milk" } });
    fireEvent.click(mic());
    hear("call the vet");

    view.rerender(
      <CaptureBox
        onSubmit={onSubmit}
        projects={[]}
        demo={false}
        focusRequestId={1}
        lastCapture={{ kind: "ok", seed: "seed-1", id: "item-1", error: null }}
        cancelDictationRequestId={0}
        onDictatingChange={onDictatingChange}
      />,
    );

    // The microphone is torn down — no hot recognizer survives the capture...
    expect(seam.aborts).toBe(1);
    expect(field().readOnly).toBe(false);
    expect(mic()).toBeTruthy();
    // ...the field is exactly what the successful capture left it (empty),
    // not the words the dead session had spliced in...
    expect(field().value).toBe("");
    // ...and a transcript still arriving from the dead recognizer changes
    // nothing at all.
    hear("something else entirely");
    expect(field().value).toBe("");
    // A cancel afterwards restores nothing either: `frozenRef` was dropped
    // alongside the session, so there is nothing left for `cancelDictation`
    // to restore from.
    bumpCancel(1);
    expect(field().value).toBe("");
  });

  it("keeps a voice-produced title after a failed capture, exactly as it keeps a typed one (#222, #382)", async () => {
    const { onSubmit, view } = renderBox();
    await startListening();
    hear("call the vet");
    fireEvent.click(screen.getByRole("button", { name: "Triage" }));
    expect(onSubmit).toHaveBeenCalledWith("call the vet", "triage", NO_FIELDS);

    view.rerender(
      <CaptureBox
        onSubmit={onSubmit}
        projects={[]}
        demo={false}
        focusRequestId={1}
        lastCapture={{ kind: "failed", seed: "seed-1", id: null, error: "Offline." }}
      />,
    );

    // #222's rule does not know or care that this title was dictated rather
    // than typed — the failure path reads the same `draft` state either way.
    expect(field().value).toBe("call the vet");
    expect(screen.getByRole("alert").textContent).toBe("Offline.");
    expect(field().readOnly).toBe(false);
    // Retryable: the mic is back and the draft can still be submitted.
    expect(mic()).toBeTruthy();
  });

  it("leaves the metadata controls alone across a session", async () => {
    const { onSubmit } = renderBox();
    await settleProbe();
    fireEvent.change(screen.getByLabelText("Context"), { target: { value: "@phone" } });
    fireEvent.click(mic());
    hear("call the vet");
    fireEvent.click(stopMic());
    fireEvent.click(screen.getByRole("button", { name: "Triage" }));
    expect(onSubmit).toHaveBeenCalledWith("call the vet", "triage", {
      ...NO_FIELDS,
      context: "@phone",
    });
  });
});

// #380: Escape while dictating cancels the session in place. The shell (not
// this box) decides that an Escape means "cancel" rather than "close" — see
// `App.tsx` — and asks by bumping `cancelDictationRequestId`, the same
// "bumped counter" idiom `focusRequestId` already uses. What belongs here is
// only what the restore actually does: byte-exact, from the same frozen
// halves the splice reads (`capture-dictation.ts`), and a session ended
// exactly the way backgrounding already ends one.
describe("CaptureBox — cancelling a dictation session", () => {
  it("restores the pre-session draft and caret exactly, and releases the microphone", async () => {
    const { bumpCancel } = renderBox();
    await settleProbe();
    fireEvent.change(field(), { target: { value: "call today" } });
    act(() => {
      field().setSelectionRange(5, 5);
    });
    fireEvent.click(mic());
    hear("the vet");
    expect(field().value).toBe("call the vet today");

    bumpCancel(1);

    expect(seam.aborts).toBe(1);
    expect(field().value).toBe("call today");
    expect(field().selectionStart).toBe(5);
    expect(field().readOnly).toBe(false);
    expect(mic()).toBeTruthy();
  });

  it("restores a selection's words, byte-for-byte, and re-selects the same range", async () => {
    // The reviewer's exact case on #380: starting dictation with "the vet"
    // selected in "call the vet today" must not delete those words on cancel.
    const { bumpCancel } = renderBox();
    await settleProbe();
    fireEvent.change(field(), { target: { value: "call the vet today" } });
    act(() => {
      field().setSelectionRange(5, 12);
    });
    fireEvent.click(mic());
    hear("the dentist");
    expect(field().value).toBe("call the dentist today");

    bumpCancel(1);

    expect(field().value).toBe("call the vet today");
    expect(field().selectionStart).toBe(5);
    expect(field().selectionEnd).toBe(12);
  });

  it("does nothing when bumped while no session is live", async () => {
    const { bumpCancel } = renderBox();
    await settleProbe();
    fireEvent.change(field(), { target: { value: "call the vet" } });
    bumpCancel(1);
    expect(seam.aborts).toBe(0);
    expect(field().value).toBe("call the vet");
  });

  it("ignores a callback that arrives after a cancel", async () => {
    const { bumpCancel } = renderBox();
    await startListening();
    hear("call the vet");
    bumpCancel(1);
    hear("something else entirely");
    act(() => {
      seam.handlers?.onError({ code: "network", message: "late error" });
    });
    expect(field().value).toBe("");
    expect(screen.queryByText("late error")).toBeNull();
  });

  it("reports whether it is dictating, for the shell's Escape handler", async () => {
    const onDictatingChange = vi.fn();
    renderBox({ onDictatingChange });
    await settleProbe();
    expect(onDictatingChange).toHaveBeenLastCalledWith(false);
    fireEvent.click(mic());
    expect(onDictatingChange).toHaveBeenLastCalledWith(true);
    fireEvent.click(stopMic());
    expect(onDictatingChange).toHaveBeenLastCalledWith(false);
  });
});

// #381: the `setup-required` arm #379 deliberately left rendering nothing.
// Two deliberate steps, per the issue and ADR-0022 Decision 5 — the first tap
// explains only, and only the hint's own control calls the installer.
describe("CaptureBox — explain, then download, the on-device speech model", () => {
  beforeEach(() => {
    seam.capability = { kind: "setup-required" };
  });

  it("the first tap renders the explanation and calls nothing — no install, no probe beyond mount's own", async () => {
    renderBox();
    await settleProbe();
    fireEvent.click(setupMic());
    expect(
      screen.getByText(
        "Local speech recognition needs a one-time download before dictation works.",
      ),
    ).toBeTruthy();
    expect(seam.installs).toBe(0);
    expect(downloadControl()).toBeTruthy();
  });

  it("only the hint's own control triggers the install", async () => {
    renderBox();
    await settleProbe();
    // Tapping the setup mic itself again, once the hint is already open, must
    // not call the installer either.
    fireEvent.click(setupMic());
    fireEvent.click(setupMic());
    expect(seam.installs).toBe(0);
    fireEvent.click(downloadControl());
    expect(seam.installs).toBe(1);
  });

  it("shows the downloading state while the install is in flight", async () => {
    const deferred = deferredInstall();
    renderBox();
    await settleProbe();
    fireEvent.click(setupMic());
    fireEvent.click(downloadControl());
    expect(screen.getByText("Downloading the on-device speech model…")).toBeTruthy();
    // No download control to click again while it is running.
    expect(screen.queryByRole("button", { name: "Download speech model" })).toBeNull();
    await act(async () => {
      deferred.resolve(true);
      await Promise.resolve();
    });
  });

  it("re-probes to ready on a successful install, and the microphone then dictates", async () => {
    const deferred = deferredInstall();
    renderBox();
    await settleProbe();
    fireEvent.click(setupMic());
    fireEvent.click(downloadControl());
    seam.capability = { kind: "ready" };
    await act(async () => {
      deferred.resolve(true);
      // Let the install's own `.then` chain (including the re-probe) settle.
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(screen.queryByRole("button", { name: "Set up dictation" })).toBeNull();
    expect(mic()).toBeTruthy();
    fireEvent.click(mic());
    hear("call the vet");
    expect(field().value).toBe("call the vet");
  });

  it("a failed install says so, preserves the draft, and leaves the field editable", async () => {
    const deferred = deferredInstall();
    renderBox();
    await settleProbe();
    fireEvent.change(field(), { target: { value: "call the vet" } });
    fireEvent.click(setupMic());
    fireEvent.click(downloadControl());
    await act(async () => {
      deferred.resolve(false);
      await Promise.resolve();
    });
    const installFailure = screen.getByRole("alert");
    expect(installFailure.textContent).toBe(
      "The speech model couldn't be installed. Typing still works.",
    );
    expect(installFailure.style.color).toBe("var(--status-danger-fg)");
    expect(field().value).toBe("call the vet");
    expect(field().readOnly).toBe(false);
    fireEvent.change(field(), { target: { value: "call the vet and buy milk" } });
    expect(field().value).toBe("call the vet and buy milk");
    // No network recognizer is ever reached: the only seam this component
    // knows about is the mocked one above, and no session was ever started.
    expect(seam.handlers).toBeNull();
  });

  it("a rejected install is treated the same as a false one", async () => {
    const deferred = deferredInstall();
    renderBox();
    await settleProbe();
    fireEvent.click(setupMic());
    fireEvent.click(downloadControl());
    await act(async () => {
      deferred.reject(new Error("NotAllowedError"));
      await Promise.resolve();
    });
    expect(
      screen.getByText("The speech model couldn't be installed. Typing still works."),
    ).toBeTruthy();
  });

  it("never installs on mount and never installs as a side effect of a capture", async () => {
    const { onSubmit } = renderBox();
    await settleProbe();
    fireEvent.change(field(), { target: { value: "call the vet" } });
    fireEvent.click(screen.getByRole("button", { name: "Triage" }));
    expect(onSubmit).toHaveBeenCalled();
    expect(seam.installs).toBe(0);
  });
});
