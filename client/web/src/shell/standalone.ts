// Whether this view is an installed home-screen app rather than a browser
// tab. Two things turn on it, and both are about the same fact: an installed
// iOS web app has a storage container separate from Safari's, and its popups
// escape to Safari and lose their opener.
//
// - `calendar/connect-error.ts` changes its advice ("try it in Safari" is
//   actively wrong for an installed app — the connection made in a tab is not
//   one the app can see).
// - `google/redirect-flow.ts` is chosen over the popup-based GIS flow, because
//   a top-level navigation is the only mechanism that completes here.
//
// Two checks because the platforms disagree: `display-mode: standalone` is the
// standard, and `navigator.standalone` is iOS Safari's own non-standard
// property, which is what an iOS home-screen app actually reports. Read live
// rather than cached — it is a property of the window, and a cached `false`
// from module-eval order would be wrong forever.
//
// Not a hook: nothing re-renders when this changes, because it cannot change
// within the life of a window.

interface IosNavigator extends Navigator {
  /** iOS Safari only; absent everywhere else. */
  standalone?: boolean;
}

export function isStandalone(): boolean {
  if (typeof window === "undefined") {
    return false;
  }
  // Guarded for the same reason `useIsPhone` guards it: this repo's jsdom
  // does not implement `matchMedia`, and an unguarded call throws inside
  // render, taking down any component test that reaches this line.
  const byDisplayMode =
    typeof window.matchMedia === "function" &&
    window.matchMedia("(display-mode: standalone)").matches;
  return byDisplayMode || (navigator as IosNavigator).standalone === true;
}
