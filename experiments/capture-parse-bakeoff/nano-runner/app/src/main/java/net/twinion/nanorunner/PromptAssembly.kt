package net.twinion.nanorunner

/**
 * The one invariant this whole app exists to preserve: the prompt Nano sees must be
 * byte-identical to the prompt the hosted baseline saw.
 *
 * `run_hosted.py:emit_prompts` does exactly:
 *
 *     schema_text = open(SCHEMA).read().rstrip()
 *     prompt = template.replace("{{SCHEMA}}", schema_text).replace("{{RAW}}", raw)
 *
 * where `template` is the ENTIRE prompt.md (header and meta-paragraph included — the
 * hosted side sent those too, so we do as well). Python's `str.replace` is a literal
 * global replace and so is Kotlin's `String.replace(String, String)`; the order matters
 * because a schema containing `{{RAW}}` would otherwise be substituted into. Nothing
 * here trims, normalises line endings, or re-encodes.
 *
 * PromptAssemblyTest re-derives all 42 prompts against a committed golden file emitted
 * by the Python side. This code is never trusted on inspection.
 */
object PromptAssembly {

    const val SCHEMA_PLACEHOLDER = "{{SCHEMA}}"
    const val RAW_PLACEHOLDER = "{{RAW}}"

    /** [schemaText] must already be trailing-trimmed — see [assembleFromFiles]. */
    fun assemble(template: String, schemaText: String, raw: String): String =
        template.replace(SCHEMA_PLACEHOLDER, schemaText).replace(RAW_PLACEHOLDER, raw)

    /** The form callers should use: takes schema.json verbatim and applies the rstrip. */
    fun assembleFromFiles(templateFileText: String, schemaFileText: String, raw: String): String =
        assemble(templateFileText, schemaFileText.trimEnd(), raw)
}
