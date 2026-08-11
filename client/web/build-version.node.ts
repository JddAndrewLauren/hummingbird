// The I/O half of the build version — `readFileSync` and the git calls that
// feed `src/shell/build-version.ts`'s pure `computeBuildVersion`. It lives at
// the package root rather than in `src/` on purpose: `node:child_process`
// must never be reachable from a module the browser bundle can pull in.
//
// Every failure here is a `null`/`true` handed to the pure function, never a
// throw and never a guess — a build with no git history renders `+unknown`
// rather than a plausible-but-wrong number.

import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { computeBuildVersion } from "./src/shell/build-version";

const packageRoot = dirname(fileURLToPath(import.meta.url));
const repoRoot = join(packageRoot, "..", "..");
const versionFile = join(repoRoot, "VERSION");

function git(...args: string[]): string | null {
  try {
    return execFileSync("git", args, {
      cwd: repoRoot,
      encoding: "utf8",
      stdio: ["ignore", "pipe", "ignore"],
    }).trim();
  } catch {
    return null;
  }
}

function readBaseText(): string | null {
  try {
    return readFileSync(versionFile, "utf8");
  } catch {
    return null;
  }
}

/** Commits on this branch since `VERSION` was last touched, or `null` if
 *  git cannot answer (no repo, no history for the file). */
function commitsSinceVersionTouched(): number | null {
  const anchor = git("log", "-1", "--format=%H", "--", versionFile);
  if (anchor === null || anchor === "") return null;
  const count = git("rev-list", "--count", `${anchor}..HEAD`);
  if (count === null || !/^\d+$/.test(count)) return null;
  return Number(count);
}

/** `GITHUB_REF` in Actions; the checked-out branch locally. Anything that is
 *  not `main` is a dev build. */
function isMainBuild(): boolean {
  const ref = process.env.GITHUB_REF;
  if (typeof ref === "string" && ref !== "") return ref === "refs/heads/main";
  return git("rev-parse", "--abbrev-ref", "HEAD") === "main";
}

export function readBuildVersion(): string {
  return computeBuildVersion({
    baseText: readBaseText(),
    commitCount: commitsSinceVersionTouched(),
    // A shallow clone's `rev-list --count` is silently truncated, so ask
    // first and refuse the number rather than shipping a wrong one.
    shallow: git("rev-parse", "--is-shallow-repository") !== "false",
    isMainBuild: isMainBuild(),
  });
}
