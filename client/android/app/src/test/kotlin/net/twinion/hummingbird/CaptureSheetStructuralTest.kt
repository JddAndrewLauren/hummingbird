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
    fun `the sheet is the light form — no details disclosure, no mic`() {
        // The full form lives in CaptureActivity. The sheet deliberately
        // renders no dictation mic: a mic without its recognizer plumbing
        // is the dead control ADR-0022 calls a defect, so until the
        // dictation helper is extracted (its own slice), the honest sheet
        // has no mic at all.
        val src = source("CaptureSheet.kt")
        assertFalse(
            "CaptureSheet must not render a mic it cannot wire (ADR-0022)",
            src.contains("ic_mic"),
        )
        assertFalse(
            "CaptureSheet must not grow the details disclosure — CaptureActivity is the full form",
            src.contains("detailsOpen"),
        )
        assertTrue(
            "submission must clear the draft — the host store outlives the sheet",
            src.contains("viewModel.clearDraft()"),
        )
    }
}
