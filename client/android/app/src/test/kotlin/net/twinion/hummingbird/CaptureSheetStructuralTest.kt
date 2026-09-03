package net.twinion.hummingbird

import java.io.File
import org.junit.Assert.assertEquals
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
        for (field in listOf(
            "ProjectField(",
            "PriorityRow(",
            "DeadlineField(",
            "CaptureDateField(",
            // #782: the Link sits below the disclosure, on both surfaces.
            "LinkField(",
        )) {
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

    /** The sheet's own shape, as the operator set it on 2026-08-20: it
     * opens cold at the top of the window, with no heading, and the details
     * disclosure is a chevron rather than words.
     *
     * The full-height pin needs both halves named, because either alone is
     * a sheet that still rests half-way: `skipPartiallyExpanded` removes the
     * half-height stop, and `fillMaxHeight()` is what makes the sheet reach
     * the top at all — an expanded `ModalBottomSheet` is otherwise only as
     * tall as its content, and this form's content is shorter than the
     * window with the disclosure shut. Nothing else in the repo can catch a
     * half-open sheet: it composes, renders and passes every field pin.
     */
    @Test
    fun `the sheet opens full height, titleless, with a chevron disclosure`() {
        val src = source("CaptureSheet.kt")
        assertTrue(
            "the sheet must skip the half-height resting state",
            src.contains("rememberModalBottomSheetState(skipPartiallyExpanded = true)"),
        )
        assertTrue(
            "the sheet must fill the window's height — skipping the stop is not enough",
            src.contains("modifier = Modifier.fillMaxHeight()"),
        )
        assertFalse(
            "the sheet renders no heading — the FAB that opened it said Capture",
            src.contains("Text(\"Capture\""),
        )
        assertTrue(
            "the disclosure is the chevron, rotated when the fields are out",
            src.contains("R.drawable.ic_chevron_down") &&
                src.contains("Modifier.rotate(if (detailsOpen) 180f else 0f)"),
        )
        // The status-bar inset, and it is here because the device found it
        // rather than a test: a half-height sheet never met the status bar,
        // so this one had never paid the inset and never needed to. Full
        // height, the first build put the title field's outline against the
        // clock — #614's dead band in the other direction. Nothing else in
        // the module can see that; the sheet composes and every field pin
        // passes with the line deleted.
        assertTrue(
            "a full-height sheet must pay the status-bar inset",
            src.contains("contentWindowInsets = { WindowInsets.statusBars }"),
        )
        assertFalse(
            "the words it replaced must not also render as a label",
            src.contains("Text(if (detailsOpen)"),
        )
        // The chevron loses no name: the words survive where a screen
        // reader can still reach them.
        assertTrue(
            "the chevron must name itself",
            src.contains("contentDescription = if (detailsOpen) \"Fewer details\" else \"More details\""),
        )
    }

    /** The chevron disclosure is the same control on both capture surfaces
     * (operator decision 2026-08-20, taken on the sheet and carried to the
     * Activity): one field set behind two doors must not be disclosed by two
     * different controls. The sheet's half is pinned above; this is the
     * Activity's, which took the change with no assertion of its own and
     * could otherwise drift back to a `TextButton` silently. */
    @Test
    fun `the Activity discloses with the same chevron as the sheet`() {
        val src = source("CaptureActivity.kt")
        assertTrue(
            "CaptureActivity's disclosure is the chevron",
            src.contains("R.drawable.ic_chevron_down") &&
                src.contains("Modifier.rotate(if (detailsOpen) 180f else 0f)"),
        )
        assertFalse(
            "the words it replaced must not also render as a label",
            src.contains("Text(if (detailsOpen)"),
        )
    }

    /** Deadline and scheduled date share a line on both capture surfaces
     * (operator decision 2026-08-20) — a `weight(1f)` each inside a `Row`,
     * which is also why [net.twinion.hummingbird.ui.forms.CaptureDateField]
     * grew a `modifier` parameter. Checked on both files because the pair
     * is a parity claim, and the Triage editor is deliberately excluded: it
     * stacks them still, inside a narrower card.
     *
     * The two are no longer the *same* component: since the pickers landed
     * (2026-08-24) the deadline is
     * [net.twinion.hummingbird.ui.forms.DeadlineField], which wraps the
     * scheduled date's own control and adds the optional minute the
     * deadline grammar accepts. So the pair is anchored on `DeadlineField(`
     * and their order is asserted too — due before planned, which is the
     * reading the shared line exists to buy. */
    @Test
    fun `both capture surfaces pair the two dates on one line`() {
        for (name in listOf("CaptureSheet.kt", "CaptureActivity.kt")) {
            val src = source(name)
            val firstDate = src.indexOf("DeadlineField(")
            // The nearest `Row(` above the first date field must be the
            // pair's own container, so nothing else may sit between them.
            // Stacked again, that search would land on the title/mic row
            // instead and find the sliders and pickers in the way.
            val rowStart = src.lastIndexOf("Row(", firstDate)
            assertFalse(
                "$name must seat the two dates in a Row of their own",
                src.substring(rowStart, firstDate).contains("Field("),
            )
            // Bounded to that Row's own block, and the bound is the whole
            // point: measured unbounded, this assertion counted the submit
            // buttons' weights instead and stayed green with a date field's
            // weight deleted.
            val scheduled = src.indexOf("CaptureDateField(", firstDate)
            val indent = " ".repeat(rowStart - src.lastIndexOf('\n', rowStart) - 1)
            val block = src.substring(rowStart, src.indexOf("\n$indent}", scheduled))
            assertEquals(
                "$name's date Row must hold the deadline and nothing else of the kind",
                1,
                Regex("""DeadlineField\(""").findAll(block).count(),
            )
            assertEquals(
                "$name's date Row must hold the scheduled date and nothing else of the kind",
                1,
                Regex("""CaptureDateField\(""").findAll(block).count(),
            )
            assertTrue(
                "$name must read due-then-planned, deadline first",
                block.indexOf("DeadlineField(") < block.indexOf("CaptureDateField("),
            )
            assertEquals(
                "$name must weight both date fields evenly",
                2,
                Regex("""modifier = Modifier\.weight\(1f\),""").findAll(block).count(),
            )
            // The pair's geometry, not just its membership.
            // `DeadlineFieldWrappingTest` builds its own Row with these two
            // and calls it "as the two capture surfaces render it" — nothing
            // made that true until here. `Alignment.Top` is the load-bearing
            // one: the deadline grows a second control when it names a
            // minute, and `CenterVertically` would slide the scheduled-date
            // field down as it did, which is the exact defect that test's
            // `naming a minute grows the deadline column without shifting its
            // neighbour` exists to catch.
            assertTrue(
                "$name's date Row must top-align, so the deadline's time row " +
                    "cannot shift the scheduled-date field",
                block.contains("verticalAlignment = Alignment.Top"),
            )
            assertTrue(
                "$name's date Row must keep the 8dp gap the half-slot budget assumes",
                block.contains("Arrangement.spacedBy(8.dp)"),
            )
        }
    }
}
