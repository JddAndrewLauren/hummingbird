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
