package net.twinion.nanorunner

import kotlinx.coroutines.CancellationException

/**
 * The run itself, with no Android in it so the resume/append/error semantics can be
 * tested on the JVM rather than argued about.
 *
 * Corpus order, each id exactly once, one fsync'd row per capture before the next one
 * starts. An engine failure is a RESULT — recorded and moved past — because "how often
 * does on-device inference fail" is one of the questions the bake-off is asking.
 */
class RunLoop(
    private val engine: NanoEngine,
    private val captures: List<Capture>,
    private val template: String,
    private val schemaText: String,
    private val results: ResultsFile,
    private val raw: ResultsFile,
    private val clock: () -> Long = System::currentTimeMillis,
) {

    data class Progress(val done: Int, val total: Int, val errors: Int)

    data class Summary(
        val total: Int,
        val ok: Int,
        val errors: Int,
        val failedIds: List<String>,
        val skipped: Int,
    )

    suspend fun run(onProgress: (Progress) -> Unit = {}): Summary {
        val alreadyDone = results.repairAndScanIds().toMutableSet()
        raw.repairAndScanIds() // keep the audit sidecar's tail sane too
        val skipped = alreadyDone.size

        var ok = 0
        var errors = 0
        val failed = mutableListOf<String>()
        onProgress(Progress(alreadyDone.size, captures.size, 0))

        for (capture in captures) {
            if (capture.id in alreadyDone) continue

            val prompt = PromptAssembly.assembleFromFiles(template, schemaText, capture.raw)
            val t0 = clock()
            var output: String? = null
            val row: String = try {
                output = engine.generate(prompt)
                val parsed = Rows.parseStrictObject(output)
                if (parsed != null) {
                    ok++
                    Rows.successRow(capture.id, parsed)
                } else {
                    errors++
                    failed += capture.id
                    Rows.errorRow(capture.id, Rows.NOT_A_JSON_OBJECT, output)
                }
            } catch (e: CancellationException) {
                throw e // the user left / the scope died — not a model failure, don't record one
            } catch (t: Throwable) {
                errors++
                failed += capture.id
                Rows.errorRow(capture.id, Rows.describe(t), output)
            }

            // Sidecar first: if we die between the two writes, the audit trail is the
            // superset, never the other way round.
            raw.appendRow(Rows.rawRow(capture.id, output, clock() - t0))
            results.appendRow(row)
            alreadyDone += capture.id
            onProgress(Progress(alreadyDone.size, captures.size, errors))
        }

        return Summary(captures.size, ok, errors, failed, skipped)
    }
}
