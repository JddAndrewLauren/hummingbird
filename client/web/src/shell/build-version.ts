// The build version the nav rail footer and the Settings "Local core" card
// show beside the api version. The api version says which *contract* the
// core speaks and never moves when the app changes; this one identifies the
// build in front of you.
//
// The scheme (recorded in CLAUDE.md): the repo-root `VERSION` file holds a
// plain `major.minor.patch` line, and the displayed patch is that patch plus
// the number of commits on main since `VERSION` was last touched. So an
// ordinary merge is +1, and the override gesture is editing `VERSION` in the
// PR — at the commit that touches the file the count is 0, so that merge
// lands as exactly what was written.
//
// This module is the whole decision and does no I/O: `build-version.node.ts`
// at the package root holds the `readFileSync`/`execFileSync` half, so
// `node:child_process` can never be pulled into the browser bundle. Same
// pure-module/`.tsx`-threading split every `screens/*.ts` keeps.

export type BuildVersionBase = { major: number; minor: number; patch: number };

export type BuildVersionInput = {
  /** The `VERSION` file's contents, or `null` if it could not be read. */
  baseText: string | null;
  /** Commits since `VERSION` was last touched, or `null` if git could not say. */
  commitCount: number | null;
  /** `git rev-parse --is-shallow-repository` — a shallow clone's count is
   *  silently truncated, so it may never produce a number. */
  shallow: boolean;
  /** Whether this build is of `main` itself. */
  isMainBuild: boolean;
};

/** Three integers or nothing — a partial or decorated line is rejected
 *  rather than coerced into a number nobody wrote. */
export function parseBase(text: string | null): BuildVersionBase | null {
  if (text === null) return null;
  const match = /^\s*(\d+)\.(\d+)\.(\d+)\s*$/.exec(text);
  if (match === null) return null;
  return {
    major: Number(match[1]),
    minor: Number(match[2]),
    patch: Number(match[3]),
  };
}

/**
 * The displayed build version.
 *
 * A number this cannot stand behind is never rendered as one: a shallow
 * clone, an unreadable `VERSION` and an unreadable git history all yield a
 * `+unknown` marker, the same discipline `Freshness::Unknown` follows in
 * `client/core/src/freshness.rs`. A local, non-`main` build gets `+dev`,
 * so a screenshot from a feature branch cannot read as the deployed build.
 */
export function computeBuildVersion(input: BuildVersionInput): string {
  const base = parseBase(input.baseText);
  if (base === null) return "0.0.0+unknown";

  const stem = `${base.major}.${base.minor}`;
  if (input.shallow || input.commitCount === null || input.commitCount < 0) {
    return `${stem}.${base.patch}+unknown`;
  }

  // The count adds to the file's own patch; it never resets to it, so an
  // override to 0.2.0 is 0.2.0, then 0.2.1 on the next merge.
  const version = `${stem}.${base.patch + input.commitCount}`;
  return input.isMainBuild ? version : `${version}+dev`;
}

// Declared here rather than in `src/vite-env.d.ts`, which the node project
// that also compiles this module does not include.
declare const __APP_VERSION__: string;

/**
 * The build version baked in by `vite.config.ts`'s `define`. Read
 * tolerantly: vitest and any other consumer without the define still
 * resolve, rather than every importer needing a second `define` of its own.
 */
export const APP_VERSION: string =
  typeof __APP_VERSION__ === "string" ? __APP_VERSION__ : "0.0.0+dev";
