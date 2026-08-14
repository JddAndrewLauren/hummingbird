// The shell's one breakpoint, as a TypeScript constant.
//
// **640, not 768.** 768 was already a documented desktop state in
// `docs/SURFACES.md`'s width matrix — "the context panel has wrapped below the
// column" — so moving the rail there would silently redefine a width the
// visual gate already photographs. 640 sits above the largest phone in
// portrait (430pt) and below iPad mini portrait (744pt), which leaves all
// three pre-existing visual projects (1440 / 1024 / 768) behaving exactly as
// their matrix rows say, and gives the new `phone` project (390) the only
// viewport below the line.
//
// This value exists in exactly two places: here, and as the literal inside
// every `@media` in `responsive.css` — CSS cannot read a custom property from
// a media query and this project has no PostCSS plugin that could inline one.
// `responsive-breakpoint.test.ts` reads that file's source text and pins the
// two equal, so the pair cannot drift.
export const PHONE_MAX_WIDTH_PX = 640;

/** The media query the phone form is chosen by. Consumed by `useIsPhone`;
 * `responsive.css` spells the same query out in CSS. */
export const PHONE_QUERY = `(max-width: ${PHONE_MAX_WIDTH_PX}px)`;
