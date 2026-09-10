package net.twinion.hummingbird.core

import java.nio.ByteBuffer
import java.nio.charset.CharacterCodingException
import java.nio.charset.CodingErrorAction

/** The one message the phone sends the watch over the Wearable Data Layer
 * (ADR-0039): a device token, as UTF-8 bytes on a fixed path. Both ends
 * read this object — `WatchTokenSender` in `:app` encodes, the watch's
 * `TokenListenerService` parses — so the wire shape has exactly one
 * definition and the listener's manifest filter (`pathPrefix`
 * `/hummingbird`) is a prefix of [PATH] by construction.
 *
 * [parse] is strict where it can be: bytes that are not well-formed UTF-8
 * answer `null` rather than a token with replacement characters in it,
 * and the decoded text goes through [TokenValidation.normalize] — the same
 * rule the phone's own entry field applies — so a token is a token by one
 * definition on every device. */
object TokenMessage {
    const val PATH = "/hummingbird/device-token"

    fun encode(token: String): ByteArray = token.toByteArray(Charsets.UTF_8)

    fun parse(bytes: ByteArray): String? {
        val decoder = Charsets.UTF_8.newDecoder()
            .onMalformedInput(CodingErrorAction.REPORT)
            .onUnmappableCharacter(CodingErrorAction.REPORT)
        val text = try {
            decoder.decode(ByteBuffer.wrap(bytes)).toString()
        } catch (e: CharacterCodingException) {
            return null
        }
        return TokenValidation.normalize(text)
    }
}
