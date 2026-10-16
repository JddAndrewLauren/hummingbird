package net.twinion.hummingbird.wear

import java.io.File
import org.junit.Assert.assertTrue
import org.junit.Test

// The watch tells the shared seam it is the watch (#880), so the seam
// refuses a Google sign-in there (ADR-0039: the watch's token only ever
// comes from the phone). `CoreHolder` has no default form, so a missing
// line fails loudly at first `get` on the device — this pins it before then,
// and pins that it is the WATCH form, which nothing at build time checks.
class WearDeviceFormStructuralTest {

    @Test
    fun `WearApp tells the core holder it is the watch, first in onCreate`() {
        val src = File(repoRoot(), "client/android/wear/src/main/kotlin/net/twinion/hummingbird/wear/WearApp.kt")
            .readText()
        val onCreate = src.substringAfter("override fun onCreate()")
        val set = onCreate.indexOf("CoreHolder.form = MobileDeviceForm.WATCH")
        assertTrue("WearApp.onCreate sets CoreHolder.form to WATCH", set >= 0)
        assertTrue(
            "before the diagnostics recorder or any worker can run",
            set < onCreate.indexOf("DiagnosticsRecorder.get(this)"),
        )
    }

    private fun repoRoot(): File =
        File(
            System.getProperty("hummingbird.repoRoot")
                ?: error("hummingbird.repoRoot not set — run under Gradle (see client/android/build.gradle.kts)"),
        )
}
