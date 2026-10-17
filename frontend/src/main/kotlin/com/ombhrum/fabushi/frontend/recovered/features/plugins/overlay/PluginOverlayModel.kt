package com.ombhrum.fabushi

import org.json.JSONArray

internal const val PLUGIN_DEFAULT_ACCOUNT_KEY = "default"

internal enum class PluginOverlayInstallMode {
    USER,
    TEAM_DEFAULT,
    TEAM_REQUIRED,
    UNKNOWN,
}

internal data class PluginOverlayVariableField(
    val key: String,
    val isRequired: Boolean,
    val defaultValue: String? = null,
)

internal data class PluginOverlayAccount(
    val serverIdentity: String,
    val accountKey: String,
)

internal data class PluginOverlayTool(
    val name: String,
    val title: String? = null,
    val description: String? = null,
    val isDisabled: Boolean,
)

internal fun parsePluginServerTools(values: JSONArray): List<PluginOverlayTool> =
    buildList {
        val seen = linkedSetOf<String>()
        for (index in 0 until values.length()) {
            val value = values.optJSONObject(index) ?: continue
            val name = value.optString("name").trim()
            if (name.isEmpty() || name.length > 512 || !seen.add(name)) continue
            val title = value.optString("title").trim().takeIf { it.isNotEmpty() && it.length <= 1_024 }
            val description = value.optString("description").trim().takeIf { it.isNotEmpty() && it.length <= 16_384 }
            add(
                PluginOverlayTool(
                    name = name,
                    title = title,
                    description = description,
                    isDisabled = value.optBoolean("isDisabled", false),
                ),
            )
        }
    }


internal data class PluginOverlayNotice(
    val isError: Boolean,
    val text: String,
)

internal fun pluginOverlayTeamPolicyLabel(mode: PluginOverlayInstallMode): String? =
    when (mode) {
        PluginOverlayInstallMode.TEAM_REQUIRED -> "Managed by your team"
        PluginOverlayInstallMode.TEAM_DEFAULT -> "Added by your team"
        PluginOverlayInstallMode.USER,
        PluginOverlayInstallMode.UNKNOWN,
        -> null
    }

internal fun pluginOverlayTeamPolicyActions(mode: PluginOverlayInstallMode): Set<String> =
    when (mode) {
        PluginOverlayInstallMode.TEAM_REQUIRED -> emptySet()
        PluginOverlayInstallMode.TEAM_DEFAULT -> setOf("remove")
        PluginOverlayInstallMode.USER,
        PluginOverlayInstallMode.UNKNOWN,
        -> setOf("uninstall")
    }

internal fun missingRequiredPluginOverlayFields(
    fields: List<PluginOverlayVariableField>,
    values: Map<String, String?>,
): List<PluginOverlayVariableField> =
    fields.filter { field ->
        field.isRequired &&
            (values[field.key] ?: field.defaultValue.orEmpty()).trim().isEmpty()
    }

internal fun normalizePluginOverlaySetupValues(
    fields: List<PluginOverlayVariableField>,
    values: Map<String, String?>,
): Map<String, String> =
    buildMap {
        fields.forEach { field ->
            values[field.key]?.trim()?.takeIf(String::isNotEmpty)?.let { put(field.key, it) }
        }
    }

internal fun pluginOverlayAccountLabel(accountKey: String): String =
    if (accountKey == PLUGIN_DEFAULT_ACCOUNT_KEY) "Default" else accountKey

internal fun pluginOverlayAccountAction(
    accounts: List<PluginOverlayAccount>,
    rawLabel: String,
): String? {
    val label = rawLabel.trim()
    if (label.isEmpty()) return null
    return if (accounts.any { it.accountKey == label }) "authenticate" else "add"
}

internal fun groupPluginOverlayAccounts(
    accounts: List<PluginOverlayAccount>,
): Map<String, List<PluginOverlayAccount>> =
    accounts.groupBy { it.serverIdentity }
        .mapValues { (_, values) ->
            values.sortedWith(
                compareBy<PluginOverlayAccount> { if (it.accountKey == PLUGIN_DEFAULT_ACCOUNT_KEY) 0 else 1 }
                    .thenBy { it.accountKey },
            )
        }

internal fun pluginOverlayAuthenticationNotice(status: String, message: String? = null): PluginOverlayNotice =
    when (status) {
        "already-authenticated" -> PluginOverlayNotice(false, "Already authenticated")
        "not-configured" -> PluginOverlayNotice(true, "Server is not configured")
        "not-supported" -> PluginOverlayNotice(true, message.orEmpty())
        "unreachable" -> PluginOverlayNotice(true, "Couldn't start sign-in: " + message.orEmpty())
        "started" -> PluginOverlayNotice(false, "Opened the OAuth flow in your browser. Return here once it completes.")
        else -> PluginOverlayNotice(true, message?.takeIf(String::isNotBlank) ?: "Authentication status is unavailable")
    }

internal fun togglePluginOverlayTool(
    tools: List<PluginOverlayTool>,
    toolName: String,
): List<PluginOverlayTool> =
    tools.map { tool ->
        if (tool.name == toolName) tool.copy(isDisabled = !tool.isDisabled) else tool.copy()
    }

internal enum class PluginServerToolsStatus {
    LOADING,
    READY,
    FAILED,
    DISPOSED,
}

internal data class PluginServerToolsProjection(
    val status: PluginServerToolsStatus = PluginServerToolsStatus.LOADING,
    val tools: List<PluginOverlayTool> = emptyList(),
    val failure: String? = null,
    val pendingTool: String? = null,
    val generation: Long = 0L,
    val request: Long = 0L,
)

/**
 * Immutable projector for Desktop server-tools.ts semantics.
 *
 * Remote list/toggle remains Host-owned. This class only projects optimistic UI state and fences
 * stale load/toggle completions by generation/request identity.
 */
internal class PluginServerToolsProjector {
    private var state = PluginServerToolsProjection()
    private var opened = false
    private var disposed = false

    fun snapshot(): PluginServerToolsProjection = state

    fun open(): Pair<Long, Long>? {
        if (disposed || opened) return null
        opened = true
        val generation = state.generation + 1
        val request = state.request + 1
        state = state.copy(
            status = PluginServerToolsStatus.LOADING,
            failure = null,
            generation = generation,
            request = request,
        )
        return generation to request
    }

    fun retry(): Pair<Long, Long>? {
        if (disposed || !opened) return null
        val request = state.request + 1
        state = state.copy(
            status = PluginServerToolsStatus.LOADING,
            failure = null,
            request = request,
        )
        return state.generation to request
    }

    fun settleLoad(
        generation: Long,
        request: Long,
        tools: List<PluginOverlayTool>?,
        failure: String? = null,
    ): Boolean {
        if (!current(generation, request)) return false
        state = if (failure == null && tools != null) {
            state.copy(
                status = PluginServerToolsStatus.READY,
                tools = tools.map(PluginOverlayTool::copy),
                failure = null,
            )
        } else {
            state.copy(
                status = PluginServerToolsStatus.FAILED,
                failure = failure ?: "Plugin tools failed to load",
            )
        }
        return true
    }

    fun beginToggle(toolName: String): Pair<Long, Long>? {
        if (disposed || !opened || state.pendingTool != null) return null
        if (state.tools.none { it.name == toolName }) return null
        val request = state.request + 1
        state = state.copy(
            status = PluginServerToolsStatus.READY,
            tools = togglePluginOverlayTool(state.tools, toolName),
            failure = null,
            pendingTool = toolName,
            request = request,
        )
        return state.generation to request
    }

    fun settleToggle(
        generation: Long,
        request: Long,
        tools: List<PluginOverlayTool>?,
        failure: String? = null,
    ): Boolean {
        if (!current(generation, request)) return false
        state = if (failure == null && tools != null) {
            state.copy(
                status = PluginServerToolsStatus.READY,
                tools = tools.map(PluginOverlayTool::copy),
                failure = null,
                pendingTool = null,
            )
        } else {
            // Desktop keeps the optimistic projection on remote failure and surfaces the failure.
            state.copy(
                status = PluginServerToolsStatus.READY,
                failure = failure ?: "Plugin tool update failed",
                pendingTool = null,
            )
        }
        return true
    }

    fun dispose() {
        if (disposed) return
        disposed = true
        opened = false
        state = state.copy(
            status = PluginServerToolsStatus.DISPOSED,
            pendingTool = null,
            generation = state.generation + 1,
            request = state.request + 1,
        )
    }

    private fun current(generation: Long, request: Long): Boolean =
        !disposed && opened && state.generation == generation && state.request == request
}
