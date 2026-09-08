// The one recipe for `design/tokens.css`: the web's design tokens, copied
// (never linked — an unpacked extension is a flat directory the browser
// reads by path) with `fonts.css` reduced to its `:root` stacks, because
// its `@font-face` rules point at `/fonts/*.woff2` and inside an extension
// page that resolves against the extension origin and silently fails; the
// stacks already fall back to a system face.
//
//   node copy-tokens.js          # rewrite design/tokens.css
//
// `tokens.test.js` renders the same thing and compares, so a token change in
// `client/web` that is not re-copied here fails CI rather than drifting.
import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const here = new URL("./", import.meta.url);
const source = new URL("../../client/web/src/design/tokens/", here);
export const target = new URL("design/tokens.css", here);

const HEADER = `/* The design tokens, copied from client/web/src/design/tokens/ (the
   mirror of the Hummingbird Design System) by copy-tokens.js; never
   hand-edit. fonts.css is deliberately reduced to its :root stacks — see
   copy-tokens.js. */
`;

export function render() {
  const read = (name) => readFileSync(new URL(name, source), "utf8");
  const fonts = read("fonts.css");
  const root = fonts.slice(fonts.indexOf("\n:root{") + 1);
  const rest = ["colors", "typography", "spacing", "radius", "elevation", "motion", "base"].map((n) => read(`${n}.css`));
  return [HEADER, root, ...rest].join("\n");
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  writeFileSync(target, render());
  console.log(`wrote ${fileURLToPath(target)}`);
}
