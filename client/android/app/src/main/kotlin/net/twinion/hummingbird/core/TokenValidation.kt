package net.twinion.hummingbird.core

// Normalisation for a pasted device token — the one piece of capture-form
// logic this skeleton has, split out so it is unit-testable without a
// device (the repo's "component tests are the gate" rule for UI-adjacent
// code).
object TokenValidation {
    /**
     * A pasted token, trimmed of the whitespace a copy from a terminal or
     * a password manager drags along (including interior newlines from
     * soft-wrapped copies). Returns null when nothing usable remains —
     * the caller treats that as "no token entered", never as an empty
     * credential to push.
     */
    fun normalize(raw: String): String? {
        val cleaned = raw.filterNot { it == '\n' || it == '\r' }.trim()
        return cleaned.ifEmpty { null }
    }
}
