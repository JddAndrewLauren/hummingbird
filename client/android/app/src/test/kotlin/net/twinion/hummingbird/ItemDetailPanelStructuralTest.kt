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

    /** The state leaks this pane has shipped, and the rule that ends the
     * class of them.
     *
     * Both inline hosts rendered the pane as `item(key = "selected-item")`
     * — a **constant** LazyColumn key — so selecting another item disposed
     * this composable and recomposed it at the same slot, and the slot's
     * saveable state was saved on the way out and offered back on the way
     * in. Those keys name the item now (their own pins hold that), and this
     * file holds the panel's half: state that says which item it belongs to
     * cannot be handed to another one whatever the host's slot key does. `rememberSaveable(itemId)` does not stop that: the `inputs` only
     * decide whether `init()` is eligible to run, and the registry is
     * consulted first under a key derived from the composition **position**.
     * A restored value wins over `init()` even when the input changed. Two
     * of those were sighted on the device — the title opened in edit mode
     * on every item selected after the first, and the details disclosure
     * carried its open/shut state across — after a year of comments here
     * claiming the `(itemId)` input was the fix.
     *
     * So the pin is on the registry key, not on the inputs: **every
     * `rememberSaveable` in this file either names the item in its `key` or
     * is the one deliberate exception.** Written as a sweep rather than as
     * one assertion per site, because the defect is a whole species and a
     * new site added without a key is the same bug again. */
    @Test
    fun `every saveable piece of composition state names its item in the registry key`() {
        // Each `rememberSaveable(` in the file, with whatever it was
        // called with, up to the `{` that opens its `init`.
        val calls = Regex("""rememberSaveable\(([^{]*)\)\s*\{""")
            .findAll(panelSrc)
            .map { it.groupValues[1].replace(Regex("""\s+"""), " ").trim() }
            .toList()
        assertEquals(
            "every rememberSaveable(...) in the pane must be accounted for here — " +
                "a new one is a new leak until its key names the item: $calls",
            3,
            calls.size,
        )
        // The one exception, and the only argument-less form allowed: the
        // discard question. It is not per item on purpose (a question on
        // screen belongs to the gesture that asked it), and it cannot
        // outlive its own dialog — which is modal, so no other item can be
        // selected under it.
        assertEquals(
            "the discard question is the only state here allowed to skip the item",
            1,
            Regex("""rememberSaveable\s*\{""").findAll(panelSrc).count(),
        )
        for (call in calls) {
            val registryKey = Regex("""key = "([^"]*)"""").find(call)?.groupValues?.get(1)
            assertTrue(
                "rememberSaveable($call) must name the item in its registry `key`, " +
                    "not only in its inputs — inputs do not stop a positional restore",
                registryKey != null && registryKey.contains("\$itemId"),
            )
        }
        // And the two whose leak was actually watched happen, by name.
        assertTrue(
            "the details disclosure must carry a per-item key",
            panelSrc.replace(Regex("""\s+"""), " ")
                .contains("var detailsOverride by rememberSaveable( itemId, key = \"details-open-\$itemId\", )"),
        )
        assertTrue(
            "and so must a section's own open/shut state",
            panelSrc.replace(Regex("""\s+"""), " ")
                .contains("key = \"section-open-\$itemId-\$label\","),
        )
    }

    /** Title-edit mode is the one piece of state here that must not
     * survive *anything* — the trap the operator hit, in two halves.
     *
     * It is a mode, not content: the typed title lives in the ViewModel's
     * draft and shows on the title line either way, so reopening a pane
     * with the field shut loses nothing. A saveable flag, per-item key or
     * not, brings a pane closed mid-edit back in edit mode — item A's own
     * restored `true` is still item A's trap. Hence a plain
     * `remember(itemId)`, and hence no `rememberSaveable` may creep back
     * onto it.
     *
     * The other half is the way out. The field used to end only on the
     * IME's Done, so a person who opened it by tapping the title — the
     * pane's own edit affordance, sitting where a tap to close the pane
     * lands — could not leave without committing a title. Back now escapes
     * the field before it reaches the draft's discard question, and
     * discarding shuts the field with the draft it was editing. */
    @Test
    fun `title-edit mode does not persist, and Back is the way out of it`() {
        val body = functionBody(panelSrc, "ItemDetailPanel")
        assertTrue(
            "title-edit mode must be a plain remember, keyed on the item",
            body.contains("var editingTitle by remember(itemId) { mutableStateOf(false) }"),
        )
        assertFalse(
            "and must never become saveable again — a restored mode is the trap",
            body.contains("editingTitle by rememberSaveable"),
        )
        val flat = body.replace(Regex("""\s+"""), " ")
        assertTrue(
            "Back must be handled while the field is open, not only while dirty",
            flat.contains("BackHandler(enabled = editingTitle || viewModel.isDirty)"),
        )
        assertTrue(
            "and it must shut the field first, reaching the discard question only after",
            flat.contains("if (editingTitle) editingTitle = false else confirmingDiscard = true"),
        )
        assertTrue(
            "discarding the draft must shut the field that was editing it",
            flat.contains("viewModel.discardDraft() editingTitle = false"),
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

    /** One action row (operator decision 2026-08-20): the grill, the
     * microtask affordance, the submit and the mark-done check share the
     * pane's last line. They used to occupy three vertical slices — a
     * `ChoiceRow` of grill + submit, the microtask section's own button
     * row, and a row holding nothing but the check — which is three bands
     * of whitespace for four controls.
     *
     * What can drift back, and what each pin catches:
     *
     * - a control on a line of its own again. There is exactly ONE `Row`
     *   under the act row, and all four are inside it.
     * - a printed label on either agent affordance. Four labels do not fit
     *   the 272dp the narrowest host gives the pane — that is measured in
     *   `ItemDetailSubmitRowTest` — so the grill and the microtask run are
     *   `IconButton`s whose words ride `contentDescription`. A `Text(` in
     *   either is the row wrapping again.
     * - the words being lost rather than unprinted. Both are still the
     *   core's own strings, not this surface's inventions.
     * - the submit or the check sliding left as the affordances come and
     *   go: the `weight(1f)` between the two groups is what holds them at
     *   the edge, and a fixed `Arrangement` in its place would not. */
    @Test
    fun `the grill, the microtask run, the submit and the check share one row`() {
        val row = functionBody(panelSrc, "DetailBody")
            .substringAfter("var grain by rememberSaveable(itemId)")
        assertTrue(
            "the action row must be one Row, laid out full width",
            row.replace(Regex("""\s+"""), " ").contains(
                "Row( modifier = Modifier.fillMaxWidth(), " +
                    "verticalAlignment = Alignment.CenterVertically,",
            ),
        )
        assertEquals(
            "and it must be the only row after it — a second is a control back on a line of its own",
            1,
            Regex("""(?m)^    Row\(""").findAll(row).count(),
        )
        for (control in listOf(
            "onClick = onGrill",
            "onMicrotaskRun(false, null)",
            "Button(onClick = onSubmit, enabled = canSave)",
            "IconButton(onClick = onComplete)",
        )) {
            assertTrue("the action row must carry $control", row.contains(control))
        }
        assertTrue(
            "the submit and the check must be held at the right edge by the weight, " +
                "not by an arrangement that moves with the button count",
            row.contains("Spacer(Modifier.weight(1f))"),
        )
        // The two agent affordances are icon-only, and the words are the
        // core's — `itemGrillButtonLabel` is shared verbatim with the web,
        // and the microtask label is the affordance's applied count.
        assertTrue(
            "the grill must speak its label through the icon's accessible name",
            row.replace(Regex("""\s+"""), " ").contains(
                "contentDescription = itemGrillButtonLabel(hasGrillDraft),",
            ),
        )
        assertTrue(
            "and so must the microtask run",
            row.replace(Regex("""\s+"""), " ").contains(
                "contentDescription = microtaskLabel(affordance),",
            ),
        )
        assertFalse(
            "neither may print a label — a third word does not fit the row",
            row.contains("Text(itemGrillButtonLabel") || row.contains("Text(microtaskLabel"),
        )
        // The microtask's answer still renders, above the row: a stream of
        // narration must not push the controls down the pane.
        val body = functionBody(panelSrc, "DetailBody")
        assertTrue(
            "the run's narration must still render",
            body.contains("MicrotaskNarration("),
        )
        assertTrue(
            "and above the action row, not below it",
            body.indexOf("MicrotaskNarration(") < body.indexOf("var grain by rememberSaveable"),
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
            "its state must be keyed on the item like every other piece here — " +
                "in the registry key, which is the half that counts (see the sweep above)",
            body.replace(Regex("""\s+"""), " ")
                .contains("""var detailsOverride by rememberSaveable( itemId, key = "details-open-${'$'}itemId", )"""),
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
