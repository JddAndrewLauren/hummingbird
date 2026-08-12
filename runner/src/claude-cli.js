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
 * @param {string} [model] the request's `model` arg, already validated by
 *   [`isValidModelId`]; omitted leaves the CLI on its configured default
 * @returns {string[]}
 */
export function buildClaudeArgs(prompt, schemaText, model) {
  return [
    "-p",
    prompt,
    "--output-format",
    "json",
    "--json-schema",
    schemaText,
    ...(model ? ["--model", model] : []),
  ];
}

/**
 * Whether a value is safe to pass as the `--model` argv element (#273).
 *
 * **A charset rule, deliberately not an allowlist.** An allowlist of known
 * model ids would make every provider swap a code change and a redeploy,
 * contradicting #41 decision 2's "switching providers is `fly secrets set`
 * alone" — and this runner is pointed at whatever `ANTHROPIC_BASE_URL`
 * names, whose model ids it cannot know.
 *
 * The risk being closed is **not** shell injection: the value becomes a
 * single argv element and nothing here is ever shelled out to. It is that
 * an argv element can read as a *flag*. A request smuggling
 * `--dangerously-skip-permissions` (or any other CLI flag) through the
 * `model` field would have it parsed as an option rather than a value —
 * hence the leading-alphanumeric requirement, which is the part of this
 * rule that must never be relaxed to "any non-empty string".
 *
 * The charset accepts every real id shape: `sonnet`,
 * `claude-sonnet-4-5-20250929`, `kimi-k3`, and Bedrock-style
 * `us.anthropic.claude-sonnet-4-5-20250929-v1:0`.
 *
 * @param {unknown} value
 * @returns {boolean}
 */
export function isValidModelId(value) {
  return typeof value === "string" && /^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$/.test(value);
}
