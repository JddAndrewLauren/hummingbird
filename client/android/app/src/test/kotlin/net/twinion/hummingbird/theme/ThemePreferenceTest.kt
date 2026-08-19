package net.twinion.hummingbird.theme

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ThemePreferenceTest {

    @Test
    fun `system follows whatever the OS currently says`() {
        assertTrue(resolveDarkTheme(ThemePreference.SYSTEM, systemPrefersDark = true))
        assertFalse(resolveDarkTheme(ThemePreference.SYSTEM, systemPrefersDark = false))
    }

    @Test
    fun `an explicit choice ignores the system setting either way`() {
        assertFalse(resolveDarkTheme(ThemePreference.LIGHT, systemPrefersDark = true))
        assertTrue(resolveDarkTheme(ThemePreference.DARK, systemPrefersDark = false))
    }
}
