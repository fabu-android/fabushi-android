package com.ombhrum.fabushi.mika

import android.content.Intent
import android.provider.Settings
import android.util.Log
import com.ombhrum.fabushi.core.MahayanaHost
import com.ombhrum.fabushi.telegram.MikaBridgeCallbacks
import com.ombhrum.fabushi.telegram.MikaBridgeUiSnapshot
import com.ombhrum.fabushi.telegram.MikaTaskSummaryItem
import com.wangzan101.mika.MikaApp
import com.wangzan101.mika.chat.ConversationStore
import com.wangzan101.mika.confirm.ConfirmationCopy
import com.wangzan101.mika.confirm.ConfirmationNotifier
import com.wangzan101.mika.core.agent.TaskState
import com.wangzan101.mika.core.cap.sandbox.SandboxStatus
import com.wangzan101.mika.settings.FeatureFlags
import com.wangzan101.mika.ui.MikaPhoneControlStatus
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
import java.util.concurrent.CopyOnWriteArrayList
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import org.json.JSONObject

/**
 * Bidirectional in-process channel adapter connecting Telegram chats (`@MikaPhoneBot`, `/mika`,
 * inline keyboard actions, and Cloud Computer local device node) with `:mika-agent`'s [MikaApp],
 * `TaskService`, `ConfirmationGate`, Floating Capsule, and Alpine `proot` Linux sandbox.
 */
class TelegramMikaChannelBridge(
    private val app: MikaApp,
) : MikaBridgeCallbacks, AutoCloseable {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private val recentTasks = CopyOnWriteArrayList<MikaTaskSummaryItem>()
    private val _snapshotFlow = MutableStateFlow(MikaBridgeUiSnapshot(available = true))
    val snapshotFlow: StateFlow<MikaBridgeUiSnapshot> = _snapshotFlow.asStateFlow()

    @Volatile private var observerJob: Job? = null
    @Volatile private var activeTaskMonitorJob: Job? = null
    @Volatile private var activeMikaConversationId: String? = null
    @Volatile private var pendingConfirmationState: TaskState.WaitingConfirmation? = null

    private val hostOrNull: MahayanaHost? by lazy {
        runCatching { MahayanaHost(app) }.getOrNull()
    }

    fun startObserving() {
        if (observerJob != null) return
        observerJob = scope.launch {
            runCatching {
                app.observePhoneControlStatus().collect { status ->
                    _snapshotFlow.value = mapStatusToSnapshot(status)
                }
            }.onFailure { err ->
                Log.w("TelegramMikaBridge", "Status flow collection stopped", err)
            }
        }
    }

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

        if (status != null && !status.hasMainModel) {
            val diag = buildDiagnosticSummary(status)
            recordRecentTask(
                MikaTaskSummaryItem(
                    taskId = "diag-${System.currentTimeMillis() % 100_000}",
                    prompt = goal,
                    status = "waiting_setup",
                    latestStep = diag,
                    updatedAtLabel = now,
                ),
            )
            onStepUpdate(diag)
            persistBridgeEvent("diagnostic", goal, sourceConversationId, diag)
            return
        }

        runCatching {
            val convId = app.launchPhoneTask(goal = goal, conversationId = activeMikaConversationId)
            activeMikaConversationId = convId
            val item = MikaTaskSummaryItem(
                taskId = convId,
                prompt = goal,
                status = "running",
                latestStep = "已分派至 Mika TaskService (会话 $convId)",
                updatedAtLabel = now,
            )
            recordRecentTask(item)
            persistBridgeEvent("started", goal, sourceConversationId, item.latestStep)
            observeActiveRunSteps(convId, goal, sourceConversationId, onStepUpdate)
        }.onFailure { err ->
            val msg = "启动 Mika 任务异常：${err.message ?: err::class.java.simpleName}"
            onStepUpdate(msg)
            persistBridgeEvent("error", goal, sourceConversationId, msg)
        }
    }

    private fun observeActiveRunSteps(
        mikaConversationId: String,
        goal: String,
        sourceConversationId: String?,
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
                    persistBridgeEvent(mappedStatus, goal, sourceConversationId, stepLine)
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
        val waiting = pendingConfirmationState ?: (app.taskState.state.value as? TaskState.WaitingConfirmation) ?: return
        val key = ConfirmationNotifier.requestKey(waiting)
        runCatching {
            if (approved) {
                app.approvePhoneTask(key, activeMikaConversationId)
            } else {
                app.declinePhoneTask(key, activeMikaConversationId)
            }
            pendingConfirmationState = null
            _snapshotFlow.value = currentSnapshot()
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

    private fun mapStatusToSnapshot(status: MikaPhoneControlStatus): MikaBridgeUiSnapshot {
        val currentState = status.currentTaskState
        val confirmPrompt = when (currentState) {
            is TaskState.WaitingConfirmation -> {
                pendingConfirmationState = currentState
                val appLabel = ConfirmationNotifier.appLabel(app, currentState.request.app)
                "${ConfirmationCopy.title(currentState.request)}\n${ConfirmationCopy.detail(currentState.request, appLabel)}"
            }
            is TaskState.WaitingTakeover -> "需要人工接管 (${currentState.reason})：${currentState.message}"
            else -> null
        }

        val paceLabel = runCatching {
            FeatureFlags.get(app).speedFlags().describe().ifBlank { "标准 (Balanced)" }
        }.getOrDefault("标准 (Balanced)")

        val activeSummary = if (status.taskBusy || currentState !is TaskState.Idle) {
            val step = formatTaskState(currentState) ?: "正在执行…"
            MikaTaskSummaryItem(
                taskId = activeMikaConversationId ?: status.activeConversations.firstOrNull() ?: "mika-live",
                prompt = recentTasks.firstOrNull()?.prompt ?: "Mika 手机任务",
                status = if (confirmPrompt != null) "waiting_confirmation" else "running",
                latestStep = step,
                updatedAtLabel = nowTime(),
            )
        } else {
            null
        }

        return MikaBridgeUiSnapshot(
            available = true,
            accessibilityEnabled = status.accessibilityReady,
            wirelessAdbPaired = status.wirelessDebuggingEnabled || status.helperRunning,
            helperBinderAlive = status.helperRunning,
            virtualDisplayActive = app.tasks.agentAppShown.value || app.tasks.webScreenOpen.value,
            capsuleVisible = status.capsuleEnabled,
            sandboxReady = status.sandboxStatus !is SandboxStatus.Unsupported,
            activeSpeedProfile = paceLabel,
            activeTask = activeSummary,
            pendingConfirmationPrompt = confirmPrompt,
            recentTasks = recentTasks.toList(),
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
            else -> "首次启动后台解压就绪中"
        }
        return "📋 本机操控诊断：无障碍 [$a11y] · Shell 助手 [$helper] · Linux 沙箱 [$sandbox]。请点击上方「打开 Mika 操控中心」完成主模型 Key 与无障碍授权后即可全自动操控手机！"
    }

    private fun recordRecentTask(item: MikaTaskSummaryItem) {
        recentTasks.removeAll { it.taskId == item.taskId }
        recentTasks.add(0, item)
        while (recentTasks.size > 15) {
            recentTasks.removeAt(recentTasks.lastIndex)
        }
    }

    private fun persistBridgeEvent(
        phase: String,
        prompt: String,
        sourceConversationId: String?,
        detail: String,
    ) {
        scope.launch(Dispatchers.IO) {
            runCatching {
                hostOrNull?.request(
                    "mika.registerTaskEvent",
                    JSONObject()
                        .put("phase", phase)
                        .put("prompt", prompt)
                        .put("conversationId", sourceConversationId ?: "bot-mika-phone")
                        .put("detail", detail)
                        .put("timestampMs", System.currentTimeMillis()),
                )
            }
        }
    }

    private fun nowTime(): String = DateFormat.getTimeInstance(DateFormat.SHORT).format(Date())

    override fun close() {
        observerJob?.cancel()
        activeTaskMonitorJob?.cancel()
        runCatching { hostOrNull?.close() }
    }
}
