import { buildClaudeArgs } from "./claude-cli.js";

/**
 * Spawns `claude -p` for one skill invocation and resolves the outcome
 * `server.js` turns into the final NDJSON envelope line. `spawn` is
 * injected (never `node:child_process` imported directly) so this stays
 * unit-testable without a real `claude` binary or credentials.
 *
 * **Unconfirmed against a live run** (recorded, not silently assumed, the
 * same posture `server/city-waste/src/page.rs`'s header keeps): this reads
 * `claude -p --output-format json --json-schema <schema>`'s stdout as the
 * schema-constrained result object directly. If a live run instead wraps
 * it in a `{type, result, ...}` envelope the way plain `--output-format
 * json` does, this parse step is the one place to adjust -- everything
 * around it (the argv, the progress heartbeat, the ok/error split) does
 * not change.
 *
 * @param {object} opts
 * @param {string} opts.skillName
 * @param {string} opts.prompt
 * @param {string} opts.schemaPath
 * @param {(command: string, args: string[]) => import("node:events").EventEmitter} opts.spawn
 * @param {(message: string) => void} opts.onProgress
 * @param {string} [opts.claudeBin]
 * @returns {Promise<{ok: true, result: unknown} | {ok: false, error: string}>}
 */
export function runSkill({ skillName, prompt, schemaPath, spawn, onProgress, claudeBin = "claude" }) {
  return new Promise((resolve) => {
    onProgress(`running skill ${skillName}`);

    const child = spawn(claudeBin, buildClaudeArgs(prompt, schemaPath));

    let stdout = "";
    let stderr = "";

    child.stdout?.on("data", (chunk) => {
      stdout += chunk.toString();
    });
    child.stderr?.on("data", (chunk) => {
      stderr += chunk.toString();
    });

    child.on("error", (err) => {
      resolve({ ok: false, error: err.message });
    });

    child.on("close", (code) => {
      if (code !== 0) {
        resolve({
          ok: false,
          error: stderr.trim() || `claude exited with code ${code}`,
        });
        return;
      }

      try {
        const result = JSON.parse(stdout);
        resolve({ ok: true, result });
      } catch {
        resolve({ ok: false, error: "could not parse claude output as JSON" });
      }
    });
  });
}
