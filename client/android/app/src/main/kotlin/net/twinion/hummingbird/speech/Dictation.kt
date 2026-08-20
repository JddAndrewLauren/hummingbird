package net.twinion.hummingbird.speech

import android.Manifest
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Bundle
import android.speech.RecognitionListener
import android.speech.RecognizerIntent
import android.speech.SpeechRecognizer
import androidx.core.content.ContextCompat

// The one dictation implementation both capture hosts share (#611):
// `CaptureActivity` (the launcher icon's full-screen surface) and the FAB's
// `CaptureSheet` in `MainActivity`. Extracted, never copied — a second copy
// of the recognizer plumbing is where the ADR-0022 invariants would drift.

/** One dictation session's owner: holds the platform recognizer and hands
 * out raw transcripts. `SpeechRecognizer` is a Context-bound resource with
 * a `destroy()` lifecycle, so every holder must pair construction with
 * [destroy] — `CaptureActivity` does it in `onCreate`/`onDestroy`; the
 * capture sheet remembers one for exactly as long as the sheet is composed
 * (`MainActivity`'s `DisposableEffect`). */
class DictationHost(private val context: Context) {

    private var recognizer: SpeechRecognizer? = null

    /** Starts one **on-device** recognition pass; `onTranscript` receives
     * the first hypothesis verbatim, and `onFailure` every other way the
     * pass can end.
     *
     * ADR-0022's prohibition is on audio leaving the device, and
     * `isRecognitionAvailable`/`createSpeechRecognizer` do not establish
     * that: they resolve the *default* recognition service, which the
     * platform documents as free to send audio to a remote server. Only the
     * `…OnDevice…` pair positively establishes local processing, so those
     * are what this asks for, and an absent on-device recognizer means the
     * feature is unavailable — never a quiet fall back to the default
     * service, which is exactly the "network-backed recognizer as an
     * error-path fallback" the ADR rejects by name.
     *
     * Every failure path ends the session and reports, per the same ADR:
     * "the prohibition explicitly includes error paths." */
    fun startListening(
        onTranscript: (String) -> Unit,
        onFailure: (DictationFailure) -> Unit,
    ) {
        if (ContextCompat.checkSelfPermission(context, Manifest.permission.RECORD_AUDIO) !=
            PackageManager.PERMISSION_GRANTED
        ) {
            onFailure(DictationFailure.NO_PERMISSION)
            return
        }
        if (!SpeechRecognizer.isOnDeviceRecognitionAvailable(context)) {
            onFailure(DictationFailure.UNAVAILABLE)
            return
        }
        val active = recognizer
            ?: SpeechRecognizer.createOnDeviceSpeechRecognizer(context).also { recognizer = it }
        active.setRecognitionListener(TranscriptListener(onTranscript, onFailure))
        val intent = Intent(RecognizerIntent.ACTION_RECOGNIZE_SPEECH).apply {
            putExtra(RecognizerIntent.EXTRA_LANGUAGE_MODEL, RecognizerIntent.LANGUAGE_MODEL_FREE_FORM)
        }
        active.startListening(intent)
    }

    /** Frees the platform session. Idempotent; a later [startListening]
     * would mint a fresh recognizer, but every current holder's lifetime
     * ends here. */
    fun destroy() {
        recognizer?.destroy()
        recognizer = null
    }
}

/** The ways a dictation pass can end without text, each with the sentence
 * the capture screen shows for it. One enum rather than four call sites
 * composing strings, because ADR-0022 treats the *set* as the requirement:
 * unavailable, refused, mid-session error, no match — all four visible.
 * Wording follows the design README's honesty rule: say what happened and
 * what still works, apologise for nothing. */
enum class DictationFailure(val message: String) {
    NO_PERMISSION("Dictation needs microphone access. Typing still works."),
    UNAVAILABLE("This device has no on-device speech recognition. Typing still works."),
    FAILED("Dictation stopped. Typing still works."),
    NO_MATCH("No speech recognised."),
}

/** A raw-transcript-only listener (ADR-0022): the first hypothesis's text,
 * verbatim, never trimmed or parsed here — `CaptureViewModel.onTranscript`
 * is the next stop, and it does not touch the string either. A result that
 * carries no hypothesis is a failure, not a silent no-op: an empty
 * `RESULTS_RECOGNITION` is the no-match case the ADR names. */
private class TranscriptListener(
    private val onTranscript: (String) -> Unit,
    private val onFailure: (DictationFailure) -> Unit,
) : RecognitionListener {
    override fun onResults(results: Bundle) {
        val transcript = results.getStringArrayList(SpeechRecognizer.RESULTS_RECOGNITION)?.firstOrNull()
        if (transcript == null) {
            onFailure(DictationFailure.NO_MATCH)
            return
        }
        onTranscript(transcript)
    }

    /** The recognizer has already ended the session by the time this
     * arrives; the only thing left is to say so. Nothing here retries, and
     * nothing here reaches for a second recognizer — that is the fallback
     * ADR-0022 rejects. */
    override fun onError(error: Int) {
        val failure = when (error) {
            SpeechRecognizer.ERROR_NO_MATCH,
            SpeechRecognizer.ERROR_SPEECH_TIMEOUT,
            -> DictationFailure.NO_MATCH
            SpeechRecognizer.ERROR_INSUFFICIENT_PERMISSIONS -> DictationFailure.NO_PERMISSION
            else -> DictationFailure.FAILED
        }
        onFailure(failure)
    }

    override fun onReadyForSpeech(params: Bundle?) {}
    override fun onBeginningOfSpeech() {}
    override fun onRmsChanged(rmsdB: Float) {}
    override fun onBufferReceived(buffer: ByteArray?) {}
    override fun onEndOfSpeech() {}
    override fun onPartialResults(partialResults: Bundle?) {}
    override fun onEvent(eventType: Int, params: Bundle?) {}
}
