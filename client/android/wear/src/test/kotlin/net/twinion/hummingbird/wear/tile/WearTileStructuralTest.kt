package net.twinion.hummingbird.wear.tile

import java.io.File
import org.junit.Assert.assertTrue
import org.junit.Test

// The tile's load-bearing shapes, pinned in source: the future the system
// waits on always resolves (a throw fails it, a cancel is honoured), the
// mirror read is guarded, and the background sync leg tells the tile the
// mirror moved (`WearApp` sets `SyncWorker.onRunFinished`). Source-text
// pins because none of this is reachable on the JVM without a `Context`.
class WearTileStructuralTest {

    private fun source(path: String): String {
        val root = System.getProperty("hummingbird.repoRoot")
            ?: error("hummingbird.repoRoot not set — run under Gradle (see client/android/build.gradle.kts)")
        val file = File(root, "client/android/wear/src/main/kotlin/net/twinion/hummingbird/wear/$path")
        check(file.isFile) { "$path not found under $root" }
        return file.readText()
            .replace(Regex("""/\*[\s\S]*?\*/"""), "")
            .replace(Regex("""(?m)^\s*//.*$"""), "")
    }

    @Test
    fun `the tile future always resolves`() {
        val src = source("tile/CaptureTileService.kt")
        assertTrue("a throw must fail the future", src.contains("completer.setException("))
        assertTrue("a cancelled future must stop the read", src.contains("completer.addCancellationListener("))
        assertTrue("a cancelled scope must not strand the future", src.contains("completer.setCancelled()"))
    }

    @Test
    fun `the mirror read is guarded`() {
        val src = source("tile/CaptureTileService.kt")
        val read = src.indexOf("fun readTileFacts(")
        assertTrue(read >= 0)
        assertTrue("readTileFacts must catch around the core", src.indexOf("catch (failed: Exception)", read) > read)
    }

    @Test
    fun `the background leg tells the tile the mirror moved`() {
        val src = source("WearApp.kt")
        assertTrue(src.contains("SyncWorker.onRunFinished = {"))
        assertTrue(src.contains("TileRefresh.request(context)"))
        assertTrue("the hook records only informative cycles", src.contains("isInformativeSyncOutcome(outcomeKind)"))
    }
}
