package net.twinion.hummingbird

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

// The board drag's contract (#801, ADR-0021 decision 9), pinned where a JVM
// render cannot reach: which file may express the gesture, and — the load-
// bearing one — that no Kotlin anywhere decides what a drop writes.
//
// Source-structural for the same reason `WindowWidthStructuralTest` is: the
// gesture only exists on a wide window, `captureToImage` times out under
// this repo's Robolectric rig, and a drag delivered through
// `performTouchInput` is the one thing a unit suite can check about it
// (`BoardDragGestureTest`, beside this file). What is left over — that the
// phone arm and Triage's rows are untouched, and that the decision stayed
// in Rust — is text over source, deliberately.
class BoardDragStructuralTest {

    private fun repoFile(relative: String): String {
        val root = System.getProperty("hummingbird.repoRoot")
            ?: error("hummingbird.repoRoot not set — run under Gradle (see client/android/build.gradle.kts)")
        val file = File(root, relative)
        check(file.isFile) { "$relative not found under $root" }
        return file.readText()
    }

    private val kotlinRoot: File
        get() = File(
            System.getProperty("hummingbird.repoRoot")
                ?: error("hummingbird.repoRoot not set"),
            "client/android/app/src/main/kotlin",
        )

    private fun sourcesContaining(needle: String): List<String> =
        kotlinRoot.walkTopDown()
            .filter { it.isFile && it.extension == "kt" }
            .filter { it.readText().contains(needle) }
            .map { it.name }
            .sorted()
            .toList()

    @Test
    fun `only the lane board carries a drag gesture`() {
        // The phone stacks its columns down one list, where a drag between
        // them would be a scroll through the board rather than a movement
        // across it — so `NowScreen.kt`'s own loop and `TriageScreen.kt`'s
        // rows must stay gesture-free.
        assertEquals(listOf("NowLaneBoard.kt"), sourcesContaining("detectDragGesturesAfterLongPress"))
    }

    @Test
    fun `no Kotlin decides what a drop writes`() {
        // The mapping is `hummingbird_core::decisions::frontier::drop_edits`
        // (ADR-0025), reached through two seam doors. If any of these words
        // appears in a drag file, some rule about which field a column sets
        // has been restated on this side of the seam.
        val board = repoFile("client/android/app/src/main/kotlin/net/twinion/hummingbird/NowLaneBoard.kt")
        for (leak in listOf("\"overdue\"", "\"calm\"", "deadline =", "defaultContext")) {
            assertTrue(
                "NowLaneBoard.kt mentions $leak — the drop mapping belongs in the core",
                !board.contains(leak),
            )
        }
        // And it reaches the core rather than guessing: a column is lit only
        // because `droppableColumns` named it.
        assertTrue(board.contains("onDroppableColumns"))
        assertTrue(board.contains("onMoveItem"))
    }

    @Test
    fun `the ViewModel wires the real seam doors and adds no rule of its own`() {
        val vm = repoFile("client/android/app/src/main/kotlin/net/twinion/hummingbird/NowViewModel.kt")
        assertTrue("moveItem must reach MobileTaskHost.moveItemToColumn", vm.contains(".moveItemToColumn("))
        assertTrue("the lit set must reach MobileTaskHost.droppableColumns", vm.contains(".droppableColumns("))
        // `NowScreenStructuralTest` counts `.act(` calls to pin that the
        // board's only act door is complete; a drag is a triage write and
        // must not be spelled as an act.
        assertTrue(
            "moveItem must not be routed through the act door",
            !Regex("""moveItem[\s\S]{0,400}?\.act\(""").containsMatchIn(vm),
        )
    }

    @Test
    fun `the drag reads the shared motion tokens rather than its own numbers`() {
        val board = repoFile("client/android/app/src/main/kotlin/net/twinion/hummingbird/NowLaneBoard.kt")
        assertTrue(board.contains("carrySpring()"))
        assertTrue(board.contains("reducedMotion()"))
        assertTrue(
            "the tilt cap belongs to ui/theme/Motion.kt, under the drift gate",
            board.contains("CarryTiltMaxDegrees"),
        )
    }
}
