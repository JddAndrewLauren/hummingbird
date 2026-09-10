package net.twinion.hummingbird.core

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

// The token message's wire contract (ADR-0039): round-trips, normalises the
// same way the phone's entry field does, and refuses what is not text.
class TokenMessageTest {

    @Test
    fun `a token round-trips through the wire bytes`() {
        assertEquals("abc123", TokenMessage.parse(TokenMessage.encode("abc123")))
    }

    @Test
    fun `whitespace and soft-wrap newlines are normalised away`() {
        assertEquals("abc123", TokenMessage.parse("  abc\n123 \r\n".toByteArray(Charsets.UTF_8)))
    }

    @Test
    fun `nothing usable is null, never an empty credential`() {
        assertNull(TokenMessage.parse(ByteArray(0)))
        assertNull(TokenMessage.parse("  \n ".toByteArray(Charsets.UTF_8)))
    }

    @Test
    fun `bytes that are not UTF-8 are refused whole`() {
        assertNull(TokenMessage.parse(byteArrayOf(0x61, 0xFF.toByte(), 0xFE.toByte(), 0x62)))
    }

    @Test
    fun `the path sits under the listener's prefix`() {
        assertEquals(true, TokenMessage.PATH.startsWith("/hummingbird/"))
    }
}
