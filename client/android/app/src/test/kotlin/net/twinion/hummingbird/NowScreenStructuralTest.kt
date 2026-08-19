package net.twinion.hummingbird

import java.io.File
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

// The CI gate M1-6/#504's brief names explicitly: `NowScreen` and
// `NowViewModel` must call no per-item decision function of their own — the
// mobile/web asymmetry `hummingbird-ffi-mobile::lib.rs`'s module doc states
// (Android reads already-decided `NowItemRecord`s, never `by_priority_then_due`,
// `compute_urgency` or `available_actions` itself), and the `when(band)`
// over `MobileUrgencyBand` must be exhaustive with no `else` arm — the
// compile-time drift gate a `uniffi::Enum` crossing buys. The same "parse
// the real source, not a hand-copied expectation" discipline
// `CaptureSubmitRefusalTest`/`ManifestAliasTest`/`ColorTokenDriftTest`
// already use for their own no-emulator gates.
class NowScreenStructuralTest {

    private fun repoFile(relative: String): String {
        val root = System.getProperty("hummingbird.repoRoot")
            ?: error("hummingbird.repoRoot not set — run under Gradle (see app/build.gradle.kts)")
        val file = File(root, relative)
        check(file.isFile) { "$relative not found under $root" }
        return file.readText()
    }

    private val nowScreenSrc by lazy {
        repoFile("client/android/app/src/main/kotlin/net/twinion/hummingbird/NowScreen.kt")
    }

    private val nowViewModelSrc by lazy {
        repoFile("client/android/app/src/main/kotlin/net/twinion/hummingbird/NowViewModel.kt")
    }

    @Test
    fun `NowViewModel imports the real uniffi nowBoard and act bindings`() {
        assertTrue(
            "expected NowViewModel to close over CoreHolder.get(...).nowBoard",
            nowViewModelSrc.contains(".nowBoard("),
        )
        assertTrue(
            "expected NowViewModel to close over CoreHolder.get(...).act",
            nowViewModelSrc.contains(".act("),
        )
    }

    @Test
    fun `the production factory wires both fns to the real MobileTaskHost, not a fake`() {
        val factory = Regex("""fun create\(context: Context\)[\s\S]*?\n {4}}""")
            .find(nowViewModelSrc)
            ?.value
            ?: error("could not locate NowViewModel.create in the source")
        assertTrue(
            "NowViewModel.create must reach CoreHolder.get(...)",
            factory.contains("CoreHolder.get(context.applicationContext)"),
        )
    }

    @Test
    fun `neither NowScreen nor NowViewModel re-derives the frontier ordering, grouping, urgency banding, or act affordances locally`() {
        // The decision functions this screen must never call directly —
        // they are not even exported to Kotlin (see lib.rs's module doc),
        // but a hand-rolled equivalent (a local priority comparator, a
        // hardcoded deadline-window band, a hardcoded per-stage action
        // list, a local grouping pass) would silently disagree with the
        // core rule the same way a Kotlin `isBlank()` copy disagrees with
        // `can_submit_capture` on a BOM-only draft. `groupBy` is M3/#530's
        // own addition to this gate: the frontier board's columns arrive
        // from `hummingbird_ffi_mobile::MobileTaskHost.nowBoard` already
        // grouped, so neither file may re-group locally either.
        for ((name, src) in listOf(
            "NowScreen.kt" to nowScreenSrc,
            "NowViewModel.kt" to nowViewModelSrc,
        )) {
            assertFalse(
                "$name must not implement its own comparator (sortedBy/sortWith)",
                src.contains("sortedBy") || src.contains("sortWith") || src.contains(".sorted("),
            )
            assertFalse("$name must not hardcode a priority rank table", src.contains("priorityRank"))
            assertFalse(
                "$name must not re-derive an urgency window (a raw day/hour arithmetic constant)",
                Regex("""\b\d+\s*\*\s*24\s*\*\s*60\b""").containsMatchIn(src),
            )
            assertFalse(
                "$name must not re-group the frontier locally (groupBy)",
                src.contains("groupBy"),
            )
        }
    }

    @Test
    fun `NowScreen's when over MobileUrgencyBand is exhaustive with no else arm`() {
        // A `when` expression exhaustive over a sealed/enum type needs no
        // `else`; adding one would silently swallow a fifth band added to
        // `hummingbird_core::decisions::urgency::UrgencyBand` /
        // `ffi-mobile::MobileUrgencyBand` in the future, defeating the
        // compile-time drift gate the brief names. Every `when (band)`
        // block in this file must cover exactly the four known cases and
        // carry no `else ->` arm.
        val whenBlocks = Regex("""when\s*\(band\)\s*\{([\s\S]*?)\n}""").findAll(nowScreenSrc).toList()
        assertTrue("expected at least one `when (band)` block in NowScreen.kt", whenBlocks.isNotEmpty())
        for (block in whenBlocks) {
            val body = block.groupValues[1]
            assertFalse("a when(band) block must not carry an else arm", body.contains("else ->"))
            for (variant in listOf("CALM", "SOON", "NOW", "OVERDUE")) {
                assertTrue(
                    "when(band) is missing the $variant arm",
                    body.contains("MobileUrgencyBand.$variant"),
                )
            }
        }
    }

    @Test
    fun `calm gets no urgency swatch`() {
        // ADR-0021 decision 2: "`calm` gets no swatch — the default is not
        // a claim worth colouring." The rule lives in urgencyColor's own
        // mapping (CALM -> null) rather than at the call site, so this
        // asserts the mapping and that the caller honours a null.
        assertTrue(
            "urgencyColor must return a nullable Color so CALM can map to no swatch",
            Regex("""fun urgencyColor\([^)]*\): Color\?""").containsMatchIn(nowScreenSrc),
        )
        assertTrue(
            "urgencyColor must map MobileUrgencyBand.CALM to null",
            Regex("""MobileUrgencyBand\.CALM\s*->\s*null""").containsMatchIn(nowScreenSrc),
        )
        assertTrue(
            "the swatch Box must be rendered only when urgencyColor gives one",
            Regex("""urgencyColor\([^)]*\)\?\.let""").containsMatchIn(nowScreenSrc),
        )
    }

    @Test
    fun `entering the screen crosses the seam once, not twice`() {
        // #530: "One seam door returns the whole board; rendering it makes a
        // single crossing." A one-shot `LaunchedEffect(Unit) { load(...) }`
        // beside the resume effect's own refresh made entry read the board
        // twice, and the two raced — the resume leg could paint a
        // default-axis board before `load` restored the persisted axis. The
        // resume effect owns the first read now (`NowViewModel.loadedOnce`),
        // so no `LaunchedEffect(Unit)` may come back; `LaunchedEffect(syncTick)`
        // is a different thing and stays.
        assertFalse(
            "NowScreen.kt must not re-add a one-shot LaunchedEffect(Unit) beside the resume effect",
            nowScreenSrc.contains("LaunchedEffect(Unit)"),
        )
        assertTrue(
            "the resume effect must branch on loadedOnce so the first resume is the load",
            nowScreenSrc.contains("viewModel.loadedOnce"),
        )
    }

    @Test
    fun `the collapse control uses an icon and a full-sized touch target`() {
        // The design system's ICONOGRAPHY rule is "Unicode as icons: never"
        // (README) — the header drew "▸"/"▾" — and its 44px row height is
        // the minimum touch target on every surface, which a bare
        // `clickable` modifier does not supply.
        for (triangle in listOf("▸", "▾")) {
            assertFalse(
                "NowScreen.kt must not draw a Unicode triangle as an icon",
                nowScreenSrc.contains(triangle),
            )
        }
        val header = Regex("""private fun ColumnHeader\([\s\S]*?\n}""")
            .find(nowScreenSrc)
            ?.value
            ?: error("could not locate ColumnHeader in NowScreen.kt")
        assertTrue(
            "the collapse toggle must carry the 44dp minimum touch target",
            header.contains("heightIn(min = 44.dp)"),
        )
        assertTrue(
            "the collapse state must be drawn with the chevron drawable",
            header.contains("R.drawable.ic_chevron_down"),
        )
    }

    @Test
    fun `the action buttons wrap rather than clipping at a narrow width`() {
        // A Ready item offers all four actions (decisions::actions::
        // available_actions), and "Start / Complete / Mark blocked /
        // Cancel" is wider than a phone card — a fixed, non-scrolling Row
        // clipped the trailing action on the ordinary case. No emulator
        // here, so this gates the container choice itself.
        val actionsBlock = Regex("""availableActions\.isNotEmpty\(\)[\s\S]*?\n {12}}""")
            .find(nowScreenSrc)
            ?.value
            ?: error("could not locate the available-actions block in NowScreen.kt")
        assertTrue(
            "the action buttons must sit in a FlowRow, not a fixed Row",
            actionsBlock.contains("FlowRow("),
        )
    }

    // ------------------------------------------------------ panes (#537)

    @Test
    fun `NowViewModel closes over the real pane-lane and do-date-write bindings, not fakes`() {
        val factory = Regex("""fun create\(context: Context\)[\s\S]*?\n {4}}""")
            .find(nowViewModelSrc)
            ?.value
            ?: error("could not locate NowViewModel.create in the source")
        for (call in listOf(".paneZoneQueries(", ".rankPanes(", ".setScheduledDate(")) {
            assertTrue(
                "NowViewModel.create must reach CoreHolder.get(...)$call",
                factory.contains(call),
            )
        }
        assertTrue(
            "the zone bridge's resolve leg must run through ZoneBridge.resolve",
            nowViewModelSrc.contains("ZoneBridge.resolve("),
        )
    }

    @Test
    fun `nowPaneLabels when over MobileStandingQuestion is exhaustive with no else arm`() {
        val block = Regex("""nowPaneLabel\(pane: MobileRankedPane\): String = when \(pane\.standingQuestion\) \{([\s\S]*?)\n}""")
            .find(nowScreenSrc)
            ?.groupValues
            ?.get(1)
            ?: error("could not locate nowPaneLabel's when block in NowScreen.kt")
        assertFalse("nowPaneLabel must not carry an else arm", block.contains("else ->"))
        for (variant in listOf("WASTE", "WEEKEND", "VACATION", "RACE", "KIMI", "GITHUB", "UPTIME", "REACHABILITY")) {
            assertTrue(
                "nowPaneLabel is missing the $variant arm",
                block.contains("MobileStandingQuestion.$variant"),
            )
        }
    }

    @Test
    fun `the pane section renders through the shared shell, never a second implementation`() {
        assertTrue(
            "NowScreen.kt must render its panes through the shared RankedPaneList",
            nowScreenSrc.contains("RankedPaneList(panes"),
        )
        assertFalse(
            "NowScreen.kt must not re-declare its own PaneRow",
            nowScreenSrc.contains("fun PaneRow("),
        )
    }

    @Test
    fun `the panes section carries no local comparator over the seams own display order`() {
        assertFalse(
            "the panes list must not be re-sorted locally",
            Regex("""panes\.sorted(By|With)?\(""").containsMatchIn(nowScreenSrc),
        )
    }
}
