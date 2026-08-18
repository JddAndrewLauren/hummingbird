package net.twinion.hummingbird

import java.io.File
import org.junit.Assert.assertFalse
import org.junit.Test

// The M4 (#535) counterpart of `RulesScreenStructuralTest`, and the gate
// ADR-0025 most needs on this slice: Settings must re-derive no sync-status
// or binding decision of its own.
//
// `syncSummary`/`deadLetterHeadingText` (`SettingsViewModel.kt`) arrive
// applied from `hummingbird_core::decisions::settings`, and every binding's
// `known`/`pending`/`value` state arrives applied from `Core::bindings`. A
// Kotlin copy of either would compile, run, and look right on every
// fixture anyone would think to write — only a source gate catches it.
class SettingsScreenStructuralTest {

    private fun repoFile(relative: String): String {
        val root = System.getProperty("hummingbird.repoRoot")
            ?: error("hummingbird.repoRoot not set — run under Gradle (see app/build.gradle.kts)")
        val file = File(root, relative)
        check(file.isFile) { "$relative not found under $root" }
        return file.readText()
    }

    /** The file's *code*, with comments removed — `RulesScreenStructuralTest`'s
     * own reasoning: a doc comment must be free to name the thing it
     * forbids. */
    private fun source(name: String) =
        repoFile("client/android/app/src/main/kotlin/net/twinion/hummingbird/$name")
            .replace(Regex("""/\*[\s\S]*?\*/"""), "")
            .replace(Regex("""(?m)^\s*//.*$"""), "")

    private val screenSrc by lazy { source("SettingsScreen.kt") }
    private val viewModelSrc by lazy { source("SettingsViewModel.kt") }

    private val both by lazy {
        listOf(
            "SettingsScreen.kt" to screenSrc,
            "SettingsViewModel.kt" to viewModelSrc,
        )
    }

    @Test
    fun `no settings surface names a sync-status word as a literal`() {
        // "Stale"/"Held"/"Synced"/"Offline" are `hummingbird_core::
        // decisions::settings::sync_status_label`'s own words — a Kotlin
        // literal copy of any of them would be the drift ADR-0025 exists
        // to prevent, however byte-identical it started out.
        for ((name, src) in both) {
            for (word in listOf("\"Stale", "\"Held", "\"Synced", "\"Offline")) {
                assertFalse("$name must not name $word literally", src.contains(word))
            }
        }
    }

    @Test
    fun `no settings surface re-derives which sync outcomes are informative`() {
        // `is_informative_sync_outcome`'s own vocabulary — a Kotlin
        // `kind != "skipped"` copy would be the third copy of this rule.
        for ((name, src) in both) {
            assertFalse("$name must not name skipped as a wire string", src.contains("\"skipped\""))
            assertFalse("$name must not name busy as a wire string", src.contains("\"busy\""))
        }
    }

    @Test
    fun `the screen renders every dead-letter reason through an exhaustive when`() {
        // The `when` over `MobileDeadLetterReason` in `SettingsScreen.kt`
        // must carry no `else ->` — a fourth reason added to the core's
        // enum is then a Kotlin compile error here, not a row that
        // silently renders as nothing.
        assertFalse(
            "SettingsScreen.kt's dead-letter reason when must be exhaustive, not else-gated",
            screenSrc.contains("is MobileDeadLetterReason") && screenSrc.contains("else ->") &&
                Regex("""when\s*\(val reason = entry\.reason\)[\s\S]*?else ->""").containsMatchIn(screenSrc),
        )
    }
}
