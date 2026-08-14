// The seam's own tests (#379). No jsdom: this module only ever reads
// `globalThis` for a constructor, so a fake constructor installed on
// `globalThis` is a *more* faithful harness than a browser-shaped one — every
// arm here is a browser this repo cannot run under test, including the one
// that matters most (a recognizer with no way to require local processing,
// which is what Safari actually ships — ADR-0022 Decision 6).
//
// What these pin is the ADR, not the implementation's shape: local processing
// is required before `start()` on every path, a missing static is
// `unsupported`, an unrecognized readiness string is `unsupported`, and no
// failure ever reaches the caller as a throw.

import { afterEach, describe, expect, it, vi } from "vitest";
import {
  DICTATION_LANG,
  describeDictationError,
  isDictationApiPresent,
  probeDictationCapability,
  startLocalDictation,
  type DictationHandlers,
} from "./local-dictation";

interface FakeResult {
  isFinal: boolean;
  0: { transcript: string };
}

/** A recognizer instance recording the order it was configured in — the
 * `processLocally` criterion is about ordering, not just about the value. */
class FakeRecognition {
  lang = "";
  continuous = false;
  interimResults = false;
  processLocally?: boolean;
  onresult: ((event: { resultIndex: number; results: Record<number, FakeResult> & { length: number } }) => void) | null =
    null;
  onerror: ((event: { error: string }) => void) | null = null;
  onend: (() => void) | null = null;
  started = false;
  stopped = 0;
  aborted = 0;
  /** Every member written before `start()` was called, in order. */
  configuredBeforeStart: string[] = [];
  startThrows = false;

  constructor() {
    return new Proxy(this, {
      set: (target, property, value) => {
        if (!target.started && typeof property === "string") {
          target.configuredBeforeStart.push(property);
        }
        Reflect.set(target, property, value);
        return true;
      },
    });
  }

  start(): void {
    if (this.startThrows) {
      throw new Error("no");
    }
    this.started = true;
  }
  stop(): void {
    this.stopped += 1;
  }
  abort(): void {
    this.aborted += 1;
  }

  /** Feeds one recognition event, the shape Chrome delivers with
   * `continuous`: a full result list plus the index the new results start at. */
  emit(resultIndex: number, results: FakeResult[]): void {
    const list = { length: resultIndex + results.length } as Record<number, FakeResult> & {
      length: number;
    };
    results.forEach((result, offset) => {
      list[resultIndex + offset] = result;
    });
    this.onresult?.({ resultIndex, results: list });
  }
}

function finalResult(transcript: string): FakeResult {
  return { isFinal: true, 0: { transcript } };
}

function interimResult(transcript: string): FakeResult {
  return { isFinal: false, 0: { transcript } };
}

type Scope = Record<string, unknown>;

/** Installs a constructor under the given global name(s) and returns the
 * instances it hands out. `available` is omitted entirely when `null` — an
 * absent static, not one that answers badly. */
function installRecognizer(options: {
  names?: string[];
  available?: (() => Promise<string>) | null;
  startThrows?: boolean;
}): FakeRecognition[] {
  const made: FakeRecognition[] = [];
  class Ctor extends FakeRecognition {
    constructor() {
      super();
      this.startThrows = options.startThrows ?? false;
      made.push(this);
    }
  }
  if (options.available !== null) {
    Object.assign(Ctor, {
      available: options.available ?? (() => Promise.resolve("available")),
    });
  }
  for (const name of options.names ?? ["SpeechRecognition", "webkitSpeechRecognition"]) {
    (globalThis as Scope)[name] = Ctor;
  }
  return made;
}

afterEach(() => {
  delete (globalThis as Scope).SpeechRecognition;
  delete (globalThis as Scope).webkitSpeechRecognition;
});

function handlers(): DictationHandlers & {
  transcripts: string[];
  errors: { code: string; message: string }[];
  ends: number;
} {
  const transcripts: string[] = [];
  const errors: { code: string; message: string }[] = [];
  const record = {
    transcripts,
    errors,
    ends: 0,
    onTranscript: (transcript: string) => transcripts.push(transcript),
    onError: (error: { code: string; message: string }) => errors.push(error),
    onEnd: () => {
      record.ends += 1;
    },
  };
  return record;
}

describe("isDictationApiPresent — the synchronous gate", () => {
  it("is false with no constructor at all", () => {
    expect(isDictationApiPresent()).toBe(false);
  });

  it("is false for a constructor with no on-device static — what Safari ships", () => {
    installRecognizer({ names: ["webkitSpeechRecognition"], available: null });
    expect(isDictationApiPresent()).toBe(false);
  });

  it("is true only when the constructor can be asked about local processing", () => {
    installRecognizer({});
    expect(isDictationApiPresent()).toBe(true);
  });
});

describe("probeDictationCapability", () => {
  it("is unsupported with no constructor", async () => {
    await expect(probeDictationCapability()).resolves.toEqual({
      kind: "unsupported",
      reason: expect.any(String),
    });
  });

  it("is unsupported for a constructor without the on-device static", async () => {
    installRecognizer({ names: ["webkitSpeechRecognition"], available: null });
    const capability = await probeDictationCapability();
    expect(capability.kind).toBe("unsupported");
  });

  it("finds the prefixed-only constructor and uses it", async () => {
    const available = vi.fn(() => Promise.resolve("available"));
    installRecognizer({ names: ["webkitSpeechRecognition"], available });
    await expect(probeDictationCapability()).resolves.toEqual({ kind: "ready" });
    // The required-`langs` dictionary form, with local processing demanded —
    // `available("en-US")` throws `TypeError` (ADR-0022 Decision 5).
    expect(available).toHaveBeenCalledWith({ langs: [DICTATION_LANG], processLocally: true });
  });

  it("maps each readiness string this repo has ever observed to its arm", async () => {
    const seen: Record<string, string> = {};
    for (const status of ["available", "downloadable", "downloading"]) {
      installRecognizer({ available: () => Promise.resolve(status) });
      seen[status] = (await probeDictationCapability()).kind;
    }
    expect(seen).toEqual({
      available: "ready",
      downloadable: "setup-required",
      downloading: "setup-required",
    });
  });

  it("routes an unobserved readiness string to unsupported, not to an assumed arm", async () => {
    // #377 assumed a fourth value, `"unavailable"`, that no browser has ever
    // produced here. Whatever a future Chrome returns lands on the safe arm.
    for (const status of ["unavailable", "installing", ""]) {
      installRecognizer({ available: () => Promise.resolve(status) });
      expect((await probeDictationCapability()).kind).toBe("unsupported");
    }
  });

  it("maps a rejecting probe to unsupported and never throws to the caller", async () => {
    installRecognizer({ available: () => Promise.reject(new TypeError("nope")) });
    const capability = await probeDictationCapability();
    expect(capability.kind).toBe("unsupported");
  });
});

describe("startLocalDictation — the session", () => {
  it("requires local processing, and sets every flag before start()", () => {
    const made = installRecognizer({});
    startLocalDictation(handlers());
    const recognition = made[0];
    expect(recognition.started).toBe(true);
    expect(recognition.processLocally).toBe(true);
    expect(recognition.lang).toBe(DICTATION_LANG);
    expect(recognition.interimResults).toBe(true);
    expect(recognition.continuous).toBe(true);
    // The ordering is the guarantee, not the values: a `processLocally` set
    // after `start()` is a session already running against whatever the
    // browser chose.
    for (const flag of ["processLocally", "lang", "interimResults", "continuous"]) {
      expect(recognition.configuredBeforeStart).toContain(flag);
    }
  });

  it("assembles a cumulative transcript over mixed final and interim results", () => {
    const made = installRecognizer({});
    const record = handlers();
    startLocalDictation(record);
    const recognition = made[0];

    recognition.emit(0, [interimResult("call the")]);
    recognition.emit(0, [interimResult("call the vet")]);
    recognition.emit(0, [finalResult("Call the vet.")]);
    recognition.emit(1, [interimResult("about")]);

    expect(record.transcripts).toEqual([
      "call the",
      // Replaced, not accumulated: the interim tail is rebuilt each event.
      "call the vet",
      "Call the vet.",
      "Call the vet. about",
    ]);
  });

  it("joins halves with at most one space, and respects a result's own spacing", () => {
    const made = installRecognizer({});
    const record = handlers();
    startLocalDictation(record);
    made[0].emit(0, [finalResult("one"), interimResult(" two")]);
    expect(record.transcripts).toEqual(["one two"]);
  });

  it("maps a known error code, and carries an unknown one verbatim", () => {
    expect(describeDictationError("not-allowed").message).toMatch(/blocked/i);
    const unknown = describeDictationError("quantum-flux");
    expect(unknown.code).toBe("quantum-flux");
    expect(unknown.message).toContain("quantum-flux");
  });

  it("fires onEnd exactly once on stop, and stops the recognizer", () => {
    const made = installRecognizer({});
    const record = handlers();
    const session = startLocalDictation(record);
    session.stop();
    session.stop();
    made[0].onend?.();
    expect(record.ends).toBe(1);
    expect(made[0].stopped).toBe(1);
  });

  it("fires onEnd exactly once on abort, and aborts the recognizer", () => {
    const made = installRecognizer({});
    const record = handlers();
    const session = startLocalDictation(record);
    session.abort();
    made[0].onend?.();
    session.stop();
    expect(record.ends).toBe(1);
    expect(made[0].aborted).toBe(1);
    expect(made[0].stopped).toBe(0);
  });

  it("fires onEnd exactly once for an error the browser follows with onend", () => {
    const made = installRecognizer({});
    const record = handlers();
    startLocalDictation(record);
    made[0].onerror?.({ error: "no-speech" });
    made[0].onend?.();
    expect(record.errors.map((error) => error.code)).toEqual(["no-speech"]);
    expect(record.ends).toBe(1);
  });

  it("fires onEnd for an error the browser does NOT follow with onend", () => {
    const made = installRecognizer({});
    const record = handlers();
    startLocalDictation(record);
    made[0].onerror?.({ error: "audio-capture" });
    expect(record.ends).toBe(1);
  });

  it("releases the microphone on the error path rather than trusting the browser to", () => {
    // `end()` detaches the handlers and makes stop()/abort() early-return, so
    // an error code that does not end the underlying session would otherwise
    // leave a live recognizer nothing can stop — a hot microphone behind a UI
    // that has already said the session ended.
    const made = installRecognizer({});
    const session = startLocalDictation(handlers());
    made[0].onerror?.({ error: "network" });
    expect(made[0].aborted).toBe(1);
    // And the caller's own stop is still inert, which is why the line above has
    // to be the seam's job.
    session.stop();
    expect(made[0].stopped).toBe(0);
  });

  it("does not re-append a final result the browser delivers twice", () => {
    // Chrome advances `resultIndex` past a final result, so this should not
    // arise — but `committed` accumulates, so a browser that re-sent one would
    // double the words rather than degrade. Pinned because the assumption is
    // invisible in the code that relies on it.
    const made = installRecognizer({});
    const record = handlers();
    startLocalDictation(record);
    made[0].emit(0, [finalResult("Call the vet.")]);
    made[0].emit(1, [interimResult("about")]);
    expect(record.transcripts.at(-1)).toBe("Call the vet. about");
  });

  it("fires onEnd on the silence timeout — an onend with no error and no stop", () => {
    const made = installRecognizer({});
    const record = handlers();
    startLocalDictation(record);
    made[0].onend?.();
    expect(record.ends).toBe(1);
    expect(record.errors).toEqual([]);
  });

  it("suppresses a result and an error arriving after the session ended", () => {
    const made = installRecognizer({});
    const record = handlers();
    const session = startLocalDictation(record);
    made[0].emit(0, [finalResult("kept")]);
    session.abort();
    // A recognizer that keeps talking after abort() changes nothing here, and
    // the caller's own generation guard is a second line, not the only one.
    made[0].onresult?.({ resultIndex: 0, results: { 0: finalResult("dropped"), length: 1 } });
    made[0].onerror?.({ error: "aborted" });
    expect(record.transcripts).toEqual(["kept"]);
    expect(record.errors).toEqual([]);
    expect(record.ends).toBe(1);
  });

  it("throws nothing when there is no API — it reports onError then onEnd", async () => {
    const record = handlers();
    const session = startLocalDictation(record);
    // Synchronously, nothing has happened yet: the caller has a session to
    // store before either callback can land.
    expect(record.errors).toEqual([]);
    expect(record.ends).toBe(0);
    session.stop();
    await Promise.resolve();
    expect(record.errors).toHaveLength(1);
    expect(record.ends).toBe(1);
  });

  it("throws nothing when start() itself throws", async () => {
    const made = installRecognizer({ startThrows: true });
    const record = handlers();
    expect(() => startLocalDictation(record)).not.toThrow();
    expect(record.ends).toBe(0);
    await Promise.resolve();
    expect(record.errors).toHaveLength(1);
    expect(record.ends).toBe(1);
    expect(made[0].started).toBe(false);
  });
});
