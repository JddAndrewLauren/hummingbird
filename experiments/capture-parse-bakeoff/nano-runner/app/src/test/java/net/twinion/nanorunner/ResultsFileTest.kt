package net.twinion.nanorunner

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

class ResultsFileTest {

    @get:Rule val tmp = TemporaryFolder()

    private fun file(contents: String): File =
        tmp.newFile().apply { writeText(contents) }

    @Test
    fun `absent file has no ids`() {
        val f = File(tmp.root, "nope.jsonl")
        assertEquals(emptyList<String>(), ResultsFile(f).use { it.repairAndScanIds() })
    }

    @Test
    fun `scans ids in order, errored rows included`() {
        val f = file(
            """
            {"id":"a","parse":{"title":"x"}}
            {"id":"b","error":"boom","raw_output":null}
            {"id":"c","parse":{"title":"y"}}
            """.trimIndent() + "\n"
        )
        assertEquals(listOf("a", "b", "c"), ResultsFile(f).use { it.repairAndScanIds() })
    }

    @Test
    fun `torn final line is truncated away`() {
        val f = file("{\"id\":\"a\",\"parse\":{\"title\":\"x\"}}\n{\"id\":\"b\",\"par")
        val ids = ResultsFile(f).use { it.repairAndScanIds() }
        assertEquals(listOf("a"), ids)
        assertEquals("{\"id\":\"a\",\"parse\":{\"title\":\"x\"}}\n", f.readText())
    }

    @Test
    fun `complete final line missing only its newline is kept, not re-run`() {
        val f = file("{\"id\":\"a\",\"parse\":{\"title\":\"x\"}}\n{\"id\":\"b\",\"error\":\"e\",\"raw_output\":null}")
        val ids = ResultsFile(f).use { it.repairAndScanIds() }
        assertEquals(listOf("a", "b"), ids)
        assertTrue("the newline should have been repaired", f.readText().endsWith("}\n"))
    }

    @Test(expected = CorruptResultsException::class)
    fun `unparseable line in the middle refuses the run`() {
        val f = file("{\"id\":\"a\",\"parse\":{}}\nnot json at all\n{\"id\":\"c\",\"parse\":{}}\n")
        ResultsFile(f).use { it.repairAndScanIds() }
    }

    @Test
    fun `append is durable and round-trips`() {
        val f = File(tmp.root, "out.jsonl")
        ResultsFile(f).use {
            it.appendRow(Rows.successRow("a", Rows.parseStrictObject("{\"title\":\"x\"}")!!))
            it.appendRow(Rows.errorRow("b", "boom", "<not json>"))
        }
        assertEquals(2, f.readLines().size)
        assertEquals(listOf("a", "b"), ResultsFile(f).use { it.repairAndScanIds() })
        assertTrue(f.readText().contains("\"raw_output\":\"<not json>\""))
    }

    @Test
    fun `resume appends only the missing ids`() {
        val f = File(tmp.root, "out.jsonl")
        ResultsFile(f).use { it.appendRow(Rows.errorRow("a", "boom", null)) }
        val seen = ResultsFile(f).use { rf ->
            val done = rf.repairAndScanIds()
            rf.appendRow(Rows.errorRow("b", "boom", null))
            done
        }
        assertEquals(listOf("a"), seen)
        assertEquals(listOf("a", "b"), ResultsFile(f).use { it.repairAndScanIds() })
    }
}
