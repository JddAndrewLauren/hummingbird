package net.twinion.hummingbird.watch

import com.google.android.gms.common.api.ApiException
import com.google.android.gms.common.api.Status
import kotlinx.coroutines.runBlocking
import net.twinion.hummingbird.core.TokenMessage
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

// `WatchTokenSender`'s control flow over fake Data Layer fns (ADR-0039):
// what it sends, to whom, and which outcome each failure becomes. The real
// clients are `create(context)`'s alone and never enter a JVM test.
class WatchTokenSenderTest {

    private val near = WatchNode("n1", "Pixel Watch", isNearby = true)
    private val far = WatchNode("n2", "Old watch", isNearby = false)

    private class Recorder {
        val sent = mutableListOf<Triple<String, String, ByteArray>>()
        val sendFn: suspend (String, String, ByteArray) -> Unit = { id, path, bytes -> sent += Triple(id, path, bytes) }
    }

    @Test
    fun `an empty or unusable field sends nothing`() = runBlocking {
        val rec = Recorder()
        val sender = WatchTokenSender({ listOf(near) }, rec.sendFn)
        assertEquals(SendOutcome.Empty, sender.send(""))
        assertEquals(SendOutcome.Empty, sender.send("   "))
        assertTrue(rec.sent.isEmpty())
    }

    @Test
    fun `no connected node is its own outcome, and nothing is sent`() = runBlocking {
        val rec = Recorder()
        val sender = WatchTokenSender({ emptyList() }, rec.sendFn)
        assertEquals(SendOutcome.NoWatch, sender.send("abc123"))
        assertTrue(rec.sent.isEmpty())
    }

    @Test
    fun `the normalised token goes to the nearby nodes only, on the token path`() = runBlocking {
        val rec = Recorder()
        val sender = WatchTokenSender({ listOf(far, near) }, rec.sendFn)
        val outcome = sender.send("  abc123 \n")
        assertEquals(SendOutcome.Sent(listOf("Pixel Watch")), outcome)
        assertEquals(1, rec.sent.size)
        val (id, path, bytes) = rec.sent.single()
        assertEquals("n1", id)
        assertEquals(TokenMessage.PATH, path)
        assertEquals("abc123", TokenMessage.parse(bytes))
    }

    @Test
    fun `with no nearby node every connected node is tried`() = runBlocking {
        val rec = Recorder()
        val sender = WatchTokenSender({ listOf(far) }, rec.sendFn)
        assertEquals(SendOutcome.Sent(listOf("Old watch")), sender.send("abc123"))
        assertEquals(listOf("n2"), rec.sent.map { it.first })
    }

    @Test
    fun `a missing Wearable API reads as unavailable, a thrown send as failed`() = runBlocking {
        val unavailable = WatchTokenSender(
            { throw ApiException(Status(17, "API_UNAVAILABLE")) },
            Recorder().sendFn,
        ).send("abc123")
        assertTrue(unavailable is SendOutcome.Unavailable)

        val failed = WatchTokenSender(
            { listOf(near) },
            { _, _, _ -> throw IllegalStateException("radio off") },
        ).send("abc123")
        assertEquals(SendOutcome.Failed("radio off"), failed)
    }
}
