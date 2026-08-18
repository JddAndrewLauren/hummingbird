package net.twinion.hummingbird.skills

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.hummingbird_ffi_mobile.MobileGrillQuestion
import uniffi.hummingbird_ffi_mobile.MobileGrillTurn
import uniffi.hummingbird_ffi_mobile.grillRunBody
import uniffi.hummingbird_ffi_mobile.microtaskRunBody

// The Kotlin side of the shared run-body fixture (#538) — the third reader
// of `client/core/tests/fixtures/skills-run-bodies.json`, after
// `client/core/tests/skills_run_bodies.rs` and
// `client/web/src/skills/run-body-fixture.test.ts`. Each asserts its own
// builder emits `expected` byte for byte, so the three cannot drift.
//
// **Why this is instrumented rather than a JVM test.** Two reasons, and
// each alone would be enough: a JVM process has no native library, so
// `grillRunBody`/`microtaskRunBody` cannot be called there at all; and the
// fixture is a file in the repo, which the device cannot see. Gradle's
// `copySkillsRunBodyFixture` task copies the real file into this suite's
// assets at build time (`app/build.gradle.kts`), so what is read here is
// the committed fixture and not a hand-typed copy of it.
//
// The JVM suite's own share of this contract is narrower and complementary:
// `SkillRunnerTest` pins that the transport posts the core's string
// verbatim, whatever the bytes are.
//
// `JSONObject` here is fine and is not a violation of
// `SkillsLaneIsolationTest`'s no-parser rule — that rule is about the lane's
// *production* source under `main/`, which must never read a runner line
// itself. This is a test reading a fixture.
@RunWith(AndroidJUnit4::class)
class RunBodyFixtureTest {

    private val fixture: JSONObject by lazy {
        val text = InstrumentationRegistry.getInstrumentation()
            .context
            .assets
            .open("skills-run-bodies.json")
            .bufferedReader()
            .use { it.readText() }
        JSONObject(text)
    }

    @Test
    fun every_fixture_case_matches_the_cores_bytes() {
        val cases = fixture.getJSONArray("cases")
        assertTrue("the fixture must carry cases", cases.length() > 0)
        var microtaskCases = 0
        var grillCases = 0

        for (index in 0 until cases.length()) {
            val case = cases.getJSONObject(index)
            val name = case.getString("name")
            val input = case.getJSONObject("input")
            val built = when (val skill = case.getString("skill")) {
                "microtask" -> {
                    microtaskCases += 1
                    microtaskRunBody(
                        itemId = input.getString("itemId"),
                        replace = input.optBoolean("replace", false),
                        grain = if (input.has("grain")) input.getLong("grain") else null,
                        model = if (input.has("model")) input.getString("model") else null,
                    )
                }
                "grill-me" -> {
                    grillCases += 1
                    grillRunBody(input.getString("ref"), turnsOf(input.getJSONArray("turns")))
                }
                else -> error("$name: unknown skill $skill")
            }
            assertEquals(name, case.getString("expected"), built)
        }

        // A fixture that lost half its cases would still pass the loop.
        assertTrue("no microtask case in the fixture", microtaskCases > 0)
        assertTrue("no grill-me case in the fixture", grillCases > 0)
    }

    private fun turnsOf(raw: org.json.JSONArray): List<MobileGrillTurn> =
        (0 until raw.length()).map { index ->
            val turn = raw.getJSONObject(index)
            val question = turn.getJSONObject("question")
            val choices = question.getJSONArray("choices")
            MobileGrillTurn(
                question = MobileGrillQuestion(
                    prompt = question.getString("prompt"),
                    recommendedAnswer = question.getString("recommendedAnswer"),
                    choices = (0 until choices.length()).map { choices.getString(it) },
                ),
                answer = turn.getString("answer"),
            )
        }
}
