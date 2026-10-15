package net.twinion.hummingbird.signin

import java.io.File
import net.twinion.hummingbird.BuildConfig
import org.junit.Assert.assertTrue
import org.junit.Test

/** The phone's `serverClientId` must be an audience the authority accepts
 * (ADR-0040 decision 3): `app/build.gradle.kts`'s `GOOGLE_SIGNIN_CLIENT_ID`
 * is one of `server/worker/wrangler.toml`'s `GOOGLE_SIGNIN_CLIENT_IDS`. A
 * client id rotated in one place and not the other compiles, installs and
 * then answers every sign-in with a 403. */
class SignInClientIdDriftTest {

    @Test
    fun `the phone signs in through a client the authority accepts`() {
        val root = System.getProperty("hummingbird.repoRoot")
            ?: error("hummingbird.repoRoot not set — run under Gradle (see client/android/build.gradle.kts)")
        val wrangler = File(root, "server/worker/wrangler.toml").readText()
        val ids = Regex("""(?m)^GOOGLE_SIGNIN_CLIENT_IDS\s*=\s*"([^"]*)"""").find(wrangler)
            ?.groupValues?.get(1)
            ?.split(",")
            ?.map { it.trim() }
            ?: error("GOOGLE_SIGNIN_CLIENT_IDS not found in wrangler.toml's [vars]")
        assertTrue(
            "${BuildConfig.GOOGLE_SIGNIN_CLIENT_ID} is not among the authority's audiences $ids",
            BuildConfig.GOOGLE_SIGNIN_CLIENT_ID in ids,
        )
    }
}
