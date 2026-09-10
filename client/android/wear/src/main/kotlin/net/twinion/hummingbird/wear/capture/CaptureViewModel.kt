package net.twinion.hummingbird.wear.capture

import android.content.Context
import androidx.work.WorkManager
import net.twinion.hummingbird.core.CoreHolder
import net.twinion.hummingbird.sync.SyncWorker
import uniffi.hummingbird_ffi_mobile.CaptureDestination
import uniffi.hummingbird_ffi_mobile.CaptureDraft
import uniffi.hummingbird_ffi_mobile.canSubmitCapture

// The watch's capture (ADR-0039): one dictated or typed line becomes a
// title-only draft through the same `capture` door the phone uses, gated by
// the same core predicate, bound for whichever of the three destinations
// the reader tapped on `DestinationScreen` — Triage, Ready, or Ready with
// today's date stamped as the deadline (the web capture box's three squares,
// on the wrist since the 2026-09-10 design handoff). **Nothing here decides
// whether the text is a title** — `canSubmitCapture` is the core's answer
// (ADR-0024's blank rule lives in Rust), and `WearCaptureRefusalTest` pins
// that no file in this package re-derives it with `isBlank`/`.trim(`. Same
// injected-fn shape as the phone's `CaptureViewModel`, so
// `CaptureViewModelTest` drives the control flow with no `.so` in the
// process.
//
// Local-first, as everywhere: `captureFn` writes the mirror and queues the
// outbound row; the sync one-shot that follows only hurries it to the
// authority. A capture with the radio off still lands, and syncs later.
class CaptureViewModel(
    private val canSubmitFn: (String) -> Boolean,
    private val captureFn: suspend (draft: CaptureDraft, nowMs: Long) -> String,
    private val enqueueSyncFn: () -> Unit,
) {
    /** The core's gate, asked before the destination screen is drawn — a
     * line the core would refuse gets its refusal at once, not after a
     * choice that could not matter. The same predicate [submit] applies. */
    fun canSubmit(text: String): Boolean = canSubmitFn(text)

    /** `true` when the text was captured; `false` when the core refused it
     * as no title. Refusal enqueues nothing. [deadline] is `""` for no
     * deadline — the wire's own resting value — or the day "Mint for today"
     * stamps (`WallClock.todayDeadline`, the one such rule on Android). */
    suspend fun submit(text: String, destination: CaptureDestination, deadline: String, nowMs: Long): Boolean {
        if (!canSubmitFn(text)) return false
        captureFn(captureDraftFromText(text, destination, deadline), nowMs)
        enqueueSyncFn()
        return true
    }

    companion object {
        /** The real doors: the uniffi predicate, `MobileTaskHost.capture`
         * on the process's one core, and a non-expedited user-trigger sync. */
        fun create(context: Context): CaptureViewModel {
            val app = context.applicationContext
            return CaptureViewModel(
                canSubmitFn = ::canSubmitCapture,
                captureFn = { draft, nowMs -> CoreHolder.get(app).capture(draft, nowMs) },
                enqueueSyncFn = { WorkManager.getInstance(app).enqueue(SyncWorker.oneShot(SyncWorker.TRIGGER_USER)) },
            )
        }
    }
}

/** The whole of what the watch captures: the text as the title, the tapped
 * destination, the one deadline the "today" button may stamp, and every
 * other field empty — size, energy, context, priority, the scheduled date
 * and the link are mint-time decisions the phone or the web make with a
 * keyboard and a screen (the design README's own line: deciding is
 * mint-time work, not capture-time work). The text is passed as spoken;
 * normalisation is the core's. */
fun captureDraftFromText(text: String, destination: CaptureDestination, deadline: String): CaptureDraft = CaptureDraft(
    title = text,
    destination = destination,
    size = "",
    energy = "",
    context = "",
    description = "",
    projectId = "",
    priority = "",
    deadline = deadline,
    scheduledDate = "",
    linkUrl = "",
    linkLabel = "",
)

/** The confirmation's second line — where the capture landed, in the mono
 * meta register: `TRIAGE`, `READY`, or `READY · DUE TODAY`. A rendering of
 * the two facts the draft carried, decided nowhere else. */
fun landedLine(destination: CaptureDestination, deadline: String): String = when (destination) {
    CaptureDestination.TRIAGE -> "TRIAGE"
    CaptureDestination.READY -> if (deadline.isEmpty()) "READY" else "READY · DUE TODAY"
}
