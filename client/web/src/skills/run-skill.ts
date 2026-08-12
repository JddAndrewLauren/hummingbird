// The seam #273 asks for: one request to the skill runner, exposed as a
// sequence of progress messages followed by exactly one terminal outcome.
//
// **An async generator, not a callback and not a reducer.** A callback
// gives the consumer no way to `await` the run's end, so "pull the steps
// once it finishes" would need a second callback; a reducer would bake
// React into the one module #274 puts a picker in front of. A generator is
// the shape both consumers want: `for await` reads the narration, and the
// loop's completion *is* the end of the run.
//
// **Which backend stays outside this module** — the caller passes it in.
// That is the whole of #273's "keep the which-backend decision outside the
// seam", and #274 replaces exactly one expression in the wiring.
//
// Four invariants, each pinned by a test:
//
// 1. **It never throws.** A rejected `fetch`, a non-200, a missing token
//    and a body that ends without a terminal line all become a synthesized
//    terminal event, so the consumer's `for await` is total and needs no
//    try/catch of its own.
// 2. **Exactly one terminal event**, and anything after it is discarded —
//    a stream carrying two terminal lines is malformed, and the first one
//    is the answer.
// 3. **No constructed message ever contains the token.** Nothing here
//    interpolates it into prose; the only place it appears is the
//    `authorization` header.
// 4. **Nothing here is enqueued, retried or timed.** A skill request is a
//    question, and questions go stale (#269) — the caller's
//    `AbortController` is the only way a run ends early, and an
//    `AbortController` is not a clock.

import { classifyLine } from "./envelope";
import { declineForResponse, declineForTransport, NO_TERMINAL_LINE, NO_TOKEN } from "./decline";
import { takeLines } from "./ndjson";
import type { SkillEvent } from "./run-state";

export interface SkillBackend {
  /** Stable id, for a caller that wants to label which lane ran. #274 makes
   * this vary. */
  id: string;
  /** Same-origin path (ADR-0018): the authority proxies to the runner. */
  endpoint: string;
}

export interface RunSkillDeps {
  fetch: typeof globalThis.fetch;
  /** The device token, or `null` when none is stored. Read on the main
   * thread from `task/token-store.ts`. */
  readToken: () => Promise<string | null>;
  signal?: AbortSignal;
}

export async function* runSkill(
  backend: SkillBackend,
  body: unknown,
  deps: RunSkillDeps,
): AsyncGenerator<SkillEvent> {
  yield { kind: "started" };

  const token = await deps.readToken();
  if (token === null || token === "") {
    // Asserted by a test: `fetch` is never called. There is nothing to
    // authenticate with, and issuing the request anyway would spend a round
    // trip to be told what is already known here.
    yield failure(NO_TOKEN);
    return;
  }

  let response: Response;
  try {
    response = await deps.fetch(backend.endpoint, {
      method: "POST",
      headers: { "content-type": "application/json", authorization: `Bearer ${token}` },
      body: JSON.stringify(body),
      ...(deps.signal ? { signal: deps.signal } : {}),
    });
  } catch (error) {
    yield failure(declineForTransport(messageOf(error)));
    return;
  }

  // A 400 or 413 from the runner is forwarded verbatim by the proxy and IS
  // a valid terminal line, so status alone does not decide — the body is
  // read either way, and only a body with nothing terminal in it falls back
  // to the status.
  let sawTerminal = false;
  try {
    for await (const line of lines(response, deps.signal)) {
      const classified = classifyLine(line);
      if (classified.kind === "progress") {
        if (!sawTerminal) yield classified;
        continue;
      }
      if (classified.kind === "unreadable") continue;
      if (sawTerminal) continue;
      sawTerminal = true;
      yield classified;
    }
  } catch (error) {
    if (!sawTerminal) yield failure(declineForTransport(messageOf(error)));
    return;
  }

  if (!sawTerminal) {
    // The status is the better answer when there is one: an empty-bodied
    // 401 says something specific, where "the run ended without an answer"
    // would be true and useless.
    yield failure(response.ok ? NO_TERMINAL_LINE : declineForResponse(response.status));
  }
}

/** A terminal event this client synthesized. **No stamp**: nothing was
 * attempted, or nothing reported what did — the same rule the proxy follows
 * (ADR-0018), so a `null` backend always means "no lane named it". */
function failure(error: string): SkillEvent {
  return { kind: "failed", error, backend: null, model: null };
}

function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/**
 * The response body as decoded NDJSON lines. `TextDecoder(…, {stream:true})`
 * is what makes a multi-byte character split across two chunks safe — a
 * per-chunk `decode()` would replace its halves with U+FFFD — and the final
 * flush is what lets a last line arriving with no trailing newline still be
 * read.
 */
async function* lines(response: Response, signal?: AbortSignal): AsyncGenerator<string> {
  if (response.body === null) return;
  const reader = response.body.getReader();
  const decoder = new TextDecoder("utf-8");
  let pending = "";
  try {
    for (;;) {
      if (signal?.aborted) return;
      const { done, value } = await reader.read();
      if (done) break;
      const taken = takeLines(pending, decoder.decode(value, { stream: true }));
      pending = taken.rest;
      for (const line of taken.lines) yield line;
    }
    const taken = takeLines(pending, decoder.decode());
    for (const line of taken.lines) yield line;
    const tail = taken.rest.trim();
    if (tail.length > 0) yield tail;
  } finally {
    reader.releaseLock();
  }
}
