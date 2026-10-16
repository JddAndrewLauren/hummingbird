package net.twinion.hummingbird

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.test.junit4.createComposeRule
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/** Settings' devices list loads once per visit (#890): the first entry is
 * itself a resume, so an entry effect beside the resume effect listed the
 * devices twice. Composed under a resumed activity with a counting loader. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = android.app.Application::class)
class DevicesLoadEffectTest {

    @get:Rule
    val composeTestRule = createComposeRule()

    private var loads = 0
    private var needsToken by mutableStateOf(true)

    private fun compose() {
        composeTestRule.setContent {
            DevicesLoadEffect(needsToken = needsToken, load = { loads += 1 })
        }
        composeTestRule.waitForIdle()
    }

    @Test
    fun `the first entry lists the devices once`() {
        compose()

        assertEquals(1, loads)
    }

    @Test
    fun `the credential changing lists them once more`() {
        compose()

        needsToken = false
        composeTestRule.waitForIdle()

        assertEquals(2, loads)
    }
}
