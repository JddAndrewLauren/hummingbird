package net.twinion.hummingbird.wear

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

// The watch's ABIs are spelled in three places that nothing links at build
// time: the cargo-ndk targets `:core-binding` cross-compiles, the rustup
// targets `android.yml` installs before it, and the `abiFilters` `:wear`
// packages. A `.so` missing from any one of them fails only on the device —
// which is how the first hardware install went (2026-09-10): the Pixel Watch
// 4 is 32-bit (`armeabi-v7a`) and the APK shipped arm64-only, so it answered
// INSTALL_FAILED_NO_MATCHING_ABIS (ADR-0039 decision 8, as amended). This
// pins the three lists to each other and the watch's ABI into all of them.
class WearAbiStructuralTest {

    private val rustTargetFor = mapOf(
        "arm64-v8a" to "aarch64-linux-android",
        "armeabi-v7a" to "armv7-linux-androideabi",
        "x86_64" to "x86_64-linux-android",
    )

    @Test
    fun `the watch packages the Pixel Watch 4's 32-bit ABI`() {
        assertTrue("armeabi-v7a in :wear abiFilters", "armeabi-v7a" in wearAbiFilters())
    }

    @Test
    fun `every ABI the watch packages is one cargo-ndk builds`() {
        val built = cargoNdkTargets()
        for (abi in wearAbiFilters()) assertTrue("cargo ndk -t $abi", abi in built)
    }

    @Test
    fun `CI installs a rustup target for every ABI cargo-ndk builds`() {
        val expected = cargoNdkTargets().map { rustTargetFor.getValue(it) }.toSet()
        assertEquals(expected, ciRustTargets())
    }

    // -- Reading the three spellings --------------------------------------

    private fun wearAbiFilters(): Set<String> {
        val text = File(repoRoot(), "client/android/wear/build.gradle.kts").readText()
        val line = Regex("""abiFilters\s*\+=\s*listOf\(([^)]*)\)""").find(text)
            ?: error("no `abiFilters += listOf(...)` in wear/build.gradle.kts")
        return Regex("\"([^\"]+)\"").findAll(line.groupValues[1]).map { it.groupValues[1] }.toSet()
    }

    private fun cargoNdkTargets(): Set<String> {
        val text = File(repoRoot(), "client/android/core-binding/build.gradle.kts").readText()
        val block = text.substringAfter("\"cargo\", \"ndk\",").substringBefore("\"build\"")
        return Regex("\"-t\",\\s*\"([^\"]+)\"").findAll(block).map { it.groupValues[1] }.toSet()
            .also { check(it.isNotEmpty()) { "no `-t` targets found in cargoNdkBuild" } }
    }

    private fun ciRustTargets(): Set<String> {
        val text = File(repoRoot(), ".github/workflows/android.yml").readText()
        val line = Regex("""targets:\s*([^\n]+)""").find(text) ?: error("no `targets:` line in android.yml")
        return line.groupValues[1].split(",").map { it.trim() }.toSet()
    }

    private fun repoRoot(): File =
        File(
            System.getProperty("hummingbird.repoRoot")
                ?: error("hummingbird.repoRoot not set — run under Gradle (see client/android/build.gradle.kts)"),
        )
}
