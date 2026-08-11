/**
 * v1's one runner op (#256 decision, 2026-08-10): turn a raw capture into
 * the minimal `{title, notes}` schema (#42's own minimal shape). Writes to
 * nothing -- no authority call anywhere in this module or its caller; the
 * write-target question (Linear vs. the owned server) stays deliberately
 * deferred (#256).
 */
export const parseCapture = {
  name: "parse-capture",

  /** Versioned beside the SKILL.md this ships with (#41 decision 4). */
  resultSchemaPath: ".claude/skills/parse-capture/schema.json",

  /**
   * @param {Record<string, unknown>} args
   * @returns {{ok: true} | {ok: false, error: string}}
   */
  validateArgs(args) {
    const text = args?.text;
    if (typeof text !== "string" || text.trim().length === 0) {
      return { ok: false, error: "\"text\" must be a non-empty string" };
    }
    return { ok: true };
  },

  /**
   * @param {{text: string}} args
   * @returns {string}
   */
  buildPrompt(args) {
    return `/parse-capture\n\n${args.text}`;
  },
};
