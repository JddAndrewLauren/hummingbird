/**
 * The one child process this runner spawns that is not `claude`.
 *
 * **Why the runner runs the ranker itself.** The `/next-up-hb` skill has
 * two arms (#116), and the interactive one drives `scripts/next-up.sh`
 * from a live session where Bash is a tool the operator has already
 * granted. The runner arm has no such grant: `claude -p` runs
 * non-interactively, where a tool call needing permission cannot be
 * prompted for and is simply denied, and `claude-cli.js` passes no
 * `--allowedTools`. A runner arm that asked the model to shell out would
 * abort on its first `Bash` call, every time -- invisible to a suite whose
 * `spawn` is a fake, exactly like the three `--json-schema`/envelope
 * mistakes `run-skill.js` records.
 *
 * Granting Bash was the other way out and is the worse one: it widens the
 * hosted model's reach from "answer in this schema" to "run anything in
 * the image" to save a process the runner can spawn itself, and it leaves
 * the ranker's failures inside the model's context to be narrated rather
 * than surfaced as an envelope `error`. So the deterministic half runs
 * here, ahead of the model, and the model receives ranked JSON it cannot
 * fail to obtain.
 *
 * `spawn` is injected for the same reason it is everywhere else here: no
 * test needs a real binary.
 */

/** How long the ranker gets. It is a pure fold over one payload -- anything slower is wedged, not busy. */
const RANK_TIMEOUT_MS = 30_000;

/**
 * @param {object} opts
 * @param {(command: string, args: string[]) => import("node:child_process").ChildProcess} opts.spawn
 * @param {string} [opts.bin] path to `next-up-rank` (the image sets `HB_NEXT_UP_BIN`)
 * @param {number} [opts.timeoutMs]
 * @returns {(envelope: unknown) => Promise<{ok: true, ranked: unknown} | {ok: false, error: string}>}
 */
export function createRankRunner({ spawn, bin = "next-up-rank", timeoutMs = RANK_TIMEOUT_MS }) {
  return (envelope) =>
    new Promise((resolve) => {
      let child;
      try {
        child = spawn(bin, []);
      } catch (err) {
        resolve({ ok: false, error: `could not spawn ${bin}: ${err.message}` });
        return;
      }

      let stdout = "";
      let stderr = "";
      let settled = false;
      const settle = (outcome) => {
        if (settled) return;
        settled = true;
        clearTimeout(timer);
        resolve(outcome);
      };

      const timer = setTimeout(() => {
        child.kill?.("SIGKILL");
        settle({ ok: false, error: `${bin} did not answer within ${timeoutMs}ms` });
      }, timeoutMs);
      // Never hold the process open for this timer alone.
      timer.unref?.();

      child.stdout?.on("data", (chunk) => {
        stdout += chunk.toString();
      });
      child.stderr?.on("data", (chunk) => {
        stderr += chunk.toString();
      });
      child.on("error", (err) => settle({ ok: false, error: `${bin}: ${err.message}` }));

      child.on("close", (code) => {
        if (code !== 0) {
          // The ranker names its own envelope problems on stderr
          // (`EnvelopeProblem`); pass that through rather than a bare exit
          // code, since it is the actionable half.
          settle({ ok: false, error: stderr.trim() || `${bin} exited with code ${code}` });
          return;
        }
        try {
          settle({ ok: true, ranked: JSON.parse(stdout) });
        } catch {
          settle({ ok: false, error: `${bin} did not answer JSON` });
        }
      });

      // A ranker that died before reading stdin makes this write EPIPE;
      // that is the `close` branch's story to tell, not an unhandled throw.
      child.stdin?.on("error", () => {});
      child.stdin?.end(JSON.stringify(envelope));
    });
}
