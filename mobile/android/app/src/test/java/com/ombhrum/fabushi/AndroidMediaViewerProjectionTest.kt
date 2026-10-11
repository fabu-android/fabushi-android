package com.ombhrum.fabushi

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.ByteArrayOutputStream
import java.util.zip.ZipEntry
import java.util.zip.ZipOutputStream

class AndroidMediaViewerProjectionTest {
    private fun message(
        id: String,
        type: String,
        mimeType: String? = null,
        fileName: String? = null,
    ): ChatMessage =
        ChatMessage(
            id = id,
            conversationId = "conversation",
            text = id,
            contentType = type,
            outgoing = false,
            time = "",
            mediaMimeType = mimeType,
            mediaFileName = fileName,
        )

    @Test
    fun previewableStartBuildsPhotoVideoGalleryInTranscriptOrder() {
        val messages = listOf(
            message("text", "text"),
            message("photo", "photo"),
            message("document", "document"),
            message("video", "video"),
        )
        val gallery = projectAndroidMediaGallery(messages, "video")
        assertEquals(listOf("photo", "video"), gallery.map { it.id })
        assertEquals(1, initialAndroidMediaGalleryIndex(gallery, "video"))
    }

    @Test
    fun nonPreviewableStartStaysSingleFileViewer() {
        val messages = listOf(message("photo", "photo"), message("document", "document"))
        val gallery = projectAndroidMediaGallery(messages, "document")
        assertEquals(listOf("document"), gallery.map { it.id })
    }

    @Test
    fun galleryNavigationWrapsBothDirections() {
        assertEquals(0, stepAndroidMediaGalleryIndex(2, 1, 3))
        assertEquals(2, stepAndroidMediaGalleryIndex(0, -1, 3))
        assertEquals(1, stepAndroidMediaGalleryIndex(0, 1, 3))
        assertEquals(-1, stepAndroidMediaGalleryIndex(0, 1, 0))
    }

    @Test
    fun missingStartDoesNotInventGallery() {
        assertTrue(projectAndroidMediaGallery(listOf(message("photo", "photo")), "missing").isEmpty())
    }
    @Test
    fun pdfDetectionUsesMimeOrFileNameWithoutJoiningPhotoVideoGallery() {
        val byMime = message("pdf-mime", "document", mimeType = "application/pdf; charset=binary")
        val byName = message("pdf-name", "document", fileName = "report.PDF")
        assertTrue(isAndroidPdfMessage(byMime))
        assertTrue(isAndroidPdfMessage(byName))
        assertEquals(
            listOf("pdf-mime"),
            projectAndroidMediaGallery(listOf(byMime), "pdf-mime").map { it.id },
        )
    }

    @Test
    fun pdfPageNavigationClampsAtDocumentEdges() {
        assertEquals(0, stepAndroidPdfPageIndex(0, -1, 4))
        assertEquals(1, stepAndroidPdfPageIndex(0, 1, 4))
        assertEquals(3, stepAndroidPdfPageIndex(3, 1, 4))
        assertEquals(-1, stepAndroidPdfPageIndex(0, 1, 0))
    }

    @Test
    fun pdfZoomMatchesDesktopBounds() {
        assertEquals(0.5f, clampAndroidPdfZoom(0.1f))
        assertEquals(1f, clampAndroidPdfZoom(1f))
        assertEquals(4f, clampAndroidPdfZoom(9f))
    }

    @Test
    fun delimitedSpreadsheetDetectionUsesExtensionOrMime() {
        assertTrue(isAndroidDelimitedSpreadsheet("report.CSV", null))
        assertTrue(isAndroidDelimitedSpreadsheet("report.tsv", "application/octet-stream"))
        assertTrue(isAndroidDelimitedSpreadsheet("report.bin", "text/csv; charset=utf-8"))
        assertEquals(
            false,
            isAndroidDelimitedSpreadsheet(
                "report.xlsx",
                "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            ),
        )
    }

    @Test
    fun csvParserPreservesQuotedCommasEscapedQuotesAndTotalRows() {
        val sheet = parseAndroidDelimitedSpreadsheet(
            "name,note\nAlice,\"hello, world\"\nBob,\"said \"\"hi\"\"\"".toByteArray(),
            "people.csv",
        )
        assertEquals(3, sheet.totalRows)
        assertEquals(listOf("name", "note"), sheet.rows[0])
        assertEquals(listOf("Alice", "hello, world"), sheet.rows[1])
        assertEquals(listOf("Bob", "said \"hi\""), sheet.rows[2])
    }

    @Test
    fun spreadsheetParserTruncatesProjectionButCountsFullRows() {
        val input = buildString {
            append("header\n")
            repeat(10) { index -> append("row-").append(index).append('\n') }
        }
        val sheet = parseAndroidDelimitedSpreadsheet(input.toByteArray(), "rows.csv", maxRows = 3)
        assertEquals(3, sheet.rows.size)
        assertEquals(11, sheet.totalRows)
    }

    private fun workbookZip(entries: Map<String, String>): ByteArray {
        val output = ByteArrayOutputStream()
        ZipOutputStream(output).use { zip ->
            entries.forEach { (name, body) ->
                zip.putNextEntry(ZipEntry(name))
                zip.write(body.toByteArray())
                zip.closeEntry()
            }
        }
        return output.toByteArray()
    }

    private fun twoSheetWorkbook(): ByteArray = workbookZip(
        mapOf(
            "xl/workbook.xml" to """
                <workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"
                    xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
                  <sheets>
                    <sheet name="People" sheetId="1" r:id="rId1"/>
                    <sheet name="Flags" sheetId="2" r:id="rId2"/>
                  </sheets>
                </workbook>
            """.trimIndent(),
            "xl/_rels/workbook.xml.rels" to """
                <Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
                  <Relationship Id="rId1" Type="worksheet" Target="worksheets/sheet1.xml"/>
                  <Relationship Id="rId2" Type="worksheet" Target="worksheets/sheet2.xml"/>
                </Relationships>
            """.trimIndent(),
            "xl/sharedStrings.xml" to """
                <sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
                  <si><t>Name</t></si>
                  <si><t>Alice</t></si>
                </sst>
            """.trimIndent(),
            "xl/worksheets/sheet1.xml" to """
                <worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
                  <sheetData>
                    <row r="1">
                      <c r="A1" t="s"><v>0</v></c>
                      <c r="B1" t="inlineStr"><is><t>Score</t></is></c>
                    </row>
                    <row r="2">
                      <c r="A2" t="s"><v>1</v></c>
                      <c r="B2"><v>42</v></c>
                    </row>
                  </sheetData>
                </worksheet>
            """.trimIndent(),
            "xl/worksheets/sheet2.xml" to """
                <worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
                  <sheetData>
                    <row r="1"><c r="A1" t="str"><v>Enabled</v></c></row>
                    <row r="2"><c r="A2" t="b"><v>1</v></c></row>
                  </sheetData>
                </worksheet>
            """.trimIndent(),
        ),
    )

    @Test
    fun openXmlSpreadsheetDetectionMatchesDesktopXlsxSurface() {
        assertTrue(isAndroidOpenXmlSpreadsheet("report.XLSX", null))
        assertTrue(
            isAndroidOpenXmlSpreadsheet(
                "report.bin",
                "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            ),
        )
        assertEquals(false, isAndroidOpenXmlSpreadsheet("legacy.xls", "application/vnd.ms-excel"))
    }

    @Test
    fun legacyXlsUsesExplicitReadOnlyExternalViewerReplacement() {
        assertTrue(isAndroidLegacyBinarySpreadsheet("legacy.XLS", null))
        assertTrue(
            isAndroidLegacyBinarySpreadsheet(
                "legacy.bin",
                "application/vnd.ms-excel; charset=binary",
            ),
        )
        assertEquals(
            "application/vnd.ms-excel",
            androidLegacySpreadsheetOpenMimeType("application/octet-stream"),
        )
        assertEquals(
            "application/vnd.ms-excel",
            androidLegacySpreadsheetOpenMimeType(null),
        )
        assertEquals(
            "application/vnd.ms-excel",
            androidLegacySpreadsheetOpenMimeType("application/vnd.ms-excel"),
        )
        assertEquals(false, isAndroidSpreadsheet("legacy.xls", "application/vnd.ms-excel"))
    }

    @Test
    fun xlsxParserProjectsMultipleSheetsSharedStringsAndTypedCells() {
        val sheets = parseAndroidSpreadsheet(
            bytes = twoSheetWorkbook(),
            fileName = "report.xlsx",
            mimeType = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        )
        assertEquals(listOf("People", "Flags"), sheets.map { it.name })
        assertEquals(2, sheets[0].totalRows)
        assertEquals(listOf("Name", "Score"), sheets[0].rows[0])
        assertEquals(listOf("Alice", "42"), sheets[0].rows[1])
        assertEquals(listOf("Enabled"), sheets[1].rows[0])
        assertEquals(listOf("TRUE"), sheets[1].rows[1])
    }

    @Test
    fun xlsxParserRejectsDoctypeAndRelationshipTraversal() {
        val doctypeWorkbook = workbookZip(
            mapOf(
                "xl/workbook.xml" to """
                    <!DOCTYPE workbook [<!ENTITY xxe SYSTEM "file:///etc/passwd">]>
                    <workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"
                        xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
                      <sheets><sheet name="Bad" sheetId="1" r:id="rId1"/></sheets>
                    </workbook>
                """.trimIndent(),
                "xl/_rels/workbook.xml.rels" to """
                    <Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
                      <Relationship Id="rId1" Type="worksheet" Target="worksheets/sheet1.xml"/>
                    </Relationships>
                """.trimIndent(),
                "xl/worksheets/sheet1.xml" to """
                    <worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
                      <sheetData/>
                    </worksheet>
                """.trimIndent(),
            ),
        )
        assertTrue(
            runCatching {
                parseAndroidSpreadsheet(doctypeWorkbook, "bad.xlsx", null)
            }.isFailure,
        )

        val traversalWorkbook = workbookZip(
            mapOf(
                "xl/workbook.xml" to """
                    <workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"
                        xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
                      <sheets><sheet name="Bad" sheetId="1" r:id="rId1"/></sheets>
                    </workbook>
                """.trimIndent(),
                "xl/_rels/workbook.xml.rels" to """
                    <Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
                      <Relationship Id="rId1" Type="worksheet" Target="../../evil.xml"/>
                    </Relationships>
                """.trimIndent(),
            ),
        )
        assertTrue(
            runCatching {
                parseAndroidSpreadsheet(traversalWorkbook, "bad.xlsx", null)
            }.isFailure,
        )
    }

}
