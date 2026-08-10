import { expect, test, type Page } from "@playwright/test";

// The visual gate's one spec. Two jobs, deliberately separated:
//
// 1. CAPTURE. Write a PNG per screen x width x theme into `visual/.captures/`
//    for a human to review for clipping, overlap, broken wrapping and
//    sticky/scroll glitches. There is no committed golden and no pixel diff:
//    this project has no baseline history, and a pixel-diff gate with nobody to
//    arbitrate it produces noise, not findings. The captures are the
//    deliverable; `/wrapup`'s visual phase reads them.
//
// 2. ASSERT the few things a machine can decide without a baseline — that
//    nothing overflows horizontally, that the brand tokens actually resolve,
//    and that the theme switch reaches the page. These fail the run.
//
// Everything renders in `?demo` mode: the fixtures are deterministic and
// populated, where real data on a dev machine is an empty mirror — the shell
// takes the authority's origin from its own (`src/worker/core.worker.ts`),
// and `vite dev` proxies `/api` to a local `wrangler dev` this run does not
// start, with no deployed authority behind it either (#95's H3 gate). The
// honest empty states are captured too, from the same screens without the
// flag.

const SCREENS = [
  { name: "now", nav: "Now" },
  { name: "triage", nav: "Triage" },
  { name: "routes", nav: "Routes" },
  { name: "alerts", nav: "Alerts" },
  { name: "settings", nav: "Settings" },
] as const;

const THEMES = ["light", "dark"] as const;

/** The app resolves `light | dark | system` onto `[data-theme]` from
 * `hb.theme` (see `src/theme/`). Seeding the key before the first paint is
 * what avoids capturing a flash of the other theme. */
async function openApp(page: Page, theme: (typeof THEMES)[number], demo: boolean) {
  await page.addInitScript((value) => {
    window.localStorage.setItem("hb.theme", value);
  }, theme);
  await page.goto(demo ? "/?demo" : "/");
  // The shell paints before the wasm core is ready (the core's status is a
  // label, not a gate), so waiting on the nav rail is enough — and waiting
  // on the core would hang on a machine with no authority to reach.
  await expect(page.getByRole("navigation")).toBeVisible();
  await page.evaluate(() => document.fonts.ready);
}

async function show(page: Page, nav: string) {
  await page.getByRole("navigation").getByRole("button", { name: nav }).click();
}

/** No horizontal overflow at any width. The layout wraps rather than using
 * media queries (`screens/layout.tsx`), so an element that refuses to shrink
 * shows up here and nowhere else — a real clipping bug, machine-decidable
 * without a golden. One pixel of slack for sub-pixel rounding. */
async function expectNoHorizontalOverflow(page: Page) {
  const overflow = await page.evaluate(() => {
    const doc = document.documentElement;
    return doc.scrollWidth - doc.clientWidth;
  });
  expect(overflow).toBeLessThanOrEqual(1);
}

for (const theme of THEMES) {
  test.describe(`${theme} theme`, () => {
    for (const screen of SCREENS) {
      test(`${screen.name} renders and captures`, async ({ page }, testInfo) => {
        await openApp(page, theme, true);
        await show(page, screen.nav);
        await expectNoHorizontalOverflow(page);
        await page.screenshot({
          path: `visual/.captures/${screen.name}-${testInfo.project.name}-${theme}.png`,
          fullPage: true,
        });
      });
    }

    test("empty states capture", async ({ page }, testInfo) => {
      // No `?demo`: the honest empty states, which are what a new device
      // actually shows and which no fixture screen ever exercises.
      await openApp(page, theme, false);
      await show(page, "Now");
      await expect(page.getByText("Nothing to start")).toBeVisible();
      await expectNoHorizontalOverflow(page);
      await page.screenshot({
        path: `visual/.captures/now-empty-${testInfo.project.name}-${theme}.png`,
        fullPage: true,
      });
    });
  });
}

test.describe("brand token bindings", () => {
  // Asserted against the tokens themselves, never a hardcoded hex — the
  // design system is the source of truth and a re-pull must be free to
  // change the values (CLAUDE.md, "The design system").
  test("the layout constants and accent resolve, and dark mode reaches the page", async ({
    page,
  }) => {
    await openApp(page, "light", true);

    const tokens = await page.evaluate(() => {
      const style = getComputedStyle(document.documentElement);
      return {
        rail: style.getPropertyValue("--rail-width").trim(),
        panel: style.getPropertyValue("--panel-width").trim(),
        content: style.getPropertyValue("--content-max").trim(),
        accent: style.getPropertyValue("--accent").trim(),
        page: style.getPropertyValue("--surface-page").trim(),
      };
    });
    // Present and non-empty is the assertion — an unresolved custom property
    // reads as "" and silently renders as nothing, which is the failure mode
    // worth catching. The specific values belong to the design system.
    for (const [name, value] of Object.entries(tokens)) {
      expect(value, `--${name} resolved`).not.toBe("");
    }
    expect(tokens.rail).toMatch(/px$/);

    const lightPage = await page.evaluate(
      () => getComputedStyle(document.body).backgroundColor,
    );

    await openApp(page, "dark", true);
    expect(await page.getAttribute("html", "data-theme")).toBe("dark");
    const darkPage = await page.evaluate(
      () => getComputedStyle(document.body).backgroundColor,
    );
    // The switch must actually repaint the page surface, not merely set an
    // attribute nothing reads.
    expect(darkPage).not.toBe(lightPage);
  });
});
