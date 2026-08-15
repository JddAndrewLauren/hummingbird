package net.twinion.hummingbird.core

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class TokenValidationTest {

    @Test
    fun `a clean token passes through unchanged`() {
        assertEquals("hb_dev_abc123", TokenValidation.normalize("hb_dev_abc123"))
    }

    @Test
    fun `terminal copies drag whitespace that is stripped`() {
        assertEquals("hb_dev_abc123", TokenValidation.normalize("  hb_dev_abc123\n"))
    }

    @Test
    fun `a soft-wrapped copy with an interior newline is rejoined`() {
        assertEquals("hb_dev_abc123", TokenValidation.normalize("hb_dev_\nabc123"))
    }

    @Test
    fun `whitespace-only input is no token, never an empty credential`() {
        assertNull(TokenValidation.normalize("   \n "))
        assertNull(TokenValidation.normalize(""))
    }
}
