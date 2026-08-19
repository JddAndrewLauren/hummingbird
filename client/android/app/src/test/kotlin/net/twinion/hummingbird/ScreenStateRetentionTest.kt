package net.twinion.hummingbird

import java.io.File
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

// `remember { SomeViewModel.create(context) }` reads as ViewModel usage and
// is not: `remember` survives recomposition, never Activity recreation, so
// a rotation or a fold/unfold built a fresh ViewModel and threw the old
// one's state away — an unsubmitted capture draft, in the case that
// motivated this gate. Being a `ViewModel` buys retention only once
// something puts it in a `ViewModelStore`, which is what `viewModel()` with
// the screen's factory does.
//
// The Pixel Fold is the only install target (#141's grilling), so the fold
// transition is the ordinary case here rather than an edge one. No emulator
// in this source set, so the gate is on the construction shape itself —
// same discipline as `CaptureSubmitRefusalTest`.
class ScreenStateRetentionTest {

    private fun repoFile(relative: String): String {
        val root = System.getProperty("hummingbird.repoRoot")
            ?: error("hummingbird.repoRoot not set — run under Gradle (see app/build.gradle.kts)")
        val file = File(root, relative)
        check(file.isFile) { "$relative not found under $root" }
        return file.readText()
    }

    private val screens = listOf(
        "CaptureActivity.kt" to "CaptureViewModel",
        "NowScreen.kt" to "NowViewModel",
        "AlertsScreen.kt" to "AlertsViewModel",
        "AlertDetailScreen.kt" to "AlertDetailViewModel",
        // The item screen has the most to lose of any of them: its edit
        // draft is content a person typed, and a fold mid-edit is exactly
        // the transition this gate exists for.
        "ItemDetailPanel.kt" to "ItemDetailViewModel",
        // The item screen's own microtask affordance (#539) shares the
        // file but not the ViewModel — a second entry, not a substitute.
        "ItemDetailPanel.kt" to "MicrotaskViewModel",
        // The rules screen has a draft too (#540/M4) — a rule someone is
        // writing, conditions and all.
        "RulesScreen.kt" to "RulesViewModel",
        // The Grill takeover (#539) has the most to lose of any of them: a
        // resumed or in-progress interview, streamed against a billed
        // runner call — `remember` would both drop the state AND re-issue
        // the request on every fold/unfold.
        "GrillTakeoverScreen.kt" to "GrillTakeoverViewModel",
    )

    @Test
    fun `every screen obtains its ViewModel from the store, never from remember`() {
        for ((file, viewModelName) in screens) {
            val src = repoFile("client/android/app/src/main/kotlin/net/twinion/hummingbird/$file")
            assertTrue(
                "$file must obtain $viewModelName via viewModel(factory = $viewModelName.factory(...))",
                Regex("""viewModel\(factory = $viewModelName\.factory\(""").containsMatchIn(src),
            )
            assertFalse(
                "$file must not hold $viewModelName in a remember — it does not survive recreation",
                Regex("""remember\s*\{\s*$viewModelName\.create\(""").containsMatchIn(src),
            )
        }
    }
}
