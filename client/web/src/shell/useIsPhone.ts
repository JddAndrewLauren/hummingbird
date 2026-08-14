import { useSyncExternalStore } from "react";
import { PHONE_QUERY } from "./breakpoints";

// The viewport class, read as external state — `theme/useTheme.ts`'s shape,
// and for the same reason: copying a media query into `useState` means
// re-syncing inside an effect, which renders once with the stale value before
// correcting itself. Here that would paint a nav rail for one frame on a
// phone, then swap it for the bar.
//
// **This is only for where the DOM tree itself differs.** Everything that is
// merely a different *style* at a different width belongs in
// `responsive.css`, whose header argues the split. Two callers today: `App`
// (rail versus bottom bar — two different components, mounting exactly one)
// and `CapturePopover` (a JS-measured anchor that has nothing to measure
// against on a phone).
//
// **The jsdom trap, and why the guard is not defensive noise.** This repo's
// jsdom does not implement `matchMedia` at all — `window.matchMedia` is
// `undefined`, and an unguarded call throws inside render, taking down every
// component test that mounts anything containing this hook (21 of
// `CapturePopover`'s did, the first time round). Treating "no `matchMedia`"
// as "not a phone" is also the honest answer: this is a viewport question,
// and a runtime that cannot answer it is not a 390px screen.
//
// The consequence for tests is the part to hold on to. Every existing
// component test keeps seeing the desktop branch, which is what they all
// want and assert. But a test of the *phone* branch would need to stub
// `window.matchMedia`, and a forgotten stub does NOT error — the component
// renders its desktop form and the test passes, having asserted nothing about
// the phone at all. So `NavBar.test.tsx` renders `NavBar` directly rather
// than driving `App` through this hook: its assertions cannot be faked by a
// missing stub, because no stub is involved.
function query(): MediaQueryList | null {
  return typeof window.matchMedia === "function" ? window.matchMedia(PHONE_QUERY) : null;
}

function subscribe(onStoreChange: () => void): () => void {
  const list = query();
  if (!list) {
    return () => {};
  }
  list.addEventListener("change", onStoreChange);
  return () => list.removeEventListener("change", onStoreChange);
}

function isPhone(): boolean {
  return query()?.matches ?? false;
}

/** True below `PHONE_MAX_WIDTH_PX`. Server snapshot is `false` — there is no
 * SSR here, but `useSyncExternalStore` requires the third argument in a
 * typed call and "desktop" is the safe default either way. */
export function useIsPhone(): boolean {
  return useSyncExternalStore(subscribe, isPhone, () => false);
}
