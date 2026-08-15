package net.twinion.hummingbird

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import java.io.File
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.hummingbird_ffi_mobile.MobileTaskHost
import uniffi.hummingbird_ffi_mobile.coreApiVersion

// The M0 end-to-end proof at the seam: the generated Kotlin binding loads
// the cross-compiled .so and round-trips real calls on a device/emulator —
// the check no JVM unit test can make.
@RunWith(AndroidJUnit4::class)
class CoreBindingTest {

    @Test
    fun the_binding_loads_and_answers_the_core_api_version() {
        assertEquals(1u, coreApiVersion())
    }

    @Test
    fun a_durable_core_initialises_under_an_app_local_namespace() = runBlocking {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val namespace = File(context.filesDir, "binding-test-ns").absolutePath
        val host = MobileTaskHost.init(namespace, "https://authority.example", "")

        assertEquals(coreApiVersion(), host.apiVersion())
        assertEquals(0u, host.activeItemCount())
        assertEquals(0u, host.queueDepth())

        // Without a credential, a run reaches no transport and reports so.
        val outcome = host.run(1_000L, "user", false, 0.0)
        assertEquals("no_credential", outcome.kind)
    }
}
