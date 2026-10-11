package com.ombhrum.fabushi

internal const val ANDROID_SPREADSHEET_MAX_ROWS = 2_000
internal const val ANDROID_SPREADSHEET_RENDER_ROWS = 200
internal const val ANDROID_SPREADSHEET_PREVIEW_BYTE_CAP = 25 * 1024 * 1024

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
