package net.twinion.hummingbird

import java.io.File
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
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

    /** `BindingRow`'s draft is `remember`ed by binding key, so it survives
     * every re-read — a seed alone leaves a stale draft sitting over a
     * value it never showed, with Save enabled to push it back over
     * another device's edit. `SettingsScreen.tsx`'s `BindingRow` grew this
     * reseed at #118's review; the phone inherited the seed without it, and
     * #565's review caught the omission.
     *
     * A source gate rather than a behavioural one for the reason this whole
     * class exists: `client/android` runs no Robolectric, so a `@Composable`
     * private to `SettingsScreen.kt` cannot be composed in a plain JVM test
     * — and a dropped reseed compiles, runs, and looks right on every
     * fixture anyone would think to write. Comments are stripped by
     * [source], so this matches the code, not the note above it. */
    @Test
    fun `the binding row reseeds its draft when the value underneath moves`() {
        assertTrue(
            "SettingsScreen.kt's BindingRow must track the value it last seeded from",
            screenSrc.contains("var seenValue by remember(binding.key)"),
        )
        assertTrue(
            "SettingsScreen.kt's BindingRow must reseed its draft when that value moves",
            Regex("""if\s*\(seenValue\s*!=\s*binding\.value\)\s*\{[\s\S]{0,200}?draft\s*=\s*bindingDraftSeed\(binding\.value\)""")
                .containsMatchIn(screenSrc),
        )
    }

    /** #564's calendar lane, held to the same rule. The seven mint error
     * codes are `ffi-mobile`'s `calendar_token::code` vocabulary, and which
     * of the four Source-connection states each one puts the device in is
     * decided there, once. A Kotlin `when` over a code would be a second
     * copy of that table — and it would agree with the first one for
     * exactly as long as nobody touched either. */
    @Test
    fun `no settings surface matches on a calendar mint error code`() {
        for ((name, src) in both) {
            for (code in listOf(
                "no_device_token",
                "authority_rejected_device_token",
                "authority_unconfigured",
                "authority_upstream",
                "authority_unreachable",
                "bad_token_response",
                "no_access_token",
            )) {
                assertFalse("$name must not name the mint code $code", src.contains(code))
            }
        }
    }

    /** The four states must each get their own sentence, and the `when`
     * that picks them must carry no `else ->` — a fifth state added to the
     * core's enum is then a Kotlin compile error, not a state that silently
     * renders as another one's words. */
    @Test
    fun `the calendar state sentence is exhaustive over the four states`() {
        val sentences = Regex(
            """fun calendarStateSentence\(state: MobileCalendarState\): String = when \(state\)\s*\{([\s\S]*?)
\}""",
        ).find(screenSrc)?.groupValues?.get(1)
            ?: error("calendarStateSentence not found — has it been renamed?")
        assertFalse("the calendar state when must be exhaustive", sentences.contains("else ->"))
        for (state in listOf(
            "NEVER_CONNECTED",
            "CONNECTED",
            "CANNOT_CONFIRM",
            "REFUSED_DEVICE_TOKEN",
            "REFUSED_SERVER_LANE",
        )) {
            assertTrue("$state needs its own sentence", sentences.contains(state))
        }
    }

    /** *Cannot confirm* must never offer Connect — an offline or
     * authority-down device is still connected, and a Connect button there
     * invites the operator to "fix" something no tap can fix. Only
     * `NEVER_CONNECTED` offers it. */
    @Test
    fun `connect is offered in exactly one state`() {
        assertTrue(
            "offersConnect must be NEVER_CONNECTED alone",
            Regex(
                """fun offersConnect\(state: MobileCalendarState\): Boolean =\s*state == MobileCalendarState\.NEVER_CONNECTED""",
            ).containsMatchIn(screenSrc),
        )
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
