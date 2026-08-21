package net.twinion.hummingbird

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

// The unified item-detail pane's own gates — the pins that used to live in
// `TriageScreenStructuralTest` against a second editor implementation, now
// held once against the panel every host renders.
//
// Robolectric cannot capture a pixel here (that ceiling is recorded in
// `docs/SURFACES.md`), so these are source pins: each one names a shape
// whose deletion is a defect no other test in this module can see. Every
// assertion below is bounded to the block it claims about — an unbounded
// `contains` over a 900-line file is green for two indistinguishable
// reasons, and the one that matters is "the shape is somewhere else
// entirely".
class ItemDetailPanelStructuralTest {

    private fun repoFile(relative: String): String {
        val root = System.getProperty("hummingbird.repoRoot")
            ?: error("hummingbird.repoRoot not set — run under Gradle (see app/build.gradle.kts)")
        val file = File(root, relative)
        check(file.isFile) { "$relative not found under $root" }
        return file.readText()
    }

    private fun source(name: String) =
        repoFile("client/android/app/src/main/kotlin/net/twinion/hummingbird/$name")
            .replace(Regex("""/\*[\s\S]*?\*/"""), "")
            .replace(Regex("""(?m)^\s*//.*$"""), "")

    private val panelSrc by lazy { source("ItemDetailPanel.kt") }
    private val viewModelSrc by lazy { source("ItemDetailViewModel.kt") }

    /** The body of a top-level or nested `fun <name>(`, from its signature
     * to the next declaration at the same indent — enough to bound an
     * assertion to the composable that must carry it. */
    private fun functionBody(src: String, name: String): String {
        val start = src.indexOf("fun $name(")
        check(start >= 0) { "could not locate fun $name( in the source" }
        val rest = src.substring(start)
        val end = Regex("""(?m)^\}""").find(rest)?.range?.first ?: rest.length
        return rest.substring(0, end)
    }

    /** One panel, four hosts — the whole point of the unification. A host
     * that quietly grows its own pane again is what this catches. */
    @Test
    fun `every item-detail host renders this one panel`() {
        for (host in listOf(
            "NowScreen.kt",
            "ItemDetailScreen.kt",
            "RecallOverlay.kt",
            "TriageScreen.kt",
        )) {
            assertTrue(
                "$host must render the shared ItemDetailPanel",
                source(host).contains("ItemDetailPanel("),
            )
        }
    }

    /** The mode decides exactly three things (its own doc says so), and
     * `SAVE` is the default so the three reading hosts need not name it. */
    @Test
    fun `the panel defaults to the saving mode and Triage is the one host that overrides it`() {
        assertTrue(
            "the default must be SAVE",
            panelSrc.contains("mode: ItemDetailPanelMode = ItemDetailPanelMode.SAVE"),
        )
        assertTrue(
            "Triage must be the host that asks for PROMOTE",
            source("TriageScreen.kt").contains("mode = ItemDetailPanelMode.PROMOTE"),
        )
        for (host in listOf("NowScreen.kt", "ItemDetailScreen.kt", "RecallOverlay.kt")) {
            assertFalse(
                "$host must take the default rather than naming a mode",
                source(host).contains("ItemDetailPanelMode"),
            )
        }
    }

    /** #360, through a ViewModel Triage shares: the promoting write is a
     * distinct seam call whose `promoteToReady` is the literal `true`, so
     * no path through this ViewModel can triage without promoting. The
     * other half of the guarantee — that Triage never reaches `save` — is
     * `TriageScreenStructuralTest`'s. */
    @Test
    fun `the promoting write is triageItem with a literal true`() {
        assertTrue(
            "the factory must wire promoteFn to triageItem(itemId, true, ...)",
            viewModelSrc.contains("triageItem(itemId, true,"),
        )
        // Whitespace-collapsed: the claim is which method each mode fires,
        // and a reformat is not a defect.
        val submitCall = functionBody(panelSrc, "ItemDetailPanel").replace(Regex("""\s+"""), " ")
        assertTrue(
            "the PROMOTE mode's submit must fire promote, not save",
            submitCall.contains("ItemDetailPanelMode.PROMOTE -> viewModel.promote("),
        )
        assertTrue(
            "the SAVE mode's submit must fire save",
            submitCall.contains("ItemDetailPanelMode.SAVE -> viewModel.save("),
        )
    }

    /** The two per-item state leaks this unification fixed. Under a
     * constant LazyColumn key the panel is NOT disposed between
     * selections, so an unkeyed `rememberSaveable` carries item A's state
     * onto item B: title-edit mode opened by itself, and the microtask
     * grain came along with it. */
    @Test
    fun `every piece of per-item composition state is keyed on the item id`() {
        assertTrue(
            "title-edit mode must be keyed on the item",
            panelSrc.contains("var editingTitle by rememberSaveable(itemId) { mutableStateOf(false) }"),
        )
        assertTrue(
            "the microtask grain must be keyed on the item",
            functionBody(panelSrc, "MicrotaskSection")
                .contains("var grain by rememberSaveable(itemId) { mutableStateOf(2L) }"),
        )
        assertTrue(
            "a section's open/shut state must be keyed on the item",
            functionBody(panelSrc, "DetailSection")
                .contains("rememberSaveable(itemId, label)"),
        )
        // The dialog flag is deliberately NOT keyed: a question on screen
        // belongs to the gesture that asked it, not to an item.
        assertTrue(
            "the discard question must survive an Activity recreation",
            panelSrc.contains("var confirmingDiscard by rememberSaveable { mutableStateOf(false) }"),
        )
    }

    /** The header is the title's display and its edit both — there is no
     * second "Title" box saying the same words, which was the defect the
     * Triage header fixed and this panel inherits. The row itself is the
     * wide door out, clickable only while not editing, so a tap into the
     * field is not a tap on the way out. */
    @Test
    fun `the title is edited in the header, which is also the way out`() {
        val panelBody = functionBody(panelSrc, "ItemDetailPanel")
        assertFalse(
            "no standalone Title box — the header title is the edit",
            panelSrc.contains("label = { Text(\"Title\") }"),
        )
        assertTrue(
            "the header must read the DRAFT's title, so an edit shows where it was made",
            panelBody.contains("openDraft?.title ?: loadedRecord?.title.orEmpty()"),
        )
        assertTrue(
            "the inline field must bind the same draft title",
            panelBody.contains("value = openDraft.title"),
        )
        assertTrue(
            "the title itself must be the thing that opens the inline edit",
            panelBody.replace(Regex("""\s+"""), " ").contains(
                """Modifier.clickable(onClickLabel = "Edit title") { editingTitle = true }""",
            ),
        )
        assertTrue(
            "the header row must close the pane on a tap, unless the title is being edited",
            panelBody.contains(
                "if (editingTitle) Modifier else Modifier.clickable { requestClose() }",
            ),
        )
        assertTrue(
            "every leaving gesture must route through the one guarded close",
            panelBody.contains("if (viewModel.isDirty) confirmingDiscard = true else onClose()"),
        )
    }

    /** The crash this pane shipped for one device pass: `editingTitle` is
     * restored per item, so reopening a pane left mid-title-edit composes
     * with the flag already true while the record is still loading — and
     * the field, which carries the `focusRequester`, only renders once
     * there is a draft. Requesting focus then throws. The fix is that the
     * effect and the field read the SAME condition, which is what this
     * pins: a `LaunchedEffect(editingTitle)` would be the defect back. */
    @Test
    fun `the title focus request keys on the field being composed, not on the flag`() {
        val panelBody = functionBody(panelSrc, "ItemDetailPanel")
        assertTrue(
            "the condition must name both the flag and the draft's presence",
            panelBody.contains("val titleFieldOpen = editingTitle && draft != null"),
        )
        assertTrue(
            "the focus effect must key on that condition",
            panelBody.replace(Regex("""\s+"""), " ").contains(
                "LaunchedEffect(titleFieldOpen) { if (titleFieldOpen) titleFocus.requestFocus() }",
            ),
        )
        assertTrue(
            "and the field must render on the same condition it focuses on",
            panelBody.contains("if (titleFieldOpen && openDraft != null)"),
        )
    }

    /** The mark-done check, on every surface: `NowRow`'s green glyph on the
     * core's own `canMarkDone` — the wider rule that answers for Triage and
     * Grilling, where `availableActions` is empty. And drawn once: the act
     * row filters `complete` out, or the same gesture is offered twice. */
    @Test
    fun `the mark-done check is gated on canMarkDone and never drawn twice`() {
        val body = functionBody(panelSrc, "DetailBody")
        assertTrue(
            "the check must be gated on the seam's own decided fact",
            body.contains("if (record.canMarkDone)"),
        )
        assertTrue(
            "the check must be NowRow's own glyph",
            body.contains("R.drawable.ic_check"),
        )
        assertTrue(
            "the check must carry NowRow's own mark-done green token pair",
            body.contains("if (dark) StatusDoneFgDark else Moss600"),
        )
        assertFalse(
            "the mark-done is an IconButton, not a Material Checkbox",
            panelSrc.contains("Checkbox("),
        )
        assertTrue(
            "the act row must filter complete out — the check is that gesture",
            body.contains("record.availableActions.filter { it != \"complete\" }"),
        )
    }

    /** The unification's other half: the pane's field set is the shared
     * `ui/forms` one, over the seam's own vocabulary. The panel's two
     * hardcoded level lists and its `VocabularyRow` are gone — and with
     * them the reason this file was exempt from
     * `CaptureFieldSetStructuralTest`'s literal ban, which now covers it. */
    @Test
    fun `the field set is the shared one, over the seam's own vocabulary`() {
        val body = functionBody(panelSrc, "DetailBody")
        for (component in listOf("LevelSlider(", "PriorityRow(", "ContextField(", "CaptureDateField(")) {
            assertTrue("the pane must render the shared $component", body.contains(component))
        }
        assertTrue(
            "the level sliders must read the seam's own vocabulary",
            body.contains("options = formMeta.energies") && body.contains("options = formMeta.sizes"),
        )
        assertTrue(
            "the context field must read the seam's own suggestions",
            body.contains("suggestions = formMeta.suggestedContexts"),
        )
        // Read mode's glyph ramp position is the option's index in the
        // core-supplied list, so the seam's order decides it (#558) and no
        // Kotlin list can drift out from under it.
        assertTrue(
            "the size glyph's position must come from the seam's own order",
            body.contains("levelPosition(formMeta.sizes.map { it.value }, record.size)"),
        )
        assertTrue(
            "the energy glyph's position must come from the seam's own order",
            body.contains("levelPosition(formMeta.energies.map { it.value }, record.energy)"),
        )
        for (retired in listOf("SIZE_VOCABULARY", "ENERGY_VOCABULARY", "VocabularyRow", "EditBody")) {
            assertFalse(
                "$retired was replaced by the shared form components and must not come back",
                panelSrc.contains(retired),
            )
        }
    }

    /** One submit for the whole draft, whatever section it was typed in —
     * not a Save per section, which is what a per-section pencil invites.
     * The word is the mode's; there is exactly one `Button` firing
     * `onSubmit`. */
    @Test
    fun `there is exactly one submit, and its word is the mode's`() {
        val body = functionBody(panelSrc, "DetailBody")
        assertEquals(
            "exactly one submit button in the pane",
            1,
            Regex("""Button\(onClick = onSubmit""").findAll(body).count(),
        )
        assertTrue(
            "the submit must refuse an unsendable draft rather than sending it",
            body.contains("Button(onClick = onSubmit, enabled = canSave)"),
        )
        assertTrue(body.contains("ItemDetailPanelMode.SAVE -> \"Save\""))
        assertTrue(body.contains("ItemDetailPanelMode.PROMOTE -> \"Promote\""))
    }

    /** The seam between the panel and its hosts, and the one thing that
     * can go wrong at it silently: a refused submit words itself into the
     * pane's own status line, so a host told "submitted" unconditionally
     * closes the pane over both that message and the draft it is about.
     * Triage is that host. Both gestures that can empty a host's list —
     * the submit and the mark-done check — must gate on the answer. */
    @Test
    fun `the host is told a submit happened only when it landed`() {
        val flat = functionBody(panelSrc, "ItemDetailPanel").replace(Regex("""\s+"""), " ")
        assertEquals(
            "onSubmitted must never be called unconditionally",
            0,
            Regex("""(?<!if \(landed\) )onSubmitted\(\)""").findAll(flat).count(),
        )
        assertEquals(
            "both the submit and the mark-done must gate on the write landing",
            2,
            Regex("""if \(landed\) onSubmitted\(\)""").findAll(flat).count(),
        )
        assertTrue(
            "the submit's answer must come from the ViewModel, not be assumed",
            flat.contains("val landed = when (mode)"),
        )
        assertTrue(
            "and so must the mark-done's",
            flat.contains("""val landed = viewModel.act(itemId, "complete","""),
        )
    }

    /** An unset field opens editable where filling it in is the work
     * (Triage), and rests as a ghost where the surface is mostly for
     * reading. `isSet` is read off the RECORD, never the draft: reading the
     * draft would flip the default on the first character typed and
     * collapse the field mid-word. */
    @Test
    fun `an unset section opens editable only on the promoting host`() {
        val body = functionBody(panelSrc, "DetailSection")
        assertTrue(
            "the default must follow the data until the human taps the row",
            body.contains("openOverride ?: (mode == ItemDetailPanelMode.PROMOTE && !isSet)"),
        )
        assertTrue(
            "an archived item's sections must never open",
            body.contains("val open = editable &&"),
        )
        for (call in Regex("""isSet = ([^,]+),""").findAll(functionBody(panelSrc, "DetailBody"))) {
            assertTrue(
                "isSet must be read off the record, not the draft: ${call.groupValues[1]}",
                call.groupValues[1].contains("record."),
            )
        }
    }

    /** The pane draws no pencil, anywhere (operator decision 2026-08-20).
     *
     * Five of them shipped — one per section plus the title's — and every
     * one is now the tapped thing itself. This is a whole-file assertion on
     * purpose, unlike its neighbours: the claim is about the *absence* of a
     * shape, which no bounded block can make, and `ic_pencil.xml` was
     * deleted in the same change, so a reintroduced `R.drawable.ic_pencil`
     * would not even resolve. What this catches is the drawable coming back
     * with it.
     *
     * The gesture's *name* is the thing a glyph gave for free, so it is
     * pinned in the same breath: every tap-to-edit target carries an
     * `onClickLabel`, or the pane loses an accessible door it used to
     * have. */
    @Test
    fun `nothing in the pane is opened by a pencil`() {
        assertFalse(
            "the pane must draw no pencil — the tapped thing is the affordance",
            panelSrc.contains("ic_pencil"),
        )
        assertFalse(
            "and the drawable must not come back with one",
            File(
                System.getProperty("hummingbird.repoRoot")!!,
                "client/android/app/src/main/res/drawable/ic_pencil.xml",
            ).exists(),
        )
        assertEquals(
            "both tap-to-edit targets must name their gesture for a screen reader",
            2,
            Regex("""clickable\(\s*onClickLabel""").findAll(panelSrc).count(),
        )
        // Bounded to the section, because the section's label is the only
        // one that has to say "done" — the header's title edit ends on the
        // field's IME Done, not on a second tap.
        assertTrue(
            "a section's own label must flip with its state",
            functionBody(panelSrc, "DetailSection").replace(Regex("""\s+"""), " ").contains(
                "onClickLabel = if (open) \"Done editing \$label\" " +
                    "else \"Edit \$label\",",
            ),
        )
    }

    /** The disclosure (operator decision 2026-08-20): the axes line is what
     * the pane is read for, and the three reference rows under it —
     * `NOTES`, `CONTEXT`, `DATES` — sit behind one chevron.
     *
     * Three things make this a defect rather than a preference if they
     * drift. The chevron must be the pane's **only** one, or it stops
     * reading as "there is more below" and starts competing with the rows
     * that mean "tap to edit". It must default open on the promoting host:
     * an unset section opens editable there (the test above), and a field
     * that opens editable behind a shut disclosure is invisible work. And
     * it must ride the axes row's trailing slot rather than a row of its
     * own — centred on its own line, its 48dp touch target cost a 64dp band
     * of whitespace, which is the whole reason the pane needed compacting
     * in the first place. */
    @Test
    fun `the reference rows sit behind the pane's one disclosure`() {
        val body = functionBody(panelSrc, "DetailBody")
        assertEquals(
            "exactly one chevron in the pane",
            1,
            Regex("""R\.drawable\.ic_chevron_down""").findAll(panelSrc).count(),
        )
        // Bounded to the axes section's own call: `trailing` is a slot any
        // section could take, and the claim is which one has it.
        val axesCall = body.substringAfter("""label = "SIZE · ENERGY · PRIORITY",""")
            .substringBefore("LevelSlider(")
        assertTrue(
            "the chevron must ride the axes row's trailing slot, not a line of its own",
            axesCall.contains("trailing = {") &&
                axesCall.contains("R.drawable.ic_chevron_down"),
        )
        assertFalse(
            "and nothing may put it back on a centred row of its own",
            body.replace(Regex("""\s+"""), " ").contains("horizontalArrangement = Arrangement.Center"),
        )
        assertTrue(
            "the disclosure must default to the mode, and open on the promoting host",
            body.contains("detailsOverride ?: (mode == ItemDetailPanelMode.PROMOTE)"),
        )
        assertTrue(
            "its state must be keyed on the item like every other piece here",
            body.contains("var detailsOverride by rememberSaveable(itemId) { mutableStateOf<Boolean?>(null) }"),
        )
        // The three that are behind it, and the one that is not: `isSet`
        // reads uniquely enough to locate each section, and the axes line
        // must stay outside the disclosed block or the pane discloses the
        // very thing it exists to show.
        val disclosed = body.substringAfter("if (detailsOpen) {")
        for (label in listOf("NOTES", "CONTEXT", "DATES")) {
            assertTrue(
                "$label must be inside the disclosure",
                disclosed.contains("""label = "$label","""),
            )
        }
        assertFalse(
            "the axes line must stay outside it",
            disclosed.contains("""label = "SIZE · ENERGY · PRIORITY","""),
        )
    }

    /** Recall's rule (#478) at both locks: no editor and no submit for an
     * archived item, and the ViewModel refuses the write even so. */
    @Test
    fun `an archived item is readable and offers no way to edit it`() {
        val section = functionBody(panelSrc, "DetailSection")
        assertTrue(
            "a section's row must only be tappable-to-edit while editable",
            section.replace(Regex("""\s+"""), " ").contains(
                "if (editable) { Modifier.clickable(",
            ),
        )
        val body = functionBody(panelSrc, "DetailBody")
        assertTrue(
            "the submit row must be gated on editability",
            body.contains("if (record.isEditable) {"),
        )
        assertTrue(
            "and the write itself must refuse, not merely be un-rendered",
            viewModelSrc.contains("if (!record.isEditable) {"),
        )
    }
}
