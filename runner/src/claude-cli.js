/**
 * The exact argv shape #41 decision 4 names: `claude -p --output-format
 * json --json-schema <schema>`, prompt passed as a single argv element
 * (never shelled out to, so a prompt carrying quotes/newlines needs no
 * escaping).
 *
 * @param {string} prompt
 * @param {string} schemaPath absolute path to the per-skill JSON schema
 * @returns {string[]}
 */
export function buildClaudeArgs(prompt, schemaPath) {
  return ["-p", prompt, "--output-format", "json", "--json-schema", schemaPath];
}
