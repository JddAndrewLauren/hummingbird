package net.twinion.hummingbird

import java.io.File
import org.junit.Assert.assertTrue
import org.junit.Test

// #522: the live-alert card restated the item's own heading verbatim.
// Always, for the one source this screen exists to land: `sweep.rs` builds
// an `item-threshold/v1` ingest with `title: item.title`, so the card's
// title and the heading directly above it are the same string by
// construction, not by coincidence.
//
// Structural for the usual reason (`NavigationStructuralTest`): no emulator
// in CI, and `ItemDetailViewModelTest` cannot catch this -- the record's
// fields are each individually correct, and the defect is only in what the
// screen chooses to draw from them.
class AlertCardTitleTest {

    private val src: String by lazy {
        val root = System.getProperty("hummingbird.repoRoot")
            ?: error("hummingbird.repoRoot not set -- run under Gradle (see app/build.gradle.kts)")
        val file = File(
            root,
            "client/android/app/src/main/kotlin/net/twinion/hummingbird/ItemDetailPanel.kt",
        )
        check(file.isFile) { "ItemDetailPanel.kt not found under $root" }
        file.readText()
            .replace(Regex("""/\*[\s\S]*?\*/"""), "")
            .replace(Regex("""(?m)^\s*//.*$"""), "")
    }

    @Test
    fun `the alert card does not restate the item heading`() {
        assertTrue(
            "the alert title must be drawn only when it differs from the item's",
            Regex("""if \(alert\.title != record\.title\) \{""").containsMatchIn(src),
        )
    }

    @Test
    fun `the title is suppressed conditionally, not deleted`() {
        // The same card renders for any source carrying a live alert. One
        // whose title says something the heading does not must still show
        // it, so the fix is a condition rather than a removal.
        assertTrue(
            "ItemDetailPanel must still be able to draw alert.title",
            src.contains("Text(alert.title"),
        )
    }
}
