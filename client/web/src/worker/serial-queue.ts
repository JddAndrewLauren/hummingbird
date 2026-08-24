// A generic one-at-a-time request queue with an abandon-on-timeout path —
// the fix issue #95's risk list names: "check-out/check-in solves
// re-entrancy but not a request that never settles, and the single-file
// queue would stall behind it." `worker/task-worker.ts`'s
// `createTaskRequestQueue` (#105) and `worker/calendar-worker.ts`'s
// `createRequestQueue` (#173, porting #73's original queue onto this one)
// are both built on this.
//
// "Abandoned" here means the *queue* moves on — the timed-out handler's
// promise is left to float, un-awaited, rather than actually cancelled
// (there is no `AbortSignal` threaded through the wasm boundary to cancel a
// Rust future with). If it eventually settles, its result is discarded. That
// is safe specifically because of the check-out/check-in pattern the host
// bindings already use (`client/ffi-web/src/lib.rs`): a straggler that
// finally resumes finds the core it was holding already checked back in by
// nobody (it still holds its own checked-out copy, so it checks in exactly
// once, same as any other call) — the *next* request the queue already
// started meanwhile either finds the core present or, in the narrow window
// both are actually mid-flight, sees "busy" rather than corrupting state or
// panicking. A hung request is a bug to fix, not a safe steady state; this
// is a backstop against the queue wedging while that bug is found.

export interface SerialQueueOptions<Req> {
  /** How long one request may run before the queue gives up waiting on it
   * and moves on to the next. */
  timeoutMs: number;
  /** Called (never thrown) when a request is abandoned this way. */
  onTimeout: (request: Req) => void;
  /** Called (never thrown) when a request's handler promise rejects.
   * Defaults to logging, matching `calendar-worker.ts`'s own queue. */
  onError?: (request: Req, error: unknown) => void;
  /** #707's diagnostics-journal hooks — this is the machinery that already
   * knows the two facts a journal reader needs about queueing: the moment a
   * request joins the tail chain (its wait begins), and the moment it
   * reaches the front and actually starts running. Optional, and never
   * thrown, so a caller with nothing wired up (`calendar-worker.ts`'s own
   * queue, which does not observe these) pays nothing. */
  onEnqueue?: (request: Req) => void;
  onDequeue?: (request: Req) => void;
}

function withTimeout<Req>(
  request: Req,
  run: () => Promise<void>,
  options: SerialQueueOptions<Req>,
): Promise<void> {
  return new Promise((resolve) => {
    let settled = false;
    const timer = setTimeout(() => {
      if (settled) {
        return;
      }
      settled = true;
      options.onTimeout(request);
      resolve();
    }, options.timeoutMs);

    run()
      .catch((error: unknown) => {
        (options.onError ?? defaultOnError)(request, error);
      })
      .finally(() => {
        if (settled) {
          // The timeout already resolved the queue's wait on this request;
          // nothing left to do but let this straggler's own effects (it
          // already ran its `post` calls, if any, before settling) stand.
          return;
        }
        settled = true;
        clearTimeout(timer);
        resolve();
      });
  });
}

function defaultOnError<Req>(request: Req, error: unknown): void {
  console.error("worker request failed", request, error);
}

/** Chains every request into one at-a-time execution, same ordering
 * guarantee as `calendar-worker.ts`'s `createRequestQueue`, plus the
 * timeout/abandon path described above. */
export function createSerialQueue<Req>(
  handle: (request: Req) => Promise<void>,
  options: SerialQueueOptions<Req>,
): (request: Req) => Promise<void> {
  let tail: Promise<void> = Promise.resolve();
  return (request) => {
    options.onEnqueue?.(request);
    tail = tail.then(() => {
      options.onDequeue?.(request);
      return withTimeout(request, () => handle(request), options);
    });
    return tail;
  };
}
