package net.twinion.hummingbird

import java.io.File
import org.junit.Assert.assertTrue
import org.junit.Test

// #521: item detail used to be reachable only by tapping a notification --
// found on hardware, where the accessibility tree showed 22 clickable nodes
// on Now and not one of them the card. With no alert ringing there was no
// way to open an item at all.
//
// Structural, like `NavigationStructuralTest` and for the same reason: the
// module has no emulator in CI, and "the card is clickable" is exactly the
// kind of claim the compiler cannot make. A `Card` without `onClick` still
// compiles and still renders.
class NowItemDoorTest {

    private fun source(name: String): String {
        val root = System.getProperty("hummingbird.repoRoot")
            ?: error("hummingbird.repoRoot not set -- run under Gradle (see app/build.gradle.kts)")
        val file = File(root, "client/android/app/src/main/kotlin/net/twinion/hummingbird/$name")
        check(file.isFile) { "$name not found under $root" }
        return file.readText()
            .replace(Regex("""/\*[\s\S]*?\*/"""), "")
            .replace(Regex("""(?m)^\s*//.*$"""), "")
    }

    @Test
    fun `the Now card is itself the door to item detail`() {
        val src = source("NowScreen.kt")
        assertTrue(
            "NowScreen must expose an onOpenItem hand-off",
            src.contains("onOpenItem: (String) -> Unit"),
        )
        assertTrue(
            "the row's Card must take onClick -- a clickable modifier would drop the " +
                "ripple, the Button role and the minimum touch target",
            Regex("""Card\(\s*onClick = onOpen""").containsMatchIn(src),
        )
    }

    @Test
    fun `opening an item from Now does not borrow the notification helper`() {
        // `openItemFromNotification`'s `popUpTo` answers a restored back
        // stack (#518). Navigating from Now, Now is already beneath, so
        // reusing it would state a policy that does nothing -- and would
        // tie this door's behaviour to a fix aimed at the other one.
        val src = source("MainActivity.kt")
        val call = Regex("""onOpenItem = \{[^}]*\}""").find(src)?.value
            ?: error("MainActivity does not pass onOpenItem to NowScreen")
        assertTrue(
            "opening an item from Now must be a plain navigate to the item route",
            call.contains("navigate(Routes.itemDetail("),
        )
        assertTrue(
            "the Now door must not reuse openItemFromNotification",
            !call.contains("openItemFromNotification"),
        )
    }
}
