package net.twinion.hummingbird

import java.io.File
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

// The Capture FAB and its sheet — the app's first in-app capture
// affordance. Before it, capture was reachable only from the second
// launcher icon and the long-press shortcut, so "the FAB exists and opens
// the sheet" is exactly the kind of reachability claim `NowItemDoorTest`
// pins for the item door, and for the same no-emulator reason.
class CaptureSheetStructuralTest {

    private fun source(name: String): String {
        val root = System.getProperty("hummingbird.repoRoot")
            ?: error("hummingbird.repoRoot not set — run under Gradle (see app/build.gradle.kts)")
        val file = File(root, "client/android/app/src/main/kotlin/net/twinion/hummingbird/$name")
        check(file.isFile) { "$name not found under $root" }
        return file.readText()
            .replace(Regex("""/\*[\s\S]*?\*/"""), "")
            .replace(Regex("""(?m)^\s*//.*$"""), "")
    }

    @Test
    fun `the Scaffold carries the extended Capture FAB, and it opens the sheet`() {
        val src = source("MainActivity.kt")
        assertTrue(
            "MainActivity's Scaffold must fill its floatingActionButton slot",
            src.contains("floatingActionButton = {"),
        )
        assertTrue(
            "the FAB is the design kit's extended form — icon and the word Capture",
            src.contains("ExtendedFloatingActionButton("),
        )
        assertTrue(
            "the FAB must carry the brand's capture glyph (feather)",
            src.contains("R.drawable.ic_feather"),
        )
        assertTrue(
            "the FAB's whole job is opening the sheet",
            src.contains("onClick = { captureSheetOpen = true }"),
        )
        assertTrue(
            "MainActivity must render CaptureSheet when the flag is up",
            src.contains("CaptureSheet("),
        )
    }

    @Test
    fun `the in-app door is the sheet, never an Intent to CaptureActivity`() {
        // CaptureActivity stays — the second launcher icon and the
        // long-press shortcut are its doors (`ManifestAliasTest`) — but an
        // in-app startActivity to it would be a second, full-screen door
        // from a surface that already has the sheet: two capture forms one
        // tap apart, each holding its own draft.
        val src = source("MainActivity.kt")
        assertFalse(
            "MainActivity must not startActivity CaptureActivity — the sheet is the in-app door",
            src.contains("CaptureActivity::class"),
        )
    }

    @Test
    fun `the sheet carries the full form and the shared-helper mic`() {
        // The mic arrived with #611, wired through the extracted
        // `speech/Dictation.kt` — never a second copy of the recognizer
        // plumbing (DictationLocalityTest bans the default-service pair in
        // this file for that reason), and never a mic without plumbing, the
        // dead control ADR-0022 calls a defect.
        //
        // The details disclosure was banned here until 2026-08-20, when the
        // operator ruled the two capture surfaces differ only in which door
        // they are, never in what a person can record through them. This
        // assertion is that decision, inverted from the one it replaces:
        // the sheet must carry the disclosure, or a reader who reaches for
        // a deadline here has to leave for the other surface to record it.
        val src = source("CaptureSheet.kt")
        assertTrue(
            "CaptureSheet must render the dictation mic (#611)",
            src.contains("ic_mic"),
        )
        assertTrue(
            "the mic must wire through the shared helper's failure type",
            src.contains("net.twinion.hummingbird.speech.DictationFailure"),
        )
        assertTrue(
            "a denied permission reports through the failure lane, never silently",
            src.contains("viewModel.onDictationFailed(DictationFailure.NO_PERMISSION)"),
        )
        assertTrue(
            "CaptureSheet must carry the details disclosure — field parity with CaptureActivity",
            src.contains("detailsOpen"),
        )
        // Field parity is the claim, so it is checked field by field rather
        // than by the disclosure's flag alone: a `detailsOpen` that reveals
        // three of the five would satisfy the line above and still send the
        // reader to the other surface.
        for (field in listOf("ProjectField(", "PriorityRow(", "CaptureDateField(")) {
            assertTrue(
                "the disclosure must render the shared $field",
                src.contains(field),
            )
        }
        assertTrue(
            "submission must clear the draft — the host store outlives the sheet",
            src.contains("viewModel.clearDraft()"),
        )
    }
}
