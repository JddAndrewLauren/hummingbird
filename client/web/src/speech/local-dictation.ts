// The browser seam for local dictation (#379) — the ONLY place in this app
// that touches `SpeechRecognition`. Browser-only and domain-free by
// construction: no React import, no Hummingbird vocabulary, no top-level side
// effect (the constructor is read inside each call, never at module scope), so
// a component test replaces the whole module with `vi.mock` and the real API
// is never needed to test the UI.
//
// ADR-0022 is the law here, and two of its clauses are load-bearing in code
// rather than in prose:
//
//   * `processLocally = true` is set on the instance BEFORE `start()`, every
//     time, unconditionally. It defaults to `false` (measured, ADR-0022
//     Decision 5), so omitting it is not a missing nicety — it IS the
//     network-backed path this app promises never to take. There is no
//     fallback here, on any path, including error paths.
//   * A constructor with no `available` static routes to `unsupported`, not to
//     "probably fine". Safari 26.6 ships a complete working recognizer with no
//     way to require local processing (ADR-0022 Decision 6), so a
//     constructor-presence check would light a microphone against cloud
//     recognition on every browser on iOS/iPadOS.
//
// The readiness string is treated as an OPEN vocabulary. Only "downloadable",
// "downloading" and "available" have ever been observed; everything else —
// including #377's assumed "unavailable" — routes to `unsupported`, the safe
// arm. A `switch` over an exhaustive union would be writing an unverified type
// into the codebase.
//
// `install()` is deliberately absent: installing the on-device pack belongs to
// #381, and ADR-0022 measured that it throws unless it is called from a real
// user gesture. Nothing here calls it.
//
// **`quality` is left at its default, and the default is `"command"`** (one of
// four instance members ADR-0022 measured: `processLocally`, `phrases`,
// `unspokenPunctuation`, `quality`). `"command"` is short-utterance tuning, so
// it is plausibly what decides how eagerly a session ends — which makes it the
// first knob to reach for if #397 finds the session boundaries wrong for
// dictating a sentence rather than issuing a command. Left alone here on
// purpose: a tracer bullet should not tune a parameter before anyone has
// listened to what the default sounds like.

/** Whether local dictation can be offered, in the three arms ADR-0022
 * Decision 2 requires — a gap is not an absence:
 *
 *  - `unsupported`: a browser fact nothing on the page can change. Renders as
 *    nothing at all, never as a disabled or warned-about microphone.
 *  - `setup-required`: the on-device pack is absent or still downloading.
 *    Actionable, and #381's to act on; #379 renders no microphone for it.
 *  - `ready`: local processing has been confirmed for `DICTATION_LANG`. */
export type DictationCapability =
  | { kind: "unsupported"; reason: string }
  | { kind: "setup-required" }
  | { kind: "ready" };

/** A recognizer failure, already mapped to something a person can read.
 * `code` is the browser's own `SpeechRecognitionErrorEvent.error` string,
 * carried verbatim so an unrecognized one is still reportable rather than
 * being flattened into "something went wrong". */
export interface DictationError {
  code: string;
  message: string;
}

export interface DictationHandlers {
  /** The session's transcript SO FAR — committed results plus the live
   * interim tail, every time. Cumulative, never a delta: this is what makes a
   * later interim result *replace* the earlier one downstream instead of
   * accumulating, given a frozen draft (`screens/capture-dictation.ts`). */
  onTranscript: (transcript: string) => void;
  onError: (error: DictationError) => void;
  /** Fires EXACTLY ONCE per session, always last, on every path — stop,
   * abort, error, silence timeout, and a synchronous start failure. Teardown
   * hangs off this and nothing else. */
  onEnd: () => void;
}

export interface DictationSession {
  /** Ends the session, releasing the microphone. Callbacks are inert
   * afterwards, so a trailing final result the recognizer had not yet
   * delivered is dropped — the transcript the caller last saw is the
   * transcript it keeps. */
  stop: () => void;
  /** Ends the session and discards audio the recognizer has not yet turned
   * into a result. Same observable contract as `stop`; the difference is only
   * how much the browser is asked to flush. */
  abort: () => void;
}

/** The one language this slice asks for. A list member, not a bare string:
 * `langs` is a required dictionary member and `available("en-US")` throws
 * `TypeError` (ADR-0022 Decision 5). */
export const DICTATION_LANG = "en-US";

interface SpeechRecognitionAlternativeLike {
  transcript: string;
}

interface SpeechRecognitionResultLike {
  isFinal: boolean;
  0?: SpeechRecognitionAlternativeLike;
}

interface SpeechRecognitionEventLike {
  resultIndex: number;
  results: { length: number; [index: number]: SpeechRecognitionResultLike };
}

interface SpeechRecognitionErrorEventLike {
  error: string;
}

interface SpeechRecognitionLike {
  lang: string;
  continuous: boolean;
  interimResults: boolean;
  /** Optional on the type for the same reason it is optional at runtime: the
   * long-standing Web Speech recognizer has no such member. `startLocalDictation`
   * is only ever reached past a positive capability check, but the type must
   * not assert what `strict` cannot see. */
  processLocally?: boolean;
  onresult: ((event: SpeechRecognitionEventLike) => void) | null;
  onerror: ((event: SpeechRecognitionErrorEventLike) => void) | null;
  onend: (() => void) | null;
  start: () => void;
  stop: () => void;
  abort: () => void;
}

/** The constructor plus the on-device statics, which are **optional** — that
 * is the whole point. Marked optional rather than asserted with `!`, so the
 * `typeof … === "function"` guards below are what narrows them and `strict` +
 * `noUnusedLocals` pass with no non-null assertion and no ambient `.d.ts`. */
interface SpeechRecognitionCtor {
  new (): SpeechRecognitionLike;
  available?: (options: { langs: string[]; processLocally: boolean }) => Promise<string>;
}

/** One narrow `globalThis` read covering both names. Chrome 151 exposes both
 * and they are the same object; Safari exposes only the prefixed one — both
 * branches are exercised by a real browser, not defensive padding. */
function readConstructor(): SpeechRecognitionCtor | null {
  const scope = globalThis as {
    SpeechRecognition?: SpeechRecognitionCtor;
    webkitSpeechRecognition?: SpeechRecognitionCtor;
  };
  return scope.SpeechRecognition ?? scope.webkitSpeechRecognition ?? null;
}

/** The cheap, SYNCHRONOUS "could this browser possibly dictate locally"
 * check — a constructor AND the static that can be asked about local
 * processing. False here means the async probe is pointless and must not run.
 *
 * It is synchronous so a component can answer "no dictation here" inside a
 * `useState` initializer: no promise, no post-render `setState`, and therefore
 * no React `act()` warning in the existing `CapturePopover.test.tsx` cases,
 * which mount the real capture box under a jsdom that has no speech API at
 * all. That is the entire reason this is separate from the probe below. */
export function isDictationApiPresent(): boolean {
  const Ctor = readConstructor();
  return Ctor !== null && typeof Ctor.available === "function";
}

/** Asks the browser whether `DICTATION_LANG` can be recognized **locally**.
 * Never rejects and never throws: every failure is an arm of the result, and
 * an unrecognized answer is `unsupported` rather than an optimistic guess. */
export async function probeDictationCapability(): Promise<DictationCapability> {
  const Ctor = readConstructor();
  if (Ctor === null) {
    return { kind: "unsupported", reason: "This browser has no speech recognition." };
  }
  if (typeof Ctor.available !== "function") {
    // ADR-0022 Decision 2: a working recognizer with no way to require local
    // processing is `unsupported`, not a microphone with a caveat.
    return {
      kind: "unsupported",
      reason: "This browser can't keep speech recognition on the device.",
    };
  }
  let status: string;
  try {
    status = await Ctor.available({ langs: [DICTATION_LANG], processLocally: true });
  } catch {
    return {
      kind: "unsupported",
      reason: "This browser refused the on-device speech check.",
    };
  }
  if (status === "available") {
    return { kind: "ready" };
  }
  if (status === "downloadable" || status === "downloading") {
    return { kind: "setup-required" };
  }
  // The open vocabulary, routed to the safe arm — see the module header.
  return { kind: "unsupported", reason: "This browser can't dictate on the device." };
}

const ERROR_MESSAGES: Record<string, string> = {
  "not-allowed": "Microphone access is blocked for this site.",
  "service-not-allowed": "This browser wouldn't start on-device dictation.",
  "no-speech": "Nothing was heard.",
  "audio-capture": "No microphone was available.",
  // Deliberately not "we'll try the cloud": there is no cloud path to try.
  network: "On-device dictation stopped being available.",
  aborted: "Dictation stopped.",
  "language-not-supported": "English isn't available for on-device dictation.",
};

/** Maps a recognizer error code to a sentence. An unknown code is carried
 * verbatim into the message rather than swallowed — a code this app has never
 * seen is exactly the one worth reading in a bug report. */
export function describeDictationError(code: string): DictationError {
  return { code, message: ERROR_MESSAGES[code] ?? `Dictation stopped (${code}).` };
}

/** Joins two halves of a transcript with at most one space, and none at all
 * when either side is empty. Chrome's own results sometimes arrive with a
 * leading space and sometimes without, so neither concatenating raw nor
 * always inserting a space is right on its own. Boundary spacing against the
 * *draft* is not this module's business — that is the splice helper's. */
function joinHalves(left: string, right: string): string {
  if (left === "") {
    return right;
  }
  if (right === "") {
    return left;
  }
  if (/\s$/.test(left) || /^\s/.test(right)) {
    return left + right;
  }
  return `${left} ${right}`;
}

const INERT_SESSION: DictationSession = { stop: () => {}, abort: () => {} };

/** Opens a local dictation session.
 *
 * **Throws nothing.** A synchronous failure — no constructor, a constructor
 * that refuses to be built, a `start()` that throws — arrives as `onError`
 * followed by `onEnd` on a microtask, so the caller has exactly one lifecycle
 * shape to reason about whatever went wrong, and never a `try`/`catch` around
 * this call competing with the callbacks.
 *
 * The returned session is safe to `stop()`/`abort()` any number of times and
 * after it has already ended; `onEnd` still fires exactly once. */
export function startLocalDictation(handlers: DictationHandlers): DictationSession {
  const Ctor = readConstructor();
  if (Ctor === null || typeof Ctor.available !== "function") {
    return failAsync(handlers, "not-supported");
  }

  let recognition: SpeechRecognitionLike;
  try {
    recognition = new Ctor();
  } catch {
    return failAsync(handlers, "not-supported");
  }

  let ended = false;
  function end(): void {
    if (ended) {
      return;
    }
    ended = true;
    recognition.onresult = null;
    recognition.onerror = null;
    recognition.onend = null;
    handlers.onEnd();
  }

  // Committed halves accumulate here; the interim tail is rebuilt from
  // scratch on every event, which is what keeps the emitted transcript
  // cumulative-but-not-doubled when a result goes interim -> final.
  let committed = "";
  recognition.onresult = (event) => {
    if (ended) {
      return;
    }
    let interim = "";
    for (let index = event.resultIndex; index < event.results.length; index += 1) {
      const result = event.results[index];
      const text = result?.[0]?.transcript ?? "";
      if (result?.isFinal) {
        committed = joinHalves(committed, text);
      } else {
        interim = joinHalves(interim, text);
      }
    }
    handlers.onTranscript(joinHalves(committed, interim));
  };
  recognition.onerror = (event) => {
    if (ended) {
      return;
    }
    handlers.onError(describeDictationError(event.error));
    // Not waiting for the recognizer's own `onend` after an error: some error
    // codes end the session and some do not, and "exactly once, always last"
    // is only guaranteeable if this module owns the ending.
    end();
    // And the microphone is released here, not left to the browser. `end()`
    // detaches the handlers and makes `stop()`/`abort()` early-return, so an
    // error code that does NOT end the underlying session would otherwise
    // leave the recognizer live with nothing able to stop it — a hot
    // microphone behind a UI that has already said the session ended. Safe
    // when the session did end: `abort()` on a finished recognizer is a no-op.
    recognition.abort();
  };
  recognition.onend = () => {
    // The silence-timeout path arrives here with no error and no stop() call.
    end();
  };

  // Every one of these is set BEFORE `start()`. `processLocally` first,
  // because it is the guarantee and the rest is tuning; `quality` is not set at
  // all, for the reason in the module header.
  recognition.processLocally = true;
  recognition.lang = DICTATION_LANG;
  recognition.interimResults = true;
  recognition.continuous = true;

  try {
    recognition.start();
  } catch {
    return failAsync(handlers, "start-failed", () => {
      ended = true;
    });
  }

  return {
    stop: () => {
      if (ended) {
        return;
      }
      // `end()` first: it is what makes "exactly once, always last" true even
      // though the recognizer will fire its own `onend` a moment later, which
      // the flag then swallows.
      end();
      recognition.stop();
    },
    abort: () => {
      if (ended) {
        return;
      }
      end();
      recognition.abort();
    },
  };
}

/** The synchronous-failure path: report on a microtask, so the caller has
 * always returned from `startLocalDictation` (and stored the session) before
 * either callback lands. */
function failAsync(
  handlers: DictationHandlers,
  code: string,
  markEnded: () => void = () => {},
): DictationSession {
  markEnded();
  queueMicrotask(() => {
    handlers.onError(
      code === "not-supported"
        ? { code, message: "This browser can't dictate on the device." }
        : { code, message: "Dictation wouldn't start." },
    );
    handlers.onEnd();
  });
  return INERT_SESSION;
}
