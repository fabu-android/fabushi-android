package com.wangzan101.mika.core.access

import com.wangzan101.mika.core.model.*
import com.wangzan101.mika.core.prompt.Prompts
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.withTimeoutOrNull
import kotlinx.serialization.json.*

/**
 * 按主要用途给应用分类(只用于整理和推荐,不改变权限)。
 * [language] 是分类理由用的语言(界面语言),由调用方给;分类名本身是固定的英文枚举。
 */
class AppClassifier(
    private val model: ChatModel,
    private val fallback: ChatModel? = null,
    private val language: String = "English",
) {
    suspend fun classify(apps: List<Pair<String, String>>): Map<String, AppClassification> {
        if (apps.isEmpty()) return emptyMap()
        val (selfApps, externalApps) = apps.partition { (pkg, _) -> isSelf(pkg) }
        val selfClassified = selfApps.associate { (pkg, _) -> pkg to SELF_CLASSIFICATION }
        if (externalApps.isEmpty()) return selfClassified
        val input = buildJsonObject {
            put("language", language)
            putJsonArray("apps") {
                externalApps.forEach { (pkg, label) -> add(buildJsonObject { put("package", pkg); put("label", label) }) }
            }
        }
        for (candidate in listOfNotNull(model, fallback?.takeUnless { it === model })) {
            val result = try {
                withTimeoutOrNull(30_000L) {
                    val reply = candidate.complete(Prompts.role("app_classifier"),
                        listOf(Message(Role.User, listOf(Part.Text(input.toString())), Author.User)), emptyList())
                    parse(reply.parts.filterIsInstance<Part.Text>().joinToString("") { it.text }, externalApps.map { it.first }.toSet())
                }
            } catch (e: CancellationException) { throw e } catch (_: Exception) { null }
            if (!result.isNullOrEmpty()) return selfClassified + result
        }
        return selfClassified
    }

    companion object {
        const val MIKA_SELF_PREFIX = "com.wangzan101.mika"
        const val FABUSHI_SELF_PREFIX = "com.ombhrum.fabushi"
        val SELF_PREFIXES = listOf(MIKA_SELF_PREFIX, FABUSHI_SELF_PREFIX)
        val SELF_CLASSIFICATION = AppClassification(AppCategory.Other, "Self protected application", manual = true)

        /** Mika / Fabushi 自身包名(含 .debug、.logs、.dev 等后缀)一律识别为自身受保护应用。 */
        fun isSelf(packageName: String): Boolean =
            SELF_PREFIXES.any { prefix -> packageName == prefix || packageName.startsWith("$prefix.") }

        fun parse(raw: String, packages: Set<String>): Map<String, AppClassification> {
            val selfEntries = packages.filter(::isSelf).associateWith { SELF_CLASSIFICATION }
            val parsed = runCatching {
                val json = raw.trim().removePrefix("```json").removePrefix("```").removeSuffix("```").trim()
                Json.parseToJsonElement(json).jsonArray.mapNotNull { item ->
                    val o = item.jsonObject
                    val pkg = o["package"]?.jsonPrimitive?.content ?: return@mapNotNull null
                    if (pkg !in packages || isSelf(pkg)) return@mapNotNull null
                    val category = o["category"]?.jsonPrimitive?.content?.let { value -> AppCategory.entries.firstOrNull { it.name == value } }
                        ?: AppCategory.Unknown
                    pkg to AppClassification(category, o["reason"]?.jsonPrimitive?.content.orEmpty().take(240))
                }.toMap()
            }.getOrDefault(emptyMap())
            return parsed + selfEntries
        }
    }
}
