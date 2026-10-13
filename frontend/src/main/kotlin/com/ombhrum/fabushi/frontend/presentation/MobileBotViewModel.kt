package com.ombhrum.fabushi

import android.app.Application
import android.os.SystemClock
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.ombhrum.fabushi.androidpreload.runtime.AndroidAgentAttachment
import com.ombhrum.fabushi.androidpreload.runtime.AndroidAvatarCrop
import com.ombhrum.fabushi.androidpreload.runtime.AndroidAvatarImageSource
import com.ombhrum.fabushi.androidpreload.runtime.AndroidCoordinatorPort
import com.ombhrum.fabushi.androidpreload.runtime.AndroidConversationOutlineItem
import com.ombhrum.fabushi.androidpreload.runtime.AndroidFindInChatEntry
import com.ombhrum.fabushi.androidpreload.runtime.AndroidGeneratedAvatarImage
import com.ombhrum.fabushi.androidpreload.runtime.parseAndroidFindInChatEntries
import com.ombhrum.fabushi.androidpreload.runtime.AndroidSendAcknowledgementEntry
import com.ombhrum.fabushi.androidpreload.runtime.AndroidSidebarSection
import com.ombhrum.fabushi.androidpreload.runtime.AndroidSidebarSectionsSnapshot
import com.ombhrum.fabushi.frontend.presentation.MobileRenderTelemetryTracker
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject
import java.io.ByteArrayOutputStream
import java.io.File
import java.util.UUID

data class MobileBotSummaryAndroid(
    override val id: String,
    val name: String,
    val title: String? = null,
    val description: String = "",
    val notifyOnUpdatesEnabled: Boolean = false,
    val avatarShape: String? = null,
    val avatarColor: String? = null,
    val avatarDataUrl: String? = null,
    val isSharedRoom: Boolean = false,
    val currentActivityKind: String? = null,
    val currentActivityVerb: String? = null,
    val currentActivityTool: String? = null,
    val isComposingMessage: Boolean = false,
    val miniAppId: String? = null,
    val menuButtonText: String? = null,
    override val isPinned: Boolean = false,
    val hasUnread: Boolean = false,
    val unreadCount: Long = 0L,
    val lastActivityAt: Long = 0L,
    val lastViewedAt: Long = 0L,
    val isManuallyUnread: Boolean = false,
    val isHidden: Boolean = false,
    val isGroup: Boolean = false,
    val memberIds: List<String> = emptyList(),
    val conversationPartnerIds: List<String> = emptyList(),
    val awaitingUserResponse: Boolean? = null,
    val isRunning: Boolean = false,
    val lastMessage: String = "",
    val updatedAt: Long = 0L,
) : SidebarOrderAgent

enum class MobileApprovalDecision(val wireValue: String) {
    ALLOW_ONCE("allow-once"),
    DENY("deny"),
    ALWAYS("always"),
    NEVER("never"),
}

data class PendingAgentApproval(
    val approvalId: String,
    val operationId: String,
    val capability: String,
    val reason: String,
    val expiresAtMs: Long?,
    val remoteDispatch: Boolean = false,
    val resolving: Boolean = false,
)

internal fun projectPendingAgentApproval(
    event: JSONObject,
    currentOperationId: String?,
): PendingAgentApproval? {
    if (event.optString("type") != "approval.requested") return null
    val operationId = event.optString("operationId")
    val liveOperationId = currentOperationId?.trim().orEmpty()
    if (liveOperationId.isEmpty() || operationId.isBlank() || operationId != liveOperationId) return null
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
        remoteDispatch = event.optBoolean("remoteDispatch", false),
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

data class MobileConversationOutlineSubagent(
    val id: String,
    val type: String,
    val title: String,
    val status: String = "running",
)

data class MobileConversationOutlineState(
    val parentAgentId: String? = null,
    val parentAgentName: String = "",
    val selectedAgentId: String? = null,
    val items: List<AndroidConversationOutlineItem> = emptyList(),
    val subagents: List<MobileConversationOutlineSubagent> = emptyList(),
    val loading: Boolean = false,
    val error: String? = null,
) {
    val isOpen: Boolean get() = parentAgentId != null
}

internal fun projectConversationOutlineSubagents(tasks: List<MobileAsyncTask>): List<MobileConversationOutlineSubagent> =
    tasks.asSequence()
        .filter { it.kind == "subagent" }
        .map {
            MobileConversationOutlineSubagent(
                id = it.id,
                type = it.subagentType?.takeIf(String::isNotBlank) ?: "subagent",
                title = it.label,
            )
        }
        .distinctBy(MobileConversationOutlineSubagent::id)
        .toList()

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


enum class MobileApprovalCleanupStatus {
    IDLE,
    CLEARING,
    FAILED,
}

data class MobileApprovalCleanupState(
    val status: MobileApprovalCleanupStatus = MobileApprovalCleanupStatus.IDLE,
    val error: String? = null,
)

enum class MobileTranscriptThreadStatus {
    IDLE,
    LOADING,
    READY,
    EMPTY,
    FAILED,
    UNAVAILABLE,
}

data class MobileTranscriptThreadState(
    val status: MobileTranscriptThreadStatus = MobileTranscriptThreadStatus.IDLE,
    val agentId: String? = null,
    val rootId: String? = null,
    val messages: List<MobileChatMessage> = emptyList(),
    val error: String? = null,
) {
    val isOpen: Boolean
        get() = rootId != null
}

data class MobileBotUiState(
    val bots: List<MobileBotSummaryAndroid> = emptyList(),
    val channelConnectionsByAgent: Map<String, List<AgentChannelConnection>> = emptyMap(),
    val channelLoadingAgentId: String? = null,
    val pinnedAgentIds: List<String> = emptyList(),
    val sidebarSnapshot: AndroidSidebarSectionsSnapshot = AndroidSidebarSectionsSnapshot(
        accountFence = "",
        revision = 0L,
        sections = emptyList(),
    ),
    val activeBot: MobileBotSummaryAndroid? = null,
    val draft: String = "",
    val draftAttachments: List<AndroidAgentAttachment> = emptyList(),
    val attachmentBusy: Boolean = false,
    val messages: List<MobileChatMessage> = emptyList(),
    val findEntries: List<AndroidFindInChatEntry> = emptyList(),
    val historyBeforeSeq: Long? = null,
    val historyHasMore: Boolean = false,
    val historyLoadingOlder: Boolean = false,
    val historyError: String? = null,
    val historyPrependRevision: Long = 0L,
    val transcriptThread: MobileTranscriptThreadState = MobileTranscriptThreadState(),
    val transcriptThreadSummaries: Map<String, Int> = emptyMap(),
    val transcriptLoadError: Boolean = false,
    val busy: Boolean = false,
    val operationId: String? = null,
    val pendingApproval: PendingAgentApproval? = null,
    val approvalCleanup: MobileApprovalCleanupState = MobileApprovalCleanupState(),
    val widgetPendingEntryIds: Set<String> = emptySet(),
    val editorSuggestions: MobileEditorSuggestionSnapshot = MobileEditorSuggestionSnapshot(),
    val modelCatalog: MobileModelCatalogState = MobileModelCatalogState(),
    val messageTargetId: String? = null,
    val paletteMessageSearch: CommandPaletteMessageSnapshot = CommandPaletteMessageSnapshot(
        status = CommandPaletteMessageStatus.IDLE,
    ),
    val paletteFileSearch: CommandPaletteFileSnapshot = CommandPaletteFileSnapshot(
        status = CommandPaletteFileStatus.IDLE,
    ),
    val paletteLinks: CommandPaletteLinkSnapshot = CommandPaletteLinkSnapshot(
        status = CommandPaletteLinkStatus.IDLE,
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
    val hiddenMutationPendingIds: Set<String> = emptySet(),
    val computerRebuild: MobileComputerRebuildUiState = MobileComputerRebuildUiState(),
    val conversationOutline: MobileConversationOutlineState = MobileConversationOutlineState(),
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
    private val hiddenChatsMutationController = HiddenChatsMutationController(
        readAgent = { id ->
            mutableState.value.bots.firstOrNull { it.id == id }?.let {
                HiddenChatsMutationAgent(it.id, it.isHidden)
            }
        },
        onOptimisticChange = { id, isHidden ->
            commitState(
                mutableState.value.copy(
                    bots = mutableState.value.bots.map { bot ->
                        if (bot.id == id) bot.copy(isHidden = isHidden) else bot
                    },
                ),
            )
        },
        onRollback = { id, _, previousValue ->
            commitState(
                mutableState.value.copy(
                    bots = mutableState.value.bots.map { bot ->
                        if (bot.id == id) bot.copy(isHidden = previousValue) else bot
                    },
                ),
            )
        },
        onPendingChange = { id, pending ->
            val next = mutableState.value.hiddenMutationPendingIds.toMutableSet().apply {
                if (pending) add(id) else remove(id)
            }
            commitState(mutableState.value.copy(hiddenMutationPendingIds = next))
        },
    )
    private val messagesByBot = mutableMapOf<String, List<MobileChatMessage>>()
    private val renderTelemetry = MobileRenderTelemetryTracker()
    private val draftWriteMutex = Mutex()
    private var draftScopeGeneration = 0L
    private var draftWriteGeneration = 0L
    private var activeDraftId: String? = null
    private var draftWriteJob: Job? = null
    private val paletteMessageFence = CommandPaletteMessageRequestFence()
    private var paletteMessageSearchJob: Job? = null
    private val paletteFileFence = CommandPaletteFileRequestFence()
    private var paletteFileSearchJob: Job? = null
    private val paletteLinkFence = CommandPaletteLinkRequestFence()
    private val paletteLinkMetadataProvider = TranscriptUrlCardMetadataProvider()
    private var paletteLinkJob: Job? = null
    private val paletteRoutineFence = CommandPaletteRoutineRequestFence()
    private var paletteRoutineJob: Job? = null
    private var openBotGeneration = 0L
    private var modelCatalogGeneration = 0L
    private var sendPresentationGeneration = 0L
    private var historyGeneration = 0L
    private var historyLoadJob: Job? = null
    private var transcriptThreadGeneration = 0L
    private var transcriptThreadLoadJob: Job? = null
    private var sidebarMutationGeneration = 0L
    private val historyOlderEntryIds = linkedSetOf<String>()
    private var asyncTasksGeneration = 0L
    private var asyncTasksJob: Job? = null
    private var outlineGeneration = 0L
    private var outlineRefreshJob: Job? = null
    private var outlinePollJob: Job? = null
    private var outlineRefreshPending = false
    private var groupMembersGeneration = 0L
    private var agentChannelsGeneration = 0L
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
        }
        mutableState.value = next
    }


    fun refreshAvailableModels() {
        val expectedAccountSlot = rosterSelectionAccountSlot
        if (expectedAccountSlot == null) {
            modelCatalogGeneration += 1
            commitState(mutableState.value.copy(modelCatalog = MobileModelCatalogState()))
            return
        }
        val token = ++modelCatalogGeneration
        val previous = mutableState.value.modelCatalog
        commitState(
            mutableState.value.copy(
                modelCatalog = previous.copy(loading = true, error = null),
            ),
        )
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                runCatching {
                    val defaultProjection = coordinator.agentDefaultModel()
                    val settledAccountSlot = canonicalRosterSelectionAccountSlot()
                    val defaultScope = defaultProjection.optString("accountScope").trim()
                    check(defaultScope == settledAccountSlot) {
                        "Default model projection belongs to a stale account"
                    }
                    val defaultModel = defaultProjection
                        .takeIf { it.has("model") && !it.isNull("model") }
                        ?.optString("model")
                        ?.trim()
                        ?.takeIf(String::isNotEmpty)
                    val projection = projectMobileAvailableModelCatalog(
                        coordinator.availableModelCatalog(),
                        defaultModel,
                    )
                    projection to settledAccountSlot
                }
            }
            if (
                token != modelCatalogGeneration ||
                expectedAccountSlot != rosterSelectionAccountSlot
            ) {
                return@launch
            }
            result.onSuccess { (projection, settledAccountSlot) ->
                if (settledAccountSlot != expectedAccountSlot) return@onSuccess
                commitState(
                    mutableState.value.copy(
                        modelCatalog = projection.copy(loading = false, error = null),
                    ),
                )
            }.onFailure { error ->
                commitState(
                    mutableState.value.copy(
                        modelCatalog = previous.copy(
                            loading = false,
                            error = error.message ?: "Available models could not be loaded",
                        ),
                    ),
                )
            }
        }
    }

    fun selectAvailableModel(modelId: String) {
        val expectedAccountSlot = rosterSelectionAccountSlot ?: return
        val token = modelCatalogGeneration
        val current = mutableState.value.modelCatalog
        runCatching { current.selectAvailableModel(modelId) }
            .onSuccess { selected ->
                commitState(mutableState.value.copy(modelCatalog = selected.copy(error = null)))
                viewModelScope.launch {
                    val result = withContext(Dispatchers.IO) {
                        runCatching {
                            coordinator.setAgentDefaultModel(
                                expectedAccountScope = expectedAccountSlot,
                                model = modelId,
                            )
                        }
                    }
                    if (
                        token != modelCatalogGeneration ||
                        expectedAccountSlot != rosterSelectionAccountSlot
                    ) {
                        return@launch
                    }
                    result.onFailure { error ->
                        commitState(
                            mutableState.value.copy(
                                modelCatalog = current.copy(
                                    error = error.message ?: "Default model could not be saved",
                                ),
                            ),
                        )
                    }
                }
            }
            .onFailure { error ->
                commitState(
                    mutableState.value.copy(
                        modelCatalog = current.copy(
                            error = error.message ?: "Model is unavailable",
                        ),
                    ),
                )
            }
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
                    hiddenChatsMutationController.setScope(nextSlot, mutableState.value.activeBot?.id)
                    resetPaletteLinks()
                    sendPresentationGeneration += 1
                    modelCatalogGeneration += 1
                    closeConversationOutline()
                    cancelEditorSuggestionRequest()
                    agentSettingsFence.accountChanged(nextSlot)
                    commitState(
                        mutableState.value.copy(
                            widgetPendingEntryIds = emptySet(),
                            approvalCleanup = MobileApprovalCleanupState(),
                            editorSuggestions = MobileEditorSuggestionSnapshot(),
                            modelCatalog = MobileModelCatalogState(),
                            channelConnectionsByAgent = emptyMap(),
                            channelLoadingAgentId = null,
                        ),
                    )
                }
            }
            val restoredSelection = rosterSelection.get().currentAgentId
            val installedResult = withContext(Dispatchers.IO) { runCatching { loadInstalledMiniAppBots() } }
            val surfaceResult = withContext(Dispatchers.IO) { runCatching { loadSurfaceBots() } }
            val pinnedResult = withContext(Dispatchers.IO) { runCatching { coordinator.agentPinnedIds() } }
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
            val canonicalBots = (installedBots + surfaceBots)
                .distinctBy { it.id }
                .sortedWith(
                    compareByDescending<MobileBotSummaryAndroid> { it.miniAppId != null }
                        .thenBy { it.name.lowercase() },
                )
            hiddenChatsMutationController.ingestAgents(
                canonicalBots.map { HiddenChatsMutationAgent(it.id, it.isHidden) },
            )
            val bots = canonicalBots.map { bot ->
                bot.copy(
                    isHidden = hiddenChatsMutationController.projectHidden(bot.id, bot.isHidden),
                )
            }
            val diagnostics = buildList {
                installedResult.exceptionOrNull()?.let { error ->
                    add("canonical installed projection: ${(error.message ?: error::class.java.simpleName).take(240)}")
                }
                surfaceResult.exceptionOrNull()?.let { error ->
                    add("surface bot list: ${(error.message ?: error::class.java.simpleName).take(240)}")
                }
                pinnedResult.exceptionOrNull()?.let { error ->
                    add("pinned Agent order: ${(error.message ?: error::class.java.simpleName).take(240)}")
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
                pinnedAgentIds = pinnedResult.getOrElse { mutableState.value.pinnedAgentIds },
                sidebarSnapshot = sectionsResult.getOrElse { mutableState.value.sidebarSnapshot },
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
                        avatarDataUrl = row.optString("avatarDataUrl").trim().takeIf(String::isNotEmpty),
                        isSharedRoom = row.optBoolean("isSharedRoom", false),
                        currentActivityKind = row.optJSONObject("currentActivity")
                            ?.optString("kind")?.trim()?.takeIf(String::isNotEmpty),
                        currentActivityVerb = row.optJSONObject("currentActivity")
                            ?.optString("verb")?.trim()?.takeIf(String::isNotEmpty),
                        currentActivityTool = row.optJSONObject("currentActivity")
                            ?.optString("tool")?.trim()?.takeIf(String::isNotEmpty),
                        isComposingMessage = row.optBoolean("isComposingMessage", false),
                        miniAppId = row.optString("miniAppId").takeIf(String::isNotBlank),
                        menuButtonText = row.optString("menuButtonText").takeIf(String::isNotBlank),
                        isPinned = row.optBoolean("isPinned"),
                        hasUnread = row.optBoolean("hasUnread"),
                        unreadCount = row.optLong("unreadCount", 0L).coerceAtLeast(0L),
                        lastActivityAt = row.optLong("lastActivityAt", 0L).coerceAtLeast(0L),
                        lastViewedAt = row.optLong("lastViewedAt", 0L).coerceAtLeast(0L),
                        isManuallyUnread = row.optBoolean("isManuallyUnread", false),
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
        refreshAgentChannels(agentId)
    }

    fun endAgentSettings(agentId: String) {
        agentChannelsGeneration += 1
        agentSettingsFence.clear(agentId)
        val current = mutableState.value
        if (current.channelLoadingAgentId == agentId) {
            commitState(current.copy(channelLoadingAgentId = null))
        }
    }

    fun refreshAgentChannels(agentId: String) {
        val bot = mutableState.value.bots.firstOrNull { it.id == agentId && !it.isGroup } ?: return
        val expectedAccountSlot = rosterSelectionAccountSlot
        val generation = ++agentChannelsGeneration
        commitState(mutableState.value.copy(channelLoadingAgentId = bot.id))
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                runCatching { coordinator.agentChannelsRefresh(bot.id) }
            }
            if (generation != agentChannelsGeneration || expectedAccountSlot != rosterSelectionAccountSlot) return@launch
            result.onSuccess { rows ->
                val connections = buildList {
                    for (index in 0 until rows.length()) {
                        val row = rows.optJSONObject(index) ?: continue
                        val platform = row.optString("platform").trim()
                        if (platform.isEmpty()) continue
                        add(
                            AgentChannelConnection(
                                platform = platform,
                                label = row.optString("label").trim().ifBlank { platform },
                                status = row.optString("status").trim().ifBlank { "connected" },
                                detail = row.optString("detail").trim().takeIf(String::isNotEmpty),
                            ),
                        )
                    }
                }
                val current = mutableState.value
                commitState(
                    current.copy(
                        channelConnectionsByAgent = current.channelConnectionsByAgent + (bot.id to connections),
                        channelLoadingAgentId = null,
                    ),
                )
            }.onFailure { error ->
                commitState(
                    mutableState.value.copy(
                        channelLoadingAgentId = null,
                        error = error.message ?: "Agent channel refresh failed",
                    ),
                )
            }
        }
    }

    fun disconnectAgentChannel(agentId: String, platform: String) {
        if (mutableState.value.bots.none { it.id == agentId && !it.isGroup }) return
        val expectedAccountSlot = rosterSelectionAccountSlot
        val generation = ++agentChannelsGeneration
        commitState(mutableState.value.copy(channelLoadingAgentId = agentId))
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                runCatching { coordinator.agentChannelDisconnect(agentId, platform) }
            }
            if (generation != agentChannelsGeneration || expectedAccountSlot != rosterSelectionAccountSlot) return@launch
            result.onSuccess {
                val rows = withContext(Dispatchers.IO) {
                    runCatching { coordinator.agentChannelsRefresh(agentId) }
                }.getOrDefault(JSONArray())
                if (generation != agentChannelsGeneration || expectedAccountSlot != rosterSelectionAccountSlot) return@onSuccess
                val connections = buildList {
                    for (index in 0 until rows.length()) {
                        val row = rows.optJSONObject(index) ?: continue
                        val projectedPlatform = row.optString("platform").trim()
                        if (projectedPlatform.isEmpty()) continue
                        add(
                            AgentChannelConnection(
                                platform = projectedPlatform,
                                label = row.optString("label").trim().ifBlank { projectedPlatform },
                                status = row.optString("status").trim().ifBlank { "connected" },
                                detail = row.optString("detail").trim().takeIf(String::isNotEmpty),
                            ),
                        )
                    }
                }
                val current = mutableState.value
                commitState(
                    current.copy(
                        channelConnectionsByAgent = current.channelConnectionsByAgent + (agentId to connections),
                        channelLoadingAgentId = null,
                    ),
                )
            }.onFailure { error ->
                commitState(
                    mutableState.value.copy(
                        channelLoadingAgentId = null,
                        error = error.message ?: "Agent channel disconnect failed",
                    ),
                )
            }
        }
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

    suspend fun generateBotAvatarImage(
        botId: String,
        description: String,
    ): Result<AndroidGeneratedAvatarImage> {
        if (mutableState.value.bots.none { it.id == botId }) {
            return Result.failure(IllegalStateException("Agent is no longer available"))
        }
        val prompt = description.trim()
        if (prompt.isEmpty()) {
            return Result.failure(IllegalArgumentException("Describe the avatar to generate"))
        }
        val accountSlot = rosterSelectionAccountSlot
        val token = agentSettingsFence.beginMutation(botId, accountSlot)
            ?: return Result.failure(IllegalStateException("Another Agent settings operation is in progress"))
        return try {
            val result = runCatching {
                withContext(Dispatchers.IO) {
                    coordinator.agentGenerateAvatarImage(prompt)
                }
            }
            if (!agentSettingsFence.isCurrent(token) || accountSlot != rosterSelectionAccountSlot) {
                Result.failure(CancellationException("Agent avatar generation became stale after account or settings change"))
            } else {
                result
            }
        } finally {
            agentSettingsFence.finish(token)
        }
    }

    suspend fun stagePickedBotAvatarImage(rawUri: String): Result<AndroidAvatarImageSource> =
        runCatching {
            withContext(Dispatchers.IO) {
                coordinator.stagePickedAgentAvatarImage(rawUri)
            }
        }

    suspend fun stageGeneratedBotAvatarImage(
        imageData: String,
        mimeType: String,
    ): Result<AndroidAvatarImageSource> =
        runCatching {
            withContext(Dispatchers.IO) {
                coordinator.stageGeneratedAgentAvatarImage(imageData, mimeType)
            }
        }

    suspend fun encodeBotAvatarCrop(
        source: AndroidAvatarImageSource,
        crop: AndroidAvatarCrop,
    ): Result<String> =
        runCatching {
            withContext(Dispatchers.IO) {
                coordinator.encodeAgentAvatarCrop(source, crop)
            }
        }


    fun updateBotAvatarImage(botId: String, avatarDataUrl: String?) {
        if (mutableState.value.bots.none { it.id == botId }) return
        val accountSlot = rosterSelectionAccountSlot
        val token = agentSettingsFence.beginMutation(botId, accountSlot) ?: return
        viewModelScope.launch {
            runCatching {
                withContext(Dispatchers.IO) {
                    coordinator.agentUpdateAvatarImage(botId, avatarDataUrl)
                }
            }.onSuccess {
                if (agentSettingsFence.isCurrent(token) && accountSlot == rosterSelectionAccountSlot) {
                    refreshBots()
                }
            }.onFailure { error ->
                if (agentSettingsFence.isCurrent(token) && accountSlot == rosterSelectionAccountSlot) {
                    mutableState.value = mutableState.value.copy(
                        error = error.message ?: "Agent avatar image update failed",
                    )
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
        setBotHidden(botId, true, "Hide agent failed")
    }

    fun unhideBot(botId: String) {
        setBotHidden(botId, false, "Unhide agent failed")
    }

    private fun setBotHidden(botId: String, isHidden: Boolean, fallbackMessage: String) {
        hiddenChatsMutationController.setScope(
            rosterSelectionAccountSlot,
            mutableState.value.activeBot?.id,
        )
        val token = hiddenChatsMutationController.begin(botId, isHidden) ?: return
        viewModelScope.launch {
            runCatching {
                withContext(Dispatchers.IO) { coordinator.agentSetHidden(botId, isHidden) }
            }.onSuccess {
                hiddenChatsMutationController.settle(
                    token,
                    HiddenChatsMutationOutcome.CONFIRMED,
                )
                refreshBots()
            }.onFailure { error ->
                val outcome = hiddenChatsMutationFailureOutcome(error)
                hiddenChatsMutationController.settle(token, outcome)
                commitState(
                    mutableState.value.copy(
                        error = error.message ?: fallbackMessage,
                    ),
                )
                if (outcome == HiddenChatsMutationOutcome.DETERMINISTIC_REJECTION) {
                    refreshBots()
                }
            }
        }
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
        val current = mutableState.value.pinnedAgentIds.toMutableList()
        if (isPinned) {
            if (botId !in current) current += botId
        } else {
            current.removeAll { it == botId }
        }
        viewModelScope.launch {
            runCatching {
                withContext(Dispatchers.IO) { coordinator.agentSetPinned(current) }
            }.onSuccess { settled ->
                mutableState.value = mutableState.value.copy(
                    pinnedAgentIds = settled,
                    error = null,
                )
                refreshBots()
            }.onFailure { error ->
                mutableState.value = mutableState.value.copy(error = error.message ?: "Pin state update failed")
            }
        }
    }

    internal fun reorderPinnedBot(movedId: String, targetId: String, position: PinnedMovePosition) {
        val current = mutableState.value.pinnedAgentIds
        val next = movePinnedAgent(current, movedId, targetId, position)
        if (next == current) return
        viewModelScope.launch {
            runCatching {
                withContext(Dispatchers.IO) { coordinator.agentSetPinned(next) }
            }.onSuccess { settled ->
                mutableState.value = mutableState.value.copy(
                    pinnedAgentIds = settled,
                    error = null,
                )
                refreshBots()
            }.onFailure { error ->
                mutableState.value = mutableState.value.copy(error = error.message ?: "Pinned Agent reorder failed")
            }
        }
    }

    fun toggleSidebarSection(sectionId: String) {
        val current = mutableState.value.sidebarSnapshot.sections
        val section = current.firstOrNull { it.id == sectionId } ?: return
        val next = current.map { row ->
            if (row.id == sectionId) row.copy(isCollapsed = !section.isCollapsed) else row
        }
        commitSidebarSections(next, "Toggle Agent section failed")
    }

    fun moveBotToSection(botId: String, sectionId: String) {
        val bot = mutableState.value.bots.firstOrNull { it.id == botId } ?: return
        if (bot.isPinned || bot.isHidden) return
        val next = moveAgentToSidebarSection(
            mutableState.value.sidebarSnapshot.sections,
            botId,
            sectionId,
        ) ?: return
        commitSidebarSections(next, "Move Agent to section failed")
    }

    fun moveBotsToSection(botIds: Set<String>, sectionId: String) {
        val eligible = mutableState.value.bots.asSequence().filter { it.id in botIds && !it.isPinned && !it.isHidden }.map { it.id }.toSet()
        val next = moveAgentsToSidebarSection(mutableState.value.sidebarSnapshot.sections, eligible, sectionId) ?: return
        commitSidebarSections(next, "Move selected Agents to section failed")
    }

    fun moveBotToNewSection(botId: String) {
        val bot = mutableState.value.bots.firstOrNull { it.id == botId } ?: return
        if (bot.isPinned || bot.isHidden) return
        val next = createSidebarSectionForAgent(
            mutableState.value.sidebarSnapshot.sections,
            botId,
            "section-${UUID.randomUUID()}",
        ) ?: return
        commitSidebarSections(next, "Create Agent section failed")
    }

    fun renameSidebarSection(sectionId: String, name: String) {
        val next = renameAgentSidebarSection(
            mutableState.value.sidebarSnapshot.sections,
            sectionId,
            name,
        ) ?: return
        commitSidebarSections(next, "Rename Agent section failed")
    }

    fun deleteSidebarSection(sectionId: String) {
        val next = deleteAgentSidebarSection(
            mutableState.value.sidebarSnapshot.sections,
            sectionId,
        ) ?: return
        commitSidebarSections(next, "Delete Agent section failed")
    }

    internal fun moveSidebarSection(
        sectionId: String,
        targetId: String,
        position: AgentSidebarSectionMovePosition,
    ) {
        val next = moveAgentSidebarSection(
            mutableState.value.sidebarSnapshot.sections,
            sectionId,
            targetId,
            position,
        ) ?: return
        commitSidebarSections(next, "Reorder Agent section failed")
    }

    private fun commitSidebarSections(
        sections: List<AndroidSidebarSection>,
        fallbackMessage: String,
    ) {
        val base = mutableState.value.sidebarSnapshot
        val accountSlot = rosterSelectionAccountSlot
        val mutationGeneration = ++sidebarMutationGeneration
        commitState(mutableState.value.copy(sidebarSnapshot = base.copy(sections = sections), error = null))
        viewModelScope.launch {
            runCatching {
                withContext(Dispatchers.IO) { coordinator.agentSetSidebarSections(base, sections) }
            }.onSuccess { settled ->
                if (mutationGeneration == sidebarMutationGeneration && accountSlot == rosterSelectionAccountSlot && settled.accountFence == base.accountFence) {
                    commitState(mutableState.value.copy(sidebarSnapshot = settled, error = null))
                }
            }.onFailure { error ->
                if (mutationGeneration == sidebarMutationGeneration && accountSlot == rosterSelectionAccountSlot && mutableState.value.sidebarSnapshot.accountFence == base.accountFence) {
                    commitState(mutableState.value.copy(sidebarSnapshot = base, error = error.message ?: fallbackMessage))
                    refreshBots()
                }
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
        resetPaletteLinks()
        hiddenChatsMutationController.setScope(rosterSelectionAccountSlot, bot.id)
        rosterSelection.select(bot.id)
        openBotGeneration += 1
        sendPresentationGeneration += 1
        historyGeneration += 1
        historyLoadJob?.cancel()
        historyLoadJob = null
        historyOlderEntryIds.clear()
        resetTranscriptThread()
        draftScopeGeneration += 1
        draftWriteGeneration += 1
        draftWriteJob?.cancel()
        draftWriteJob = null
        activeDraftId = null
        val generation = openBotGeneration
        val draftScope = draftScopeGeneration
        val draftEditAtOpen = draftWriteGeneration
        val cachedMessages = messagesByBot[bot.id].orEmpty()
        val baselineEntryIds = cachedMessages.mapTo(linkedSetOf(), MobileChatMessage::id)
        commitState(
            mutableState.value.copy(
                activeBot = bot,
                draft = "",
                draftAttachments = emptyList(),
                attachmentBusy = false,
                messages = cachedMessages,
                historyBeforeSeq = null,
                historyHasMore = false,
                historyLoadingOlder = false,
                historyError = null,
                historyPrependRevision = 0L,
                pendingApproval = null,
                approvalCleanup = MobileApprovalCleanupState(),
                widgetPendingEntryIds = emptySet(),
                editorSuggestions = MobileEditorSuggestionSnapshot(),
                messageTargetId = targetEntryId,
                error = null,
            ),
        )
        viewModelScope.launch {
            val restored = withContext(Dispatchers.IO) {
                runCatching { coordinator.composerDraftRead(rosterSelectionAccountSlot, bot.id) }
            }
            if (
                draftScope != draftScopeGeneration ||
                draftEditAtOpen != draftWriteGeneration ||
                mutableState.value.activeBot?.id != bot.id
            ) {
                return@launch
            }
            restored.onSuccess { draft ->
                activeDraftId = draft?.draftId
                val prompt = draft?.prompt.orEmpty()
                commitState(
                    mutableState.value.copy(
                        draft = prompt,
                        draftAttachments = draft?.attachments.orEmpty(),
                        attachmentBusy = false,
                    ),
                )
                refreshEditorSuggestions(prompt)
            }.onFailure { error ->
                commitState(
                    mutableState.value.copy(
                        error = error.message ?: "Composer draft restore failed",
                    ),
                )
            }
        }
        refreshEditorSuggestions(mutableState.value.draft)
        if (bot.miniAppId != null) {
            rosterSelection.settle(bot.id)
            return
        }

        refreshAvailableModels()

        val loadStartedAtMs = SystemClock.elapsedRealtime()
        val loadWasCold = cachedMessages.isEmpty()
        viewModelScope.launch {
            val rpcStartedAtMs = SystemClock.elapsedRealtime()
            try {
                val durable = withContext(Dispatchers.IO) {
                    loadDurableAgentTranscript(bot.id, rosterSelectionAccountSlot)
                }
                val rpcFinishedAtMs = SystemClock.elapsedRealtime()
                val stale = generation != openBotGeneration || mutableState.value.activeBot?.id != bot.id
                if (stale) {
                    reportAgentLoadTelemetry(
                        loadStartedAtMs = loadStartedAtMs,
                        rpcStartedAtMs = rpcStartedAtMs,
                        rpcFinishedAtMs = rpcFinishedAtMs,
                        cold = loadWasCold,
                        stale = true,
                        outcome = "superseded",
                    )
                    return@launch
                }
                val merged = durable.copy(
                    messages = mergeCanonicalMobileTranscript(
                        baselineEntryIds = baselineEntryIds,
                        current = mutableState.value.messages,
                        canonical = durable.messages,
                    ),
                )
                commitState(
                    mutableState.value.copy(
                        messages = merged.messages,
                        findEntries = merged.findEntries,
                        transcriptThreadSummaries = merged.threadCounts,
                        historyBeforeSeq = merged.nextBeforeSeq,
                        historyHasMore = merged.nextBeforeSeq != null,
                        historyLoadingOlder = false,
                        historyError = null,
                        transcriptLoadError = false,
                        messageTargetId = targetEntryId,
                        error = null,
                    ),
                )
                rosterSelection.settle(bot.id)
                reportAgentLoadTelemetry(
                    loadStartedAtMs = loadStartedAtMs,
                    rpcStartedAtMs = rpcStartedAtMs,
                    rpcFinishedAtMs = rpcFinishedAtMs,
                    cold = loadWasCold,
                    stale = false,
                    outcome = "ready",
                )
            } catch (error: CancellationException) {
                throw error
            } catch (error: Throwable) {
                val failedAtMs = SystemClock.elapsedRealtime()
                val stale = generation != openBotGeneration || mutableState.value.activeBot?.id != bot.id
                reportAgentLoadTelemetry(
                    loadStartedAtMs = loadStartedAtMs,
                    rpcStartedAtMs = rpcStartedAtMs,
                    rpcFinishedAtMs = failedAtMs,
                    cold = loadWasCold,
                    stale = stale,
                    outcome = if (stale) "superseded" else "failed",
                )
                if (!stale) {
                    commitState(
                        mutableState.value.copy(
                            transcriptLoadError = true,
                            error = null,
                        ),
                    )
                    rosterSelection.settle(bot.id)
                }
            }
        }
    }

    private fun reportAgentLoadTelemetry(
        loadStartedAtMs: Long,
        rpcStartedAtMs: Long,
        rpcFinishedAtMs: Long,
        cold: Boolean,
        stale: Boolean,
        outcome: String,
    ) {
        val finishedAtMs = SystemClock.elapsedRealtime()
        val queueMs = (rpcStartedAtMs - loadStartedAtMs).coerceAtLeast(0L)
        val openRpcMs = (rpcFinishedAtMs - rpcStartedAtMs).coerceAtLeast(0L)
        val renderMs = (finishedAtMs - rpcFinishedAtMs).coerceAtLeast(0L)
        val durationMs = (finishedAtMs - loadStartedAtMs).coerceAtLeast(0L)
        viewModelScope.launch(Dispatchers.IO) {
            runCatching {
                coordinator.reportAgentLoad(
                    durationMs = durationMs,
                    openRpcMs = openRpcMs,
                    queueMs = queueMs,
                    renderMs = renderMs,
                    cold = cold,
                    stale = stale,
                    boxState = "unknown",
                    outcome = outcome,
                )
            }
        }
    }

    fun openTranscriptThread(rootId: String) {
        val snapshot = mutableState.value
        val bot = snapshot.activeBot
        val accountSlot = rosterSelectionAccountSlot
        val normalizedRootId = rootId.trim()
        if (bot == null || bot.miniAppId != null || accountSlot.isNullOrBlank() || normalizedRootId.isEmpty()) {
            transcriptThreadGeneration += 1
            transcriptThreadLoadJob?.cancel()
            transcriptThreadLoadJob = null
            commitState(
                snapshot.copy(
                    transcriptThread = MobileTranscriptThreadState(
                        status = MobileTranscriptThreadStatus.UNAVAILABLE,
                        agentId = bot?.id,
                        rootId = normalizedRootId.takeIf(String::isNotEmpty),
                    ),
                ),
            )
            return
        }
        val current = snapshot.transcriptThread
        if (
            transcriptThreadLoadJob?.isActive == true &&
            current.agentId == bot.id &&
            current.rootId == normalizedRootId &&
            current.status == MobileTranscriptThreadStatus.LOADING
        ) {
            return
        }

        transcriptThreadGeneration += 1
        transcriptThreadLoadJob?.cancel()
        val generation = transcriptThreadGeneration
        commitState(
            snapshot.copy(
                transcriptThread = MobileTranscriptThreadState(
                    status = MobileTranscriptThreadStatus.LOADING,
                    agentId = bot.id,
                    rootId = normalizedRootId,
                ),
            ),
        )
        transcriptThreadLoadJob = viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                runCatching {
                    val projection = coordinator.transcriptThread(bot.id, normalizedRootId)
                    canonicalMobileTranscriptForAgent(projection.entries, bot.id)
                }
            }
            if (
                generation != transcriptThreadGeneration ||
                accountSlot != rosterSelectionAccountSlot ||
                mutableState.value.activeBot?.id != bot.id ||
                mutableState.value.transcriptThread.rootId != normalizedRootId
            ) {
                return@launch
            }
            result.onSuccess { messages ->
                commitState(
                    mutableState.value.copy(
                        transcriptThread = MobileTranscriptThreadState(
                            status = if (messages.isEmpty()) {
                                MobileTranscriptThreadStatus.EMPTY
                            } else {
                                MobileTranscriptThreadStatus.READY
                            },
                            agentId = bot.id,
                            rootId = normalizedRootId,
                            messages = messages,
                        ),
                    ),
                )
            }.onFailure { error ->
                commitState(
                    mutableState.value.copy(
                        transcriptThread = MobileTranscriptThreadState(
                            status = MobileTranscriptThreadStatus.FAILED,
                            agentId = bot.id,
                            rootId = normalizedRootId,
                            error = error.message ?: "Thread could not be loaded",
                        ),
                    ),
                )
            }
            transcriptThreadLoadJob = null
        }
    }

    fun retryTranscriptThread() {
        val rootId = mutableState.value.transcriptThread.rootId ?: return
        openTranscriptThread(rootId)
    }

    fun closeTranscriptThread() {
        resetTranscriptThread()
        if (mutableState.value.transcriptThread != MobileTranscriptThreadState()) {
            commitState(mutableState.value.copy(transcriptThread = MobileTranscriptThreadState()))
        }
    }

    private fun resetTranscriptThread() {
        transcriptThreadGeneration += 1
        transcriptThreadLoadJob?.cancel()
        transcriptThreadLoadJob = null
        if (mutableState.value.transcriptThread != MobileTranscriptThreadState()) {
            commitState(mutableState.value.copy(transcriptThread = MobileTranscriptThreadState()))
        }
    }

    fun retryTranscriptLoad() {
        val bot = mutableState.value.activeBot ?: return
        openBot(bot, mutableState.value.messageTargetId)
    }

    fun consumeMessageTarget(entryId: String) {
        if (mutableState.value.messageTargetId == entryId) {
            commitState(mutableState.value.copy(messageTargetId = null))
        }
    }

    fun closeBot() {
        if (mutableState.value.busy) return
        resetPaletteMessageSearch()
        resetPaletteFileSearch()
        resetPaletteLinks()
        hiddenChatsMutationController.setScope(rosterSelectionAccountSlot, null)
        closeConversationOutline()
        rosterSelection.select(null)
        openBotGeneration += 1
        sendPresentationGeneration += 1
        historyGeneration += 1
        historyLoadJob?.cancel()
        historyLoadJob = null
        historyOlderEntryIds.clear()
        resetTranscriptThread()
        draftScopeGeneration += 1
        draftWriteGeneration += 1
        draftWriteJob?.cancel()
        draftWriteJob = null
        activeDraftId = null
        cancelEditorSuggestionRequest()
        commitState(
            mutableState.value.copy(
                activeBot = null,
                draft = "",
                messages = emptyList(),
                findEntries = emptyList(),
                transcriptThreadSummaries = emptyMap(),
                historyBeforeSeq = null,
                historyHasMore = false,
                historyLoadingOlder = false,
                historyError = null,
                transcriptLoadError = false,
                pendingApproval = null,
                approvalCleanup = MobileApprovalCleanupState(),
                widgetPendingEntryIds = emptySet(),
                editorSuggestions = MobileEditorSuggestionSnapshot(),
                messageTargetId = null,
                error = null,
            ),
        )
    }

    fun resetAccountScope() {
        resetPaletteMessageSearch()
        resetPaletteFileSearch()
        resetPaletteLinks()
        resetPaletteRoutines()
        hiddenChatsMutationController.reset()
        closeConversationOutline()
        rosterSelection.reset()
        rosterSelectionAccountSlot = null
        openBotGeneration += 1
        sendPresentationGeneration += 1
        historyGeneration += 1
        historyLoadJob?.cancel()
        historyLoadJob = null
        historyOlderEntryIds.clear()
        resetTranscriptThread()
        cancelEditorSuggestionRequest()
        asyncTasksGeneration += 1
        asyncTasksJob?.cancel()
        asyncTasksJob = null
        groupMembersGeneration += 1
        computerRebuildGeneration += 1
        agentSettingsFence.clear()
        messagesByBot.clear()
        draftScopeGeneration += 1
        draftWriteGeneration += 1
        draftWriteJob?.cancel()
        draftWriteJob = null
        activeDraftId = null
        commitState(MobileBotUiState())
    }

    fun openConversationOutline(bot: MobileBotSummaryAndroid) {
        if (bot.miniAppId != null) return
        outlineGeneration += 1
        outlineRefreshJob?.cancel()
        outlinePollJob?.cancel()
        outlineRefreshJob = null
        outlinePollJob = null
        outlineRefreshPending = false
        commitState(
            mutableState.value.copy(
                conversationOutline = MobileConversationOutlineState(
                    parentAgentId = bot.id,
                    parentAgentName = bot.name,
                    selectedAgentId = bot.id,
                    loading = true,
                ),
            ),
        )
        refreshConversationOutline()
        startConversationOutlinePolling()
    }

    fun selectConversationOutlineAgent(agentId: String) {
        val current = mutableState.value.conversationOutline
        val parentId = current.parentAgentId ?: return
        val allowed = agentId == parentId || current.subagents.any { it.id == agentId }
        if (!allowed || current.selectedAgentId == agentId) return
        outlineGeneration += 1
        outlineRefreshJob?.cancel()
        outlineRefreshJob = null
        outlineRefreshPending = false
        commitState(
            mutableState.value.copy(
                conversationOutline = current.copy(
                    selectedAgentId = agentId,
                    items = emptyList(),
                    loading = true,
                    error = null,
                ),
            ),
        )
        refreshConversationOutline()
        startConversationOutlinePolling()
    }

    fun closeConversationOutline() {
        outlineGeneration += 1
        outlineRefreshPending = false
        outlineRefreshJob?.cancel()
        outlinePollJob?.cancel()
        outlineRefreshJob = null
        outlinePollJob = null
        if (mutableState.value.conversationOutline.isOpen) {
            commitState(mutableState.value.copy(conversationOutline = MobileConversationOutlineState()))
        }
    }

    fun refreshConversationOutline() {
        val snapshot = mutableState.value.conversationOutline
        val parentId = snapshot.parentAgentId ?: return
        val selectedId = snapshot.selectedAgentId ?: parentId
        if (outlineRefreshJob?.isActive == true) {
            outlineRefreshPending = true
            return
        }
        val generation = outlineGeneration
        val accountSlot = rosterSelectionAccountSlot
        outlineRefreshJob = viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                runCatching {
                    val items = coordinator.conversationOutlineSnapshot(selectedId)
                    val subagents = projectConversationOutlineSubagents(
                        projectMobileAsyncTasks(coordinator.agentAsyncTasks(parentId)),
                    )
                    items to subagents
                }
            }
            if (
                generation != outlineGeneration ||
                accountSlot != rosterSelectionAccountSlot ||
                mutableState.value.conversationOutline.parentAgentId != parentId ||
                mutableState.value.conversationOutline.selectedAgentId != selectedId
            ) {
                return@launch
            }
            result.onSuccess { (items, subagents) ->
                val selectedStillAllowed = selectedId == parentId || subagents.any { it.id == selectedId }
                commitState(
                    mutableState.value.copy(
                        conversationOutline = mutableState.value.conversationOutline.copy(
                            selectedAgentId = if (selectedStillAllowed) selectedId else parentId,
                            items = if (selectedStillAllowed) items else emptyList(),
                            subagents = subagents,
                            loading = false,
                            error = null,
                        ),
                    ),
                )
                if (!selectedStillAllowed) outlineRefreshPending = true
            }.onFailure { error ->
                commitState(
                    mutableState.value.copy(
                        conversationOutline = mutableState.value.conversationOutline.copy(
                            loading = false,
                            error = error.message ?: "Conversation outline refresh failed",
                        ),
                    ),
                )
            }
            outlineRefreshJob = null
            if (outlineRefreshPending && generation == outlineGeneration) {
                outlineRefreshPending = false
                refreshConversationOutline()
            }
        }
    }

    private fun startConversationOutlinePolling() {
        outlinePollJob?.cancel()
        val generation = outlineGeneration
        outlinePollJob = viewModelScope.launch {
            while (generation == outlineGeneration && mutableState.value.conversationOutline.isOpen) {
                delay(2_000)
                val current = mutableState.value.conversationOutline
                val selectedId = current.selectedAgentId
                if (current.subagents.any { it.id == selectedId && it.status == "running" }) {
                    refreshConversationOutline()
                }
            }
        }
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
        val bot = mutableState.value.activeBot ?: return
        val accountSlot = rosterSelectionAccountSlot
        val scope = draftScopeGeneration
        val writeGeneration = ++draftWriteGeneration
        commitState(mutableState.value.copy(draft = value))
        refreshEditorSuggestions(value)
        draftWriteJob = viewModelScope.launch {
            val result = runCatching {
                draftWriteMutex.withLock {
                    if (
                        scope != draftScopeGeneration ||
                        writeGeneration != draftWriteGeneration ||
                        mutableState.value.activeBot?.id != bot.id
                    ) {
                        return@withLock null
                    }
                    withContext(Dispatchers.IO) {
                        coordinator.composerDraftWrite(accountSlot, bot.id, value)
                    }
                }
            }
            if (
                scope != draftScopeGeneration ||
                writeGeneration != draftWriteGeneration ||
                mutableState.value.activeBot?.id != bot.id
            ) {
                return@launch
            }
            result.onSuccess { draft ->
                if (draft != null) activeDraftId = draft.draftId
            }.onFailure { error ->
                commitState(
                    mutableState.value.copy(
                        error = error.message ?: "Composer draft could not be persisted",
                    ),
                )
            }
        }
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

    fun resetPaletteLinks() {
        paletteLinkJob?.cancel()
        paletteLinkJob = null
        paletteLinkFence.cancel()
        paletteLinkMetadataProvider.reset()
        mutableState.value = mutableState.value.copy(
            paletteLinks = CommandPaletteLinkSnapshot(
                status = CommandPaletteLinkStatus.IDLE,
            ),
        )
    }

    fun refreshPaletteLinks() {
        paletteLinkJob?.cancel()
        paletteLinkJob = null
        paletteLinkFence.cancel()
        paletteLinkMetadataProvider.reset()
        val scope = CommandPaletteLinkRequestScope(
            accountSlot = rosterSelectionAccountSlot,
            conversationId = mutableState.value.activeBot?.id,
        )
        if (scope.conversationId == null) {
            mutableState.value = mutableState.value.copy(
                paletteLinks = CommandPaletteLinkSnapshot(
                    status = CommandPaletteLinkStatus.EMPTY,
                ),
            )
            return
        }
        val links = commandPaletteLinksFromConversation(
            mutableState.value.messages.map(MobileChatMessage::text),
        )
        if (links.isEmpty()) {
            mutableState.value = mutableState.value.copy(
                paletteLinks = CommandPaletteLinkSnapshot(
                    status = CommandPaletteLinkStatus.EMPTY,
                ),
            )
            return
        }
        val token = paletteLinkFence.begin(scope)
        mutableState.value = mutableState.value.copy(
            paletteLinks = CommandPaletteLinkSnapshot(
                status = CommandPaletteLinkStatus.LOADING,
                value = links,
            ),
        )
        paletteLinkJob = viewModelScope.launch {
            try {
                val metadata = linkedMapOf<String, CommandPaletteLinkMetadata>()
                for (link in links) {
                    val snapshot = withContext(Dispatchers.IO) {
                        paletteLinkMetadataProvider.load(link.url)
                    }
                    if (!paletteLinkFence.accepts(
                            token,
                            CommandPaletteLinkRequestScope(
                                accountSlot = rosterSelectionAccountSlot,
                                conversationId = mutableState.value.activeBot?.id,
                            ),
                        )
                    ) return@launch
                    commandPaletteLinkMetadataProjection(snapshot)?.let { projected ->
                        metadata[link.url] = projected
                        mutableState.value = mutableState.value.copy(
                            paletteLinks = CommandPaletteLinkSnapshot(
                                status = CommandPaletteLinkStatus.LOADING,
                                value = links,
                                metadata = metadata.toMap(),
                            ),
                        )
                    }
                }
                if (!paletteLinkFence.accepts(
                            token,
                            CommandPaletteLinkRequestScope(
                                accountSlot = rosterSelectionAccountSlot,
                                conversationId = mutableState.value.activeBot?.id,
                            ),
                        )
                    ) return@launch
                mutableState.value = mutableState.value.copy(
                    paletteLinks = CommandPaletteLinkSnapshot(
                        status = CommandPaletteLinkStatus.READY,
                        value = links,
                        metadata = metadata.toMap(),
                    ),
                )
            } catch (error: CancellationException) {
                throw error
            } catch (_: Throwable) {
                if (
                    paletteLinkFence.accepts(
                        token,
                        CommandPaletteLinkRequestScope(
                            accountSlot = rosterSelectionAccountSlot,
                            conversationId = mutableState.value.activeBot?.id,
                        ),
                    )
                ) {
                    mutableState.value = mutableState.value.copy(
                        paletteLinks = CommandPaletteLinkSnapshot(
                            status = CommandPaletteLinkStatus.FAILED,
                            value = links,
                            metadata = mutableState.value.paletteLinks.metadata,
                        ),
                    )
                }
            }
        }
    }

    fun resetPaletteFileSearch() {
        paletteFileSearchJob?.cancel()
        paletteFileSearchJob = null
        paletteFileFence.cancel()
        mutableState.value = mutableState.value.copy(
            paletteFileSearch = CommandPaletteFileSnapshot(
                status = CommandPaletteFileStatus.IDLE,
            ),
        )
    }

    fun setPaletteFileQuery(query: String) {
        paletteFileSearchJob?.cancel()
        paletteFileSearchJob = null
        paletteFileFence.cancel()
        val token = paletteFileFence.begin()
        val previous = mutableState.value.paletteFileSearch.value
        mutableState.value = mutableState.value.copy(
            paletteFileSearch = CommandPaletteFileSnapshot(
                status = CommandPaletteFileStatus.LOADING,
                value = previous,
            ),
        )
        paletteFileSearchJob = viewModelScope.launch {
            try {
                val matches = withContext(Dispatchers.IO) {
                    coordinator.contentSearchMedia(query, limit = COMMAND_PALETTE_FILE_RESULT_LIMIT)
                }
                val results = commandPaletteFilesFromHostMatches(matches)
                if (!paletteFileFence.accepts(token)) return@launch
                mutableState.value = mutableState.value.copy(
                    paletteFileSearch = CommandPaletteFileSnapshot(
                        status = if (results.isEmpty()) {
                            CommandPaletteFileStatus.EMPTY
                        } else {
                            CommandPaletteFileStatus.READY
                        },
                        value = results,
                    ),
                )
            } catch (error: CancellationException) {
                throw error
            } catch (_: Throwable) {
                if (paletteFileFence.accepts(token)) {
                    mutableState.value = mutableState.value.copy(
                        paletteFileSearch = CommandPaletteFileSnapshot(
                            status = CommandPaletteFileStatus.FAILED,
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
                val matches = withContext(Dispatchers.IO) {
                    coordinator.globalContentSearchMessages(normalized, limit = 50)
                }
                val results = commandPaletteMessagesFromHostMatches(matches)
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


    fun addDraftAttachments(contentUris: List<String>) {
        val snapshot = mutableState.value
        val bot = snapshot.activeBot ?: return
        if (snapshot.busy || snapshot.attachmentBusy || bot.miniAppId != null) return
        val uris = contentUris
            .map(String::trim)
            .filter { it.startsWith("content://") }
            .distinct()
            .take(6 - snapshot.draftAttachments.size)
        if (uris.isEmpty()) return
        val accountSlot = rosterSelectionAccountSlot
        val scope = draftScopeGeneration
        commitState(snapshot.copy(attachmentBusy = true, error = null))
        viewModelScope.launch {
            val failures = mutableListOf<String>()
            for (contentUri in uris) {
                val result = withContext(Dispatchers.IO) {
                    runCatching { coordinator.composerDraftStageAttachment(accountSlot, bot.id, contentUri) }
                }
                if (
                    scope != draftScopeGeneration ||
                    mutableState.value.activeBot?.id != bot.id ||
                    accountSlot != rosterSelectionAccountSlot
                ) {
                    return@launch
                }
                result.onSuccess { draft ->
                    activeDraftId = draft.draftId
                    commitState(
                        mutableState.value.copy(
                            draftAttachments = draft.attachments,
                            error = null,
                        ),
                    )
                }.onFailure { error ->
                    failures += error.message ?: "Attachment could not be staged"
                    val recovered = withContext(Dispatchers.IO) {
                        runCatching { coordinator.composerDraftRead(accountSlot, bot.id) }.getOrNull()
                    }
                    if (
                        scope != draftScopeGeneration ||
                        mutableState.value.activeBot?.id != bot.id ||
                        accountSlot != rosterSelectionAccountSlot
                    ) {
                        return@launch
                    }
                    activeDraftId = recovered?.draftId
                    commitState(
                        mutableState.value.copy(
                            draftAttachments = recovered?.attachments.orEmpty(),
                        ),
                    )
                }
            }
            if (
                scope == draftScopeGeneration &&
                mutableState.value.activeBot?.id == bot.id &&
                accountSlot == rosterSelectionAccountSlot
            ) {
                val notice = when {
                    failures.isEmpty() -> null
                    failures.size == 1 -> failures.single()
                    failures.all { it.contains("too large to attach") } ->
                        "${failures.size} files are too large to attach (max 25 MB, or 200 MB for video)."
                    else -> "${failures.size} files couldn't be attached."
                }
                commitState(
                    mutableState.value.copy(
                        attachmentBusy = false,
                        error = notice,
                    ),
                )
            }
        }
    }

    fun removeDraftAttachment(attachmentId: String) {
        val snapshot = mutableState.value
        val bot = snapshot.activeBot ?: return
        if (snapshot.busy || snapshot.attachmentBusy || bot.miniAppId != null || attachmentId.isBlank()) return
        if (snapshot.draftAttachments.none { it.id == attachmentId }) return
        val accountSlot = rosterSelectionAccountSlot
        val scope = draftScopeGeneration
        commitState(snapshot.copy(attachmentBusy = true, error = null))
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                runCatching { coordinator.composerDraftRemoveAttachment(accountSlot, bot.id, attachmentId) }
            }
            if (
                scope != draftScopeGeneration ||
                mutableState.value.activeBot?.id != bot.id ||
                accountSlot != rosterSelectionAccountSlot
            ) {
                return@launch
            }
            result.onSuccess { draft ->
                activeDraftId = draft?.draftId
                commitState(
                    mutableState.value.copy(
                        draftAttachments = draft?.attachments.orEmpty(),
                        attachmentBusy = false,
                        error = null,
                    ),
                )
            }.onFailure { error ->
                commitState(
                    mutableState.value.copy(
                        attachmentBusy = false,
                        error = error.message ?: "Attachment could not be removed",
                    ),
                )
            }
        }
    }

    fun loadAgentAttachmentBytes(
        agentId: String,
        attachmentId: String,
        sizeBytes: Long,
        onResult: (Result<ByteArray>) -> Unit,
    ) {
        val accountSlot = rosterSelectionAccountSlot
        val expectedBotId = mutableState.value.activeBot?.id
        if (
            agentId.isBlank() ||
            attachmentId.isBlank() ||
            sizeBytes <= 0L ||
            sizeBytes > Int.MAX_VALUE ||
            expectedBotId != agentId
        ) {
            onResult(Result.failure(IllegalArgumentException("Agent attachment is unavailable")))
            return
        }
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                runCatching {
                    val output = ByteArrayOutputStream(sizeBytes.toInt())
                    var offset = 0L
                    while (offset < sizeBytes) {
                        val requested = minOf(4L * 1024L * 1024L, sizeBytes - offset).toInt()
                        val chunk = coordinator.composerDraftAttachmentReadRange(
                            accountSlot = accountSlot,
                            agentId = agentId,
                            attachmentId = attachmentId,
                            offset = offset,
                            length = requested,
                        )
                        check(chunk.attachmentId == attachmentId) {
                            "Agent attachment reader switched attachment identity"
                        }
                        check(chunk.offset == offset && chunk.nextOffset > offset) {
                            "Agent attachment reader returned a stale continuation"
                        }
                        check(chunk.sizeBytes == sizeBytes) {
                            "Agent attachment size changed while reading"
                        }
                        check(chunk.bytes.size.toLong() == chunk.nextOffset - offset) {
                            "Agent attachment chunk length did not match continuation"
                        }
                        output.write(chunk.bytes)
                        offset = chunk.nextOffset
                        if (chunk.eof) {
                            check(offset == sizeBytes) {
                                "Agent attachment ended before its declared size"
                            }
                            break
                        }
                    }
                    check(offset == sizeBytes) { "Agent attachment read ended before its declared size" }
                    output.toByteArray()
                }
            }
            if (
                accountSlot != rosterSelectionAccountSlot ||
                mutableState.value.activeBot?.id != agentId
            ) {
                onResult(Result.failure(CancellationException("Agent attachment read became stale")))
            } else {
                onResult(result)
            }
        }
    }

    fun loadAgentAttachmentFile(
        agentId: String,
        attachmentId: String,
        sizeBytes: Long,
        onResult: (Result<File>) -> Unit,
    ) {
        val accountSlot = rosterSelectionAccountSlot
        val expectedBotId = mutableState.value.activeBot?.id
        if (
            agentId.isBlank() ||
            attachmentId.isBlank() ||
            sizeBytes <= 0L ||
            sizeBytes > Int.MAX_VALUE ||
            expectedBotId != agentId
        ) {
            onResult(Result.failure(IllegalArgumentException("Agent attachment is unavailable")))
            return
        }
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                runCatching {
                    val directory = File(getApplication<Application>().cacheDir, "agent-attachment-viewer")
                    streamAndroidBlobToTemporaryFile(
                        sizeBytes = sizeBytes.toInt(),
                        directory = directory,
                    ) { offset, length ->
                        val chunk = coordinator.composerDraftAttachmentReadRange(
                            accountSlot = accountSlot,
                            agentId = agentId,
                            attachmentId = attachmentId,
                            offset = offset.toLong(),
                            length = length,
                        )
                        check(chunk.attachmentId == attachmentId) {
                            "Agent attachment reader switched attachment identity"
                        }
                        check(chunk.offset == offset.toLong()) {
                            "Agent attachment reader returned a stale offset"
                        }
                        check(chunk.sizeBytes == sizeBytes) {
                            "Agent attachment size changed while streaming"
                        }
                        chunk.bytes
                    }
                }
            }
            if (
                accountSlot != rosterSelectionAccountSlot ||
                mutableState.value.activeBot?.id != agentId
            ) {
                result.getOrNull()?.let { runCatching { it.delete() } }
                onResult(Result.failure(CancellationException("Agent attachment stream became stale")))
            } else {
                onResult(result)
            }
        }
    }

    fun send() {
        val snapshot = mutableState.value
        val bot = snapshot.activeBot ?: return
        val text = snapshot.draft.trim()
        if (snapshot.busy || snapshot.attachmentBusy) return
        val miniAppId = bot.miniAppId
        if (text.isBlank() && (miniAppId != null || snapshot.draftAttachments.isEmpty())) return
        if (!miniAppId.isNullOrBlank()) {
            val requestId = "android-mobile-bot-chat-${UUID.randomUUID()}"
            val accountSlot = rosterSelectionAccountSlot
            val scopeGeneration = sendPresentationGeneration
            val draftRevisionAtSend = draftWriteGeneration
            commitState(
                snapshot.copy(
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
            viewModelScope.launch {
                try {
                    val sentDraft = draftWriteMutex.withLock {
                        withContext(Dispatchers.IO) {
                            coordinator.composerDraftWrite(accountSlot, bot.id, snapshot.draft)
                        }
                    }
                    if (!isCurrentSendScope(bot.id, accountSlot, scopeGeneration)) return@launch
                    val cleared = withContext(Dispatchers.IO) {
                        coordinator.composerDraftClearIfCurrent(accountSlot, bot.id, sentDraft.draftId)
                    }
                    if (!isCurrentSendScope(bot.id, accountSlot, scopeGeneration)) return@launch
                    if (cleared && draftWriteGeneration == draftRevisionAtSend) {
                        activeDraftId = null
                        draftWriteGeneration += 1
                        draftWriteJob?.cancel()
                        draftWriteJob = null
                        commitState(
                            mutableState.value.copy(
                                draft = "",
                                draftAttachments = emptyList(),
                                attachmentBusy = false,
                            ),
                        )
                        refreshEditorSuggestions("")
                    }
                    sendMiniApp(miniAppId, text, requestId)
                } catch (error: CancellationException) {
                    throw error
                } catch (error: Throwable) {
                    if (isCurrentSendScope(bot.id, accountSlot, scopeGeneration)) {
                        commitState(
                            mutableState.value.copy(
                                busy = false,
                                operationId = null,
                                error = error.message ?: "Mini App composer draft could not be journaled",
                            ),
                        )
                    }
                }
            }
            return
        }

        val accountSlot = rosterSelectionAccountSlot
        val scopeGeneration = sendPresentationGeneration
        val draftRevisionAtSend = draftWriteGeneration
        val approvalAtSend = snapshot.pendingApproval
        val nonce = "android-mobile-bot-chat-${UUID.randomUUID()}"
        commitState(
            snapshot.copy(
                busy = true,
                error = null,
                approvalCleanup = MobileApprovalCleanupState(),
                messageTargetId = null,
            ),
        )
        viewModelScope.launch {
            try {
                val sentDraft = draftWriteMutex.withLock {
                    withContext(Dispatchers.IO) {
                        coordinator.composerDraftWrite(accountSlot, bot.id, snapshot.draft)
                    }
                }
                if (!isCurrentSendScope(bot.id, accountSlot, scopeGeneration)) return@launch
                val acknowledgement = withContext(Dispatchers.IO) {
                    coordinator.sendAcknowledgementBeginWithModelAndAttachments(
                        accountSlot = accountSlot,
                        agentId = bot.id,
                        nonce = nonce,
                        text = text,
                        richText = plainTipTapRichText(text),
                        model = snapshot.modelCatalog.selectedModelId,
                        attachments = sentDraft.attachments,
                        entries = listOf(messageAcknowledgementEntry(nonce)),
                    )
                }
                if (!isCurrentSendScope(bot.id, accountSlot, scopeGeneration)) return@launch
                val clearedDraft = withContext(Dispatchers.IO) {
                    coordinator.composerDraftClearIfCurrent(
                        accountSlot = accountSlot,
                        agentId = bot.id,
                        draftId = sentDraft.draftId,
                    )
                }
                if (!isCurrentSendScope(bot.id, accountSlot, scopeGeneration)) return@launch
                if (clearedDraft && draftWriteGeneration == draftRevisionAtSend) {
                    activeDraftId = null
                    draftWriteGeneration += 1
                    draftWriteJob?.cancel()
                    draftWriteJob = null
                    commitState(
                        mutableState.value.copy(
                            draft = "",
                            draftAttachments = emptyList(),
                            attachmentBusy = false,
                        ),
                    )
                    refreshEditorSuggestions("")
                }
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
                if (operationId == null) {
                    commitState(mutableState.value.copy(busy = false, operationId = null))
                    refreshDurableSendProjection(bot.id, accountSlot, scopeGeneration)
                    return@launch
                }
                renderTelemetry.begin(operationId)
                clearApprovalsAfterJournal(
                    agentId = bot.id,
                    accountSlot = accountSlot,
                    scopeGeneration = scopeGeneration,
                    nonce = dispatched.acknowledgement.nonce,
                    generation = dispatched.acknowledgement.generation,
                    approval = approvalAtSend,
                )
                if (!isCurrentSendScope(bot.id, accountSlot, scopeGeneration)) return@launch
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
                    renderTelemetry.begin(continuationOperationId)
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

    fun cancelQueuedSend(nonce: String) {
        val snapshot = mutableState.value
        val bot = snapshot.activeBot ?: return
        if (bot.miniAppId != null || nonce.isBlank()) return
        val queued = snapshot.messages.firstOrNull {
            it.clientNonce == nonce && isQueuedSendCancellationAllowed(it)
        } ?: return
        val generation = queued.sendAcknowledgementGeneration ?: return
        val accountSlot = rosterSelectionAccountSlot
        val scopeGeneration = sendPresentationGeneration
        viewModelScope.launch {
            runCatching {
                withContext(Dispatchers.IO) {
                    coordinator.sendAcknowledgementCancelQueued(
                        accountSlot = accountSlot,
                        agentId = bot.id,
                        nonce = nonce,
                        generation = generation,
                    )
                }
            }.onSuccess { cancelled ->
                if (!cancelled || !isCurrentSendScope(bot.id, accountSlot, scopeGeneration)) {
                    return@onSuccess
                }
                runCatching {
                    refreshDurableSendProjection(bot.id, accountSlot, scopeGeneration)
                }
            }.onFailure { error ->
                if (isCurrentSendScope(bot.id, accountSlot, scopeGeneration)) {
                    commitState(
                        mutableState.value.copy(
                            error = error.message ?: "Queued send cancellation failed",
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
        val approvalAtRetry = snapshot.pendingApproval
        val freshNonce = "sand-resend-v1:${nonce.length}:$nonce:${UUID.randomUUID()}"
        commitState(snapshot.copy(busy = true, error = null, approvalCleanup = MobileApprovalCleanupState()))
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
                if (operationId == null) {
                    commitState(mutableState.value.copy(busy = false, operationId = null))
                    refreshDurableSendProjection(bot.id, accountSlot, scopeGeneration)
                    return@launch
                }
                renderTelemetry.begin(operationId)
                clearApprovalsAfterJournal(
                    agentId = bot.id,
                    accountSlot = accountSlot,
                    scopeGeneration = scopeGeneration,
                    nonce = dispatched.acknowledgement.nonce,
                    generation = dispatched.acknowledgement.generation,
                    approval = approvalAtRetry,
                )
                if (!isCurrentSendScope(bot.id, accountSlot, scopeGeneration)) return@launch
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

    internal fun plainTipTapRichText(text: String): JSONObject =
        JSONObject()
            .put("type", "doc")
            .put(
                "content",
                JSONArray().apply {
                    text.split('\n').forEach { line ->
                        put(
                            JSONObject()
                                .put("type", "paragraph")
                                .put(
                                    "content",
                                    JSONArray().apply {
                                        if (line.isNotEmpty()) {
                                            put(JSONObject().put("type", "text").put("text", line))
                                        }
                                    },
                                ),
                        )
                    }
                },
            )

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


    private suspend fun clearApprovalsAfterJournal(
        agentId: String,
        accountSlot: String?,
        scopeGeneration: Long,
        nonce: String,
        generation: Long,
        approval: PendingAgentApproval?,
    ) {
        if (!isCurrentSendScope(agentId, accountSlot, scopeGeneration)) return
        commitState(
            mutableState.value.copy(
                approvalCleanup = MobileApprovalCleanupState(
                    status = MobileApprovalCleanupStatus.CLEARING,
                ),
            ),
        )
        val cleanup = withContext(Dispatchers.IO) {
            runCatching {
                coordinator.clearApprovalsAfterJournal(
                    accountSlot = accountSlot,
                    agentId = agentId,
                    nonce = nonce,
                    generation = generation,
                    approvalId = approval?.approvalId,
                    approvalOperationId = approval?.operationId,
                )
            }
        }
        if (!isCurrentSendScope(agentId, accountSlot, scopeGeneration)) return
        cleanup.onSuccess { result ->
            val current = mutableState.value
            commitState(
                current.copy(
                    pendingApproval = if (
                        result.cleared &&
                        approval != null &&
                        current.pendingApproval?.approvalId == approval.approvalId
                    ) {
                        null
                    } else {
                        current.pendingApproval
                    },
                    approvalCleanup = MobileApprovalCleanupState(),
                ),
            )
        }.onFailure { error ->
            commitState(
                mutableState.value.copy(
                    approvalCleanup = MobileApprovalCleanupState(
                        status = MobileApprovalCleanupStatus.FAILED,
                        error = error.message ?: "Approval cleanup failed after send was accepted",
                    ),
                ),
            )
        }
    }

    private data class DurableAgentTranscriptProjection(
        val messages: List<MobileChatMessage>,
        val findEntries: List<AndroidFindInChatEntry>,
        val nextBeforeSeq: Long?,
        val threadCounts: Map<String, Int>,
    )

    private fun loadCanonicalAgentTranscriptPage(
        agentId: String,
        beforeSeq: Long?,
    ): DurableAgentTranscriptProjection {
        val page = coordinator.transcriptPage(
            agentId = agentId,
            beforeSeq = beforeSeq,
            limit = 200,
        )
        val canonical = canonicalMobileTranscriptForAgent(page.entries, agentId)
        return DurableAgentTranscriptProjection(
            messages = joinClientSideToolResultCards(
                canonical = canonical,
                events = coordinator.replayClientSideToolV2(),
                agentId = agentId,
            ),
            findEntries = parseAndroidFindInChatEntries(page.entries, agentId),
            nextBeforeSeq = page.nextBeforeSeq,
            threadCounts = page.threadCounts,
        )
    }

    private fun loadDurableAgentTranscript(
        agentId: String,
        accountSlot: String?,
    ): DurableAgentTranscriptProjection {
        val page = loadCanonicalAgentTranscriptPage(agentId, beforeSeq = null)
        val acknowledgements = coordinator.sendAcknowledgementSnapshot(accountSlot, agentId)
        return page.copy(
            messages = projectDurableSendAcknowledgements(page.messages, acknowledgements),
        )
    }

    fun loadOlderMessages() {
        val snapshot = mutableState.value
        val bot = snapshot.activeBot ?: return
        val beforeSeq = snapshot.historyBeforeSeq ?: return
        if (
            bot.miniAppId != null ||
            !snapshot.historyHasMore ||
            snapshot.historyLoadingOlder ||
            historyLoadJob?.isActive == true
        ) {
            return
        }
        val accountSlot = rosterSelectionAccountSlot
        val generation = historyGeneration
        val baselineIds = snapshot.messages.mapTo(linkedSetOf(), MobileChatMessage::id)
        commitState(snapshot.copy(historyLoadingOlder = true, historyError = null))
        historyLoadJob = viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                runCatching { loadCanonicalAgentTranscriptPage(bot.id, beforeSeq) }
            }
            if (
                generation != historyGeneration ||
                accountSlot != rosterSelectionAccountSlot ||
                mutableState.value.activeBot?.id != bot.id
            ) {
                return@launch
            }
            result.onSuccess { older ->
                val current = mutableState.value
                val currentIds = current.messages.mapTo(linkedSetOf(), MobileChatMessage::id)
                val arrivedDuringLoad = current.messages.filter {
                    it.id !in baselineIds && it.id !in older.messages.mapTo(linkedSetOf(), MobileChatMessage::id)
                }
                val baselineCurrent = current.messages.filter { it.id in baselineIds }
                val merged = prependCanonicalMobileTranscriptPage(
                    older = older.messages,
                    current = baselineCurrent + arrivedDuringLoad,
                )
                historyOlderEntryIds.addAll(older.messages.map(MobileChatMessage::id))
                val olderFindIds = older.findEntries.mapTo(linkedSetOf(), AndroidFindInChatEntry::entryId)
                val mergedFind = (older.findEntries + current.findEntries.filter { it.entryId !in olderFindIds })
                    .distinctBy(AndroidFindInChatEntry::entryId)
                commitState(
                    current.copy(
                        messages = merged,
                        findEntries = mergedFind,
                        transcriptThreadSummaries = older.threadCounts + current.transcriptThreadSummaries,
                        historyBeforeSeq = older.nextBeforeSeq,
                        historyHasMore = older.nextBeforeSeq != null,
                        historyLoadingOlder = false,
                        historyError = null,
                        historyPrependRevision = current.historyPrependRevision + 1L,
                    ),
                )
            }.onFailure { error ->
                commitState(
                    mutableState.value.copy(
                        historyLoadingOlder = false,
                        historyError = error.message ?: "Older transcript page failed",
                    ),
                )
            }
        }
    }

    private suspend fun reconcilePendingApprovalAfterHostRestore(
        agentId: String,
        accountSlot: String?,
        scopeGeneration: Long,
    ) {
        if (!isCurrentSendScope(agentId, accountSlot, scopeGeneration)) return
        val approval = mutableState.value.pendingApproval ?: return
        val pending = withContext(Dispatchers.IO) {
            runCatching { coordinator.featureApprovalStatus(approval.approvalId) }
        }.getOrNull() ?: return
        if (!isCurrentSendScope(agentId, accountSlot, scopeGeneration)) return
        val current = mutableState.value
        if (!pending && current.pendingApproval?.approvalId == approval.approvalId) {
            commitState(
                current.copy(
                    pendingApproval = null,
                    approvalCleanup = MobileApprovalCleanupState(),
                ),
            )
        }
    }

    private suspend fun refreshDurableSendProjection(
        agentId: String,
        accountSlot: String?,
        scopeGeneration: Long,
    ) {
        val baselineIds = mutableState.value.messages.mapTo(linkedSetOf(), MobileChatMessage::id)
        val durable = withContext(Dispatchers.IO) {
            loadDurableAgentTranscript(agentId, accountSlot)
        }
        if (!isCurrentSendScope(agentId, accountSlot, scopeGeneration)) return
        val current = mutableState.value
        val canonicalIds = durable.messages.mapTo(linkedSetOf(), MobileChatMessage::id)
        val preservedOlder = current.messages.filter {
            it.id in historyOlderEntryIds && it.id !in canonicalIds
        }
        val arrivedDuringLoad = current.messages.filter {
            it.id !in baselineIds &&
                it.id !in canonicalIds &&
                it.id !in historyOlderEntryIds
        }
        val merged = (preservedOlder + durable.messages + arrivedDuringLoad)
            .distinctBy(MobileChatMessage::id)
        val preservedOlderFind = current.findEntries.filter { it.entryId in historyOlderEntryIds }
        commitState(
            current.copy(
                messages = merged,
                findEntries = (preservedOlderFind + durable.findEntries)
                    .distinctBy(AndroidFindInChatEntry::entryId),
                transcriptThreadSummaries =
                    current.transcriptThreadSummaries + durable.threadCounts,
                historyBeforeSeq = if (historyOlderEntryIds.isEmpty()) {
                    durable.nextBeforeSeq
                } else {
                    current.historyBeforeSeq
                },
                historyHasMore = if (historyOlderEntryIds.isEmpty()) {
                    durable.nextBeforeSeq != null
                } else {
                    current.historyHasMore
                },
            ),
        )
        reconcilePendingApprovalAfterHostRestore(agentId, accountSlot, scopeGeneration)
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

    fun resolveApproval(decision: MobileApprovalDecision) {
        val pending = mutableState.value.pendingApproval ?: return
        if (pending.resolving) return
        if (!pending.remoteDispatch && decision in setOf(MobileApprovalDecision.ALWAYS, MobileApprovalDecision.NEVER)) {
            commitState(mutableState.value.copy(error = "Standing permission applies only to Remote computer actions."))
            return
        }
        commitState(mutableState.value.copy(pendingApproval = pending.copy(resolving = true), error = null))
        viewModelScope.launch {
            runCatching {
                withContext(Dispatchers.IO) {
                    coordinator.featureApprovalResolve(
                        JSONObject()
                            .put("approvalId", pending.approvalId)
                            .put("decision", decision.wireValue),
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
            val visibleAgentId = mutableState.value.asyncTasksAgent?.id
            if (visibleAgentId != null && event.optString("parentAgentId") == visibleAgentId) {
                refreshAsyncTasks()
            }
            val parentId = event.optString("parentAgentId")
            if (mutableState.value.conversationOutline.parentAgentId == parentId) {
                refreshConversationOutline()
            }
            return
        }
        if (type == "agent.async-tasks.refresh") {
            if (mutableState.value.asyncTasksAgent != null) {
                refreshAsyncTasks()
            }
            return
        }
        if (type == "send-acknowledgement-flushed") {
            val active = mutableState.value.activeBot ?: return
            val accountSlot = rosterSelectionAccountSlot
            val eventAccountSlot = event.opt("accountSlot").let { raw ->
                if (raw is String) raw.takeIf(String::isNotBlank) else null
            }
            if (event.optString("agentId") != active.id || eventAccountSlot != accountSlot) return
            val operationId = event.optString("operationId").takeIf(String::isNotBlank) ?: return
            renderTelemetry.begin(operationId)
            val nonce = event.optString("nonce").takeIf(String::isNotBlank) ?: return
            val generation = event.optLong("generation", 0L).takeIf { it > 0L } ?: return
            val scopeGeneration = sendPresentationGeneration
            val approval = mutableState.value.pendingApproval
            viewModelScope.launch {
                clearApprovalsAfterJournal(
                    agentId = active.id,
                    accountSlot = accountSlot,
                    scopeGeneration = scopeGeneration,
                    nonce = nonce,
                    generation = generation,
                    approval = approval,
                )
                if (!isCurrentSendScope(active.id, accountSlot, scopeGeneration)) return@launch
                commitState(
                    mutableState.value.copy(
                        busy = true,
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
                runCatching {
                    refreshDurableSendProjection(active.id, accountSlot, scopeGeneration)
                }
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

        if (
            mutableState.value.conversationOutline.isOpen &&
            type in setOf(
                "chat.message",
                "chat.delta",
                "agent.step",
                "operation.started",
                "operation.completed",
                "operation.interrupted",
                "operation.failed",
            )
        ) {
            refreshConversationOutline()
        }

        val operationId = mutableState.value.operationId ?: return
        if (!mutableState.value.busy) return
        val eventOperationId = event.optString("operationId").ifBlank { operationId }
        if (type in setOf("chat.message", "chat.delta", "agent.step", "operation.started", "operation.completed", "operation.interrupted", "operation.failed", "model.routed", "approval.requested", "approval.resolved") && eventOperationId != operationId) {
            return
        }
        when (type) {
            "operation.started" -> renderTelemetry.begin(operationId)
            "chat.message" -> if (event.optString("role") != "user") {
                recordRenderChunk(operationId)
                removeThinking(operationId)
                upsertAssistant(operationId, event.optString("text"), append = false, streaming = false)
            }
            "chat.delta" -> {
                recordRenderChunk(operationId)
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
            "approval.resolved", "approval.cleared" -> {
                val approvalId = event.optString("approvalId")
                if (approvalId.isNotBlank() && mutableState.value.pendingApproval?.approvalId == approvalId) {
                    commitState(mutableState.value.copy(pendingApproval = null))
                }
            }
            "operation.completed" -> {
                finishRenderTelemetry(operationId, "turn_ended")
                removeThinking(operationId)
                finishAssistant(operationId)
                commitState(mutableState.value.copy(busy = false, operationId = null, pendingApproval = null))
            }
            "operation.interrupted" -> {
                finishRenderTelemetry(operationId, "superseded")
                removeThinking(operationId)
                finishAssistant(operationId)
                commitState(mutableState.value.copy(busy = false, operationId = null, pendingApproval = null))
            }
            "operation.failed" -> {
                renderTelemetry.abandon(operationId)
                removeThinking(operationId)
                finishAssistant(operationId)
                commitState(mutableState.value.copy(busy = false, operationId = null, pendingApproval = null, error = event.optString("message").ifBlank { "Bot run failed" }))
            }
        }
    }

    private fun recordRenderChunk(operationId: String) {
        val ttfr = renderTelemetry.observeChunk(operationId, "assistant_text") ?: return
        viewModelScope.launch(Dispatchers.IO) {
            runCatching {
                coordinator.reportRenderTtfr(
                    ttfrMs = ttfr.ttfrMs,
                    outcome = "rendered",
                    chunkType = ttfr.chunkType,
                    enterEpochMs = ttfr.enterEpochMs,
                    isFork = false,
                )
            }
        }
    }

    private fun finishRenderTelemetry(operationId: String, outcome: String) {
        val stream = renderTelemetry.finish(operationId, outcome) ?: return
        viewModelScope.launch(Dispatchers.IO) {
            runCatching {
                coordinator.reportRenderStream(
                    outcome = stream.outcome,
                    chunkCount = stream.chunkCount,
                    maxGapMs = stream.maxGapMs,
                    stalledMs = stream.stalledMs,
                    stallCount = stream.stallCount,
                    streamMs = stream.streamMs,
                    isFork = false,
                )
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
        paletteFileSearchJob?.cancel()
        paletteFileSearchJob = null
        paletteFileFence.cancel()
        paletteLinkJob?.cancel()
        paletteLinkJob = null
        paletteLinkFence.cancel()
        paletteLinkMetadataProvider.dispose()
        paletteRoutineJob?.cancel()
        paletteRoutineJob = null
        paletteRoutineFence.cancel()
        openBotGeneration += 1
        sendPresentationGeneration += 1
        transcriptThreadGeneration += 1
        transcriptThreadLoadJob?.cancel()
        transcriptThreadLoadJob = null
        asyncTasksGeneration += 1
        asyncTasksJob?.cancel()
        asyncTasksJob = null
        outlineGeneration += 1
        outlineRefreshPending = false
        outlineRefreshJob?.cancel()
        outlinePollJob?.cancel()
        outlineRefreshJob = null
        outlinePollJob = null
        groupMembersGeneration += 1
        editorSuggestionJob?.cancel()
        editorSuggestionJob = null
        draftScopeGeneration += 1
        draftWriteGeneration += 1
        draftWriteJob?.cancel()
        draftWriteJob = null
        activeDraftId = null
        editorSuggestionFence.dispose()
        agentSettingsFence.dispose()
        hiddenChatsMutationController.dispose()
        rosterSelection.dispose()
        featureEventSubscription?.close()
        featureEventSubscription = null
        renderTelemetry.clear()
        super.onCleared()
    }
}
