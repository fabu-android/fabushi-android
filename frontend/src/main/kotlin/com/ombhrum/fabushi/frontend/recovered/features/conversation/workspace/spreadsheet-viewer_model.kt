package com.ombhrum.fabushi

import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.net.URI
import java.util.zip.ZipInputStream
import javax.xml.XMLConstants
import javax.xml.parsers.DocumentBuilderFactory
import org.w3c.dom.Document
import org.w3c.dom.Element

internal const val ANDROID_SPREADSHEET_MAX_ROWS = 2_000
internal const val ANDROID_SPREADSHEET_RENDER_ROWS = 200
internal const val ANDROID_SPREADSHEET_PREVIEW_BYTE_CAP = 25 * 1024 * 1024
private const val ANDROID_SPREADSHEET_EXPANDED_BYTE_CAP = 64 * 1024 * 1024
private const val ANDROID_SPREADSHEET_ENTRY_BYTE_CAP = 24 * 1024 * 1024
private const val ANDROID_SPREADSHEET_ENTRY_COUNT_CAP = 512

internal data class AndroidSpreadsheetSheet(
    val name: String,
    val rows: List<List<String>>,
    val totalRows: Int,
)

internal fun isAndroidDelimitedSpreadsheet(fileName: String?, mimeType: String?): Boolean {
    val extension = fileName?.substringAfterLast('.', missingDelimiterValue = "")?.lowercase()
    val mime = mimeType?.substringBefore(';')?.trim()?.lowercase()
    return extension == "csv" || extension == "tsv" ||
        mime == "text/csv" || mime == "text/tab-separated-values"
}

internal fun isAndroidOpenXmlSpreadsheet(fileName: String?, mimeType: String?): Boolean {
    val extension = fileName?.substringAfterLast('.', missingDelimiterValue = "")?.lowercase()
    val mime = mimeType?.substringBefore(';')?.trim()?.lowercase()
    return extension in setOf("xlsx", "xlsm", "xltx", "xltm") ||
        mime == "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" ||
        mime == "application/vnd.ms-excel.sheet.macroenabled.12" ||
        mime == "application/vnd.openxmlformats-officedocument.spreadsheetml.template" ||
        mime == "application/vnd.ms-excel.template.macroenabled.12"
}

internal fun isAndroidSpreadsheet(fileName: String?, mimeType: String?): Boolean =
    isAndroidDelimitedSpreadsheet(fileName, mimeType) ||
        isAndroidOpenXmlSpreadsheet(fileName, mimeType)

internal fun parseAndroidSpreadsheet(
    bytes: ByteArray,
    fileName: String?,
    mimeType: String?,
    maxRows: Int = ANDROID_SPREADSHEET_MAX_ROWS,
): List<AndroidSpreadsheetSheet> {
    require(maxRows > 0) { "spreadsheet maxRows must be positive" }
    require(bytes.size <= ANDROID_SPREADSHEET_PREVIEW_BYTE_CAP) { "Spreadsheet too large to preview" }
    return when {
        isAndroidDelimitedSpreadsheet(fileName, mimeType) ->
            listOf(parseAndroidDelimitedSpreadsheet(bytes, fileName, maxRows))
        isAndroidOpenXmlSpreadsheet(fileName, mimeType) ->
            parseAndroidOpenXmlSpreadsheet(bytes, maxRows)
        else -> error("Unsupported spreadsheet format")
    }
}

internal fun parseAndroidDelimitedSpreadsheet(
    bytes: ByteArray,
    fileName: String?,
    maxRows: Int = ANDROID_SPREADSHEET_MAX_ROWS,
): AndroidSpreadsheetSheet {
    require(maxRows > 0) { "spreadsheet maxRows must be positive" }
    require(bytes.size <= ANDROID_SPREADSHEET_PREVIEW_BYTE_CAP) { "Spreadsheet too large to preview" }
    val delimiter = if (fileName?.lowercase()?.endsWith(".tsv") == true) '\t' else ','
    val text = bytes.toString(Charsets.UTF_8)
    val rows = parseAndroidDelimitedRows(text, delimiter, maxRows)
    val totalRows = if (rows.size < maxRows) rows.size else countAndroidDelimitedRows(text)
    return AndroidSpreadsheetSheet("Sheet 1", rows, totalRows)
}

private fun parseAndroidOpenXmlSpreadsheet(
    bytes: ByteArray,
    maxRows: Int,
): List<AndroidSpreadsheetSheet> {
    val entries = readAndroidSpreadsheetZipEntries(bytes)
    val workbook = parseAndroidSpreadsheetXml(
        entries["xl/workbook.xml"] ?: error("Spreadsheet workbook is missing"),
    )
    val relationships = parseAndroidSpreadsheetRelationships(
        entries["xl/_rels/workbook.xml.rels"] ?: error("Spreadsheet workbook relationships are missing"),
    )
    val sharedStrings = entries["xl/sharedStrings.xml"]?.let(::parseAndroidSpreadsheetSharedStrings).orEmpty()
    val sheetNodes = workbook.getElementsByTagNameNS("*", "sheet")
    return buildList(sheetNodes.length) {
        for (index in 0 until sheetNodes.length) {
            val sheet = sheetNodes.item(index) as? Element ?: continue
            val name = sheet.getAttribute("name").trim().ifEmpty { "Sheet " + (index + 1) }
            val relationshipId = sheet.getAttributeNS(
                "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
                "id",
            ).trim().ifEmpty { sheet.getAttribute("r:id").trim() }
            val target = relationships[relationshipId]
                ?: error("Spreadsheet sheet relationship is missing")
            val payload = entries[target] ?: error("Spreadsheet sheet data is missing")
            add(parseAndroidSpreadsheetWorksheet(name, payload, sharedStrings, maxRows))
        }
    }
}

private fun readAndroidSpreadsheetZipEntries(bytes: ByteArray): Map<String, ByteArray> {
    val captured = linkedMapOf<String, ByteArray>()
    var capturedBytes = 0L
    var entryCount = 0
    ZipInputStream(ByteArrayInputStream(bytes)).use { zip ->
        while (true) {
            val entry = zip.nextEntry ?: break
            entryCount += 1
            require(entryCount <= ANDROID_SPREADSHEET_ENTRY_COUNT_CAP) {
                "Spreadsheet archive has too many entries"
            }
            val name = entry.name.replace('\\', '/')
            require(
                name.isNotBlank() &&
                    !name.startsWith("/") &&
                    name.split('/').none { it == ".." },
            ) {
                "Spreadsheet archive contains an unsafe path"
            }
            if (!entry.isDirectory && shouldReadAndroidSpreadsheetEntry(name)) {
                val output = ByteArrayOutputStream()
                val buffer = ByteArray(8 * 1024)
                var entryBytes = 0L
                while (true) {
                    val read = zip.read(buffer)
                    if (read < 0) break
                    if (read == 0) continue
                    entryBytes += read
                    capturedBytes += read
                    require(entryBytes <= ANDROID_SPREADSHEET_ENTRY_BYTE_CAP) {
                        "Spreadsheet archive entry is too large"
                    }
                    require(capturedBytes <= ANDROID_SPREADSHEET_EXPANDED_BYTE_CAP) {
                        "Spreadsheet archive expands beyond the preview limit"
                    }
                    output.write(buffer, 0, read)
                }
                captured[name] = output.toByteArray()
            }
            zip.closeEntry()
        }
    }
    return captured
}

private fun shouldReadAndroidSpreadsheetEntry(name: String): Boolean =
    name == "xl/workbook.xml" ||
        name == "xl/_rels/workbook.xml.rels" ||
        name == "xl/sharedStrings.xml" ||
        name.startsWith("xl/worksheets/")

private fun parseAndroidSpreadsheetXml(bytes: ByteArray): Document {
    val factory = DocumentBuilderFactory.newInstance()
    factory.isNamespaceAware = true
    factory.isExpandEntityReferences = false
    factory.isXIncludeAware = false
    fun requireFeature(name: String, value: Boolean) {
        runCatching { factory.setFeature(name, value) }.getOrElse {
            throw IllegalStateException("Secure spreadsheet XML parser is unavailable", it)
        }
    }
    requireFeature("http://apache.org/xml/features/disallow-doctype-decl", true)
    requireFeature("http://xml.org/sax/features/external-general-entities", false)
    requireFeature("http://xml.org/sax/features/external-parameter-entities", false)
    requireFeature("http://apache.org/xml/features/nonvalidating/load-external-dtd", false)
    runCatching { factory.setAttribute(XMLConstants.ACCESS_EXTERNAL_DTD, "") }
    runCatching { factory.setAttribute(XMLConstants.ACCESS_EXTERNAL_SCHEMA, "") }
    return factory.newDocumentBuilder().parse(ByteArrayInputStream(bytes))
}

private fun parseAndroidSpreadsheetRelationships(bytes: ByteArray): Map<String, String> {
    val document = parseAndroidSpreadsheetXml(bytes)
    val nodes = document.getElementsByTagNameNS("*", "Relationship")
    val workbookUri = URI("file:/xl/workbook.xml")
    return buildMap {
        for (index in 0 until nodes.length) {
            val relationship = nodes.item(index) as? Element ?: continue
            if (relationship.getAttribute("TargetMode").equals("External", ignoreCase = true)) continue
            val id = relationship.getAttribute("Id").trim()
            val target = relationship.getAttribute("Target").trim().replace('\\', '/')
            if (id.isEmpty() || target.isEmpty()) continue
            val resolved = workbookUri.resolve(target).normalize()
            require(resolved.scheme == "file") { "Spreadsheet relationship must stay inside the workbook" }
            val path = resolved.path.removePrefix("/")
            require(
                path.startsWith("xl/") &&
                    path.split('/').none { it == ".." },
            ) {
                "Spreadsheet relationship escapes the workbook"
            }
            put(id, path)
        }
    }
}

private fun parseAndroidSpreadsheetSharedStrings(bytes: ByteArray): List<String> {
    val document = parseAndroidSpreadsheetXml(bytes)
    val nodes = document.getElementsByTagNameNS("*", "si")
    return buildList(nodes.length) {
        for (index in 0 until nodes.length) {
            val item = nodes.item(index) as? Element ?: continue
            val textNodes = item.getElementsByTagNameNS("*", "t")
            val value = buildString {
                for (textIndex in 0 until textNodes.length) {
                    append(textNodes.item(textIndex)?.textContent.orEmpty())
                }
            }
            add(value)
        }
    }
}

private fun parseAndroidSpreadsheetWorksheet(
    name: String,
    bytes: ByteArray,
    sharedStrings: List<String>,
    maxRows: Int,
): AndroidSpreadsheetSheet {
    val document = parseAndroidSpreadsheetXml(bytes)
    val rowNodes = document.getElementsByTagNameNS("*", "row")
    val rows = ArrayList<List<String>>(minOf(maxRows, rowNodes.length))
    var totalRows = 0
    for (rowIndex in 0 until rowNodes.length) {
        val row = rowNodes.item(rowIndex) as? Element ?: continue
        val cellNodes = row.getElementsByTagNameNS("*", "c")
        if (cellNodes.length == 0) continue
        totalRows += 1
        if (rows.size >= maxRows) continue
        val values = mutableListOf<String>()
        var fallbackColumn = 0
        for (cellIndex in 0 until cellNodes.length) {
            val cell = cellNodes.item(cellIndex) as? Element ?: continue
            val column = androidSpreadsheetColumnIndex(cell.getAttribute("r"), fallbackColumn)
            while (values.size <= column) values += ""
            values[column] = androidSpreadsheetCellText(cell, sharedStrings)
            fallbackColumn = column + 1
        }
        rows += values
    }
    return AndroidSpreadsheetSheet(name, rows, totalRows)
}

private fun androidSpreadsheetColumnIndex(reference: String, fallback: Int): Int {
    var value = 0
    var hasColumn = false
    for (character in reference) {
        if (!character.isLetter()) break
        val upper = character.uppercaseChar()
        if (upper !in 'A'..'Z') break
        value = value * 26 + (upper.code - 'A'.code + 1)
        hasColumn = true
    }
    return if (hasColumn) value - 1 else fallback
}

private fun androidSpreadsheetCellText(
    cell: Element,
    sharedStrings: List<String>,
): String {
    val type = cell.getAttribute("t").trim()
    val raw = androidSpreadsheetFirstText(cell, "v")
    return when (type) {
        "s" -> raw?.toIntOrNull()?.let(sharedStrings::getOrNull).orEmpty()
        "inlineStr" -> {
            val nodes = cell.getElementsByTagNameNS("*", "t")
            buildString {
                for (index in 0 until nodes.length) append(nodes.item(index)?.textContent.orEmpty())
            }
        }
        "b" -> if (raw == "1") "TRUE" else "FALSE"
        "str", "e", "d" -> raw.orEmpty()
        else -> raw ?: androidSpreadsheetFirstText(cell, "f")?.let { "=" + it }.orEmpty()
    }
}

private fun androidSpreadsheetFirstText(parent: Element, localName: String): String? {
    val nodes = parent.getElementsByTagNameNS("*", localName)
    if (nodes.length == 0) return null
    return nodes.item(0)?.textContent
}

private fun parseAndroidDelimitedRows(text: String, delimiter: Char, maxRows: Int): List<List<String>> {
    val rows = mutableListOf<List<String>>()
    var field = StringBuilder()
    var row = mutableListOf<String>()
    var quoted = false
    var hasContent = false
    fun pushField() { row += field.toString(); field = StringBuilder() }
    fun pushRow() { pushField(); rows += row; row = mutableListOf(); hasContent = false }
    var index = 0
    while (index < text.length && rows.size < maxRows) {
        val ch = text[index]
        if (quoted) {
            if (ch == '"') {
                if (index + 1 < text.length && text[index + 1] == '"') {
                    field.append('"'); index += 1
                } else quoted = false
            } else field.append(ch)
            index += 1
            continue
        }
        when {
            ch == '"' -> { quoted = true; hasContent = true }
            ch == delimiter -> { hasContent = true; pushField() }
            ch == '\n' || ch == '\r' -> {
                if (ch == '\r' && index + 1 < text.length && text[index + 1] == '\n') index += 1
                pushRow()
            }
            else -> { field.append(ch); hasContent = true }
        }
        index += 1
    }
    if (rows.size < maxRows && (hasContent || field.isNotEmpty() || row.isNotEmpty())) pushRow()
    return rows
}

private fun countAndroidDelimitedRows(text: String): Int {
    var rows = 0
    var quoted = false
    var hasContent = false
    var index = 0
    while (index < text.length) {
        val ch = text[index]
        if (ch == '"') {
            if (quoted && index + 1 < text.length && text[index + 1] == '"') index += 1
            else quoted = !quoted
            hasContent = true
            index += 1
            continue
        }
        if (!quoted && (ch == '\n' || ch == '\r')) {
            if (ch == '\r' && index + 1 < text.length && text[index + 1] == '\n') index += 1
            rows += 1
            hasContent = false
        } else if (!ch.isWhitespace()) hasContent = true
        index += 1
    }
    return if (hasContent) rows + 1 else rows
}
