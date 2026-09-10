package net.twinion.hummingbird.wear.token

import android.content.Context
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import net.twinion.hummingbird.core.TokenStore

/** Whether this watch holds a device token — the one fact the home screen's
 * conditional line reads (ADR-0039: "Send a token from the phone"). A
 * process-wide flow rather than a re-read per composition because the token
 * arrives on a thread the UI never sees: `TokenListenerService` saves it
 * and calls [refresh], and the open home screen reacts without a relaunch.
 * The value is only ever what [TokenStore] says; nothing here decides
 * whether the token is *accepted* — that is the sync outcome's to report. */
object TokenPresence {
    private val present = MutableStateFlow(false)

    val presentFlow: StateFlow<Boolean> = present.asStateFlow()

    fun refresh(context: Context) {
        present.value = TokenStore.load(context) != null
    }
}
