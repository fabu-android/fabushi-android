package com.ombhrum.fabushi.mika

import android.content.Intent
import android.provider.Settings
import android.util.Log
import com.ombhrum.fabushi.core.MahayanaHost
import com.ombhrum.fabushi.telegram.MikaBridgeCallbacks
import com.ombhrum.fabushi.telegram.MikaBridgeUiSnapshot
import com.ombhrum.fabushi.telegram.MikaTaskSummaryItem
import com.wangzan101.mika.MikaApp
import com.wangzan101.mika.cap.sandbox.AndroidSandbox
import com.wangzan101.mika.cap.sandbox.SandboxState
import com.wangzan101.mika.chat.ConversationStore
import com.wangzan101.mika.confirm.ConfirmationCopy
import com.wangzan101.mika.confirm.ConfirmationNotifier
import com.wangzan101.mika.core.agent.TaskState
import com.wangzan101.mika.core.cap.sandbox.SandboxStatus
import com.wangzan101.mika.core.run.ChannelPeer
import com.wangzan101.mika.core.run.Delivery
import com.wangzan101.mika.core.run.MemoryOwnerIdentity
import com.wangzan101.mika.mcp.McpServerState
import com.wangzan101.mika.settings.FeatureFlags
import com.wangzan101.mika.settings.ProviderConfig
import com.wangzan101.mika.ui.MikaPhoneControlStatus
import com.wangzan101.mika.ui.answerPhoneTask
import com.wangzan101.mika.ui.approvePhoneTask
import com.wangzan101.mika.ui.cancelPhoneTask
import com.wangzan101.mika.ui.declinePhoneTask
import com.wangzan101.mika.ui.launchPhoneTask
import com.wangzan101.mika.ui.observePhoneControlStatus
import com.wangzan101.mika.ui.openMikaStandaloneActivity
import com.wangzan101.mika.ui.phoneControlStatus
import com.wangzan101.mika.ui.setCapsuleEnabled
import com.wangzan101.mika.ui.terminal.TerminalLauncher
import java.text.DateFormat
import java.util.Date
import java.util.UUID
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.atomic.AtomicInteger
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.launch
import org.json.JSONObject

/**
 * Bidirectional in-process HostChannel adapter connecting Telegram chats, inline confirmation
 * actions, and the `local-android-mika` node with `:mika-agent`'s [MikaApp], `registerHostChannel`,
 * `TaskService`, `ConfirmationGate`, `McpRuntime`, `ProviderStore`, and `AndroidSandbox`.
 */
class TelegramMikaChannelBridge(
    private val app: MikaApp,
) : MikaBridgeCallbacks, AutoCloseable {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private val recentTasksLock = Any()
    private val recentTasks = CopyOnWriteArrayList<MikaTaskSummaryItem>()
    private val stepListeners = ConcurrentHashMap<String, (String) -> Unit>()
    private val stepCounter = AtomicInteger(0)

    val owners: MemoryOwnerIdentity = MemoryOwnerIdentity()
    val channel: TelegramMikaChannel = TelegramMikaChannel(
        outbox = app.outbox,
        onDeliveryEvent = ::handleChannelDeliveryEvent,
    )

    private val _snapshotFlow = MutableStateFlow(MikaBridgeUiSnapshot())
    val snapshotFlow: StateFlow<MikaBridgeUiSnapshot> = _snapshotFlow.asStateFlow()

    @Volatile private var hostChannelRegistration: AutoCloseable? = null
    @Volatile private var observerJob: Job? = null
    @Volatile private var activeTaskMonitorJob: Job? = null
    @Volatile private var activeMikaConversationId: String? = null
    @Volatile private var activePeer: ChannelPeer? = null
    @Volatile private var pendingConfirmationState: TaskState.WaitingConfirmation? = null

    private val hostInitLock = Any()
    @Volatile private var hostInitialized = false
    @Volatile private var hostRef: MahayanaHost? = null

    private val hostOrNull: MahayanaHost?
        get() {
            if (!hostInitialized) {
                synchronized(hostInitLock) {
                    if (!hostInitialized) {
                        hostRef = runCatching { MahayanaHost(app) }.getOrNull()
                        hostInitialized = true
                    }
                }
            }
            return hostRef
        }

    fun startObserving() {
        if (hostChannelRegistration == null) {
            runCatching {
                hostChannelRegistration = app.registerHostChannel(channel, owners)
            }.onFailure { err ->
                Log.w("TelegramMikaBridge", "Failed to register TelegramMikaChannel host channel", err)
            }
        }
        if (observerJob != null) return
        observerJob = scope.launch {
            runCatching {
                combine(
                    app.observePhoneControlStatus(),
                    app.mcp.state,
                    app.mcp.settings.enabled,
                    app.mcp.usb.enabled,
                    app.mcp.relayState,
                ) { status, _, _, _, _ ->
                    mapStatusToSnapshot(status)
                }.collect { snap ->
                    _snapshotFlow.value = snap
                }
            }.onFailure { err ->
                Log.w("TelegramMikaBridge", "Status flow collection stopped", err)
            }
        }
    }

    fun bindOwnerPeer(peerId: String, threadId: String): ChannelPeer {
        val peer = ChannelPeer(TelegramMikaChannel.ID, peerId, threadId)
        owners.bind(peer)
        activePeer = peer
        return peer
    }

    override fun channelEvents(): SharedFlow<MikaChannelCardEvent> = channel.events

    override fun currentSnapshot(): MikaBridgeUiSnapshot {
        return runCatching {
            val status = app.phoneControlStatus()
            mapStatusToSnapshot(status).also { _snapshotFlow.value = it }
        }.getOrElse {
            _snapshotFlow.value
        }
    }

    override fun submitPhoneTask(
        prompt: String,
        sourceConversationId: String?,
        onStepUpdate: (String) -> Unit,
    ) {
        val goal = prompt.trim()
        if (goal.isEmpty()) return

        val status = runCatching { app.phoneControlStatus() }.getOrNull()
        val now = nowTime()
        val threadId = sourceConversationId?.takeIf { it.isNotBlank() }
            ?: activeMikaConversationId
            ?: runCatching { ConversationStore.shared(app).newConversation() }.getOrElse { "mika-${UUID.randomUUID().toString().take(8)}" }

        if (status != null && !status.hasMainModel) {
            val diag = buildDiagnosticSummary(status)
            val diagId = "diag-${System.currentTimeMillis() % 100_000}"
            recordRecentTask(
                MikaTaskSummaryItem(
                    taskId = diagId,
                    prompt = goal,
                    status = "waiting_setup",
                    latestStep = diag,
                    updatedAtLabel = now,
                ),
            )
            onStepUpdate(diag)
            persistBridgeEvent(diagId, "diagnostic", diag)
            return
        }

        stepListeners[threadId] = onStepUpdate
        activeMikaConversationId = threadId
        val peer = bindOwnerPeer(activeOwnerId(), threadId)

        if (channel.hasActiveSink()) {
            val messageId = "msg-${UUID.randomUUID()}"
            val item = MikaTaskSummaryItem(
                taskId = threadId,
                prompt = goal,
                status = "running",
                latestStep = "已通过 HostChannel 投递至 Mika TaskService (会话 $threadId)",
                updatedAtLabel = now,
            )
            recordRecentTask(item)
            persistBridgeEvent(threadId, "started", item.latestStep)
            scope.launch {
                val ok = runCatching { channel.submitTask(peer, messageId, goal) }.getOrDefault(false)
                if (!ok) {
                    launchFallbackPhoneTask(goal, threadId, onStepUpdate)
                } else {
                    observeActiveRunSteps(threadId, goal, onStepUpdate)
                }
            }
        } else {
            launchFallbackPhoneTask(goal, threadId, onStepUpdate)
        }
    }

    private fun launchFallbackPhoneTask(
        goal: String,
        threadId: String,
        onStepUpdate: (String) -> Unit,
    ) {
        val now = nowTime()
        runCatching {
            val convId = app.launchPhoneTask(goal = goal, conversationId = threadId)
            activeMikaConversationId = convId
            val item = MikaTaskSummaryItem(
                taskId = convId,
                prompt = goal,
                status = "running",
                latestStep = "已分派至 Mika TaskService (会话 $convId)",
                updatedAtLabel = now,
            )
            recordRecentTask(item)
            persistBridgeEvent(convId, "started", item.latestStep)
            observeActiveRunSteps(convId, goal, onStepUpdate)
        }.onFailure { err ->
            val msg = "启动 Mika 任务异常：${err.message ?: err::class.java.simpleName}"
            onStepUpdate(msg)
            persistBridgeEvent(threadId, "error", msg)
        }
    }

    private suspend fun handleChannelDeliveryEvent(event: MikaChannelCardEvent) {
        val stepText = when (event.kind) {
            MikaChannelCardEvent.Kind.ASK_CONFIRM -> "⚠️ 待确认：${event.text}"
            MikaChannelCardEvent.Kind.ASK_INPUT -> "❓ 等待补充输入：${event.text}"
            MikaChannelCardEvent.Kind.ASK_TAKEOVER -> "🤚 请求人工接管：${event.text}"
            MikaChannelCardEvent.Kind.RESULT -> "✅ ${event.title?.ifBlank { null } ?: "任务完成"}：${event.text}"
            MikaChannelCardEvent.Kind.IMAGE -> "🖼 已回传截图：${event.imageName ?: event.text}"
            MikaChannelCardEvent.Kind.PROGRESS -> event.text
            MikaChannelCardEvent.Kind.NOTICE -> event.text
        }
        val status = when (event.kind) {
            MikaChannelCardEvent.Kind.ASK_CONFIRM,
            MikaChannelCardEvent.Kind.ASK_INPUT,
            MikaChannelCardEvent.Kind.ASK_TAKEOVER -> "waiting_confirmation"
            MikaChannelCardEvent.Kind.RESULT -> "completed"
            else -> "running"
        }
        recordRecentTask(
            MikaTaskSummaryItem(
                taskId = event.threadId,
                prompt = recentTasks.firstOrNull { it.taskId == event.threadId }?.prompt ?: "Mika 手机任务",
                status = status,
                latestStep = stepText,
                updatedAtLabel = nowTime(),
            ),
        )
        stepListeners[event.threadId]?.invoke(stepText)
        persistBridgeEvent(event.threadId, status, stepText)
        _snapshotFlow.value = currentSnapshot()
    }

    private fun observeActiveRunSteps(
        mikaConversationId: String,
        goal: String,
        onStepUpdate: (String) -> Unit,
    ) {
        activeTaskMonitorJob?.cancel()
        activeTaskMonitorJob = scope.launch {
            var lastRendered = ""
            app.taskState.state.collect { state ->
                val stepLine = formatTaskState(state)
                if (state is TaskState.WaitingConfirmation) {
                    pendingConfirmationState = state
                } else if (pendingConfirmationState != null) {
                    pendingConfirmationState = null
                }

                if (stepLine != null && stepLine != lastRendered) {
                    lastRendered = stepLine
                    val mappedStatus = when (state) {
                        is TaskState.WaitingConfirmation, is TaskState.WaitingTakeover, is TaskState.WaitingInput -> "waiting_confirmation"
                        is TaskState.Succeeded -> "completed"
                        is TaskState.Failed, TaskState.Cancelled -> "failed"
                        else -> "running"
                    }
                    recordRecentTask(
                        MikaTaskSummaryItem(
                            taskId = mikaConversationId,
                            prompt = goal,
                            status = mappedStatus,
                            latestStep = stepLine,
                            updatedAtLabel = nowTime(),
                        ),
                    )
                    _snapshotFlow.value = currentSnapshot()
                    onStepUpdate(stepLine)
                    persistBridgeEvent(mikaConversationId, mappedStatus, stepLine)
                }
            }
        }
    }

    override fun stopActiveTask() {
        runCatching {
            app.cancelPhoneTask(activeMikaConversationId)
            pendingConfirmationState = null
            _snapshotFlow.value = currentSnapshot()
        }
    }

    override fun resolveConfirmation(approved: Boolean) {
        resolveConfirmation(approved, activeMikaConversationId, null)
    }

    override fun resolveConfirmation(approved: Boolean, threadId: String?, askId: String?) {
        val targetThread = threadId?.takeIf { it.isNotBlank() }
            ?: channel.pendingAsksFlow.value.entries.firstOrNull { it.value.ask.kind == Delivery.Ask.Kind.Confirm }?.key
            ?: activeMikaConversationId
        scope.launch {
            val handledByChannel = if (targetThread != null) {
                runCatching { channel.resolveConfirmationForThread(targetThread, approved, askId) }.getOrDefault(false)
            } else false
            if (!handledByChannel) {
                if (askId != null) {
                    // Explicit askId did not match any active pending channel confirmation; reject
                    // cleanly without falling back to app.approvePhoneTask or clearing pending state.
                    return@launch
                }
                val waiting = pendingConfirmationState ?: (app.taskState.state.value as? TaskState.WaitingConfirmation)
                if (waiting != null) {
                    val key = ConfirmationNotifier.requestKey(waiting)
                    runCatching {
                        if (approved) {
                            app.approvePhoneTask(key, targetThread)
                        } else {
                            app.declinePhoneTask(key, targetThread)
                        }
                    }
                }
            }
            pendingConfirmationState = null
            _snapshotFlow.value = currentSnapshot()
        }
    }

    override fun answerAsk(text: String, threadId: String?, askId: String?): Boolean {
        val clean = text.trim()
        if (clean.isEmpty()) return false
        val targetThread = threadId?.takeIf { it.isNotBlank() }
            ?: channel.pendingAsksFlow.value.entries.firstOrNull { it.value.ask.kind == Delivery.Ask.Kind.Input }?.key
            ?: activeMikaConversationId
        val pending = targetThread?.let { channel.pendingAsksFlow.value[it] }
        if (pending != null && pending.ask.kind == Delivery.Ask.Kind.Input) {
            if (askId != null && pending.ask.askId != askId) {
                return false
            }
            scope.launch {
                runCatching {
                    channel.answerAsk(pending.peer, "ans-${UUID.randomUUID()}", clean, askId ?: pending.ask.askId)
                }
                _snapshotFlow.value = currentSnapshot()
            }
            return true
        }
        if (askId != null) {
            // Explicit askId was provided but no matching pending channel input ask exists.
            return false
        }
        if (app.taskState.state.value is TaskState.WaitingInput && (targetThread == null || targetThread == activeMikaConversationId)) {
            runCatching { app.answerPhoneTask(clean, targetThread) }
            _snapshotFlow.value = currentSnapshot()
            return true
        }
        return false
    }

    override suspend fun executeLocalSandboxCommand(command: String, timeoutMs: Long): Result<String> {
        val clean = command.trim()
        if (clean.isEmpty()) {
            return Result.failure(IllegalArgumentException("LOCAL_SANDBOX_EMPTY_COMMAND"))
        }
        val state = SandboxState.get(app)
        val status = state.status.value
        if (status !is SandboxStatus.Ready) {
            return Result.failure(
                IllegalStateException("LOCAL_SANDBOX_NOT_READY: Alpine PRoot sandbox status is ${status::class.java.simpleName}"),
            )
        }
        return runCatching {
            val sandbox = AndroidSandbox(app, state)
            val workspace = activeMikaConversationId ?: "local-android-mika"
            val result = sandbox.run(
                workspace = workspace,
                argv = listOf("/bin/sh", "-c", clean),
                cwd = null,
                timeoutMs = timeoutMs.coerceIn(1_000L, 120_000L),
                onProgress = {},
            )
            if (result.exitCode != 0) {
                error("LOCAL_SANDBOX_EXIT_${result.exitCode}: ${result.output.ifBlank { "command failed" }}")
            }
            result.output
        }
    }

    override fun syncMagpieProviderKey(
        presetId: String,
        displayName: String,
        protocol: String,
        baseUrl: String,
        defaultModel: String,
        apiKey: String,
    ) {
        if (apiKey.isBlank()) return
        val kind = when (protocol.lowercase()) {
            "anthropic" -> ProviderConfig.Kind.Anthropic
            "gemini" -> ProviderConfig.Kind.Gemini
            "responses", "openai_responses" -> ProviderConfig.Kind.OpenAiResponses
            else -> ProviderConfig.Kind.OpenAiCompat
        }
        runCatching {
            app.providers.connect(
                name = displayName.ifBlank { presetId },
                kind = kind,
                baseUrl = baseUrl,
                apiKey = apiKey,
                models = emptyList(),
                untested = true,
            )
        }.onFailure { err ->
            Log.w("TelegramMikaBridge", "Failed to sync Magpie provider key to Mika ProviderStore", err)
        }
    }

    override fun toggleFloatingCapsule(enabled: Boolean) {
        runCatching {
            app.setCapsuleEnabled(enabled)
            _snapshotFlow.value = currentSnapshot()
        }
    }

    override fun openAccessibilitySettings() {
        runCatching {
            val intent = Intent(Settings.ACTION_ACCESSIBILITY_SETTINGS).apply {
                addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            }
            app.startActivity(intent)
        }
    }

    override fun openWirelessAdbPairing() {
        runCatching {
            val devEnabled = app.phoneControlStatus().developerOptionsEnabled
            val action = if (devEnabled) {
                Settings.ACTION_APPLICATION_DEVELOPMENT_SETTINGS
            } else {
                Settings.ACTION_DEVICE_INFO_SETTINGS
            }
            app.startActivity(Intent(action).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        }.onFailure {
            runCatching { app.openMikaStandaloneActivity() }
        }
    }

    override fun openLinuxTerminal() {
        runCatching {
            val convId = activeMikaConversationId ?: ConversationStore.shared(app).newConversation()
            activeMikaConversationId = convId
            TerminalLauncher.open(app, convId, "Alpine PRoot Linux Sandbox")
        }
    }

    private fun activeOwnerId(): String {
        val uid = runCatching {
            hostOrNull?.request("telegram.server.session")?.optLong("userId", 0L) ?: 0L
        }.getOrDefault(0L)
        return if (uid > 0L) "user:$uid" else "android-owner"
    }

    private fun mapStatusToSnapshot(status: MikaPhoneControlStatus): MikaBridgeUiSnapshot {
        val currentState = status.currentTaskState
        val pendingChannelAsks = channel.pendingAsksFlow.value
        val pendingConfirmEntry = pendingChannelAsks.values.firstOrNull { it.ask.kind == Delivery.Ask.Kind.Confirm }
        val pendingInputEntry = pendingChannelAsks.values.firstOrNull { it.ask.kind == Delivery.Ask.Kind.Input }

        val confirmPrompt = pendingConfirmEntry?.ask?.text ?: when (currentState) {
            is TaskState.WaitingConfirmation -> {
                pendingConfirmationState = currentState
                val appLabel = ConfirmationNotifier.appLabel(app, currentState.request.app)
                "${ConfirmationCopy.title(currentState.request)}\n${ConfirmationCopy.detail(currentState.request, appLabel)}"
            }
            is TaskState.WaitingTakeover -> "需要人工接管 (${currentState.reason})：${currentState.message}"
            else -> null
        }
        val inputPrompt = pendingInputEntry?.ask?.text ?: (currentState as? TaskState.WaitingInput)?.prompt

        val paceLabel = runCatching {
            FeatureFlags.get(app).speedFlags().describe()
        }.getOrDefault("")

        val activeSummary = if (status.taskBusy || currentState !is TaskState.Idle) {
            val step = formatTaskState(currentState) ?: "正在执行…"
            MikaTaskSummaryItem(
                taskId = activeMikaConversationId ?: status.activeConversations.firstOrNull() ?: "mika-live",
                prompt = recentTasks.firstOrNull()?.prompt ?: "Mika 手机任务",
                status = if (confirmPrompt != null || inputPrompt != null) "waiting_confirmation" else "running",
                latestStep = step,
                updatedAtLabel = nowTime(),
            )
        } else {
            null
        }

        val mcpState = runCatching { app.mcp.state.value }.getOrDefault(McpServerState.Off)
        val mcpLanEnabled = runCatching { app.mcp.settings.enabled.value }.getOrDefault(false)
        val mcpLanPort = when (mcpState) {
            is McpServerState.Listening -> mcpState.port
            else -> runCatching { app.mcp.settings.port }.getOrDefault(0)
        }
        val mcpUsbEnabled = runCatching { app.mcp.usb.enabled.value }.getOrDefault(false)
        val mcpRelayEnabled = runCatching { app.mcp.relaySettings.config.value.enabled }.getOrDefault(false)
        val mcpRelayState = runCatching { app.mcp.relayState.value.toString() }.getOrDefault("")
        val mcpServerLabel = when (mcpState) {
            McpServerState.Off -> "Off"
            is McpServerState.Listening -> "Listening:${mcpState.port}"
            is McpServerState.Failed -> "Failed:${mcpState.message}"
        }

        val sandboxReady = status.sandboxStatus is SandboxStatus.Ready
        val isAvailable = status.canStartTask || status.phoneReady || status.accessibilityReady || status.helperRunning || sandboxReady

        return MikaBridgeUiSnapshot(
            available = isAvailable,
            accessibilityEnabled = status.accessibilityReady,
            wirelessAdbPaired = status.wirelessDebuggingEnabled || status.helperRunning,
            helperBinderAlive = status.helperRunning,
            virtualDisplayActive = app.tasks.agentAppShown.value || app.tasks.webScreenOpen.value,
            capsuleVisible = status.capsuleEnabled,
            sandboxReady = sandboxReady,
            activeSpeedProfile = paceLabel,
            activeTask = activeSummary,
            pendingConfirmationPrompt = confirmPrompt,
            recentTasks = recentTasks.toList(),
            mcpLanEnabled = mcpLanEnabled,
            mcpLanPort = mcpLanPort,
            mcpUsbEnabled = mcpUsbEnabled,
            mcpRelayEnabled = mcpRelayEnabled,
            mcpRelayState = mcpRelayState,
            mcpServerState = mcpServerLabel,
            pendingAskId = pendingConfirmEntry?.ask?.askId ?: pendingInputEntry?.ask?.askId,
            pendingAskThreadId = pendingConfirmEntry?.ask?.threadId ?: pendingInputEntry?.ask?.threadId,
            pendingInputPrompt = inputPrompt,
        )
    }

    private fun formatTaskState(state: TaskState): String? = when (state) {
        TaskState.Idle -> null
        is TaskState.Running -> {
            val detail = state.narration?.takeIf { it.isNotBlank() }
                ?: state.reason?.takeIf { it.isNotBlank() }
                ?: state.progress?.let { "连贯回放步骤执行中" }
                ?: state.tool?.let { "调用工具 `$it`" }
                ?: state.note.takeIf { it.isNotBlank() }
                ?: "思考规划中…"
            "步骤 #${state.step}: $detail"
        }
        is TaskState.Held -> "⏸ 等待前置条件 (${state.reason.code})：${state.reason.why.ifBlank { "等待资源就绪" }}"
        is TaskState.WaitingTakeover -> "🤚 请求人工接管 (${state.reason})：${state.message}"
        is TaskState.WaitingInput -> "❓ 等待补充输入：${state.prompt}"
        is TaskState.WaitingConfirmation -> {
            val appLabel = ConfirmationNotifier.appLabel(app, state.request.app)
            "⚠️ 安全门控拦截待确认：${ConfirmationCopy.body(state.request, appLabel)}"
        }
        is TaskState.Succeeded -> "✅ 任务完成：${state.answer}"
        is TaskState.Failed -> "❌ 任务未完成 (${state.kind.name})：${state.reason}"
        TaskState.Cancelled -> "⏹ 任务已手动取消"
    }

    private fun buildDiagnosticSummary(status: MikaPhoneControlStatus): String {
        val a11y = if (status.accessibilityReady) "已开启" else "未开启"
        val helper = if (status.helperRunning) "UID 2000 已连接" else "待无线 ADB 配对"
        val sandbox = when (status.sandboxStatus) {
            is SandboxStatus.Ready -> "Alpine PRoot 已就绪"
            is SandboxStatus.Unsupported -> "当前架构不支持"
            else -> "未就绪"
        }
        return "📋 本机操控诊断：无障碍 [$a11y] · Shell 助手 [$helper] · Linux 沙箱 [$sandbox]。请先配置主模型 API Key 并开启所需权限。"
    }

    internal fun recordRecentTask(item: MikaTaskSummaryItem) {
        synchronized(recentTasksLock) {
            recentTasks.removeAll { it.taskId == item.taskId }
            recentTasks.add(0, item)
            while (recentTasks.size > 15) {
                recentTasks.removeAt(recentTasks.lastIndex)
            }
        }
    }

    internal fun recentTasksSnapshot(): List<MikaTaskSummaryItem> = synchronized(recentTasksLock) {
        recentTasks.toList()
    }

    private fun persistBridgeEvent(
        taskId: String,
        phase: String,
        summary: String,
    ) {
        scope.launch(Dispatchers.IO) {
            hostOrNull?.requestResult(
                "mika.registerTaskEvent",
                JSONObject()
                    .put("taskId", taskId)
                    .put("stepIndex", stepCounter.incrementAndGet())
                    .put("phase", phase)
                    .put("summary", summary),
            )
        }
    }

    private fun nowTime(): String = DateFormat.getTimeInstance(DateFormat.SHORT).format(Date())

    override fun close() {
        runCatching { hostChannelRegistration?.close() }
        hostChannelRegistration = null
        channel.close()
        observerJob?.cancel()
        activeTaskMonitorJob?.cancel()
        scope.cancel()
        val initializedHost = synchronized(hostInitLock) {
            val existing = hostRef
            hostRef = null
            hostInitialized = true
            existing
        }
        runCatching { initializedHost?.close() }
    }
}
