package net.twinion.hummingbird.notify

// Which thread group summaries are left standing with nothing under them
// (#922, ADR-0042). Pure, so the rule is testable without a notification
// manager; [AlertNotifier] feeds it what is posted and cancels what it names.
//
// Android does not retire a group summary when the app cancels the last
// notification in its group — only the user's own swipe does that — so the
// app has to. "The summary goes away when its last alert is cancelled or
// acked" is this function, run after every cancel and every post.
object ThreadGroups {

    /** The tag a thread's summary posts under. Distinct from any alert id, so
     * a summary can never replace (or be replaced by) an alert's own
     * notification, and a tag prefix says whose it is. */
    private const val SUMMARY_TAG_PREFIX = "thread:"

    fun summaryTag(thread: String): String = SUMMARY_TAG_PREFIX + thread

    /** One notification the app has posted, as the platform reports it. */
    data class Posted(
        val tag: String?,
        val id: Int,
        /** The notification's group key, null when ungrouped. */
        val group: String?,
        val isSummary: Boolean,
    )

    /** The thread summaries in [active] whose group holds no other
     * notification, not counting any alert in [cancelling]: an alert whose
     * cancel is in flight is gone as far as its group is concerned, even
     * when the platform's snapshot still lists it. Only summaries this file
     * tagged are ever returned, so a summary some other feature posts is
     * never touched. */
    fun orphanedSummaries(active: List<Posted>, cancelling: Set<String> = emptySet()): List<Posted> {
        val grouped = active.filter { !it.isSummary && it.tag !in cancelling }
            .mapNotNull { it.group }.toSet()
        return active.filter {
            it.isSummary && it.tag?.startsWith(SUMMARY_TAG_PREFIX) == true && it.group !in grouped
        }
    }
}

/** The summary bookkeeping's one piece of state: the alerts being cancelled.
 *
 * Two Ack workers can finish for a thread's last two alerts at once. Each
 * reads its own snapshot of what is posted, and a snapshot taken before the
 * other's cancel landed still lists the other alert — so if each excluded
 * only its own, each would see a surviving child and the empty summary would
 * stay for good. So the ids stay in [cancelling] until a snapshot no longer
 * lists them (the platform has caught up), every pass excludes the whole
 * set, and [AlertNotifier] runs each pass under this object's monitor so two
 * passes never interleave. */
internal class SummaryBookkeeping {
    private val cancelling = mutableSetOf<String>()

    /** [alertId] is being cancelled; returns the summaries left empty, given
     * the platform's [active] snapshot (which may still list it). */
    @Synchronized
    fun cancelled(alertId: String, active: List<ThreadGroups.Posted>): List<ThreadGroups.Posted> {
        cancelling += alertId
        return sweep(active)
    }

    /** [alertId] is posted (again): no longer being cancelled, so a re-raise
     * after an ack counts as a child again. */
    @Synchronized
    fun posted(alertId: String) {
        cancelling -= alertId
    }

    /** The empty summaries in [active], excluding every alert still being
     * cancelled, then forgets the ids the snapshot confirms are gone. */
    @Synchronized
    fun sweep(active: List<ThreadGroups.Posted>): List<ThreadGroups.Posted> {
        val orphans = ThreadGroups.orphanedSummaries(active, cancelling)
        cancelling.retainAll(active.mapNotNull { it.tag }.toSet())
        return orphans
    }
}
