package net.twinion.hummingbird.signin

import android.content.Context
import androidx.credentials.Credential
import androidx.credentials.CredentialManager
import androidx.credentials.CustomCredential
import androidx.credentials.GetCredentialRequest
import androidx.credentials.exceptions.GetCredentialCancellationException
import androidx.credentials.exceptions.GetCredentialException
import androidx.credentials.exceptions.NoCredentialException
import com.google.android.libraries.identity.googleid.GetSignInWithGoogleOption
import com.google.android.libraries.identity.googleid.GoogleIdTokenCredential
import com.google.android.libraries.identity.googleid.GoogleIdTokenParsingException
import net.twinion.hummingbird.BuildConfig
import uniffi.hummingbird_ffi_mobile.MobileSignInOutcome
import uniffi.hummingbird_ffi_mobile.signInRefusal

// The phone's Sign in with Google (#841, ADR-0040 decisions 1 and 3), in two
// halves:
//
// - [GoogleCredentialSource] asks Credential Manager's Sign in with Google
//   option for an ID token, with the `hummingbird-signin` Web client as
//   `serverClientId` (`BuildConfig.GOOGLE_SIGNIN_CLIENT_ID`), and folds what
//   comes back into a [GoogleCredential]. That call needs an Activity and a
//   phone, so it is the one piece no JVM test runs; [credentialFailure], the
//   exception-to-arm fold inside it, is plain and tested.
// - [signInWithGoogle] trades that ID token through the mobile seam
//   (`MobileTaskHost.signInWithGoogle`, always platform `android`) and, on
//   `SignedIn`, hands the token to the **same save callback a pasted token
//   takes** — `AppRoot.saveToken`, which stores it, pushes it into the core,
//   re-registers push and runs the first sync. Nothing after that knows how
//   the token was minted (ADR-0040 decision 1).
//
// **A refusal touches no credential.** Every arm but `SignedIn` returns its
// own sentence and calls nothing, so a held token — including one the
// authority has just 401'd, which `TokenStore` still holds while the card
// re-prompts — stays exactly where it was.

/** What Credential Manager's Sign in with Google came back with. */
sealed interface GoogleCredential {
    data class IdToken(val idToken: String) : GoogleCredential

    /** The operator dismissed the sheet. */
    data object Cancelled : GoogleCredential

    /** No Google account on this phone to sign in with. */
    data object NoAccount : GoogleCredential

    /** Anything else Credential Manager or the ID-token parse refused. */
    data class Failed(val detail: String) : GoogleCredential
}

/** Folds a Credential Manager failure into its arm. Cancel and no-account
 * are their own arms because each gets its own sentence; everything else
 * carries the library's words. */
fun credentialFailure(error: GetCredentialException): GoogleCredential = when (error) {
    is GetCredentialCancellationException -> GoogleCredential.Cancelled
    is NoCredentialException -> GoogleCredential.NoAccount
    else -> GoogleCredential.Failed(error.errorMessage?.toString() ?: error.type)
}

object GoogleCredentialSource {
    /** Shows the Sign in with Google sheet over [activity] and answers the
     * ID token it yields, or why there is none. [activity] must be an
     * Activity context: Credential Manager draws its sheet over it. */
    suspend fun request(activity: Context): GoogleCredential {
        val option = GetSignInWithGoogleOption.Builder(BuildConfig.GOOGLE_SIGNIN_CLIENT_ID).build()
        val request = GetCredentialRequest.Builder().addCredentialOption(option).build()
        return try {
            idTokenOf(CredentialManager.create(activity).getCredential(activity, request).credential)
        } catch (error: GetCredentialException) {
            credentialFailure(error)
        }
    }

    private fun idTokenOf(credential: Credential): GoogleCredential {
        if (credential !is CustomCredential ||
            credential.type != GoogleIdTokenCredential.TYPE_GOOGLE_ID_TOKEN_CREDENTIAL
        ) {
            return GoogleCredential.Failed("Google answered with a ${credential.type} credential, not an ID token.")
        }
        return try {
            GoogleCredential.IdToken(GoogleIdTokenCredential.createFrom(credential.data).idToken)
        } catch (error: GoogleIdTokenParsingException) {
            GoogleCredential.Failed(error.message ?: "Google's ID token could not be read.")
        }
    }
}

/** The sentence for a sign-in that did not mint a token, or `null` for
 * [MobileSignInOutcome.SignedIn]. One arm per refusal the authority can
 * give — the seam already split the two 503s — and no `else`, so an arm
 * added to the seam is a compile error here rather than a silent fold.
 * The four refusals are the core's sentences (`signInRefusal`, #887), the
 * same ones the web and the extension show; the arms carrying a detail,
 * and the watch's own refusal, are worded here. */
fun exchangeMessage(outcome: MobileSignInOutcome): String? = when (outcome) {
    is MobileSignInOutcome.SignedIn -> null
    MobileSignInOutcome.NotAllowed,
    MobileSignInOutcome.RateLimited,
    MobileSignInOutcome.Unconfigured,
    MobileSignInOutcome.Unavailable -> signInRefusal(outcome)
    is MobileSignInOutcome.Invalid -> "The authority refused this device's name — ${outcome.detail}"
    is MobileSignInOutcome.Failed -> "Sign-in didn't reach the authority — ${outcome.detail}"
    MobileSignInOutcome.RefusedOnWatch ->
        "A watch doesn't sign in — it takes its token from the phone. Nothing was sent."
}

/** The sentence for a [GoogleCredential] that carries no ID token. */
fun credentialMessage(credential: GoogleCredential): String? = when (credential) {
    is GoogleCredential.IdToken -> null
    GoogleCredential.Cancelled -> "Sign-in cancelled."
    GoogleCredential.NoAccount ->
        "No Google account on this phone. Add one in Android's settings, or paste a token instead."
    is GoogleCredential.Failed -> "Google sign-in failed — ${credential.detail}"
}

/** One sign-in, after the sheet: trades [credential]'s ID token for this
 * device's token under [deviceName], and hands a minted token to
 * [saveToken] — the pasted token's own save path. Answers `null` when it
 * signed in, else the sentence to show; a refusal calls nothing. */
suspend fun signInWithGoogle(
    credential: GoogleCredential,
    deviceName: String,
    exchange: suspend (idToken: String, name: String) -> MobileSignInOutcome,
    saveToken: suspend (String) -> Unit,
): String? {
    if (credential !is GoogleCredential.IdToken) return credentialMessage(credential)
    val outcome = exchange(credential.idToken, deviceName)
    if (outcome is MobileSignInOutcome.SignedIn) {
        saveToken(outcome.token)
        return null
    }
    return exchangeMessage(outcome)
}
