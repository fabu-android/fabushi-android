package com.ombhrum.fabushi

import com.ombhrum.fabushi.androidpreload.runtime.AndroidContentSearchMedia
import org.json.JSONArray

internal const val COMMAND_PALETTE_FILE_RESULT_LIMIT = 50

enum class CommandPaletteFileStatus {
    UNAVAILABLE,
    IDLE,
    LOADING,
    READY,
    EMPTY,
    FAILED,
    CANCELLED,
}

enum class CommandPaletteFileKind {
    IMAGE,
    VIDEO,
    AUDIO,
    PDF,
    MARKDOWN,
    TABLE,
    JSON,
    TEXT,
    DOCUMENT,
    ARCHIVE,
    FILE,
}

data class CommandPaletteFile(
    val agentId: String,
    val entryId: String,
    val fileName: String,
    val path: String,
    val ext: String,
    val mime: String?,
    val kind: CommandPaletteFileKind,
    val timestampMs: Long,
    val sizeBytes: Long?,
)

data class CommandPaletteFileSnapshot(
    val status: CommandPaletteFileStatus = CommandPaletteFileStatus.IDLE,
    val value: List<CommandPaletteFile> = emptyList(),
)

internal class CommandPaletteFileRequestFence {
    private var generation = 0L

    fun begin(): Long {
        generation += 1
        return generation
    }

    fun cancel() {
        generation += 1
    }

    fun accepts(token: Long): Boolean = token == generation
}

private fun commandPaletteFileExtension(name: String): String {
    val clean = name.trim()
    val index = clean.lastIndexOf('.')
    return if (index <= 0 || index == clean.lastIndex) "" else clean.substring(index + 1).lowercase()
}

internal fun commandPaletteFileKind(fileName: String, mime: String?): CommandPaletteFileKind {
    val ext = commandPaletteFileExtension(fileName)
    val normalizedMime = mime?.trim()?.lowercase().orEmpty()
    return when {
        normalizedMime.startsWith("image/") || ext in setOf("png", "jpg", "jpeg", "gif", "webp", "heic", "heif", "svg") ->
            CommandPaletteFileKind.IMAGE
        normalizedMime.startsWith("video/") || ext in setOf("mp4", "mov", "m4v", "webm", "mkv") ->
            CommandPaletteFileKind.VIDEO
        normalizedMime.startsWith("audio/") || ext in setOf("mp3", "m4a", "wav", "aac", "ogg", "flac") ->
            CommandPaletteFileKind.AUDIO
        normalizedMime == "application/pdf" || ext == "pdf" -> CommandPaletteFileKind.PDF
        ext in setOf("md", "markdown", "mdx") -> CommandPaletteFileKind.MARKDOWN
        ext in setOf("csv", "tsv", "xls", "xlsx", "ods") -> CommandPaletteFileKind.TABLE
        normalizedMime == "application/json" || ext == "json" -> CommandPaletteFileKind.JSON
        normalizedMime.startsWith("text/") || ext in setOf("txt", "log") -> CommandPaletteFileKind.TEXT
        ext in setOf("doc", "docx", "odt", "rtf", "pages") -> CommandPaletteFileKind.DOCUMENT
        ext in setOf("zip", "tar", "gz", "tgz", "7z", "rar") -> CommandPaletteFileKind.ARCHIVE
        else -> CommandPaletteFileKind.FILE
    }
}

internal fun commandPaletteFilesFromHostMatches(
    matches: List<AndroidContentSearchMedia>,
): List<CommandPaletteFile> =
    matches.map { match ->
        require(match.agentId.isNotBlank()) { "Host media-search match has no Agent id" }
        require(match.entryId.isNotBlank()) { "Host media-search match has no entry id" }
        require(match.fileName.isNotBlank() && match.path.isNotBlank()) {
            "Host media-search match has no open metadata"
        }
        val kind = when (match.kind) {
            "image" -> CommandPaletteFileKind.IMAGE
            "video" -> CommandPaletteFileKind.VIDEO
            "audio" -> CommandPaletteFileKind.AUDIO
            "pdf" -> CommandPaletteFileKind.PDF
            "markdown" -> CommandPaletteFileKind.MARKDOWN
            "table" -> CommandPaletteFileKind.TABLE
            "json" -> CommandPaletteFileKind.JSON
            "text" -> CommandPaletteFileKind.TEXT
            "document" -> CommandPaletteFileKind.DOCUMENT
            "archive" -> CommandPaletteFileKind.ARCHIVE
            "file" -> CommandPaletteFileKind.FILE
            else -> error("Unsupported Host media-search kind: ${match.kind}")
        }
        CommandPaletteFile(
            agentId = match.agentId,
            entryId = match.entryId,
            fileName = match.fileName,
            path = match.path,
            ext = match.ext,
            mime = match.mime,
            kind = kind,
            timestampMs = match.timestampMs,
            sizeBytes = match.sizeBytes,
        )
    }

internal fun commandPaletteFilesFromTranscript(
    transcript: JSONArray,
    query: String,
    limit: Int = COMMAND_PALETTE_FILE_RESULT_LIMIT,
): List<CommandPaletteFile> {
    if (limit <= 0) return emptyList()
    val normalizedQuery = query.trim()
    val candidates = mutableListOf<Pair<Int, CommandPaletteFile>>()
    val seenMessageEntries = mutableSetOf<String>()

    canonicalAgentTranscriptMessages(transcript)
        .sortedByDescending { it.timestampMs }
        .forEach { message ->
            for (attachment in message.attachments) {
                val fileName = attachment.name.trim()
                val path = attachment.path.trim()
                if (fileName.isEmpty() || path.isEmpty()) continue
                val kind = commandPaletteFileKind(fileName, attachment.mimeType)
                val searchable = listOf(
                    fileName,
                    commandPaletteFileExtension(fileName),
                    attachment.mimeType.orEmpty(),
                    kind.name,
                ).joinToString(" ")
                val score = if (normalizedQuery.isEmpty()) 0 else fuzzyPaletteScore(normalizedQuery, searchable) ?: continue
                val identity = "${message.agentId}\u0000${message.id}"
                if (!seenMessageEntries.add(identity)) break
                candidates += score to CommandPaletteFile(
                    agentId = message.agentId,
                    entryId = message.id,
                    fileName = fileName,
                    path = path,
                    ext = commandPaletteFileExtension(fileName),
                    mime = attachment.mimeType,
                    kind = kind,
                    timestampMs = message.timestampMs,
                    sizeBytes = attachment.sizeBytes,
                )
                break
            }
        }

    return candidates
        .sortedWith(
            compareByDescending<Pair<Int, CommandPaletteFile>> { it.first }
                .thenByDescending { it.second.timestampMs },
        )
        .take(limit)
        .map { it.second }
}

internal fun commandPaletteFileEntries(
    files: List<CommandPaletteFile>,
    agentNames: Map<String, String>,
    onOpen: (CommandPaletteFile) -> Unit,
): List<CommandPaletteEntry> =
    files.mapNotNull { file ->
        val agentName = agentNames[file.agentId]?.takeIf(String::isNotBlank) ?: return@mapNotNull null
        val type = when (file.kind) {
            CommandPaletteFileKind.IMAGE -> "Image"
            CommandPaletteFileKind.VIDEO -> "Video"
            CommandPaletteFileKind.AUDIO -> "Audio"
            CommandPaletteFileKind.PDF -> "PDF"
            CommandPaletteFileKind.MARKDOWN -> "Markdown"
            CommandPaletteFileKind.TABLE -> "Table"
            CommandPaletteFileKind.JSON -> "JSON"
            CommandPaletteFileKind.TEXT -> "Text"
            CommandPaletteFileKind.DOCUMENT -> "Document"
            CommandPaletteFileKind.ARCHIVE -> "Archive"
            CommandPaletteFileKind.FILE -> "File"
        }
        CommandPaletteEntry(
            id = "file:${file.agentId}:${file.entryId}",
            kind = CommandPaletteEntryKind.FILE,
            label = file.fileName,
            detail = "$type · $agentName",
            searchText = listOf(file.fileName, file.ext, file.mime.orEmpty(), type, agentName).joinToString(" "),
            activate = { onOpen(file) },
        )
    }
