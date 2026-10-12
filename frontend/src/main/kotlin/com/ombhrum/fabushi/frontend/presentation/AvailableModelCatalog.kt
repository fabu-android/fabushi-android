package com.ombhrum.fabushi

import org.json.JSONObject

data class MobileAvailableModelParameter(
    val id: String,
    val name: String,
    val markdownTooltip: String? = null,
    val kind: String = "unknown",
    val isCycleableByHotkey: Boolean? = null,
)

data class MobileAvailableModel(
    val id: String,
    val displayName: String,
    val defaultOn: Boolean = false,
    val supportsThinking: Boolean? = null,
    val supportsImages: Boolean? = null,
    val supportsMaxMode: Boolean? = null,
    val supportsPlanMode: Boolean? = null,
    val contextTokenLimit: Long? = null,
    val vendorName: String? = null,
    val tagline: String? = null,
    val parameterDefinitions: List<MobileAvailableModelParameter> = emptyList(),
)

data class MobileModelCatalogState(
    val accountFence: String? = null,
    val models: List<MobileAvailableModel> = emptyList(),
    val selectedModelId: String? = null,
    val loading: Boolean = false,
    val error: String? = null,
)

internal fun projectMobileAvailableModelCatalog(
    value: JSONObject,
    previousSelection: String?,
): MobileModelCatalogState {
    val accountFence = value.optString("accountFence").trim().takeIf(String::isNotEmpty)
        ?: error("Available model catalog did not include an account fence")
    val catalog = value.optJSONObject("catalog")
        ?: error("Available model catalog payload is missing")
    check(catalog.optString("scope") == "user-available") {
        "Available model catalog scope is not user-available"
    }
    val rows = catalog.optJSONArray("models") ?: error("Available model catalog models are missing")
    val models = buildList {
        for (index in 0 until rows.length()) {
            val row = rows.optJSONObject(index) ?: continue
            val id = row.optString("name").trim()
            if (id.isEmpty()) continue
            if (row.optNullableBoolean("isHidden") == true) continue
            if (row.optNullableBoolean("supportsAgent") == false) continue
            val parameters = row.optJSONArray("parameterDefinitions")?.let { values ->
                buildList {
                    for (parameterIndex in 0 until values.length()) {
                        val parameter = values.optJSONObject(parameterIndex) ?: continue
                        val parameterId = parameter.optString("id").trim()
                        if (parameterId.isEmpty()) continue
                        add(
                            MobileAvailableModelParameter(
                                id = parameterId,
                                name = parameter.optString("name").trim().ifBlank { parameterId },
                                markdownTooltip = parameter.optNullableString("markdownTooltip"),
                                kind = parameter.optString("kind").trim().ifBlank { "unknown" },
                                isCycleableByHotkey = parameter.optNullableBoolean("isCycleableByHotkey"),
                            ),
                        )
                    }
                }
            }.orEmpty()
            add(
                MobileAvailableModel(
                    id = id,
                    displayName = row.optString("displayName").trim().ifBlank { id },
                    defaultOn = row.optBoolean("defaultOn", false),
                    supportsThinking = row.optNullableBoolean("supportsThinking"),
                    supportsImages = row.optNullableBoolean("supportsImages"),
                    supportsMaxMode = row.optNullableBoolean("supportsMaxMode"),
                    supportsPlanMode = row.optNullableBoolean("supportsPlanMode"),
                    contextTokenLimit = row.optNullableLong("contextTokenLimit"),
                    vendorName = row.optNullableString("vendorName"),
                    tagline = row.optNullableString("tagline"),
                    parameterDefinitions = parameters,
                ),
            )
        }
    }.distinctBy(MobileAvailableModel::id)
    val selected = previousSelection
        ?.takeIf { candidate -> models.any { it.id == candidate } }
        ?: models.firstOrNull(MobileAvailableModel::defaultOn)?.id
        ?: models.firstOrNull()?.id
    return MobileModelCatalogState(
        accountFence = accountFence,
        models = models,
        selectedModelId = selected,
    )
}

internal fun MobileModelCatalogState.selectAvailableModel(modelId: String): MobileModelCatalogState {
    val normalized = modelId.trim()
    require(models.any { it.id == normalized }) { "Model is not in the current user-available catalog" }
    return copy(selectedModelId = normalized, error = null)
}

private fun JSONObject.optNullableString(key: String): String? =
    if (!has(key) || isNull(key)) null else optString(key).trim().takeIf(String::isNotEmpty)

private fun JSONObject.optNullableBoolean(key: String): Boolean? =
    if (!has(key) || isNull(key)) null else optBoolean(key)

private fun JSONObject.optNullableLong(key: String): Long? =
    if (!has(key) || isNull(key)) null else optLong(key)
