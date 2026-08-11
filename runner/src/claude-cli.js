/**
 * The exact argv shape #41 decision 4 names: `claude -p --output-format
 * json --json-schema <schema>`, prompt passed as a single argv element
 * (never shelled out to, so a prompt carrying quotes/newlines needs no
 * escaping).
 *
 * **`--json-schema` takes the schema's TEXT, never a path to it** —
 * confirmed against the CLI, which rejects a path outright (`--json-schema
 * is not valid JSON: Unrecognized token '/'`) before it runs anything, so
 * passing one makes every invocation fail. `run-skill.js` reads the
 * versioned per-skill file; this module only ever sees what it read.
 *
 * @param {string} prompt
 * @param {string} schemaText the per-skill JSON schema's contents
 * @returns {string[]}
 */
export function buildClaudeArgs(prompt, schemaText) {
  return ["-p", prompt, "--output-format", "json", "--json-schema", schemaText];
}
