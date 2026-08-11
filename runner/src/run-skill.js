import { readFileSync } from "node:fs";
import { buildClaudeArgs } from "./claude-cli.js";

/**
 * Spawns `claude -p` for one skill invocation and resolves the outcome
 * `server.js` turns into the final NDJSON envelope line. `spawn` and
 * `readSchema` are both injected (never `node:child_process` /
 * `node:fs` reached for inline) so this stays unit-testable without a real
 * `claude` binary, real credentials or a real repo on disk.
 *
 * **Confirmed against a live run**, and both halves were wrong before:
 *
 * 1. `--json-schema` takes the schema's *text*, not a path — hence
 *    `readSchema`. A read failure is an ordinary `{ok:false}` outcome, not
 *    a throw, so it still terminates the stream in the envelope.
 * 2. `--output-format json` wraps everything in the CLI's own metadata
 *    envelope (`{is_error, usage, result, structured_output, …}`). The
 *    schema-constrained object is `structured_output`; `result` is the
 *    same thing as a *string*. Parsing raw stdout as the result handed
 *    callers the metadata instead of `{title, notes}`.
 *
 * @param {object} opts
 * @param {string} opts.skillName
 * @param {string} opts.prompt
 * @param {string} opts.schemaPath
 * 3. The CLI discovers a repo's skills at `<cwd>/.claude/skills`, and the
 *    prompt every op here builds is a slash command. The image's final
 *    `WORKDIR` is `/app/runner` while the skills are baked at
 *    `/app/.claude/skills`, so a spawn that inherited the runner's own cwd
 *    would leave `/parse-capture` and `/next-up-hb` unresolved -- and
 *    unresolved fails *softly*: the text is passed through as ordinary
 *    prose and a compliant-looking answer comes back against the schema
 *    with none of SKILL.md's rules applied. Hence the explicit `cwd`,
 *    which `server.js` sets to the same `repoRoot` the schema path
 *    resolves against.
 *
 * @param {(command: string, args: string[], options?: object) => import("node:events").EventEmitter} opts.spawn
 * @param {(message: string) => void} opts.onProgress
 * @param {string} [opts.claudeBin]
 * @param {string} [opts.cwd] the directory whose `.claude/skills` the CLI reads
 * @param {(path: string) => string} [opts.readSchema]
 * @returns {Promise<{ok: true, result: unknown} | {ok: false, error: string}>}
 */
export function runSkill({
  skillName,
  prompt,
  schemaPath,
  spawn,
  onProgress,
  claudeBin = "claude",
  cwd,
  readSchema = (path) => readFileSync(path, "utf8"),
}) {
  return new Promise((resolve) => {
    onProgress(`running skill ${skillName}`);

    let schemaText;
    try {
      schemaText = readSchema(schemaPath);
    } catch (err) {
      resolve({ ok: false, error: `could not read schema ${schemaPath}: ${err.message}` });
      return;
    }

    const child = spawn(claudeBin, buildClaudeArgs(prompt, schemaText), ...(cwd ? [{ cwd }] : []));

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

      resolve(readOutcome(stdout));
    });
  });
}

/**
 * The CLI metadata envelope → this runner's outcome. Kept separate from
 * the spawn plumbing above so every shape it has to survive -- a
 * successful run, the CLI's own `is_error`, a run that produced no
 * structured output at all -- is one direct test rather than a fake child
 * process.
 *
 * @param {string} stdout
 * @returns {{ok: true, result: unknown} | {ok: false, error: string}}
 */
export function readOutcome(stdout) {
  let envelope;
  try {
    envelope = JSON.parse(stdout);
  } catch {
    return { ok: false, error: "could not parse claude output as JSON" };
  }

  if (envelope === null || typeof envelope !== "object") {
    return { ok: false, error: "claude output was not the expected result envelope" };
  }

  if (envelope.is_error) {
    // `result` carries the CLI's own error prose on this branch.
    const detail = typeof envelope.result === "string" ? envelope.result : envelope.subtype;
    return { ok: false, error: detail || "claude reported an error" };
  }

  // A run that answered in prose rather than against the schema is a
  // failure here, not an empty success: the whole point of the per-skill
  // schema is that `result` conforms to it (#41 decision 4).
  if (envelope.structured_output === undefined || envelope.structured_output === null) {
    return { ok: false, error: "claude returned no structured output" };
  }

  return { ok: true, result: envelope.structured_output };
}
