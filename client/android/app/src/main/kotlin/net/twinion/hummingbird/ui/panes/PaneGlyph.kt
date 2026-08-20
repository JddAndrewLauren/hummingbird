package net.twinion.hummingbird.ui.panes

import androidx.compose.ui.graphics.Color

/** One small mark a pane puts on its collapsed row — a coloured dot (a
 * kerbside bin) or a vendored Lucide icon — `PaneGlyph` in the web's
 * `questions/contract.ts`, ported. `label` is mandatory on both arms and is
 * not decoration: a glyph carries meaning colour alone cannot convey to
 * TalkBack, and a dot with no accessible name is a blank box. The shell
 * renders these; a pane only says which. */
internal sealed interface PaneGlyph {
    val label: String

    data class Dot(val fill: Color, val edge: Color, override val label: String) : PaneGlyph

    data class Icon(val iconRes: Int, override val label: String) : PaneGlyph
}

/** How many glyphs a collapsed row may carry — the web contract's own cap,
 * applied by the shell (`PaneShell.kt`), never trusted to the pane: the cap
 * exists to protect the row from the pane rather than to be honoured by
 * it. */
internal const val MAX_GLYPHS = 4
