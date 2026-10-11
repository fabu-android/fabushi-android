package com.ombhrum.fabushi

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.ombhrum.fabushi.androidpreload.runtime.AndroidCoordinatorPort
import com.ombhrum.fabushi.androidpreload.runtime.AndroidSendAcknowledgementEntry
import com.ombhrum.fabushi.androidpreload.runtime.AndroidSidebarSection
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONObject
import java.util.UUID

data class MobileBotSummaryAndroid(
    val id: String,
    val name: String,
    val title: String? = null,
    val description: String = "",
    val notifyOnUpdatesEnabled: Boolean = false,
    val avatarShape: String? = null,
    val avatarColor: String? = null,
    val miniAppId: String? = null,
    val menuButtonText: String? = null,
    val isPinned: Boolean = false,
    val hasUnread: Boolean = false,
    val isHidden: Boolean = false,
    val isGroup: Boolean = false,
    val memberIds: List<String> = emptyList(),
    val conversationPartnerIds: List<String> = emptyList(),
    val awaitingUserResponse: Boolean? = null,
    val isRunning: Boolean = false,
    val lastMessage: String = "",
    val updatedAt: Long = 0L,
)

data class PendingAgentApproval(
    val approvalId: String,
    val operationId: String,
    val capability: String,
    val reason: String,
    val expiresAtMs: Long?,
    val resolving: Boolean = false,
)

internal fun projectPendingAgentApproval(
    event: JSONObject,
    currentOperationId: String,
): PendingAgentApproval? {
    if (event.optString("type") != "approval.requested") return null
    val operationId = event.optString("operationId")
    if (operationId.isBlank() || operationId != currentOperationId) return null
    val approvalId = event.optString("approvalId")
    if (approvalId.isBlank()) return null
    val expiresAtMs = if (!event.has("expiresAtMs") || event.isNull("expiresAtMs")) {
        null
    } else {
        event.optLong("expiresAtMs").takeIf { it > 0L }
    }
    return PendingAgentApproval(
        approvalId = approvalId,
        operationId = operationId,
        capability = event.optString("capability").ifBlank { "agent.subagent.review" },
        reason = event.optString("reason").ifBlank { "This Agent action requires approval." },
        expiresAtMs = expiresAtMs,
    )
}

data class MobileAsyncTask(
    val kind: String,
    val id: String,
    val label: String,
    val startedAtMs: Long,
    val detail: String? = null,
    val subagentType: String? = null,
)

internal fun projectMobileAsyncTasks(rows: org.json.JSONArray): List<MobileAsyncTask> =
    buildList {
        for (index in 0 until rows.length()) {
            val row = rows.optJSONObject(index)
                ?: throw IllegalStateException("Async task row $index is not an object")
            val kind = row.optString("kind")
            check(kind in setOf("subagent", "shell", "cloud-agent")) {
                "Async task row $index has unsupported kind"
            }
            val id = row.optString("id").trim()
            val label = row.optString("label").trim()
            val status = row.optString("status")
            val startedAtMs = row.optLong("startedAtMs", -1L)
            check(id.isNotEmpty() && label.isNotEmpty() && status == "running" && startedAtMs >= 0L) {
                "Async task row $index is malformed"
            }
            add(
                MobileAsyncTask(
                    kind = kind,
                    id = id,
                    label = label,
                    startedAtMs = startedAtMs,
                    detail = row.optString("detail").trim().takeIf(String::isNotEmpty),
                    subagentType = row.optString("subagentType").trim().takeIf(String::isNotEmpty),
                ),
            )
        }
    }

data class MobileComputerRebuildUiState(
    val imageUpdateAvailable: Boolean? = null,
    val pending: Boolean = false,
    val outcomeUnknown: Boolean = false,
    val kind: String? = null,
    val queuedWhenIdle: Boolean = false,
    val loading: Boolean = false,
    val error: String? = null,
)

internal fun projectMobileComputerRebuildStatus(value: JSONObject): MobileComputerRebuildUiState =
    MobileComputerRebuildUiState(
        imageUpdateAvailable = value.opt("imageUpdateAvailable").let { if (it is Boolean) it else null },
        pending = value.optBoolean("pending", false),
        outcomeUnknown = value.optBoolean("outcomeUnknown", false),
        kind = value.opt("kind").let { raw ->
            if (raw is String) raw.trim().takeIf(String::isNotEmpty) else null
        },
        queuedWhenIdle = value.optBoolean("queuedWhenIdle", false),
    )

data class MobileBotUiState(
    val bots: List<MobileBotSummaryAndroid> = emptyList(),
    val sidebarSections: List<AndroidSidebarSection> = emptyList(),
    val activeBot: MobileBotSummaryAndroid? = null,
    val draft: String = "",
    val messages: List<MobileChatMessage> = emptyList(),
    val busy: Boolean = false,
    val operationId: String? = null,
    val pendingApproval: PendingAgentApproval? = null,
    val widgetPendingEntryIds: Set<String> = emptySet(),
    val editorSuggestions: MobileEditorSuggestionSnapshot = MobileEditorSuggestionSnapshot(),
    val messageTargetId: String? = null,
    val paletteMessageSearch: CommandPaletteMessageSnapshot = CommandPaletteMessageSnapshot(
        status = CommandPaletteMessageStatus.IDLE,
    ),
    val paletteRoutines: CommandPaletteRoutineSnapshot = CommandPaletteRoutineSnapshot(
        status = CommandPaletteRoutineStatus.IDLE,
    ),
    val error: String? = null,
    val creating: Boolean = false,
    val rosterLoading: Boolean = false,
    val asyncTasksAgent: MobileBotSummaryAndroid? = null,
    val asyncTasks: List<MobileAsyncTask> = emptyList(),
    val asyncTasksLoading: Boolean = false,
    val asyncTasksError: String? = null,
    val groupMembersUpdatingId: String? = null,
    val computerRebuild: MobileComputerRebuildUiState = MobileComputerRebuildUiState(),
)

class MobileBotViewModel(application: Application) : AndroidViewModel(application) {
    private val coordinator: AndroidCoordinatorPort = CoordinatorClient.presentation()
    private val miniApps = MiniAppPlatformBridge(coordinator)
    private val rosterSelection = RosterSelectionStore(
        SharedPreferencesRosterSelectionPersistence(
            application.getSharedPreferences("fabushi.agent-selection", 0),
        ),
    )
    private var rosterSelectionAccountSlot: String? = null
    private val agentSettingsFence = AgentSettingsMutationFence()
    private val mutableState = MutableStateFlow(MobileBotUiState())
    private val messagesByBot = mutableMapOf<String, List<MobileChatMessage>>()
    private val draftsByBot = mutableMapOf<String, String>()
    private val paletteMessageFence = CommandPaletteMessageRequestFence()
    private var paletteMessageSearchJob: Job? = null
    private val paletteRoutineFence = CommandPaletteRoutineRequestFence()
    private var paletteRoutineJob: Job? = null
    private var openBotGeneration = 0L
    private var sendPresentationGeneration = 0L
    private var asyncTasksGeneration = 0L
    private var asyncTasksJob: Job? = null
    private var groupMembersGeneration = 0L
    private var computerRebuildGeneration = 0L
    private val editorSuggestionFence = MobileEditorSuggestionRequestFence()
    private var editorSuggestionJob: Job? = null
    val state: StateFlow<MobileBotUiState> = mutableState.asStateFlow()
    private var featureEventSubscription: AutoCloseable? = coordinator.addFeatureEventListener { event ->
        viewModelScope.launch { handleOperationEvent(event) }
    }

    private fun commitState(next: MobileBotUiState) {
        next.activeBot?.let { bot ->
            messagesByBot[bot.id] = next.messages
            draftsByBot[bot.id] = next.draft
        }
        mutableState.value = next
    }


    fun refreshComputerRebuildStatus() {
        val token = ++computerRebuildGeneration
        val previous = mutableState.value.computerRebuild
        commitState(
            mutableState.value.copy(
                computerRebuild = previous.copy(loading = true, error = null),
            ),
        )
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                runCatching { projectMobileComputerRebuildStatus(coordinator.computerRebuildStatus()) }
            }
            if (token != computerRebuildGeneration) return@launch
            result.onSuccess { projection ->
                commitState(
                    mutableState.value.copy(
                        computerRebuild = projection.copy(loading = false),
                    ),
                )
            }.onFailure { error ->
                commitState(
                    mutableState.value.copy(
                        computerRebuild = previous.copy(
                            loading = false,
                            error = error.message ?: "Computer update status unavailable",
                        ),
                    ),
                )
            }
        }
    }

    /**
     * Requests either an immediate idle update or a durable wait-until-idle intent. The
     * Coordinator re-reads canonical Host roster truth and owns request identity/outcome-unknown
     * recovery; this ViewModel never retries a possibly-started update.
     */
    internal fun requestComputerUpdate(
        action: CommandPaletteComputerUpdateAction = CommandPaletteComputerUpdateAction.READY,
        forceNow: Boolean = false,
    ) {
        if (forceNow && action != CommandPaletteComputerUpdateAction.BUSY_OVERRIDE) return
        val current = mutableState.value
        val active = current.activeBot ?: return
        val snapshot = current.computerRebuild
        val projectedAction = commandPaletteComputerUpdateAction(
            activeAgentIsGroup = active.isGroup,
            workingAgentCount = current.bots.count { it.isRunning } + if (current.busy) 1 else 0,
            snapshot = CommandPaletteComputerUpdateSnapshot(
                imageUpdateAvailable = snapshot.imageUpdateAvailable,
                pending = snapshot.pending,
                outcomeUnknown = snapshot.outcomeUnknown,
                rebuildKind = snapshot.kind,
                queuedWhenIdle = snapshot.queuedWhenIdle,
            ),
        )
        if (projectedAction != action || snapshot.loading) return

        val token = ++computerRebuildGeneration
        val expectedAgentId = active.id
        commitState(
            current.copy(
                computerRebuild = snapshot.copy(loading = true, error = null),
            ),
        )
        viewModelScope.launch {
            // Re-read Coordinator truth immediately before the side effect. Returning to Main
            // between the read and dispatch lets account/agent changes fence the request.
            val latestResult = withContext(Dispatchers.IO) {
                runCatching { projectMobileComputerRebuildStatus(coordinator.computerRebuildStatus()) }
            }
            if (
                token != computerRebuildGeneration ||
                mutableState.value.activeBot?.id != expectedAgentId
            ) {
                return@launch
            }
            val latest = latestResult.getOrElse { error ->
                commitState(
                    mutableState.value.copy(
                        computerRebuild = snapshot.copy(
                            loading = false,
                            error = error.message ?: "Computer update status unavailable",
                        ),
                    ),
                )
                return@launch
            }
            val latestAction = commandPaletteComputerUpdateAction(
                activeAgentIsGroup = mutableState.value.activeBot?.isGroup,
                workingAgentCount = mutableState.value.bots.count { it.isRunning } +
                    if (mutableState.value.busy) 1 else 0,
                snapshot = CommandPaletteComputerUpdateSnapshot(
                    imageUpdateAvailable = latest.imageUpdateAvailable,
                    pending = latest.pending,
                    outcomeUnknown = latest.outcomeUnknown,
                    rebuildKind = latest.kind,
                    queuedWhenIdle = latest.queuedWhenIdle,
                ),
            )
            val actionStillAllowed = when (action) {
                CommandPaletteComputerUpdateAction.READY ->
                    latestAction == CommandPaletteComputerUpdateAction.READY
                CommandPaletteComputerUpdateAction.BUSY_OVERRIDE ->
                    latestAction == CommandPaletteComputerUpdateAction.BUSY_OVERRIDE ||
                        latestAction == CommandPaletteComputerUpdateAction.READY
            }
            if (!actionStillAllowed) {
                commitState(mutableState.value.copy(computerRebuild = latest.copy(loading = false)))
                return@launch
            }

            val request = withContext(Dispatchers.IO) {
                runCatching {
                    when {
                        forceNow && latestAction == CommandPaletteComputerUpdateAction.BUSY_OVERRIDE ->
                            coordinator.computerRebuildForceUpdate()
                        action == CommandPaletteComputerUpdateAction.BUSY_OVERRIDE &&
                            latestAction == CommandPaletteComputerUpdateAction.BUSY_OVERRIDE ->
                            coordinator.computerRebuildQueueWhenIdle()
                        else ->
                            coordinator.computerRebuildRequest(
                                preserveData = true,
                                forceRecreate = false,
                            )
                    }
                }
            }
            if (token != computerRebuildGeneration) return@launch
            request.onSuccess {
                // Never infer success from request return alone; refresh the durable owner.
                refreshComputerRebuildStatus()
            }.onFailure { error ->
                // Coordinator marks request outcome-unknown on ambiguous transport failure.
                // Surface the error and re-read durable truth; do not replay the side effect.
                commitState(
                    mutableState.value.copy(
                        computerRebuild = latest.copy(
                            loading = false,
                            error = error.message ?: "Computer update request failed",
                        ),
                    ),
                )
                refreshComputerRebuildStatus()
            }
        }
    }

    fun refreshBots() {
        if (mutableState.value.busy || mutableState.value.rosterLoading) return
        mutableState.value = mutableState.value.copy(rosterLoading = true)
        viewModelScope.launch {
            val previous = mutableState.value.bots
            val selectionAccountResult = withContext(Dispatchers.IO) {
                runCatching { canonicalRosterSelectionAccountSlot() }
            }
            if (selectionAccountResult.isSuccess) {
                val nextSlot = selectionAccountResult.getOrNull()
                if (nextSlot != rosterSelectionAccountSlot) {
                    withContext(Dispatchers.IO) { rosterSelection.restore(nextSlot) }
                    rosterSelectionAccountSlot = nextSlot
                    sendPresentationGeneration += 1
                    cancelEditorSuggestionRequest()
                    agentSettingsFence.accountChanged(nextSlot)
                    commitState(
                        mutableState.value.copy(
                            widgetPendingEntryIds = emptySet(),
                            editorSuggestions = MobileEditorSuggestionSnapshot(),
                        ),
                    )
                }
            }
            val restoredSelection = rosterSelection.get().currentAgentId
            val installedResult = withContext(Dispatchers.IO) { runCatching { loadInstalledMiniAppBots() } }
            val surfaceResult = withContext(Dispatchers.IO) { runCatching { loadSurfaceBots() } }
            val sectionsResult = withContext(Dispatchers.IO) { runCatching { coordinator.agentSidebarSections() } }
            val cachedInstalledBots = if (installedResult.isFailure) {
                withContext(Dispatchers.IO) {
                    runCatching {
                        miniApps.lastInstalledMiniApps()?.let(::projectInstalledMiniAppBots)
                    }.getOrNull()
                }
            } else null
            val installedBots = installedResult.getOrElse {
                cachedInstalledBots ?: previous.filter { it.miniAppId != null }
            }
            val surfaceBots = surfaceResult.getOrElse { previous.filter { it.miniAppId == null } }
            val bots = (installedBots + surfaceBots)
                .distinctBy { it.id }
                .sortedWith(
                    compareByDescending<MobileBotSummaryAndroid> { it.miniAppId != null }
                        .thenBy { it.name.lowercase() },
                )
            val diagnostics = buildList {
                installedResult.exceptionOrNull()?.let { error ->
                    add("canonical installed projection: ${(error.message ?: error::class.java.simpleName).take(240)}")
                }
                surfaceResult.exceptionOrNull()?.let { error ->
                    add("surface bot list: ${(error.message ?: error::class.java.simpleName).take(240)}")
                }
                sectionsResult.exceptionOrNull()?.let { error ->
                    add("sidebar sections: ${(error.message ?: error::class.java.simpleName).take(240)}")
                }
            }
            withContext(Dispatchers.IO) {
                rosterSelection.reconcile(
                    agentIds = listOf("mahayana-assistant") + bots.map(MobileBotSummaryAndroid::id),
                    isRosterComplete = installedResult.isSuccess && surfaceResult.isSuccess,
                )
            }
            mutableState.value = mutableState.value.copy(
                bots = bots,
                sidebarSections = sectionsResult.getOrElse { mutableState.value.sidebarSections },
                error = diagnostics.takeIf { it.isNotEmpty() }?.joinToString(" | "),
                rosterLoading = false,
            )
            val settledSelection = rosterSelection.get().currentAgentId
            if (
                mutableState.value.activeBot == null &&
                restoredSelection != null &&
                restoredSelection == settledSelection
            ) {
                botForSelection(restoredSelection, bots)?.let(::openBot)
            }
        }
    }

    private fun canonicalRosterSelectionAccountSlot(): String? {
        val auth = coordinator.authStatus()
        if (!auth.optBoolean("loggedIn", false)) return null
        val user = auth.optJSONObject("user")
        return auth.optString("authId").trim()
            .ifBlank { user?.optString("id").orEmpty().trim() }
            .ifBlank { user?.optString("email").orEmpty().trim() }
            .takeIf(String::isNotEmpty)
    }

    private fun botForSelection(
        agentId: String,
        bots: List<MobileBotSummaryAndroid>,
    ): MobileBotSummaryAndroid? =
        if (agentId == "mahayana-assistant") {
            MobileBotSummaryAndroid(
                id = "mahayana-assistant",
                name = "Mahayana",
                description = "Mahayana multi-step agent",
            )
        } else {
            bots.firstOrNull { it.id == agentId }
        }

    private fun loadSurfaceBots(): List<MobileBotSummaryAndroid> {
        val rows = coordinator.agentList()
        return buildList {
            for (index in 0 until rows.length()) {
                val row = rows.optJSONObject(index) ?: continue
                val id = row.optString("id")
                if (id.isBlank() || id == "mahayana-assistant") continue
                add(
                    MobileBotSummaryAndroid(
                        id = id,
                        name = row.optString("name").ifBlank { row.optString("displayName").ifBlank { id } },
                        title = row.optString("title").trim().takeIf(String::isNotEmpty),
                        description = row.optString("description"),
                        notifyOnUpdatesEnabled = row.optBoolean("notifyOnUpdatesEnabled", false),
                        avatarShape = row.optString("avatarShape").takeIf(String::isNotBlank),
                        avatarColor = row.optString("avatarColor").takeIf(String::isNotBlank),
                        miniAppId = row.optString("miniAppId").takeIf(String::isNotBlank),
                        menuButtonText = row.optString("menuButtonText").takeIf(String::isNotBlank),
                        isPinned = row.optBoolean("isPinned"),
                        hasUnread = row.optBoolean("hasUnread"),
                        isHidden = row.optBoolean("isHiddenFromSidebar") || row.optBoolean("hiddenFromSidebar"),
                        isGroup = row.optBoolean("isGroup"),
                        memberIds = row.optJSONArray("memberIds")?.let { values ->
                            buildList {
                                for (itemIndex in 0 until values.length()) {
                                    values.optString(itemIndex).takeIf(String::isNotBlank)?.let(::add)
                                }
                            }
                        }.orEmpty(),
                        conversationPartnerIds = row.optJSONArray("conversationPartnerIds")?.let { values ->
                            buildList {
                                for (itemIndex in 0 until values.length()) {
                                    values.optString(itemIndex).takeIf(String::isNotBlank)?.let(::add)
                                }
                            }
                        }.orEmpty(),
                        awaitingUserResponse = if (
                            row.opt("awaitingUserResponse") == null ||
                            row.opt("awaitingUserResponse") == JSONObject.NULL
                        ) {
                            null
                        } else {
                            true
                        },
                        isRunning = row.optBoolean("isRunning"),
                        lastMessage = row.optString("lastMessage"),
                        updatedAt = row.optLong("updatedAt"),
                    ),
                )
            }
        }
    }

    /**
     * Canonical Mini App Bot projection. Messenger refresh is read-only: account installation
     * mutation belongs to the install flow, while `/marketplace/added` owns cross-device identity.
     * No Android-private Bot/contact database is created. The only fallback is the last validated
     * in-process canonical projection, never a second persistent install/Bot truth.
     */
    private fun loadInstalledMiniAppBots(): List<MobileBotSummaryAndroid> {
        val manifests = miniApps.readInstalledMiniApps()
        val bots = projectInstalledMiniAppBots(manifests)
        miniApps.rememberInstalledMiniApps(manifests)
        return bots
    }

    private fun projectInstalledMiniAppBots(manifests: org.json.JSONArray): List<MobileBotSummaryAndroid> = buildList {
        for (index in 0 until manifests.length()) {
            val manifest = manifests.optJSONObject(index)
                ?: error("Canonical installed projection entry $index was not an object")
            val pluginId = manifest.optString("id").ifBlank { manifest.optString("pluginId") }
            check(pluginId.isNotBlank()) { "Canonical installed projection entry $index did not include plugin id" }
            val bot = manifest.optJSONObject("bot") ?: continue
            val botId = bot.optString("id")
            check(botId.isNotBlank()) { "Canonical bot projection for $pluginId did not include id" }
            val menu = bot.optJSONObject("menuButton")
            val miniAppId = menu?.takeIf { it.optString("action") == "open-miniapp" }
                ?.optString("miniAppId")
                ?.takeIf(String::isNotBlank)
                ?: pluginId
            add(
                MobileBotSummaryAndroid(
                    id = botId,
                    name = bot.optString("displayName").ifBlank { manifest.optString("title").ifBlank { botId } },
                    description = bot.optString("description").ifBlank { manifest.optString("description") },
                    miniAppId = miniAppId,
                    menuButtonText = menu?.optString("text")?.takeIf(String::isNotBlank) ?: "打开应用",
                ),
            )
        }
    }

    fun createGroup(name: String, description: String, memberIds: List<String>, onCreated: (() -> Unit)? = null) {
        val cleanName = name.replace(Regex("\\s+"), " ").trim().take(72)
        val members = memberIds.map(String::trim).filter(String::isNotEmpty).distinct().take(6)
        if (cleanName.isBlank() || members.isEmpty() || mutableState.value.creating) return
        mutableState.value = mutableState.value.copy(creating = true, error = null)
        viewModelScope.launch {
            runCatching {
                withContext(Dispatchers.IO) {
                    coordinator.agentCreateGroup(cleanName, description.trim().take(240), members)
                }
            }.onSuccess {
                mutableState.value = mutableState.value.copy(creating = false)
                refreshBots()
                onCreated?.invoke()
            }.onFailure { error ->
                mutableState.value = mutableState.value.copy(creating = false, error = error.message ?: "Agent group creation failed")
            }
        }
    }

    fun setGroupMembers(
        groupId: String,
        memberIds: List<String>,
        onUpdated: (() -> Unit)? = null,
    ) {
        val members = memberIds.map(String::trim).filter(String::isNotEmpty).distinct().take(6)
        val group = mutableState.value.bots.firstOrNull { it.id == groupId && it.isGroup } ?: return
        if (
            groupId.isBlank() ||
            members.isEmpty() ||
            mutableState.value.groupMembersUpdatingId != null ||
            members == group.memberIds
        ) {
            return
        }
        val generation = ++groupMembersGeneration
        mutableState.value = mutableState.value.copy(
            groupMembersUpdatingId = groupId,
            error = null,
        )
        viewModelScope.launch {
            runCatching {
                withContext(Dispatchers.IO) {
                    coordinator.agentSetGroupMembers(groupId, members)
                }
            }.onSuccess {
                if (
                    generation == groupMembersGeneration &&
                    mutableState.value.groupMembersUpdatingId == groupId
                ) {
                    mutableState.value = mutableState.value.copy(groupMembersUpdatingId = null)
                    refreshBots()
                    onUpdated?.invoke()
                }
            }.onFailure { error ->
                if (
                    generation == groupMembersGeneration &&
                    mutableState.value.groupMembersUpdatingId == groupId
                ) {
                    mutableState.value = mutableState.value.copy(
                        groupMembersUpdatingId = null,
                        error = error.message ?: "Agent group member update failed",
                    )
                }
            }
        }
    }

    fun createBot(name: String, description: String, onCreated: (() -> Unit)? = null) {
        val cleanName = name.replace(Regex("\\s+"), " ").trim().take(72)
        if (cleanName.isBlank() || mutableState.value.creating) return
        mutableState.value = mutableState.value.copy(creating = true, error = null)
        viewModelScope.launch {
            runCatching {
                withContext(Dispatchers.IO) {
                    coordinator.agentCreate(cleanName, description.trim().take(240))
                }
            }.onSuccess {
                mutableState.value = mutableState.value.copy(creating = false)
                refreshBots()
                onCreated?.invoke()
            }.onFailure { error ->
                mutableState.value = mutableState.value.copy(creating = false, error = error.message ?: "Bot creation failed")
            }
        }
    }

    fun renameBot(botId: String, name: String) {
        val bot = mutableState.value.bots.firstOrNull { it.id == botId } ?: return
        val committed = committedAgentName(bot.name, name) ?: return
        viewModelScope.launch {
            runCatching {
                withContext(Dispatchers.IO) {
                    coordinator.agentUpdate(botId, committed, bot.description)
                }
            }.onSuccess {
                refreshBots()
            }.onFailure { error ->
                mutableState.value = mutableState.value.copy(error = error.message ?: "Agent rename failed")
            }
        }
    }

    fun beginAgentSettings(agentId: String) {
        if (mutableState.value.bots.none { it.id == agentId }) return
        agentSettingsFence.select(agentId, rosterSelectionAccountSlot)
    }

    fun endAgentSettings(agentId: String) {
        agentSettingsFence.clear(agentId)
    }

    fun updateBotProfile(
        botId: String,
        name: String,
        title: String?,
        description: String,
        avatarShape: String?,
        avatarColor: String?,
    ) {
        val bot = mutableState.value.bots.firstOrNull { it.id == botId } ?: return
        val committed = committedAgentProfile(
            initialName = bot.name,
            initialTitle = bot.title,
            initialDescription = bot.description,
            draftName = name,
            draftTitle = title,
            draftDescription = description,
            initialAvatarShape = bot.avatarShape,
            initialAvatarColor = bot.avatarColor,
            draftAvatarShape = avatarShape,
            draftAvatarColor = avatarColor,
        ) ?: return
        val accountSlot = rosterSelectionAccountSlot
        val token = agentSettingsFence.beginMutation(botId, accountSlot) ?: return
        viewModelScope.launch {
            runCatching {
                withContext(Dispatchers.IO) {
                    coordinator.agentUpdateProfile(
                        botId, committed.name, committed.description, committed.title,
                        committed.avatarShape, committed.avatarColor,
                    )
                }
            }.onSuccess {
                if (accountSlot == rosterSelectionAccountSlot) {
                    refreshBots()
                }
            }.onFailure { error ->
                if (agentSettingsFence.isCurrent(token) && accountSlot == rosterSelectionAccountSlot) {
                    mutableState.value = mutableState.value.copy(error = error.message ?: "Agent profile update failed")
                }
            }
            agentSettingsFence.finish(token)
        }
    }

    fun setBotNotifyOnUpdates(botId: String, isEnabled: Boolean) {
        val bot = mutableState.value.bots.firstOrNull { it.id == botId && !it.isGroup } ?: return
        if (bot.notifyOnUpdatesEnabled == isEnabled) return
        val accountSlot = rosterSelectionAccountSlot
        val token = agentSettingsFence.beginMutation(botId, accountSlot) ?: return
        viewModelScope.launch {
            runCatching {
                withContext(Dispatchers.IO) {
                    coordinator.agentSetNotifyOnUpdates(botId, isEnabled)
                }
            }.onSuccess {
                if (accountSlot == rosterSelectionAccountSlot) {
                    refreshBots()
                }
            }.onFailure { error ->
                if (agentSettingsFence.isCurrent(token) && accountSlot == rosterSelectionAccountSlot) {
                    mutableState.value = mutableState.value.copy(error = error.message ?: "Agent notification update failed")
                }
            }
            agentSettingsFence.finish(token)
        }
    }

    fun hideBot(botId: String) {
        mutateAgent(botId, "Hide agent failed") { coordinator.agentSetHidden(botId, true) }
    }

    fun setBotUnread(botId: String, isUnread: Boolean) {
        mutateAgent(botId, "Unread state update failed") { coordinator.agentSetUnread(botId, isUnread) }
    }

    fun duplicateBot(botId: String) {
        mutateAgent(botId, "Agent duplication failed") { coordinator.agentDuplicate(botId) }
    }

    fun deleteBot(botId: String) {
        viewModelScope.launch {
            runCatching { deleteBotAndAwait(botId) }
                .onFailure { error ->
                    mutableState.value = mutableState.value.copy(
                        error = error.message ?: "Agent deletion failed",
                    )
                }
        }
    }

    suspend fun deleteBotAndAwait(botId: String) {
        withContext(Dispatchers.IO) {
            coordinator.agentDelete(botId)
        }
        if (mutableState.value.activeBot?.id == botId) {
            commitState(
                mutableState.value.copy(
                    activeBot = null,
                    busy = false,
                    operationId = null,
                    pendingApproval = null,
                ),
            )
        }
        refreshBots()
    }

    fun setBotPinned(botId: String, isPinned: Boolean) {
        val current = mutableState.value.bots.filter { it.isPinned }.map { it.id }.toMutableList()
        if (isPinned) {
            if (botId !in current) current += botId
        } else {
            current.removeAll { it == botId }
        }
        viewModelScope.launch {
            runCatching {
                withContext(Dispatchers.IO) { coordinator.agentSetPinned(current) }
            }.onSuccess {
                refreshBots()
            }.onFailure { error ->
                mutableState.value = mutableState.value.copy(error = error.message ?: "Pin state update failed")
            }
        }
    }

    fun moveBotToSection(botId: String, sectionId: String) {
        val bot = mutableState.value.bots.firstOrNull { it.id == botId } ?: return
        if (bot.isPinned || bot.isHidden) return
        val next = moveAgentToSidebarSection(
            mutableState.value.sidebarSections,
            botId,
            sectionId,
        ) ?: return
        commitSidebarSections(next, "Move Agent to section failed")
    }

    fun moveBotToNewSection(botId: String) {
        val bot = mutableState.value.bots.firstOrNull { it.id == botId } ?: return
        if (bot.isPinned || bot.isHidden) return
        val next = createSidebarSectionForAgent(
            mutableState.value.sidebarSections,
            botId,
            "section-${UUID.randomUUID()}",
        ) ?: return
        commitSidebarSections(next, "Create Agent section failed")
    }

    private fun commitSidebarSections(
        sections: List<AndroidSidebarSection>,
        fallbackMessage: String,
    ) {
        viewModelScope.launch {
            runCatching {
                withContext(Dispatchers.IO) { coordinator.agentSetSidebarSections(sections) }
            }.onSuccess { settled ->
                mutableState.value = mutableState.value.copy(
                    sidebarSections = settled,
                    error = null,
                )
            }.onFailure { error ->
                mutableState.value = mutableState.value.copy(
                    error = error.message ?: fallbackMessage,
                )
            }
        }
    }

    private fun mutateAgent(
        botId: String,
        fallbackMessage: String,
        mutation: () -> JSONObject,
    ) {
        viewModelScope.launch {
            runCatching {
                withContext(Dispatchers.IO) { mutation() }
            }.onSuccess {
                refreshBots()
            }.onFailure { error ->
                mutableState.value = mutableState.value.copy(error = error.message ?: fallbackMessage)
            }
        }
    }

    fun openBot(bot: MobileBotSummaryAndroid, targetEntryId: String? = null) {
        rosterSelection.select(bot.id)
        openBotGeneration += 1
        sendPresentationGeneration += 1
        val generation = openBotGeneration
        val cachedMessages = messagesByBot[bot.id].orEmpty()
        val baselineEntryIds = cachedMessages.mapTo(linkedSetOf(), MobileChatMessage::id)
        commitState(
            mutableState.value.copy(
                activeBot = bot,
                draft = draftsByBot[bot.id].orEmpty(),
                messages = cachedMessages,
                pendingApproval = null,
                widgetPendingEntryIds = emptySet(),
                editorSuggestions = MobileEditorSuggestionSnapshot(),
                messageTargetId = targetEntryId,
                error = null,
            ),
        )
        refreshEditorSuggestions(mutableState.value.draft)
        if (bot.miniAppId != null) {
            rosterSelection.settle(bot.id)
            return
        }

        viewModelScope.launch {
            try {
                val durable = withContext(Dispatchers.IO) {
                    loadDurableAgentTranscript(bot.id, rosterSelectionAccountSlot)
                }
                if (generation != openBotGeneration || mutableState.value.activeBot?.id != bot.id) return@launch
                val merged = mergeCanonicalMobileTranscript(
                    baselineEntryIds = baselineEntryIds,
                    current = mutableState.value.messages,
                    canonical = durable,
                )
                commitState(
                    mutableState.value.copy(
                        messages = merged,
                        messageTargetId = targetEntryId,
                        error = null,
                    ),
                )
                rosterSelection.settle(bot.id)
            } catch (error: CancellationException) {
                throw error
            } catch (error: Throwable) {
                if (generation == openBotGeneration && mutableState.value.activeBot?.id == bot.id) {
                    commitState(
                        mutableState.value.copy(
                            error = error.message ?: "Bot transcript restore failed",
                        ),
                    )
                    rosterSelection.settle(bot.id)
                }
            }
        }
    }

    fun consumeMessageTarget(entryId: String) {
        if (mutableState.value.messageTargetId == entryId) {
            commitState(mutableState.value.copy(messageTargetId = null))
        }
    }

    fun closeBot() {
        if (mutableState.value.busy) return
        rosterSelection.select(null)
        openBotGeneration += 1
        sendPresentationGeneration += 1
        cancelEditorSuggestionRequest()
        commitState(
            mutableState.value.copy(
                activeBot = null,
                draft = "",
                messages = emptyList(),
                pendingApproval = null,
                widgetPendingEntryIds = emptySet(),
                editorSuggestions = MobileEditorSuggestionSnapshot(),
                messageTargetId = null,
                error = null,
            ),
        )
    }

    fun resetAccountScope() {
        rosterSelection.reset()
        rosterSelectionAccountSlot = null
        openBotGeneration += 1
        sendPresentationGeneration += 1
        cancelEditorSuggestionRequest()
        asyncTasksGeneration += 1
        asyncTasksJob?.cancel()
        asyncTasksJob = null
        groupMembersGeneration += 1
        computerRebuildGeneration += 1
        agentSettingsFence.clear()
        messagesByBot.clear()
        draftsByBot.clear()
        commitState(MobileBotUiState())
    }

    fun openAsyncTasks(bot: MobileBotSummaryAndroid) {
        asyncTasksGeneration += 1
        asyncTasksJob?.cancel()
        asyncTasksJob = null
        commitState(
            mutableState.value.copy(
                asyncTasksAgent = bot,
                asyncTasks = emptyList(),
                asyncTasksLoading = true,
                asyncTasksError = null,
            ),
        )
        refreshAsyncTasks()
    }

    fun closeAsyncTasks() {
        asyncTasksGeneration += 1
        asyncTasksJob?.cancel()
        asyncTasksJob = null
        commitState(
            mutableState.value.copy(
                asyncTasksAgent = null,
                asyncTasks = emptyList(),
                asyncTasksLoading = false,
                asyncTasksError = null,
            ),
        )
    }

    fun refreshAsyncTasks() {
        val agent = mutableState.value.asyncTasksAgent ?: return
        val generation = asyncTasksGeneration
        asyncTasksJob?.cancel()
        asyncTasksJob = viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                runCatching { projectMobileAsyncTasks(coordinator.agentAsyncTasks(agent.id)) }
            }
            if (
                generation != asyncTasksGeneration ||
                mutableState.value.asyncTasksAgent?.id != agent.id
            ) {
                return@launch
            }
            result.onSuccess { tasks ->
                commitState(
                    mutableState.value.copy(
                        asyncTasks = tasks,
                        asyncTasksLoading = false,
                        asyncTasksError = null,
                    ),
                )
            }.onFailure { error ->
                commitState(
                    mutableState.value.copy(
                        asyncTasksLoading = false,
                        asyncTasksError = error.message ?: "Async tasks refresh failed",
                    ),
                )
            }
        }
    }

    fun setDraft(value: String) {
        commitState(mutableState.value.copy(draft = value))
        refreshEditorSuggestions(value)
    }

    private fun cancelEditorSuggestionRequest() {
        editorSuggestionJob?.cancel()
        editorSuggestionJob = null
        editorSuggestionFence.invalidate()
    }

    private fun refreshEditorSuggestions(draft: String) {
        val query = findMobileEditorSuggestionQuery(draft)
        if (query == null) {
            cancelEditorSuggestionRequest()
            if (mutableState.value.editorSuggestions.status != MobileEditorSuggestionStatus.IDLE) {
                commitState(mutableState.value.copy(editorSuggestions = MobileEditorSuggestionSnapshot()))
            }
            return
        }
        val bot = mutableState.value.activeBot
        val accountSlot = rosterSelectionAccountSlot
        if (bot == null || bot.miniAppId != null || accountSlot.isNullOrBlank()) {
            cancelEditorSuggestionRequest()
            commitState(
                mutableState.value.copy(
                    editorSuggestions = MobileEditorSuggestionSnapshot(
                        status = MobileEditorSuggestionStatus.UNAVAILABLE,
                        trigger = query.trigger,
                        query = query.query,
                        replaceFrom = query.replaceFrom,
                    ),
                ),
            )
            return
        }

        editorSuggestionJob?.cancel()
        val token = editorSuggestionFence.begin(accountSlot, bot.id) ?: return
        val previous = mutableState.value.editorSuggestions.rows
        commitState(
            mutableState.value.copy(
                editorSuggestions = MobileEditorSuggestionSnapshot(
                    status = MobileEditorSuggestionStatus.LOADING,
                    rows = previous,
                    trigger = query.trigger,
                    query = query.query,
                    replaceFrom = query.replaceFrom,
                ),
            ),
        )
        editorSuggestionJob = viewModelScope.launch {
            try {
                val rows = withContext(Dispatchers.IO) {
                    projectMobileEditorSuggestions(
                        agents = coordinator.agentList(),
                        automations = coordinator.automationList(),
                        activeAgentId = bot.id,
                        trigger = query.trigger,
                        query = query.query,
                    )
                }
                if (!editorSuggestionFence.accepts(token, rosterSelectionAccountSlot, mutableState.value.activeBot?.id)) {
                    return@launch
                }
                commitState(
                    mutableState.value.copy(
                        editorSuggestions = MobileEditorSuggestionSnapshot(
                            status = if (rows.isEmpty()) MobileEditorSuggestionStatus.EMPTY else MobileEditorSuggestionStatus.READY,
                            rows = rows,
                            trigger = query.trigger,
                            query = query.query,
                            replaceFrom = query.replaceFrom,
                        ),
                    ),
                )
            } catch (error: CancellationException) {
                throw error
            } catch (_: Throwable) {
                if (editorSuggestionFence.accepts(token, rosterSelectionAccountSlot, mutableState.value.activeBot?.id)) {
                    commitState(
                        mutableState.value.copy(
                            editorSuggestions = MobileEditorSuggestionSnapshot(
                                status = MobileEditorSuggestionStatus.FAILED,
                                rows = previous,
                                trigger = query.trigger,
                                query = query.query,
                                replaceFrom = query.replaceFrom,
                            ),
                        ),
                    )
                }
            }
        }
    }

    fun resetPaletteRoutines() {
        paletteRoutineJob?.cancel()
        paletteRoutineJob = null
        paletteRoutineFence.cancel()
        mutableState.value = mutableState.value.copy(
            paletteRoutines = CommandPaletteRoutineSnapshot(
                status = CommandPaletteRoutineStatus.IDLE,
            ),
        )
    }

    fun refreshPaletteRoutines() {
        paletteRoutineJob?.cancel()
        paletteRoutineJob = null
        paletteRoutineFence.cancel()
        val token = paletteRoutineFence.begin()
        val previous = mutableState.value.paletteRoutines.value
        mutableState.value = mutableState.value.copy(
            paletteRoutines = CommandPaletteRoutineSnapshot(
                status = CommandPaletteRoutineStatus.LOADING,
                value = previous,
            ),
        )
        paletteRoutineJob = viewModelScope.launch {
            try {
                val raw = withContext(Dispatchers.IO) { coordinator.automationList() }
                val routines = commandPaletteRoutinesFromAutomationList(raw)
                if (!paletteRoutineFence.accepts(token)) return@launch
                mutableState.value = mutableState.value.copy(
                    paletteRoutines = CommandPaletteRoutineSnapshot(
                        status = if (routines.isEmpty()) {
                            CommandPaletteRoutineStatus.EMPTY
                        } else {
                            CommandPaletteRoutineStatus.READY
                        },
                        value = routines,
                    ),
                )
            } catch (error: CancellationException) {
                throw error
            } catch (_: Throwable) {
                if (paletteRoutineFence.accepts(token)) {
                    mutableState.value = mutableState.value.copy(
                        paletteRoutines = CommandPaletteRoutineSnapshot(
                            status = CommandPaletteRoutineStatus.FAILED,
                            value = previous,
                        ),
                    )
                }
            }
        }
    }

    fun resetPaletteMessageSearch() {
        paletteMessageSearchJob?.cancel()
        paletteMessageSearchJob = null
        paletteMessageFence.cancel()
        mutableState.value = mutableState.value.copy(
            paletteMessageSearch = CommandPaletteMessageSnapshot(
                status = CommandPaletteMessageStatus.IDLE,
            ),
        )
    }

    fun setPaletteMessageQuery(query: String) {
        paletteMessageSearchJob?.cancel()
        paletteMessageSearchJob = null
        paletteMessageFence.cancel()
        val normalized = query.trim()
        if (normalized.isEmpty()) {
            mutableState.value = mutableState.value.copy(
                paletteMessageSearch = CommandPaletteMessageSnapshot(
                    status = CommandPaletteMessageStatus.IDLE,
                ),
            )
            return
        }

        val token = paletteMessageFence.begin()
        mutableState.value = mutableState.value.copy(
            paletteMessageSearch = CommandPaletteMessageSnapshot(
                status = CommandPaletteMessageStatus.LOADING,
            ),
        )
        paletteMessageSearchJob = viewModelScope.launch {
            try {
                delay(COMMAND_PALETTE_MESSAGE_DEBOUNCE_MS)
                val transcript = withContext(Dispatchers.IO) { coordinator.transcriptSnapshot() }
                val results = commandPaletteMessagesFromTranscript(transcript, normalized)
                if (!paletteMessageFence.accepts(token)) return@launch
                mutableState.value = mutableState.value.copy(
                    paletteMessageSearch = CommandPaletteMessageSnapshot(
                        status = if (results.isEmpty()) {
                            CommandPaletteMessageStatus.EMPTY
                        } else {
                            CommandPaletteMessageStatus.READY
                        },
                        value = results,
                    ),
                )
            } catch (error: CancellationException) {
                throw error
            } catch (_: Throwable) {
                if (paletteMessageFence.accepts(token)) {
                    mutableState.value = mutableState.value.copy(
                        paletteMessageSearch = CommandPaletteMessageSnapshot(
                            status = CommandPaletteMessageStatus.FAILED,
                        ),
                    )
                }
            }
        }
    }

    fun send() {
        val snapshot = mutableState.value
        val bot = snapshot.activeBot ?: return
        val text = snapshot.draft.trim()
        if (text.isBlank() || snapshot.busy) return
        val miniAppId = bot.miniAppId
        if (!miniAppId.isNullOrBlank()) {
            val requestId = "android-mobile-bot-chat-${UUID.randomUUID()}"
            commitState(
                snapshot.copy(
                    draft = "",
                    busy = true,
                    error = null,
                    operationId = requestId,
                    pendingApproval = null,
                    messageTargetId = null,
                    messages = snapshot.messages + MobileChatMessage(
                        requestId,
                        MobileChatRole.USER,
                        text,
                    ),
                ),
            )
            sendMiniApp(miniAppId, text, requestId)
            return
        }

        val accountSlot = rosterSelectionAccountSlot
        val scopeGeneration = sendPresentationGeneration
        val nonce = "android-mobile-bot-chat-${UUID.randomUUID()}"
        commitState(
            snapshot.copy(
                busy = true,
                error = null,
                pendingApproval = null,
                messageTargetId = null,
            ),
        )
        viewModelScope.launch {
            try {
                val acknowledgement = withContext(Dispatchers.IO) {
                    coordinator.sendAcknowledgementBegin(
                        accountSlot = accountSlot,
                        agentId = bot.id,
                        nonce = nonce,
                        text = text,
                        entries = listOf(messageAcknowledgementEntry(nonce)),
                    )
                }
                if (!isCurrentSendScope(bot.id, accountSlot, scopeGeneration)) return@launch
                commitState(mutableState.value.copy(draft = ""))
                refreshDurableSendProjection(bot.id, accountSlot, scopeGeneration)

                val dispatched = withContext(Dispatchers.IO) {
                    coordinator.sendAcknowledgementDispatch(
                        accountSlot = accountSlot,
                        agentId = bot.id,
                        nonce = acknowledgement.nonce,
                        generation = acknowledgement.generation,
                    )
                }
                if (!isCurrentSendScope(bot.id, accountSlot, scopeGeneration)) return@launch
                val operationId = dispatched.operationId
                commitState(
                    mutableState.value.copy(
                        operationId = operationId,
                        messages = mutableState.value.messages.filterNot {
                            it.kind == MobileChatEntryKind.THINKING &&
                                it.operationId == operationId
                        } + MobileChatMessage(
                            id = "thinking:$operationId",
                            role = MobileChatRole.ASSISTANT,
                            text = "",
                            kind = MobileChatEntryKind.THINKING,
                            operationId = operationId,
                            actionTitle = "Thinking",
                            actionStatus = "running",
                        ),
                    ),
                )
                refreshDurableSendProjection(bot.id, accountSlot, scopeGeneration)
            } catch (error: CancellationException) {
                throw error
            } catch (error: Throwable) {
                if (isCurrentSendScope(bot.id, accountSlot, scopeGeneration)) {
                    runCatching {
                        refreshDurableSendProjection(bot.id, accountSlot, scopeGeneration)
                    }
                    commitState(
                        mutableState.value.copy(
                            busy = false,
                            operationId = null,
                            error = error.message ?: "Bot run failed",
                        ),
                    )
                }
            }
        }
    }

    fun respondToWidget(entryId: String, value: String) {
        runWidgetAction(entryId, value.trim().takeIf(String::isNotEmpty))
    }

    fun dismissWidget(entryId: String) {
        runWidgetAction(entryId, null)
    }

    private fun runWidgetAction(entryId: String, response: String?) {
        val snapshot = mutableState.value
        val bot = snapshot.activeBot ?: return
        if (bot.miniAppId != null || entryId.isBlank()) return
        val widget = snapshot.messages.firstOrNull {
            it.id == entryId && it.kind == MobileChatEntryKind.WIDGET
        }?.widget ?: return
        if (widget.respondedValue != null || widget.dismissed) return
        if (snapshot.widgetPendingEntryIds.isNotEmpty()) return
        if (response != null && response.isBlank()) return

        val accountSlot = rosterSelectionAccountSlot
        val scopeGeneration = sendPresentationGeneration
        commitState(
            snapshot.copy(
                widgetPendingEntryIds = snapshot.widgetPendingEntryIds + entryId,
                error = null,
            ),
        )
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                runCatching {
                    if (response == null) {
                        coordinator.dismissWidget(
                            accountSlot = accountSlot,
                            agentId = bot.id,
                            entryId = entryId,
                            generation = scopeGeneration,
                        )
                    } else {
                        coordinator.respondToWidget(
                            accountSlot = accountSlot,
                            agentId = bot.id,
                            entryId = entryId,
                            value = response,
                            generation = scopeGeneration,
                        )
                    }
                }
            }
            if (!isCurrentSendScope(bot.id, accountSlot, scopeGeneration)) return@launch
            val withoutPending = mutableState.value.widgetPendingEntryIds - entryId
            val action = result.getOrNull()
            if (result.isFailure || action?.accepted != true) {
                commitState(
                    mutableState.value.copy(
                        widgetPendingEntryIds = withoutPending,
                        error = result.exceptionOrNull()?.message
                            ?: action?.reason
                            ?: "Widget action was rejected",
                    ),
                )
                return@launch
            }
            try {
                val continuationOperationId = action.operationId
                if (response != null && continuationOperationId != null) {
                    commitState(
                        mutableState.value.copy(
                            busy = true,
                            operationId = continuationOperationId,
                            widgetPendingEntryIds = withoutPending,
                            messages = mutableState.value.messages.filterNot {
                                it.kind == MobileChatEntryKind.THINKING &&
                                    it.operationId == continuationOperationId
                            } + MobileChatMessage(
                                id = "thinking:$continuationOperationId",
                                role = MobileChatRole.ASSISTANT,
                                text = "",
                                kind = MobileChatEntryKind.THINKING,
                                operationId = continuationOperationId,
                                actionTitle = "Thinking",
                                actionStatus = "running",
                            ),
                        ),
                    )
                }
                refreshDurableSendProjection(bot.id, accountSlot, scopeGeneration)
                if (isCurrentSendScope(bot.id, accountSlot, scopeGeneration)) {
                    commitState(
                        mutableState.value.copy(
                            widgetPendingEntryIds = mutableState.value.widgetPendingEntryIds - entryId,
                        ),
                    )
                }
            } catch (error: CancellationException) {
                throw error
            } catch (error: Throwable) {
                if (isCurrentSendScope(bot.id, accountSlot, scopeGeneration)) {
                    commitState(
                        mutableState.value.copy(
                            widgetPendingEntryIds = withoutPending,
                            error = error.message ?: "Widget reconciliation failed",
                        ),
                    )
                }
            }
        }
    }

    fun retrySend(nonce: String) {
        val snapshot = mutableState.value
        val bot = snapshot.activeBot ?: return
        if (bot.miniAppId != null || snapshot.busy || nonce.isBlank()) return
        val failed = snapshot.messages.firstOrNull {
            it.clientNonce == nonce && isExplicitSendRetryAllowed(it)
        } ?: return
        val generation = failed.sendAcknowledgementGeneration ?: return
        val accountSlot = rosterSelectionAccountSlot
        val scopeGeneration = sendPresentationGeneration
        val freshNonce = "sand-resend-v1:${nonce.length}:$nonce:${UUID.randomUUID()}"
        commitState(snapshot.copy(busy = true, error = null))
        viewModelScope.launch {
            try {
                val dispatched = withContext(Dispatchers.IO) {
                    coordinator.sendAcknowledgementRetry(
                        accountSlot = accountSlot,
                        agentId = bot.id,
                        nonce = nonce,
                        generation = generation,
                        freshNonce = freshNonce,
                        entries = listOf(messageAcknowledgementEntry(freshNonce)),
                    )
                }
                if (!isCurrentSendScope(bot.id, accountSlot, scopeGeneration)) return@launch
                val operationId = dispatched.operationId
                refreshDurableSendProjection(bot.id, accountSlot, scopeGeneration)
                commitState(
                    mutableState.value.copy(
                        operationId = operationId,
                        messages = mutableState.value.messages.filterNot {
                            it.kind == MobileChatEntryKind.THINKING &&
                                it.operationId == operationId
                        } + MobileChatMessage(
                            id = "thinking:$operationId",
                            role = MobileChatRole.ASSISTANT,
                            text = "",
                            kind = MobileChatEntryKind.THINKING,
                            operationId = operationId,
                            actionTitle = "Thinking",
                            actionStatus = "running",
                        ),
                    ),
                )
                refreshDurableSendProjection(bot.id, accountSlot, scopeGeneration)
            } catch (error: CancellationException) {
                throw error
            } catch (error: Throwable) {
                if (isCurrentSendScope(bot.id, accountSlot, scopeGeneration)) {
                    runCatching {
                        refreshDurableSendProjection(bot.id, accountSlot, scopeGeneration)
                    }
                    commitState(
                        mutableState.value.copy(
                            busy = false,
                            operationId = null,
                            error = error.message ?: "Retry failed",
                        ),
                    )
                }
            }
        }
    }

    private fun messageAcknowledgementEntry(nonce: String): AndroidSendAcknowledgementEntry =
        AndroidSendAcknowledgementEntry(
            id = nonce,
            kind = "message",
            clientNonce = nonce,
        )

    private fun isCurrentSendScope(
        agentId: String,
        accountSlot: String?,
        scopeGeneration: Long,
    ): Boolean =
        scopeGeneration == sendPresentationGeneration &&
            mutableState.value.activeBot?.id == agentId &&
            rosterSelectionAccountSlot == accountSlot

    private fun loadDurableAgentTranscript(
        agentId: String,
        accountSlot: String?,
    ): List<MobileChatMessage> {
        val transcript = coordinator.transcriptSnapshot()
        val canonical = canonicalMobileTranscriptForAgent(transcript, agentId)
        val acknowledgements = coordinator.sendAcknowledgementSnapshot(accountSlot, agentId)
        val durableMessages = projectDurableSendAcknowledgements(canonical, acknowledgements)
        return joinClientSideToolResultCards(
            canonical = durableMessages,
            events = coordinator.replayClientSideToolV2(),
            agentId = agentId,
        )
    }

    private suspend fun refreshDurableSendProjection(
        agentId: String,
        accountSlot: String?,
        scopeGeneration: Long,
    ) {
        val durable = withContext(Dispatchers.IO) {
            loadDurableAgentTranscript(agentId, accountSlot)
        }
        if (!isCurrentSendScope(agentId, accountSlot, scopeGeneration)) return
        val merged = mergeCanonicalMobileTranscript(
            baselineEntryIds = emptySet(),
            current = mutableState.value.messages,
            canonical = durable,
        )
        commitState(mutableState.value.copy(messages = merged))
    }

    private fun sendMiniApp(pluginId: String, text: String, operationId: String) {
        commitState(
            mutableState.value.copy(
                messages = mutableState.value.messages + MobileChatMessage(
                    id = "thinking:$operationId",
                    role = MobileChatRole.ASSISTANT,
                    text = "",
                    kind = MobileChatEntryKind.THINKING,
                    operationId = operationId,
                    actionTitle = "正在处理",
                    actionDetail = "正在调用 Mini App…",
                    actionStatus = "running",
                ),
            ),
        )
        viewModelScope.launch {
            runCatching {
                withContext(Dispatchers.IO) {
                    val routed = miniApps.routeInput(pluginId, text)
                    val execution = routed.optJSONObject("execution")
                    if (execution == null) {
                        return@withContext routed.optString("message").ifBlank {
                            "全球法布施没有把这条输入解析成可执行命令。"
                        }
                    }
                    val command = routed.optJSONObject("command")
                    val slash = command?.optString("slash").orEmpty()
                    if (routed.optBoolean("requiresApproval", false)) {
                        return@withContext listOf(
                            "已通过统一 Mini App 路由解析${if (slash.isBlank()) "" else "为 $slash"}。",
                            "该 Tool 需要宿主明确批准；Android 当前不会静默执行写入/破坏性调用。",
                        ).joinToString("\n")
                    }
                    val kind = execution.optString("kind")
                    val tool = execution.optString("tool")
                    check(kind == "mcp-http") { "Android Mini App Bot does not support execution surface $kind" }
                    check(tool.isNotBlank()) { "Mini App route did not return an MCP tool" }
                    val arguments = routed.optJSONObject("arguments") ?: JSONObject()
                    val result = miniApps.callOfficialMcpTool(pluginId, tool, arguments)
                    mcpResultText(result)
                }
            }.onSuccess { reply ->
                commitState(mutableState.value.copy(
                    busy = false,
                    operationId = null,
                    messages = mutableState.value.messages.filterNot { it.kind == MobileChatEntryKind.THINKING && it.operationId == operationId } + MobileChatMessage(
                        id = "assistant:$operationId",
                        role = MobileChatRole.ASSISTANT,
                        text = reply,
                        operationId = operationId,
                    ),
                ))
            }.onFailure { error ->
                commitState(mutableState.value.copy(
                    busy = false,
                    operationId = null,
                    error = error.message ?: "Mini App WebMCP call failed",
                    messages = mutableState.value.messages.filterNot { it.kind == MobileChatEntryKind.THINKING && it.operationId == operationId } + MobileChatMessage(
                        id = "assistant:$operationId:error",
                        role = MobileChatRole.ASSISTANT,
                        text = "Mini App 调用失败：${error.message ?: "unknown error"}",
                        operationId = operationId,
                    ),
                ))
            }
        }
    }

    private fun mcpResultText(result: JSONObject): String {
        val content = result.optJSONArray("content")
        if (content != null) {
            val text = buildList {
                for (index in 0 until content.length()) {
                    val item = content.optJSONObject(index) ?: continue
                    if (item.optString("type") == "text") item.optString("text").takeIf(String::isNotBlank)?.let(::add)
                }
            }.joinToString("\n")
            if (text.isNotBlank()) return text
        }
        val structured = result.optJSONObject("structuredContent")
        return structured?.toString(2) ?: result.toString(2)
    }

    fun resolveApproval(approved: Boolean) {
        val pending = mutableState.value.pendingApproval ?: return
        if (pending.resolving) return
        commitState(mutableState.value.copy(pendingApproval = pending.copy(resolving = true), error = null))
        viewModelScope.launch {
            runCatching {
                withContext(Dispatchers.IO) {
                    coordinator.featureApprovalResolve(
                        JSONObject()
                            .put("approvalId", pending.approvalId)
                            .put("approved", approved),
                    )
                }
            }.onSuccess {
                if (mutableState.value.pendingApproval?.approvalId == pending.approvalId) {
                    commitState(mutableState.value.copy(pendingApproval = null))
                }
            }.onFailure { error ->
                val current = mutableState.value.pendingApproval
                if (current?.approvalId == pending.approvalId) {
                    commitState(
                        mutableState.value.copy(
                            pendingApproval = current.copy(resolving = false),
                            error = error.message ?: "Approval resolution failed",
                        ),
                    )
                }
            }
        }
    }

    fun stop() {
        val operationId = mutableState.value.operationId ?: return
        if (mutableState.value.activeBot?.miniAppId != null) return
        viewModelScope.launch {
            runCatching { withContext(Dispatchers.IO) { coordinator.featureInterrupt( JSONObject().put("operationId", operationId)) } }
        }
    }

    private fun handleOperationEvent(event: JSONObject) {
        val type = event.optString("type")
        if (type == "agents" || type == "agent-upserted" || type == "agent-deleted") {
            refreshBots()
            return
        }
        if (type == "agent.async-tasks.changed") {
            val visibleAgentId = mutableState.value.asyncTasksAgent?.id ?: return
            if (event.optString("parentAgentId") == visibleAgentId) {
                refreshAsyncTasks()
            }
            return
        }
        if (type == "agent.async-tasks.refresh") {
            if (mutableState.value.asyncTasksAgent != null) {
                refreshAsyncTasks()
            }
            return
        }
        if (type == "client-side-tool-v2") {
            val active = mutableState.value.activeBot ?: return
            val accountSlot = rosterSelectionAccountSlot
            val scopeGeneration = sendPresentationGeneration
            val wireEvent = event.optJSONObject("event")
                ?: event.optJSONObject("clientSideToolV2")
                ?: return
            viewModelScope.launch {
                val result = withContext(Dispatchers.IO) {
                    runCatching { coordinator.acceptClientSideToolV2(wireEvent) }
                }
                if (!isCurrentSendScope(active.id, accountSlot, scopeGeneration)) return@launch
                val accepted = result.getOrElse { error ->
                    commitState(
                        mutableState.value.copy(
                            error = error.message ?: "Tool result projection was rejected",
                        ),
                    )
                    return@launch
                }
                if (accepted.accountSlot != "host" || accepted.agentId != active.id) return@launch
                runCatching {
                    refreshDurableSendProjection(active.id, accountSlot, scopeGeneration)
                }.onFailure { error ->
                    if (isCurrentSendScope(active.id, accountSlot, scopeGeneration)) {
                        commitState(
                            mutableState.value.copy(
                                error = error.message ?: "Tool result reconciliation failed",
                            ),
                        )
                    }
                }
            }
            return
        }

        val operationId = mutableState.value.operationId ?: return
        if (!mutableState.value.busy) return
        val eventOperationId = event.optString("operationId").ifBlank { operationId }
        if (type in setOf("chat.message", "chat.delta", "agent.step", "operation.started", "operation.completed", "operation.interrupted", "operation.failed", "model.routed", "approval.requested", "approval.resolved") && eventOperationId != operationId) {
            return
        }
        when (type) {
            "chat.message" -> if (event.optString("role") != "user") {
                removeThinking(operationId)
                upsertAssistant(operationId, event.optString("text"), append = false, streaming = false)
            }
            "chat.delta" -> {
                removeThinking(operationId)
                upsertAssistant(operationId, event.optString("delta"), append = true, streaming = true)
            }
            "agent.step" -> {
                val id = "action:" + operationId + ":" + event.optString("stepId").ifBlank { UUID.randomUUID().toString() }
                upsert(
                    MobileChatMessage(
                        id = id,
                        role = MobileChatRole.ASSISTANT,
                        text = "",
                        kind = MobileChatEntryKind.ACTION,
                        operationId = operationId,
                        actionTitle = event.optString("title").ifBlank { "Working" },
                        actionDetail = event.optString("detail"),
                        actionStatus = event.optString("status").ifBlank { "completed" },
                    ),
                )
            }
            "model.routed" -> {
                val detail = listOf(event.optString("provider"), event.optString("model")).filter { it.isNotBlank() }.joinToString(" · ")
                upsert(MobileChatMessage("action:" + operationId + ":model", MobileChatRole.ASSISTANT, "", MobileChatEntryKind.ACTION, operationId, "Model", detail, "completed"))
            }
            "approval.requested" -> {
                projectPendingAgentApproval(event, operationId)?.let { pending ->
                    commitState(mutableState.value.copy(pendingApproval = pending))
                }
            }
            "approval.resolved" -> {
                val approvalId = event.optString("approvalId")
                if (approvalId.isNotBlank() && mutableState.value.pendingApproval?.approvalId == approvalId) {
                    commitState(mutableState.value.copy(pendingApproval = null))
                }
            }
            "operation.completed", "operation.interrupted" -> {
                removeThinking(operationId)
                finishAssistant(operationId)
                commitState(mutableState.value.copy(busy = false, operationId = null, pendingApproval = null))
            }
            "operation.failed" -> {
                removeThinking(operationId)
                finishAssistant(operationId)
                commitState(mutableState.value.copy(busy = false, operationId = null, pendingApproval = null, error = event.optString("message").ifBlank { "Bot run failed" }))
            }
        }
    }

    private fun removeThinking(operationId: String) {
        commitState(mutableState.value.copy(messages = mutableState.value.messages.filterNot { it.kind == MobileChatEntryKind.THINKING && it.operationId == operationId }))
    }

    private fun upsertAssistant(operationId: String, text: String, append: Boolean, streaming: Boolean) {
        if (text.isBlank()) return
        val rows = mutableState.value.messages.toMutableList()
        val index = rows.indexOfLast { it.role == MobileChatRole.ASSISTANT && it.kind == MobileChatEntryKind.MESSAGE && it.operationId == operationId }
        if (index >= 0) {
            val current = rows[index]
            rows[index] = current.copy(text = if (append) current.text + text else text, streaming = streaming)
        } else {
            rows += MobileChatMessage("assistant:$operationId", MobileChatRole.ASSISTANT, text, operationId = operationId, streaming = streaming)
        }
        commitState(mutableState.value.copy(messages = rows))
    }

    private fun finishAssistant(operationId: String) {
        val rows = mutableState.value.messages.map { message ->
            if (message.role == MobileChatRole.ASSISTANT && message.operationId == operationId) {
                message.copy(streaming = false)
            } else {
                message
            }
        }
        commitState(mutableState.value.copy(messages = rows))
    }

    private fun upsert(message: MobileChatMessage) {
        val rows = mutableState.value.messages.toMutableList()
        val index = rows.indexOfFirst { it.id == message.id }
        if (index >= 0) rows[index] = message else rows += message
        commitState(mutableState.value.copy(messages = rows))
    }

    override fun onCleared() {
        paletteMessageSearchJob?.cancel()
        paletteMessageSearchJob = null
        paletteMessageFence.cancel()
        paletteRoutineJob?.cancel()
        paletteRoutineJob = null
        paletteRoutineFence.cancel()
        openBotGeneration += 1
        sendPresentationGeneration += 1
        asyncTasksGeneration += 1
        asyncTasksJob?.cancel()
        asyncTasksJob = null
        groupMembersGeneration += 1
        editorSuggestionJob?.cancel()
        editorSuggestionJob = null
        editorSuggestionFence.dispose()
        agentSettingsFence.dispose()
        rosterSelection.dispose()
        featureEventSubscription?.close()
        featureEventSubscription = null
        super.onCleared()
    }
}
