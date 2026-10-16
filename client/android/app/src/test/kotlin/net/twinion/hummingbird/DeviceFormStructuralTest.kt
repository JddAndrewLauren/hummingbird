package net.twinion.hummingbird

import java.io.File
import org.junit.Assert.assertTrue
import org.junit.Test

// The phone tells the shared seam it is the phone (#880) — the form under
// which a Google sign-in mints an `android` token (#839). `CoreHolder` has
// no default form, so a missing line fails loudly at first `get`; this pins
// it, and that it is PHONE, before then. The watch's twin is `:wear`'s
// `WearDeviceFormStructuralTest`.
class DeviceFormStructuralTest {

    @Test
    fun `HummingbirdApp tells the core holder it is the phone, first in onCreate`() {
        val src = File(repoRoot(), "client/android/app/src/main/kotlin/net/twinion/hummingbird/HummingbirdApp.kt")
            .readText()
        val onCreate = src.substringAfter("override fun onCreate()")
        val set = onCreate.indexOf("CoreHolder.form = MobileDeviceForm.PHONE")
        assertTrue("HummingbirdApp.onCreate sets CoreHolder.form to PHONE", set >= 0)
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
