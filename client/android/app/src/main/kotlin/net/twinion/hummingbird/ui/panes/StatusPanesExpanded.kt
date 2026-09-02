package net.twinion.hummingbird.ui.panes

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import net.twinion.hummingbird.ui.theme.Amber600
import net.twinion.hummingbird.ui.theme.LocalHbDark
import net.twinion.hummingbird.ui.theme.StatusWarnFgDark
import uniffi.hummingbird_ffi_mobile.MobileKimiGap
import uniffi.hummingbird_ffi_mobile.MobileKimiResolved
import uniffi.hummingbird_ffi_mobile.MobilePaneBand
import uniffi.hummingbird_ffi_mobile.MobilePaneFacts
import uniffi.hummingbird_ffi_mobile.MobilePaneFreshness
import uniffi.hummingbird_ffi_mobile.MobilePollerResolved
import uniffi.hummingbird_ffi_mobile.MobileProbeExpected
import uniffi.hummingbird_ffi_mobile.MobileProbeGap
import uniffi.hummingbird_ffi_mobile.MobileProbeResolved
import uniffi.hummingbird_ffi_mobile.MobileRankedPane
import uniffi.hummingbird_ffi_mobile.MobileReachabilityFacts
import uniffi.hummingbird_ffi_mobile.MobileWorkflowGap
import uniffi.hummingbird_ffi_mobile.MobileWorkflowResolved

// The Status four's expanded renderings (the pane-content slice) — each
// web `*PaneExpanded.tsx`, ported: a headline first, the supporting detail
// below it, freshness last, and a gap is words on screen, never a blank
// card. The shell (`PaneShell.kt`) already draws the collapsed row, the
// expanded header, the one-line headline and the unbound setup door;
// these composables fill the `expandedContent` slot under them, so what
// they add is the detail the collapsed sentence had to leave out.
//
// **Nothing here decides.** The band, answer state and facts arrive on the
// pane; the one banding-adjacent choice — reading an imminent-and-stale
// github/uptime pane as stale rather than stalled — is `PaneAnswers.kt`'s
// own recorded deviation, applied identically so the card can never
// contradict its collapsed row. The web's Badge chips ("cron stalled",
// "cash owed") render as coloured meta words: Android's design port has no
// Badge composable, and a coloured `labelSmall` says the same fact.
//
// Exhaustive `when`s with no `else` arm throughout — the house drift gate.

/** `--status-warn-fg`, the pair `SettingsScreen.kt:494` reads. */
@Composable
internal fun warnColor(): Color = if (LocalHbDark.current) StatusWarnFgDark else Amber600

/** The web's shared stale caveat line — "stale — as of Nh ago", or the
 * honest no-number arm (`WastePaneExpanded.tsx`'s own note: an unknown age
 * has no hours to name, so it says that instead of fabricating one). */
internal fun staleWords(freshness: MobilePaneFreshness): String = when (freshness) {
    is MobilePaneFreshness.Age -> "stale — as of ${freshness.ageMs / 3_600_000}h ago"
    MobilePaneFreshness.Unknown -> "stale — age unknown"
}

@Composable
internal fun StaleLine(freshness: MobilePaneFreshness) {
    Text(
        staleWords(freshness),
        style = MaterialTheme.typography.labelSmall,
        color = warnColor(),
    )
}

@Composable
private fun GapBody(reason: String) {
    Text(
        reason,
        style = MaterialTheme.typography.bodyMedium,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
}

/** The Status surface's `expandedContent` — one dispatcher, exhaustive
 * over every facts arm the way `StatusScreen.kt`'s `paneLabel` is, so a
 * Now-surface question reaching this slot is a loud error, never a blank
 * expansion. */
/** @param headline whether this content draws the pane's own headline.
 *
 * Both callers pass `false` today — the quiet stack's announcing card and
 * its open chip each draw `paneHeadline` themselves, beside the pane's icon
 * and its band word, so a body that drew its own would say the same sentence
 * twice two lines apart. The parameter is kept rather than inlined because
 * it is what makes that split explicit at the call site; there is no default,
 * so a third host has to choose rather than inherit. */
@Composable
internal fun StatusPaneExpanded(pane: MobileRankedPane, nowMs: Long, headline: Boolean) {
    when (val facts = pane.facts) {
        is MobilePaneFacts.Kimi -> KimiPaneExpanded(pane, facts.resolved, headline)
        is MobilePaneFacts.Github -> GithubPaneExpanded(pane, facts.resolved, nowMs, headline)
        is MobilePaneFacts.Uptime -> UptimePaneExpanded(pane, facts.resolved, headline)
        is MobilePaneFacts.Reachability -> ReachabilityPaneExpanded(facts.facts, headline)
        is MobilePaneFacts.Poller -> PollerPaneExpanded(pane, facts.resolved, headline)
        is MobilePaneFacts.Homework,
        is MobilePaneFacts.Scps,
        is MobilePaneFacts.Waste,
        is MobilePaneFacts.Weekend,
        is MobilePaneFacts.Vacation,
        is MobilePaneFacts.Race ->
            error("a Now-surface question reached the Status expanded slot: ${pane.standingQuestion}")
    }
}

// ------------------------------------------------------------------ kimi

/** `gapReason` in `kimi.ts`, ported per kind. */
internal fun kimiGapReason(gap: MobileKimiGap): String = when (gap) {
    MobileKimiGap.NotFetched -> "No balance has been fetched yet."
    is MobileKimiGap.Malformed -> "The balance payload couldn't be read: ${gap.reason}"
    is MobileKimiGap.UnknownSchema ->
        "This device doesn't know how to read ${gap.schema} yet. Update the app."
    MobileKimiGap.NotJson -> "The balance payload isn't JSON."
    MobileKimiGap.NotAnObject -> "The balance payload isn't an object."
    MobileKimiGap.BadNumbers -> "The balance payload's numbers can't be read."
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun KimiPaneExpanded(
    pane: MobileRankedPane,
    resolved: MobileKimiResolved,
    headline: Boolean,
) {
    // No setup arm (`KimiPaneExpanded.tsx`'s own note): this question has
    // no per-device binding to point Settings at, so "never polled yet" is
    // the whole story, whatever the answer state says.
    val facts = when (resolved) {
        is MobileKimiResolved.Gap -> {
            GapBody(kimiGapReason(resolved.gap))
            return
        }
        is MobileKimiResolved.Facts -> resolved.facts
    }
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        if (headline) {
            Text(
                "${formatUsd(facts.availableBalance)} left",
                style = MaterialTheme.typography.headlineSmall,
                color = when (pane.answer.band) {
                    MobilePaneBand.LIVE -> MaterialTheme.colorScheme.error
                    MobilePaneBand.IMMINENT -> MaterialTheme.colorScheme.error
                    MobilePaneBand.NEAR -> warnColor()
                    MobilePaneBand.DISTANT -> MaterialTheme.colorScheme.onSurface
                    MobilePaneBand.DORMANT -> MaterialTheme.colorScheme.onSurface
                },
            )
        }
        FlowRow(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            Text(
                "voucher ${formatUsd(facts.voucherBalance)}",
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            Text(
                "cash ${formatUsd(facts.cashBalance)}",
                style = MaterialTheme.typography.labelSmall,
                color = if (facts.cashBalance < 0) {
                    MaterialTheme.colorScheme.error
                } else {
                    MaterialTheme.colorScheme.onSurfaceVariant
                },
            )
            // The fact the ADR names explicitly: a positive available
            // balance can hide a negative cash position — the account owes,
            // even while the headline number is still positive.
            if (facts.cashBalance < 0) {
                Text(
                    "cash owed",
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.error,
                )
            }
        }
        if (facts.stale) {
            StaleLine(facts.freshness)
        }
    }
}

// ---------------------------------------------------------------- github

/** `gapReason` in `github.ts`, ported per kind. */
internal fun githubGapReason(gap: MobileWorkflowGap): String = when (gap) {
    MobileWorkflowGap.NotFetched -> "No answer has been fetched yet."
    is MobileWorkflowGap.Malformed -> "The workflow payload couldn't be read: ${gap.reason}"
    is MobileWorkflowGap.UnknownSchema ->
        "This device doesn't know how to read ${gap.schema} yet. Update the app."
    MobileWorkflowGap.NotJson -> "The workflow payload isn't JSON."
    MobileWorkflowGap.NotAnObject -> "The workflow payload isn't an object."
    MobileWorkflowGap.UnreadableFields -> "The workflow payload's fields can't be read."
}

/** The last-run meta line — `GithubPaneExpanded.tsx`'s own sentence. The
 * mobile mirror's `lastRunEvent` is optional where the web assumes it; an
 * absent event drops its parenthetical rather than printing a null. */
internal fun githubLastRunWords(
    lastRunAtMs: Long?,
    conclusion: String?,
    event: String?,
    nowMs: Long,
): String {
    if (lastRunAtMs == null) return "never run"
    val eventPart = event?.let { " ($it)" } ?: ""
    return "last run ${conclusion ?: "in progress"}$eventPart, ${ageWords(nowMs - lastRunAtMs)}"
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun GithubPaneExpanded(
    pane: MobileRankedPane,
    resolved: MobileWorkflowResolved,
    nowMs: Long,
    headline: Boolean,
) {
    val view = when (resolved) {
        is MobileWorkflowResolved.Gap -> {
            GapBody(githubGapReason(resolved.gap))
            return
        }
        is MobileWorkflowResolved.View -> resolved.view
    }
    val body = view.body
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        if (headline) {
            Text(
                body.displayName,
                style = MaterialTheme.typography.titleLarge,
                color = when (pane.answer.band) {
                    MobilePaneBand.LIVE -> MaterialTheme.colorScheme.error
                    // The recorded deviation (`PaneAnswers.kt`'s header):
                    // imminent-and-stale reads as stale, not stalled.
                    MobilePaneBand.IMMINENT ->
                        if (view.stale) warnColor() else MaterialTheme.colorScheme.error
                    MobilePaneBand.NEAR -> warnColor()
                    MobilePaneBand.DISTANT -> warnColor()
                    MobilePaneBand.DORMANT -> MaterialTheme.colorScheme.onSurface
                },
            )
        }
        FlowRow(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            Text(
                githubLastRunWords(
                    body.lastRunAtMs,
                    body.lastRunConclusion,
                    body.lastRunEvent,
                    nowMs,
                ),
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            val lastOk = body.lastScheduledSuccessAtMs
            Text(
                if (lastOk == null) {
                    "no scheduled success on record"
                } else {
                    "last scheduled success ${ageWords(nowMs - lastOk)}"
                },
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            when (pane.answer.band) {
                MobilePaneBand.LIVE -> Text(
                    "cron stalled",
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.error,
                )
                MobilePaneBand.IMMINENT -> if (!view.stale) {
                    Text(
                        "cron stalled",
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.error,
                    )
                }
                MobilePaneBand.NEAR,
                MobilePaneBand.DISTANT,
                MobilePaneBand.DORMANT -> if (
                    body.declaredCadenceMs == null || pane.answer.band == MobilePaneBand.DISTANT
                ) {
                    // The overdue judgement could not be made — the card
                    // must say so, not present two green facts unwarned.
                    // Two ways in: an unreadable cadence, or a row whose
                    // observation instant could not be located at all
                    // (`github.rs`'s `observed_at_ms`).
                    Text(
                        if (body.declaredCadenceMs == null) "cadence unreadable" else "observed when unknown",
                        style = MaterialTheme.typography.labelSmall,
                        color = warnColor(),
                    )
                }
            }
        }
        if (view.stale) {
            StaleLine(view.freshness)
        }
    }
}

// ---------------------------------------------------------------- uptime

/** `gapReason` in `uptime.ts`, ported per kind. */
internal fun uptimeGapReason(gap: MobileProbeGap): String = when (gap) {
    MobileProbeGap.NotFetched -> "No answer has been fetched yet."
    is MobileProbeGap.Malformed -> "The probe payload couldn't be read: ${gap.reason}"
    is MobileProbeGap.UnknownSchema ->
        "This device doesn't know how to read ${gap.schema} yet. Update the app."
    MobileProbeGap.NotJson -> "The probe payload isn't JSON."
    MobileProbeGap.NotAnObject -> "The probe payload isn't an object."
    MobileProbeGap.FieldsUnreadable -> "The probe payload's fields can't be read."
    MobileProbeGap.ObservationUnreadable -> "The probe payload's observation can't be read."
}

/** The observation meta line — `UptimePaneExpanded.tsx`'s own sentence.
 * The mobile mirror's `observedStatus` is optional where the web assumes
 * it; an absent status says so rather than printing a null. */
internal fun uptimeObservationWords(
    error: String?,
    observedStatus: Long?,
    expectStatus: Long,
): String {
    if (error != null) return "unreachable — $error"
    if (observedStatus == null) return "no status recorded (wanted $expectStatus)"
    return "answered $observedStatus (wanted $expectStatus)"
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun UptimePaneExpanded(
    pane: MobileRankedPane,
    resolved: MobileProbeResolved,
    headline: Boolean,
) {
    val facts = when (resolved) {
        is MobileProbeResolved.Gap -> {
            GapBody(uptimeGapReason(resolved.gap))
            return
        }
        is MobileProbeResolved.Facts -> resolved.facts
    }
    val body = facts.body
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        if (headline) {
            Text(
                facts.serviceId,
                style = MaterialTheme.typography.titleLarge,
                color = when (pane.answer.band) {
                    MobilePaneBand.LIVE -> MaterialTheme.colorScheme.error
                    MobilePaneBand.NEAR -> warnColor()
                    // Imminence for this question only ever comes from
                    // staleness (`uptime.ts`'s own note) — the stale line below
                    // carries it, so the name stays undramatised.
                    MobilePaneBand.IMMINENT -> MaterialTheme.colorScheme.onSurface
                    MobilePaneBand.DISTANT -> MaterialTheme.colorScheme.onSurface
                    MobilePaneBand.DORMANT -> MaterialTheme.colorScheme.onSurface
                },
            )
        }
        FlowRow(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            Text(
                "expected ${
                    when (body.expected) {
                        MobileProbeExpected.ON -> "on"
                        MobileProbeExpected.OFF -> "off"
                    }
                }",
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            Text(
                uptimeObservationWords(body.error, body.observedStatus, body.expectStatus),
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            if (body.expected == MobileProbeExpected.OFF && body.error == null) {
                Text(
                    "reachable when it should be off",
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.error,
                )
            } else if (body.expected == MobileProbeExpected.ON && body.error != null) {
                Text(
                    "unreachable",
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.error,
                )
            } else if (body.expected == MobileProbeExpected.ON &&
                body.observedStatus != body.expectStatus
            ) {
                Text(
                    "unexpected status",
                    style = MaterialTheme.typography.labelSmall,
                    color = warnColor(),
                )
            }
        }
        if (facts.stale) {
            StaleLine(facts.freshness)
        }
    }
}

// ----------------------------------------------------------------- poller

/** `PollerPaneBody` in `PollerPaneExpanded.tsx`, ported: a headline first
 * (the source itself — this pane touches no body, so there is no separate
 * name to draw), then the raw age and the declared cadence, no stale line
 * of its own (unlike its siblings, this pane's whole answer already IS a
 * freshness judgement — `poller.rs`'s own header). */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun PollerPaneExpanded(
    pane: MobileRankedPane,
    resolved: MobilePollerResolved,
    headline: Boolean,
) {
    val facts = when (resolved) {
        is MobilePollerResolved.Gap -> {
            GapBody("No answer has been fetched yet.")
            return
        }
        is MobilePollerResolved.Facts -> resolved.facts
    }
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        if (headline) {
            Text(
                pane.subjectKey,
                style = MaterialTheme.typography.titleLarge,
                color = when (facts.band) {
                    MobilePaneBand.IMMINENT -> MaterialTheme.colorScheme.error
                    MobilePaneBand.DISTANT -> warnColor()
                    MobilePaneBand.LIVE,
                    MobilePaneBand.NEAR,
                    MobilePaneBand.DORMANT -> MaterialTheme.colorScheme.onSurface
                },
            )
        }
        FlowRow(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            Text(
                when (val freshness = facts.freshness) {
                    is MobilePaneFreshness.Age -> "as of ${freshness.ageMs / 60_000}m ago"
                    MobilePaneFreshness.Unknown -> "age unknown"
                },
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            Text(
                when (val freshness = facts.freshness) {
                    is MobilePaneFreshness.Age ->
                        freshness.declaredCadenceMs?.let { "declared cadence ${it / 60_000}m" }
                            ?: "cadence unreadable"
                    MobilePaneFreshness.Unknown -> "cadence unreadable"
                },
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

// ---------------------------------------------------------- reachability

@Composable
private fun ReachabilityPaneExpanded(facts: MobileReachabilityFacts?, headline: Boolean) {
    if (facts == null) {
        // The shell's headline already says "Never synced on this device."
        // — this is the web card's body sentence under it.
        GapBody("No successful authority sync is recorded for this device.")
        return
    }
    // This pane's whole answer IS its headline — there is no supporting
    // detail under it, so when the card draws the headline this body has
    // nothing left to say and says nothing rather than repeating it.
    if (!headline) return
    Text(
        reachabilityHeadline(
            ReachabilityWords(facts.ageMs, facts.stale, facts.latestAttemptLanded),
        ),
        style = MaterialTheme.typography.titleLarge,
        color = if (facts.stale) {
            MaterialTheme.colorScheme.error
        } else {
            MaterialTheme.colorScheme.onSurface
        },
    )
}
