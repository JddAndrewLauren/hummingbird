import { SCREENS, type Screen } from "./screens";

// The phone nav's partition, and the product decision behind it.
//
// Nine screens cannot sit in a 390px bar: `--touch-min` is 44px, and nine
// 44px targets need 396px before any gap or padding. So four are on the bar
// and the rest live behind a "More" sheet.
//
// **Which four**, and why: the bar is for the surfaces you *act* on, in the
// order the day runs. `now` is the app's answer to its own question.
// `triage` is the inbox — the one surface with a count that means work is
// waiting. `alerts` is the interrupt lane (ADR-0012): the only thing that
// asks for attention rather than waiting for it. `status` is "is everything
// working" (ADR-0017), which on a phone is the question you have when
// something looks wrong and you are nowhere near a desktop.
//
// What that leaves behind the sheet is the looking-and-configuring set:
// `projects` and `rules` are edited rarely and badly suited to 390px anyway;
// `done` and `ledger` are history, read not acted on; `settings` is where
// you go deliberately. None of them is unreachable — they are one tap
// further, which is the whole trade.
//
// **Both halves derive from `SCREENS`.** `screens.ts`'s header already makes
// the rule explicit: the order lives there and nowhere else. A second
// hand-written list here would silently drop a screen the day one is added —
// off the bar AND out of the sheet, which is the one outcome with no visible
// symptom. `nav-bar.test.ts` asserts the two halves reconstruct `SCREENS`
// exactly, so a new screen lands in the sheet by default and the test says so.
const ON_THE_BAR: ReadonlySet<Screen> = new Set<Screen>(["now", "triage", "alerts", "status"]);

/** The four screens with their own control on the bar, in `SCREENS` order. */
export const NAV_BAR_PRIMARY: readonly Screen[] = SCREENS.filter((screen) => ON_THE_BAR.has(screen));

/** Everything else, in `SCREENS` order — the contents of the More sheet. */
export const NAV_BAR_OVERFLOW: readonly Screen[] = SCREENS.filter(
  (screen) => !ON_THE_BAR.has(screen),
);

/** Whether the More control should read as the current location: it does when
 * the open screen is one of the ones it hides. Without this the bar shows no
 * current page at all while Settings is open, and a bar with nothing selected
 * reads as "you are nowhere". */
export function isOverflowScreen(screen: Screen): boolean {
  return !ON_THE_BAR.has(screen);
}
