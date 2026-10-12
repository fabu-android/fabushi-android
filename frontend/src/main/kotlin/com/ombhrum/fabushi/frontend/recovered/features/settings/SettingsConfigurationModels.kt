package com.ombhrum.fabushi

import org.json.JSONObject

internal enum class AutoReviewInstructionBehavior {
    ALLOW,
    ASK,
}

internal data class AutoReviewInstructions(
    val allowInstructions: List<String> = emptyList(),
    val blockInstructions: List<String> = emptyList(),
)

internal data class AutoReviewInstructionRow(
    val behavior: AutoReviewInstructionBehavior,
    val text: String,
    val listIndex: Int,
)

internal const val MAX_AUTO_REVIEW_INSTRUCTIONS_PER_BEHAVIOR = 20

internal fun parseCursorAuthSubject(authId: String?): String? {
    val value = authId ?: return null
    val separator = value.indexOf('|')
    if (separator <= 0 || separator >= value.lastIndex) return null
    return value.substring(separator + 1)
}

internal fun autoReviewInstructionRows(
    instructions: AutoReviewInstructions,
): List<AutoReviewInstructionRow> = buildList {
    instructions.allowInstructions.forEachIndexed { index, text ->
        add(AutoReviewInstructionRow(AutoReviewInstructionBehavior.ALLOW, text, index))
    }
    instructions.blockInstructions.forEachIndexed { index, text ->
        add(AutoReviewInstructionRow(AutoReviewInstructionBehavior.ASK, text, index))
    }
}

private fun AutoReviewInstructions.listFor(
    behavior: AutoReviewInstructionBehavior,
): List<String> =
    if (behavior == AutoReviewInstructionBehavior.ALLOW) allowInstructions else blockInstructions

private fun AutoReviewInstructions.replaceList(
    behavior: AutoReviewInstructionBehavior,
    values: List<String>,
): AutoReviewInstructions =
    if (behavior == AutoReviewInstructionBehavior.ALLOW) {
        copy(allowInstructions = values)
    } else {
        copy(blockInstructions = values)
    }

internal fun removeAutoReviewInstruction(
    instructions: AutoReviewInstructions,
    row: AutoReviewInstructionRow,
): AutoReviewInstructions {
    val list = instructions.listFor(row.behavior)
    return instructions.replaceList(
        row.behavior,
        list.filterIndexed { index, _ -> index != row.listIndex },
    )
}

internal fun reconcileAutoReviewInstructionRow(
    instructions: AutoReviewInstructions,
    row: AutoReviewInstructionRow,
): AutoReviewInstructionRow? {
    val index = instructions.listFor(row.behavior).indexOf(row.text)
    if (index < 0) return null
    return if (index == row.listIndex) row else row.copy(listIndex = index)
}

internal fun saveAutoReviewInstruction(
    instructions: AutoReviewInstructions,
    text: String,
    behavior: AutoReviewInstructionBehavior,
    editing: AutoReviewInstructionRow?,
): AutoReviewInstructions? {
    val target = instructions.listFor(behavior)
    val sameList = editing?.behavior == behavior
    val duplicate = target.withIndex().any { (index, item) ->
        item == text && !(sameList && index == editing?.listIndex)
    }
    if (duplicate) return null

    if (editing == null) {
        if (target.size >= MAX_AUTO_REVIEW_INSTRUCTIONS_PER_BEHAVIOR) return null
        return instructions.replaceList(behavior, target + text)
    }

    if (sameList) {
        if (editing.listIndex !in target.indices) return null
        return instructions.replaceList(
            behavior,
            target.mapIndexed { index, value -> if (index == editing.listIndex) text else value },
        )
    }

    if (target.size >= MAX_AUTO_REVIEW_INSTRUCTIONS_PER_BEHAVIOR) return null
    val withoutPrevious = removeAutoReviewInstruction(instructions, editing)
    return withoutPrevious.replaceList(behavior, withoutPrevious.listFor(behavior) + text)
}

internal enum class RouterProviderId(val wireId: String) {
    CURSOR("cursor"),
    CLAUDE_CODE("claude-code"),
    CODEX("codex"),
    OPENROUTER("openrouter");

    companion object {
        fun fromWire(value: String?): RouterProviderId? =
            values().firstOrNull { it.wireId == value }
    }
}

internal data class RouterProvider(
    val id: RouterProviderId,
    val label: String,
    val description: String,
    val usageDescription: String,
    val usageSource: String,
)

internal val ROUTER_PROVIDERS_ANDROID: List<RouterProvider> = listOf(
    RouterProvider(
        RouterProviderId.CURSOR,
        "Cursor",
        "Use the signed-in Cursor account and hosted agent models.",
        "Included and on-demand usage from the Cursor account.",
        "cursor",
    ),
    RouterProvider(
        RouterProviderId.CLAUDE_CODE,
        "Claude Code",
        "Route agent requests through Anthropic Claude Code.",
        "Usage is managed by the connected Anthropic account.",
        "external",
    ),
    RouterProvider(
        RouterProviderId.CODEX,
        "Codex",
        "Route agent requests through OpenAI Codex.",
        "Usage is managed by the connected OpenAI account.",
        "external",
    ),
    RouterProvider(
        RouterProviderId.OPENROUTER,
        "OpenRouter",
        "Route requests through models and billing in OpenRouter.",
        "Usage and spend are managed in OpenRouter.",
        "external",
    ),
)
internal val DEFAULT_ROUTER_PROVIDER_ANDROID = RouterProviderId.CURSOR
internal const val ROUTER_PROVIDER_PERSISTENCE_KEY_ANDROID = "settings.router-provider.v1"

internal fun routerProviderByIdAndroid(id: RouterProviderId): RouterProvider =
    ROUTER_PROVIDERS_ANDROID.firstOrNull { it.id == id } ?: ROUTER_PROVIDERS_ANDROID.first()

internal fun parseRouterProviderPreferenceAndroid(raw: String?): RouterProviderId {
    if (raw == null) return DEFAULT_ROUTER_PROVIDER_ANDROID
    return runCatching {
        val parsed = JSONObject(raw)
        if (parsed.optInt("schemaVersion", -1) != 1) return@runCatching DEFAULT_ROUTER_PROVIDER_ANDROID
        RouterProviderId.fromWire(parsed.optString("provider")) ?: DEFAULT_ROUTER_PROVIDER_ANDROID
    }.getOrDefault(DEFAULT_ROUTER_PROVIDER_ANDROID)
}

internal fun encodeRouterProviderPreferenceAndroid(provider: RouterProviderId): String =
    JSONObject()
        .put("schemaVersion", 1)
        .put("provider", provider.wireId)
        .toString()
