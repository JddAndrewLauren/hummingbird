package net.twinion.hummingbird.watch

import android.content.Context
import com.google.android.gms.common.api.ApiException
import com.google.android.gms.wearable.Wearable
import kotlinx.coroutines.tasks.await
import net.twinion.hummingbird.core.TokenMessage
import net.twinion.hummingbird.core.TokenValidation

// The phone's half of the watch's token hand-off (ADR-0039): one device
// token, typed once into Settings' Watch card, sent over the Wearable Data
// Layer's `MessageClient` to every paired watch running this package under
// this key, and then forgotten. **The phone keeps nothing**: the raw string
// lives in the text field until the send succeeds and in no `ViewModel`
// field ever (`SettingsScreenStructuralTest` pins it); the watch stores it
// in its own `TokenStore` (`TokenListenerService`). The two devices hold
// different tokens — the phone's own, and `device-watch` — so a leak here
// would cost the watch's credential, not the phone's, but a device token
// is write-everything and the rule is the same for both.
//
// Same package and same signing certificate on both ends is the Data
// Layer's delivery condition. A message to a watch that runs the debug
// build while the phone runs release is *accepted* by `sendMessage` and
// never arrives — there is no error to show. That is why `deploy.sh`
// prints both APKs' certificate lines, and why [SendOutcome.Sent] says
// "sent", never "received".

/** One paired node, as much of `com.google.android.gms.wearable.Node` as the
 * sender reads — a value type so a JVM test can hand in fakes. */
data class WatchNode(val id: String, val displayName: String, val isNearby: Boolean)

sealed interface SendOutcome {
    /** The field held nothing a token could be made of. */
    data object Empty : SendOutcome

    /** The Data Layer answered, and no watch is paired and connected. */
    data object NoWatch : SendOutcome

    /** Accepted for delivery to the named node(s). Not "received" — see the
     * header for the one failure this cannot see. */
    data class Sent(val nodeNames: List<String>) : SendOutcome

    /** This phone has no usable Wearable API (no Play services, or the API
     * is missing) — a fact about the phone, not the watch. */
    data class Unavailable(val message: String) : SendOutcome

    /** The Data Layer refused or threw on the send itself. */
    data class Failed(val message: String) : SendOutcome
}

class WatchTokenSender(
    private val connectedNodesFn: suspend () -> List<WatchNode>,
    private val sendFn: suspend (nodeId: String, path: String, bytes: ByteArray) -> Unit,
) {
    /** Normalises [raw] with the same rule the phone's own token entry
     * uses, then sends it to the nearby nodes (or, with none nearby, to
     * every connected one). One send per node; the first failure is the
     * outcome. */
    suspend fun send(raw: String): SendOutcome {
        val token = TokenValidation.normalize(raw) ?: return SendOutcome.Empty
        val nodes = try {
            connectedNodesFn()
        } catch (e: ApiException) {
            return SendOutcome.Unavailable(e.message ?: "Wearable API unavailable")
        } catch (e: Exception) {
            return SendOutcome.Failed(e.message ?: e.javaClass.simpleName)
        }
        if (nodes.isEmpty()) return SendOutcome.NoWatch
        val targets = nodes.filter { it.isNearby }.ifEmpty { nodes }
        val bytes = TokenMessage.encode(token)
        for (node in targets) {
            try {
                sendFn(node.id, TokenMessage.PATH, bytes)
            } catch (e: Exception) {
                return SendOutcome.Failed(e.message ?: e.javaClass.simpleName)
            }
        }
        return SendOutcome.Sent(targets.map { it.displayName })
    }

    companion object {
        /** The real Data Layer clients. `getNodeClient`/`getMessageClient`
         * never throw at construction; an absent Wearable API surfaces as
         * an `ApiException` from the first call, which [send] maps. */
        fun create(context: Context): WatchTokenSender {
            val app = context.applicationContext
            return WatchTokenSender(
                connectedNodesFn = {
                    Wearable.getNodeClient(app).connectedNodes.await()
                        .map { WatchNode(it.id, it.displayName, it.isNearby) }
                },
                sendFn = { nodeId, path, bytes ->
                    Wearable.getMessageClient(app).sendMessage(nodeId, path, bytes).await()
                },
            )
        }
    }
}
