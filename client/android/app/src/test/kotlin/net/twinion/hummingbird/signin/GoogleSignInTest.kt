package net.twinion.hummingbird.signin

import androidx.credentials.exceptions.GetCredentialCancellationException
import androidx.credentials.exceptions.GetCredentialUnknownException
import androidx.credentials.exceptions.NoCredentialException
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.hummingbird_ffi_mobile.MobileSignInOutcome
import uniffi.hummingbird_ffi_mobile.signInRefusal

// #841's acceptance boxes, driven with fakes: every way a sign-in can fail
// has its own sentence and leaves the held token alone, and a sign-in that
// works hands its token to the save callback the paste uses.
class GoogleSignInTest {

    /** A stand-in for `AppRoot.saveToken` over `TokenStore`: what is held,
     * and every save that reached it. */
    private class FakeTokenStore(var held: String?) {
        val saves = mutableListOf<String>()

        suspend fun save(token: String) {
            saves += token
            held = token
        }
    }

    private fun exchangeAnswering(outcome: MobileSignInOutcome, calls: MutableList<Pair<String, String>>) =
        { idToken: String, name: String ->
            calls += idToken to name
            outcome
        }

    @Test
    fun `credential manager's cancel and no-account each fold to their own arm`() {
        assertEquals(GoogleCredential.Cancelled, credentialFailure(GetCredentialCancellationException()))
        assertEquals(GoogleCredential.NoAccount, credentialFailure(NoCredentialException()))
        val other = credentialFailure(GetCredentialUnknownException("play services down"))
        assertEquals(GoogleCredential.Failed("play services down"), other)
    }

    @Test
    fun `cancel, no account, 403, 429 and both 503s each say something different and touch no token`() =
        runBlocking {
            val failures: List<Pair<String, suspend (FakeTokenStore) -> String?>> = listOf(
                "cancel" to { store ->
                    signInWithGoogle(GoogleCredential.Cancelled, "Pixel", { _, _ -> error("no exchange") }, store::save)
                },
                "no account" to { store ->
                    signInWithGoogle(GoogleCredential.NoAccount, "Pixel", { _, _ -> error("no exchange") }, store::save)
                },
                "403" to { store ->
                    signInWithGoogle(GoogleCredential.IdToken("jwt"), "Pixel", { _, _ -> MobileSignInOutcome.NotAllowed }, store::save)
                },
                "429" to { store ->
                    signInWithGoogle(GoogleCredential.IdToken("jwt"), "Pixel", { _, _ -> MobileSignInOutcome.RateLimited }, store::save)
                },
                "503 unconfigured" to { store ->
                    signInWithGoogle(GoogleCredential.IdToken("jwt"), "Pixel", { _, _ -> MobileSignInOutcome.Unconfigured }, store::save)
                },
                "503 unavailable" to { store ->
                    signInWithGoogle(GoogleCredential.IdToken("jwt"), "Pixel", { _, _ -> MobileSignInOutcome.Unavailable }, store::save)
                },
            )

            val messages = failures.map { (name, attempt) ->
                val store = FakeTokenStore(held = "hb_held")
                val message = attempt(store)
                assertNotNull("$name must say why", message)
                assertEquals("$name must leave the held token", "hb_held", store.held)
                assertTrue("$name must save nothing", store.saves.isEmpty())
                message!!
            }
            assertEquals("every refusal has its own sentence", messages.size, messages.toSet().size)
        }

    @Test
    fun `a refusal with no token held leaves none held`() = runBlocking {
        val store = FakeTokenStore(held = null)

        signInWithGoogle(GoogleCredential.IdToken("jwt"), "Pixel", { _, _ -> MobileSignInOutcome.NotAllowed }, store::save)

        assertNull(store.held)
    }

    @Test
    fun `a successful exchange hands the minted token to the save callback, once`() = runBlocking {
        val store = FakeTokenStore(held = null)
        val calls = mutableListOf<Pair<String, String>>()

        val message = signInWithGoogle(
            GoogleCredential.IdToken("google-id-token"),
            "Pixel 10 Pro Fold",
            exchangeAnswering(MobileSignInOutcome.SignedIn(id = "device-android-pixel", token = "hb_minted"), calls),
            store::save,
        )

        assertNull(message)
        assertEquals(listOf("google-id-token" to "Pixel 10 Pro Fold"), calls)
        assertEquals(listOf("hb_minted"), store.saves)
    }

    /** #887: the four refusals the authority gives read the core's
     * sentences through the real seam call — the same strings the core's,
     * the web's and the extension's tests pin. */
    @Test
    fun `each refusal the authority gives is the core's sentence`() {
        for ((outcome, sentence) in listOf(
            MobileSignInOutcome.NotAllowed to
                "That Google account isn't allowed to sign in here. Nothing on this device changed.",
            MobileSignInOutcome.RateLimited to
                "Too many sign-ins in the last hour. Try again later, or paste a token instead.",
            MobileSignInOutcome.Unconfigured to
                "Sign-in isn't set up on the authority yet. Paste a token instead.",
            MobileSignInOutcome.Unavailable to
                "The authority couldn't check with Google just now. Try again in a minute.",
        )) {
            assertEquals(sentence, exchangeMessage(outcome))
            assertEquals(signInRefusal(outcome), exchangeMessage(outcome))
        }
    }

    @Test
    fun `the seam's other refusals still say something`() {
        assertNotNull(exchangeMessage(MobileSignInOutcome.Invalid("name too long")))
        assertTrue(exchangeMessage(MobileSignInOutcome.Failed("offline"))!!.contains("offline"))
        assertNull(exchangeMessage(MobileSignInOutcome.SignedIn(id = "x", token = "hb_x")))
    }
}
