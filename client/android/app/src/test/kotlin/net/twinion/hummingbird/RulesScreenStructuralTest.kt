package net.twinion.hummingbird

import java.io.File
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

// The M4 counterpart of `AlertsScreenStructuralTest`, and the gate ADR-0025
// most needs on this slice: the rules surfaces must re-derive no rule
// decision of their own.
//
// #540 sank six of them into `hummingbird_core::decisions::rules` —
// operator legality, the duration grammar, the deadline reading, the
// kind → field cascade, the widget cascade and the validity read — and
// every one arrives here already applied, on a `RuleRecord`, a
// `RuleFieldRecord` or a `BacktestRecord`. A Kotlin copy of any of them
// would compile, run, and look right on every fixture anyone would think
// to write; only a source gate catches it, which is why this test reads
// the files.
class RulesScreenStructuralTest {

    private fun repoFile(relative: String): String {
        val root = System.getProperty("hummingbird.repoRoot")
            ?: error("hummingbird.repoRoot not set — run under Gradle (see app/build.gradle.kts)")
        val file = File(root, relative)
        check(file.isFile) { "$relative not found under $root" }
        return file.readText()
    }

    /** The file's *code*, with comments removed. A doc comment must be
     * free to name the thing it forbids — "no `23:59` here" is the
     * clearest way to say it — so a gate that scanned raw text would
     * punish the documentation it depends on. */
    private fun source(name: String) =
        repoFile("client/android/app/src/main/kotlin/net/twinion/hummingbird/$name")
            .replace(Regex("""/\*[\s\S]*?\*/"""), "")
            .replace(Regex("""(?m)^\s*//.*$"""), "")

    private val screenSrc by lazy { source("RulesScreen.kt") }
    private val viewModelSrc by lazy { source("RulesViewModel.kt") }

    private val both by lazy {
        listOf(
            "RulesScreen.kt" to screenSrc,
            "RulesViewModel.kt" to viewModelSrc,
        )
    }

    @Test
    fun `no rules surface re-derives the deadline reading`() {
        // ADR-0013's day-only resolution has one owner
        // (`hummingbird_domain::deadline_sort_key`), and #540 is the slice
        // that stopped it being re-derived per client.
        for ((name, src) in both) {
            assertFalse(
                "$name must not carry the day-only deadline resolution",
                src.contains("23:59"),
            )
        }
    }

    @Test
    fun `no rules surface names a relative-time operator as a string`() {
        // The operators arrive as `MobileOperator` values, gated by
        // `Operator::is_legal_for`. A wire string here would be a second
        // vocabulary, and a `when` over it would not fail to compile when
        // ADR-0013 grows an eighth operator.
        for ((name, src) in both) {
            assertFalse(
                "$name must not name within_next as a wire string",
                src.contains("\"within_next\""),
            )
            assertFalse(
                "$name must not name within_last as a wire string",
                src.contains("\"within_last\""),
            )
        }
    }

    @Test
    fun `no rules surface orders anything`() {
        // `Core::rules` hands the list over in the mirror's own order, and
        // `fields_for_kind` hands the fields over core-first. Both are
        // decided; a comparator here would be a second opinion.
        for ((name, src) in both) {
            for (spelling in listOf("sortedBy", "sortWith", "sortedWith", ".sorted(")) {
                assertFalse(
                    "$name must not order anything — the seam already did ($spelling)",
                    src.contains(spelling),
                )
            }
        }
    }

    @Test
    fun `no rules surface writes its own blank check`() {
        // The standard library's disagrees with the real rule on a pasted
        // BOM — M1-5/#503's trap, and the reason `hasContentFn` is
        // injected rather than inlined.
        for ((name, src) in both) {
            assertFalse(
                "$name must not use isBlank — canSubmitCapture is the rule",
                src.contains("isBlank("),
            )
            assertFalse(
                "$name must not use isNotBlank — canSubmitCapture is the rule",
                src.contains("isNotBlank("),
            )
            assertFalse(
                "$name must not trim on the caller's behalf",
                src.contains(".trim("),
            )
        }
    }

    @Test
    fun `no rules surface parses a duration literal`() {
        // ADR-0013's duration grammar is
        // `hummingbird_domain::parse_duration`, reached through
        // `rules::parse_duration_ms`. The units a field offers arrive on
        // `RuleFieldRecord.durationUnits`; nothing here builds them.
        for ((name, src) in both) {
            assertFalse(
                "$name must not carry a duration regex",
                Regex("""Regex\(.*[mhd].*\\d""").containsMatchIn(src),
            )
            for (unit in listOf("\"m\"", "\"h\"", "\"d\"")) {
                assertFalse(
                    "$name must not hand-write the duration unit $unit",
                    src.contains(unit),
                )
            }
        }
    }

    @Test
    fun `no rules surface hand-writes a field or operator list`() {
        // The field list is `fields_for_kind`'s answer, narrowed by kind;
        // the operator list is `legal_operators`'. Both arrive on the
        // form record.
        assertTrue(
            "the field pickers must read form.fields",
            screenSrc.contains("form.fields"),
        )
        assertTrue(
            "the operator picker must read the field's legalOperators",
            screenSrc.contains("legalOperators"),
        )
        for ((name, src) in both) {
            assertFalse(
                "$name must not hand-write a field name list",
                src.contains("\"scheduled_date\""),
            )
        }
    }

    @Test
    fun `validity is read, never computed, and never confused with enablement`() {
        assertTrue(
            "the invalid badge must gate on the seam's isValid",
            screenSrc.contains("isValid"),
        )
        assertTrue(
            "the invalid fields must be the seam's own list",
            screenSrc.contains("invalidFields"),
        )
        for ((name, src) in both) {
            // A disabled rule is not an invalid one. Deriving either from
            // the other is the silently-dead-rule failure mode wearing a
            // different hat.
            assertFalse(
                "$name must not derive validity from enabled",
                Regex("""isValid\s*=\s*[^\n]*enabled""").containsMatchIn(src),
            )
            assertFalse(
                "$name must not derive enablement from validity",
                Regex("""enabled\s*=\s*[^\n]*isValid""").containsMatchIn(src),
            )
        }
    }

    @Test
    fun `the duration warning and the backtest count are read, never measured`() {
        assertTrue(
            "the duration warning must be the seam's belowAlarmInterval",
            screenSrc.contains("belowAlarmInterval"),
        )
        assertTrue(
            "the backtest count must be the seam's matchCount",
            screenSrc.contains("matchCount"),
        )
        for ((name, src) in both) {
            assertFalse(
                "$name must not compare against the alarm interval itself",
                Regex("""alarmIntervalMs\s*(<|>|<=|>=)""").containsMatchIn(src),
            )
        }
        // A bare count is the one thing the panel must not show: the
        // corpus here is the frontier, not the sweep's own `load_live_items`.
        assertTrue(
            "the backtest count must carry its corpus caveat",
            screenSrc.contains("corpusNoteKey"),
        )
    }

    @Test
    fun `the value widget is the seam's answer, not a Kotlin cascade`() {
        assertTrue(
            "the value control must read the seam's defaultWidget",
            screenSrc.contains("defaultWidget"),
        )
        assertFalse(
            "the widget must not be re-derived from the field name",
            Regex("""field\.name\s*==\s*"deadline"""").containsMatchIn(screenSrc),
        )
    }

    /** Every `when` over a seam enum must be exhaustive with no `else ->`
     * arm — the compile-time drift gate the brief names. An eighth
     * operator, a seventh widget or a third tier added to the Rust side
     * then fails this module's build rather than rendering as nothing. */
    @Test
    fun `every when over a seam enum is exhaustive with no wildcard arm`() {
        for (enum in listOf("MobileOperator", "MobileTier", "MobileValueWidget")) {
            val arm = Regex("""$enum\.[A-Z_]+\s*->""")
            assertTrue(
                "RulesScreen.kt must map $enum by its variants",
                arm.containsMatchIn(screenSrc),
            )
        }
        // The `when` bodies that map these enums carry no wildcard. The
        // one `else ->` this file may hold is over a *string* key
        // (`kindLabel`, `corpusNote`), where an unknown value is a real,
        // reachable case: the registry's kind keys are an open vocabulary
        // (ADR-0013), so falling back to the raw key is the honest answer.
        val wildcards = Regex("""(?m)^\s*else\s*->""").findAll(screenSrc).count()
        assertTrue(
            "RulesScreen.kt has $wildcards wildcard arms; only the two " +
                "open-vocabulary string maps (kindLabel, corpusNote) and the " +
                "absent-field value label may have one",
            wildcards <= 3,
        )
    }

    @Test
    fun `the rules route is registered and deliberately unreachable`() {
        val main = source("MainActivity.kt")
        assertTrue(
            "MainActivity must register the rules route",
            main.contains("composable(Routes.RULES)"),
        )
        // #541 owns reachability. Until then nothing navigates here, and
        // that absence is the decision — asserted so a later slice adding
        // an entry does it on purpose.
        assertFalse(
            "nothing may navigate to Routes.RULES until #541 gives it a home",
            main.contains("navigate(Routes.RULES)"),
        )
    }
}
