package net.twinion.hummingbird.ui.panes

import uniffi.hummingbird_ffi_mobile.MobilePaneAnswer
import uniffi.hummingbird_ffi_mobile.MobilePaneAnswerState
import uniffi.hummingbird_ffi_mobile.MobilePaneBand

// ADR-0015's **band-scoped, device-local collapse** (#245), ported from the
// web's `questions/collapse.ts` — read that file's header for the two
// deliberate decisions this module inherits verbatim:
//
// **This is not a binding.** A collapse is a view preference on one device,
// never a cross-device fact, so it lives in `PanePrefs`' DataStore — never
// the `settings` table, which has no DELETE and syncs everywhere.
//
// **The override is scoped to the band it was made in.** "I collapsed the
// waste pane" means "…while it was dormant", not "…forever": a stored
// override applies only while the pane's computed band still matches the
// band it was stored against — and a mismatch is a *read-time non-match*,
// never a deletion, which is what makes the round trip dormant → imminent
// → dormant RESURRECT the original override instead of quietly losing it.
//
// Pure logic, no storage: `PanePrefs` owns the DataStore; this object is
// what `PaneCollapseTest` (the port of the web's `collapse.test.ts`)
// exercises.

/** One stored override: which way, and the band it was made in. Both
 * halves are load-bearing — the direction, because a reader may equally
 * want a dormant pane *open* (the collapse default is a default, not a
 * rule), and the band, because that is the scope. */
data class CollapseOverride(val band: MobilePaneBand, val collapsed: Boolean)

internal object PaneCollapse {

    /** The shell's default when nobody has said otherwise: collapsed when
     * the pane is dormant, or when it has no answer to show — collapsed is
     * not *hidden*: the collapsed row still says which question it is and
     * what state it is in ("visibly broken, never quietly empty"). */
    fun defaultCollapsed(answer: MobilePaneAnswer): Boolean =
        answer.band == MobilePaneBand.DORMANT ||
            answer.answerState != MobilePaneAnswerState.ANSWERED

    /** Whether this pane renders collapsed right now: the stored override
     * when it was made in the band the pane is in *now*, and the default
     * rule otherwise. */
    fun resolve(
        stored: Map<String, CollapseOverride>,
        key: String,
        answer: MobilePaneAnswer,
    ): Boolean {
        val override = stored[key]
        if (override != null && override.band == answer.band) {
            return override.collapsed
        }
        return defaultCollapsed(answer)
    }

    /** Records one override into a new map: stamped with the band it was
     * made in, and pruned of panes that are no longer ranked at all —
     * band-MISMATCHED entries are deliberately kept (the resurrection
     * case), only unranked keys go. */
    fun write(
        current: Map<String, CollapseOverride>,
        key: String,
        override: CollapseOverride,
        livePaneKeys: Collection<String>,
    ): Map<String, CollapseOverride> {
        val live = livePaneKeys.toSet()
        val next = current.filterKeys { live.contains(it) }.toMutableMap()
        next[key] = override
        return next
    }

    // ---- the stored string form (PanePrefs' round-trip) ----------------
    //
    // One line per override, three fields joined by the ASCII unit
    // separator (U+001F): paneKey, band name, collapsed as 0/1 — never
    // JSON, which a plain JVM unit test could only parse by growing an
    // android.jar dependency. A pane key is `"{question}:{subjectKey}"`
    // and a subject key is a series/service/file name; one containing the
    // separator or a newline is not a key this app can mint, and a line
    // that fails to read is SKIPPED rather than an error — the web's own
    // "a view preference is never worth an error" rule.

    private const val SEP = '\u001F'

    fun encode(map: Map<String, CollapseOverride>): String =
        map.entries.joinToString("\n") { (key, override) ->
            "$key$SEP${override.band.name}$SEP${if (override.collapsed) "1" else "0"}"
        }

    fun decode(raw: String?): Map<String, CollapseOverride> {
        if (raw.isNullOrEmpty()) return emptyMap()
        val map = mutableMapOf<String, CollapseOverride>()
        for (line in raw.split("\n")) {
            val parts = line.split(SEP)
            if (parts.size != 3) continue
            val band = runCatching { MobilePaneBand.valueOf(parts[1]) }.getOrNull() ?: continue
            val collapsed = if (parts[2] == "1") {
                true
            } else if (parts[2] == "0") {
                false
            } else {
                // Anything a newer build wrote in a shape this one cannot
                // use reads as no entry, never an error.
                continue
            }
            map[parts[0]] = CollapseOverride(band, collapsed)
        }
        return map
    }
}
