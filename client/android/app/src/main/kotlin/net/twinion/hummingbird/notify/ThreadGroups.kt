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
     * notification. Only summaries this file tagged are ever returned, so a
     * summary some other feature posts is never touched. */
    fun orphanedSummaries(active: List<Posted>): List<Posted> {
        val grouped = active.filter { !it.isSummary }.mapNotNull { it.group }.toSet()
        return active.filter {
            it.isSummary && it.tag?.startsWith(SUMMARY_TAG_PREFIX) == true && it.group !in grouped
        }
    }
}
