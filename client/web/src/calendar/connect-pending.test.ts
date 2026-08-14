// @vitest-environment jsdom
//
// The per-file opt-in `vitest.config.ts` documents — `environment: "node"` is
// the default here, and this module is nothing but `window`/`document`
// listeners. It is a `.test.ts` rather than a `.test.tsx` because no component
// is involved: the whole point of extracting this seam was to make the resume
// behaviour assertable by dispatching two events, with no React, no store and
// no GIS anywhere near it.

import { afterEach, describe, expect, it, vi } from "vitest";
import { watchConnectPendingRestore } from "./connect-pending";

/** Drives `document.visibilityState`, which jsdom exposes as a getter with no
 * setter. Returns the teardown so each test leaves the document visible. */
function setVisibility(state: DocumentVisibilityState): void {
  Object.defineProperty(document, "visibilityState", {
    configurable: true,
    get: () => state,
  });
}

function watch(isRedirectInFlight: () => boolean) {
  const onRestore = vi.fn();
  const stop = watchConnectPendingRestore({ isRedirectInFlight, onRestore });
  return { onRestore, stop };
}

afterEach(() => {
  setVisibility("visible");
});

describe("watchConnectPendingRestore", () => {
  it("clears on a bfcache restore, which is the whole bug", () => {
    // Press Back from Google's consent screen and the page is commonly
    // restored from bfcache: module scope does not re-run, React state comes
    // back exactly as it was, and `connectPending` — set just before the
    // navigation — is still true. The button is disabled and spinning with no
    // error and no way out but a reload.
    const { onRestore, stop } = watch(() => true);
    window.dispatchEvent(new Event("pageshow"));
    expect(onRestore).toHaveBeenCalledTimes(1);
    stop();
  });

  it("does not filter `pageshow` on `persisted`", () => {
    // `shell/useAppUpdate.ts` records why, and it applies unchanged here:
    // several real resume paths report `persisted: false` for a restore that
    // nonetheless skipped every other signal. A `PageTransitionEvent` with
    // `persisted: false` — and a bare `Event`, which has no `persisted` at all
    // — must both count.
    const { onRestore, stop } = watch(() => true);
    window.dispatchEvent(new PageTransitionEvent("pageshow", { persisted: false }));
    expect(onRestore).toHaveBeenCalledTimes(1);
    stop();
  });

  it("clears on a resume that only fires visibilitychange", () => {
    const { onRestore, stop } = watch(() => true);
    setVisibility("visible");
    document.dispatchEvent(new Event("visibilitychange"));
    expect(onRestore).toHaveBeenCalledTimes(1);
    stop();
  });

  it("ignores the hiding edge", () => {
    // That edge is the navigation to Google itself — the moment the pending
    // flag is most legitimately set.
    const { onRestore, stop } = watch(() => true);
    setVisibility("hidden");
    document.dispatchEvent(new Event("visibilitychange"));
    expect(onRestore).not.toHaveBeenCalled();
    stop();
  });

  it("leaves a popup attempt that is still running alone", () => {
    // Desktop keeps the popup, and there `connectPending` covers a live
    // `await connect(deps)` that clears the flag itself when it settles. A
    // reader who switches tabs and comes back mid-consent fires
    // `visibilitychange`; re-enabling the button under a request that is still
    // in flight invites a second concurrent consent. Only the redirect branch
    // arms this watcher, and only it is unrecoverable without it.
    const { onRestore, stop } = watch(() => false);
    window.dispatchEvent(new Event("pageshow"));
    document.dispatchEvent(new Event("visibilitychange"));
    expect(onRestore).not.toHaveBeenCalled();
    stop();
  });

  it("stops listening when torn down", () => {
    const { onRestore, stop } = watch(() => true);
    stop();
    window.dispatchEvent(new Event("pageshow"));
    document.dispatchEvent(new Event("visibilitychange"));
    expect(onRestore).not.toHaveBeenCalled();
  });
});
