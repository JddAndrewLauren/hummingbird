package net.twinion.hummingbird

import java.io.File
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

// The Grill takeover's review card, structurally (#595) — the first
// structural test this screen has had, in the house pattern
// (`RecallScreenStructuralTest`'s comment-stripping variant, so a doc
// comment is free to name the thing a rule forbids).
class GrillTakeoverStructuralTest {

    private val src: String by lazy {
        val root = System.getProperty("hummingbird.repoRoot")
            ?: error("hummingbird.repoRoot not set — run under Gradle (see app/build.gradle.kts)")
        val file = File(
            root,
            "client/android/app/src/main/kotlin/net/twinion/hummingbird/GrillTakeoverScreen.kt",
        )
        check(file.isFile) { "GrillTakeoverScreen.kt not found under $root" }
        file.readText()
            .replace(Regex("""/\*[\s\S]*?\*/"""), "")
            .replace(Regex("""(?m)^\s*//.*$"""), "")
    }

    @Test
    fun `the proposed edit renders as rows from the core seam, never as an editable JSON field`() {
        // #595: the card was rendering `patchJson` in an OutlinedTextField —
        // braces, escapes and all — on the screen where the user decides
        // whether to Confirm. The rows come decided from
        // `grillProposalRows` (ADR-0025: applied results across the seam),
        // and no patch text field remains for a revert to hide in.
        assertTrue(
            "the review card must take its rows from grillProposalRows (#595)",
            src.contains("grillProposalRows("),
        )
        assertFalse(
            "no composable may hold patch JSON in an editable field (#595)",
            src.contains("patchDraft"),
        )
    }

    @Test
    fun `kotlin never parses patch_json — the parse lives across the seam`() {
        // ADR-0025 plus the seam's own contract (`lib.rs`,
        // `MobileGrillProposal.patch_json`): the one reader is Rust.
        for (banned in listOf(
            "JSONObject", "JSONArray", "JSONTokener",
            "Json.decodeFromString", "JsonElement", "JsonParser",
            "Gson", "fromJson",
        )) {
            assertFalse(
                "GrillTakeoverScreen.kt must not parse the patch itself ($banned)",
                src.contains(banned),
            )
        }
    }

    @Test
    fun `confirm records the proposal unchanged — appliedPatch is the model's own patchJson`() {
        // Android ships no inline edit, so what was proposed is what is
        // recorded; a divergence here would be an edit affordance that
        // exists nowhere on this client.
        assertTrue(
            "appliedPatch must be the untouched proposal patch",
            Regex("""appliedPatch\s*=\s*turn\.proposal\.patchJson""").containsMatchIn(src),
        )
    }

    @Test
    fun `the card says what Confirm does — recorded on the Grill, never applied`() {
        // The web hint, verbatim (`GrillTakeover.tsx`): without it the
        // labelled diff reads as "Confirm applies this", which is false.
        assertTrue(
            "the recorded-on-the-Grill hint is missing",
            src.contains("Recorded on the Grill — never applied to the item automatically."),
        )
    }
}
