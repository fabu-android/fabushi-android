package com.wangzan101.mika.ui

import android.Manifest
import android.app.Activity
import android.content.Context
import android.content.Intent
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.app.ActivityCompat
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.compose.LifecycleStartEffect
import com.wangzan101.mika.MikaApp
import com.wangzan101.mika.cap.phone.PhoneServices
import com.wangzan101.mika.cap.phone.a11y.ScreenAccessibility
import com.wangzan101.mika.cap.phone.setup.DeveloperSettings
import com.wangzan101.mika.cap.sandbox.SandboxState
import com.wangzan101.mika.capsule.CapsuleMode
import com.wangzan101.mika.chat.ConversationStore
import com.wangzan101.mika.core.agent.TaskState
import com.wangzan101.mika.core.cap.sandbox.SandboxStatus
import com.wangzan101.mika.core.shell.BuiltinShellStatus
import com.wangzan101.mika.quit.MikaQuit
import com.wangzan101.mika.settings.AccessibilityState
import com.wangzan101.mika.settings.FeatureFlags
import com.wangzan101.mika.settings.Prerequisites
import com.wangzan101.mika.settings.ProviderStore
import com.wangzan101.mika.settings.SystemStatus
import com.wangzan101.mika.settings.SystemStatusMonitor
import com.wangzan101.mika.task.TaskService
import com.wangzan101.mika.ui.components.KeyboardDismissArea
import com.wangzan101.mika.ui.theme.MikaTheme
import com.wangzan101.mika.ui.theme.animationsOff
import com.wangzan101.mika.ui.theme.appearanceDark
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.callbackFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.launch

/**
 * Snapshot of Mika's phone-control prerequisites, capsule overlay state, and active task runs
 * exposed to the host application (`:app`) for UI badges and Telegram/agent integration.
 */
data class MikaPhoneControlStatus(
    val systemStatus: SystemStatus,
    val accessibilityState: AccessibilityState,
    val accessibilityReady: Boolean,
    val helperStatus: BuiltinShellStatus,
    val helperRunning: Boolean,
    val wirelessDebuggingEnabled: Boolean,
    val developerOptionsEnabled: Boolean,
    val phoneSetupNeeded: Boolean,
    val phoneReady: Boolean,
    val canStartTask: Boolean,
    val hasMainModel: Boolean,
    val capsuleMode: CapsuleMode,
    val capsuleEnabled: Boolean,
    val sandboxStatus: SandboxStatus,
    val taskBusy: Boolean,
    val currentTaskState: TaskState,
    val activeConversations: Set<String>,
)

/**
 * Embeds the complete interactive Mika phone-control & AI workspace (chat, virtual-display PiP,
 * onboarding, phone setup, Wireless ADB pairing, model setup, memory, skills, browser, and plans)
 * inside a host Compose container such as Fabushi's tab or drawer screen.
 */
@Composable
fun EmbeddedMikaWorkspace(
    onBackToTelegram: (() -> Unit)? = null,
) {
    EmbeddedMikaWorkspace(
        modifier = Modifier.fillMaxSize(),
        onBackToTelegram = onBackToTelegram,
    )
}

/**
 * Parameterized overload of [EmbeddedMikaWorkspace] allowing the host screen to target a specific
 * conversation, open the phone-control setup flow directly, or launch into Teach mode.
 */
@Composable
fun EmbeddedMikaWorkspace(
    modifier: Modifier = Modifier,
    initialConversationId: String? = null,
    openPhoneSetupInitially: Boolean = false,
    openTeachInitially: Boolean = false,
    showBackToTelegramChip: Boolean = false,
    onBackToTelegram: (() -> Unit)? = null,
) {
    val context = LocalContext.current
    val app = remember(context) { context.applicationContext as MikaApp }
    val phone = remember(context) { PhoneServices.of(context) }
    val store = remember(context) { ProviderStore.get(context) }
    val monitor = remember(context) { SystemStatusMonitor(context, phone.shellControl) }

    var requestedConversation by rememberSaveable(initialConversationId) { mutableStateOf(initialConversationId) }
    var openPhoneSetup by rememberSaveable(openPhoneSetupInitially) { mutableStateOf(openPhoneSetupInitially) }
    var openTeach by rememberSaveable(openTeachInitially) { mutableStateOf(openTeachInitially) }

    val notificationLauncher = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
        val activity = context as? Activity
        if (!granted && (activity == null || !ActivityCompat.shouldShowRequestPermissionRationale(activity, Manifest.permission.POST_NOTIFICATIONS))) {
            openMikaAppNotificationSettings(context)
        }
        monitor.refresh()
    }

    LifecycleStartEffect(monitor, app) {
        MikaQuit.resume(context)
        monitor.start()
        app.mcp.ensureRunning()
        onStopOrDispose {
            monitor.stop()
        }
    }

    LifecycleResumeEffect(monitor, app) {
        monitor.refresh()
        app.autoArchiveNow()
        onPauseOrDispose {}
    }

    DisposableEffect(monitor) {
        onDispose { monitor.stop() }
    }

    val dark = appearanceDark()
    MikaTheme(dark = dark, reduceMotion = animationsOff(context)) {
        KeyboardDismissArea(modifier.fillMaxSize()) {
            Box(Modifier.fillMaxSize()) {
                MikaRoot(
                    store = store,
                    monitor = monitor,
                    activityFix = { fix ->
                        performMikaSystemFix(
                            context = context,
                            phone = phone,
                            fix = fix,
                            requestNotifications = { notificationLauncher.launch(Manifest.permission.POST_NOTIFICATIONS) },
                        )
                    },
                    requestedConversation = requestedConversation,
                    onConversationOpened = { requestedConversation = null },
                    requestedCalls = false,
                    requestedCallHistoryId = null,
                    onCallsOpened = {},
                    sharedDraft = null,
                    onSharedDraftOpened = { _, _ -> },
                    openTeach = openTeach,
                    onTeachOpened = { openTeach = false },
                    openPhoneSetup = openPhoneSetup,
                    onPhoneSetupOpened = { openPhoneSetup = false },
                    onExitToHost = onBackToTelegram,
                )
                if (showBackToTelegramChip && onBackToTelegram != null) {
                    BackToHostPill(
                        dark = dark,
                        onClick = onBackToTelegram,
                        modifier = Modifier
                            .align(Alignment.BottomEnd)
                            .navigationBarsPadding()
                            .padding(end = 16.dp, bottom = 84.dp),
                    )
                }
            }
        }
    }
}

@Composable
private fun BackToHostPill(
    dark: Boolean,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val shape = RoundedCornerShape(999.dp)
    val bg = if (dark) Color(0xCC1E2430) else Color(0xE6FFFFFF)
    val border = if (dark) Color(0x33FFFFFF) else Color(0x22000000)
    val fg = if (dark) Color(0xFFF2F5FA) else Color(0xFF1A2130)
    Row(
        modifier = modifier
            .clip(shape)
            .background(bg, shape)
            .border(1.dp, border, shape)
            .clickable(role = Role.Button, onClick = onClick)
            .padding(horizontal = 12.dp, vertical = 7.dp),
        horizontalArrangement = Arrangement.spacedBy(6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            text = "← Telegram",
            color = fg,
            fontSize = 12.sp,
            fontWeight = FontWeight.SemiBold,
        )
    }
}

/** Resolves the [MikaApp] singleton from any Android [Context]. */
fun Context.mikaApp(): MikaApp = applicationContext as MikaApp

/** Reads a one-shot snapshot of Mika's phone-control setup, capsule, and task status. */
fun MikaApp.phoneControlStatus(): MikaPhoneControlStatus {
    val monitor = SystemStatusMonitor(this, phone.shellControl)
    val status = monitor.status.value
    val hasMain = providers.roles.value.main != null
    val check = Prerequisites.check(status, hasMain)
    val flags = FeatureFlags.get(this)
    val mode = flags.capsuleMode()
    return MikaPhoneControlStatus(
        systemStatus = status,
        accessibilityState = status.accessibility,
        accessibilityReady = status.accessibility == AccessibilityState.On,
        helperStatus = status.builtinShell,
        helperRunning = status.builtinShell == BuiltinShellStatus.Running,
        wirelessDebuggingEnabled = status.wirelessDebugging,
        developerOptionsEnabled = status.developerOptions,
        phoneSetupNeeded = phone.setupPrompt.needed.value && status.builtinShell != BuiltinShellStatus.Running,
        phoneReady = check.phoneReady,
        canStartTask = check.canStartTask,
        hasMainModel = hasMain,
        capsuleMode = mode,
        capsuleEnabled = mode != CapsuleMode.Off && !flags.quitRequested(),
        sandboxStatus = SandboxState.get(this).status.value,
        taskBusy = taskState.busy.value,
        currentTaskState = taskState.state.value,
        activeConversations = taskState.activeConversations.value.toSet(),
    )
}

/** Observes live updates to [MikaPhoneControlStatus] across accessibility, shell helper, capsule, and task state changes. */
fun MikaApp.observePhoneControlStatus(): Flow<MikaPhoneControlStatus> {
    val app = this
    val flags = FeatureFlags.get(app)
    val monitorFlow: Flow<SystemStatus> = callbackFlow {
        val monitor = SystemStatusMonitor(app, app.phone.shellControl)
        monitor.start()
        val job = launch {
            monitor.status.collect { s -> trySend(s) }
        }
        awaitClose {
            job.cancel()
            monitor.stop()
        }
    }
    val baseFlow = combine(
        monitorFlow,
        providers.roles,
        phone.setupPrompt.needed,
        flags.capsuleModeChanges(),
        flags.quitChanges(),
    ) { status, roles, setupNeeded, capsuleMode, quit ->
        val hasMain = roles.main != null
        val check = Prerequisites.check(status, hasMain)
        MikaPhoneControlStatus(
            systemStatus = status,
            accessibilityState = status.accessibility,
            accessibilityReady = status.accessibility == AccessibilityState.On,
            helperStatus = status.builtinShell,
            helperRunning = status.builtinShell == BuiltinShellStatus.Running,
            wirelessDebuggingEnabled = status.wirelessDebugging,
            developerOptionsEnabled = status.developerOptions,
            phoneSetupNeeded = setupNeeded && status.builtinShell != BuiltinShellStatus.Running,
            phoneReady = check.phoneReady,
            canStartTask = check.canStartTask,
            hasMainModel = hasMain,
            capsuleMode = capsuleMode,
            capsuleEnabled = capsuleMode != CapsuleMode.Off && !quit,
            sandboxStatus = SandboxState.get(app).status.value,
            taskBusy = app.taskState.busy.value,
            currentTaskState = app.taskState.state.value,
            activeConversations = app.taskState.activeConversations.value.toSet(),
        )
    }
    return combine(
        baseFlow,
        SandboxState.get(app).status,
        taskState.busy,
        taskState.state,
        taskState.activeConversations,
    ) { partial, sandbox, busy, state, active ->
        partial.copy(
            sandboxStatus = sandbox,
            taskBusy = busy,
            currentTaskState = state,
            activeConversations = active.toSet(),
        )
    }.distinctUntilChanged()
}

/** True when [MikaAccessibilityService] is actively bound and connected. */
fun MikaApp.isAccessibilityReady(): Boolean = ScreenAccessibility.instance.value != null

/** True when the privileged Mika shell helper is connected and running. */
fun MikaApp.isHelperReady(): Boolean = phone.shellControl.status.value == BuiltinShellStatus.Running

/** True when Wireless ADB / shell helper is paired and ready to execute privileged commands. */
fun MikaApp.isWirelessAdbReady(): Boolean = isHelperReady()

/** True when Android Wireless Debugging is currently enabled in Developer Options. */
fun MikaApp.isWirelessAdbEnabled(): Boolean = DeveloperSettings.wirelessDebugging(contentResolver)

/** True when Mika's floating capsule overlay preference is enabled (DuringTasks or Always) and Mika is not in quit state. */
fun MikaApp.isCapsuleEnabled(): Boolean {
    val flags = FeatureFlags.get(this)
    return flags.capsuleMode() != CapsuleMode.Off && !flags.quitRequested()
}

/** Enables or disables Mika's floating capsule overlay. */
fun MikaApp.setCapsuleEnabled(enabled: Boolean, activeMode: CapsuleMode = CapsuleMode.Always) {
    val flags = FeatureFlags.get(this)
    if (enabled) {
        MikaQuit.resume(this)
        val target = if (activeMode == CapsuleMode.Off) CapsuleMode.Always else activeMode
        flags.setCapsuleMode(target)
    } else {
        flags.setCapsuleMode(CapsuleMode.Off)
    }
}

/** Toggles Mika's floating capsule overlay and returns whether it is now enabled. */
fun MikaApp.toggleCapsule(): Boolean {
    val flags = FeatureFlags.get(this)
    if (flags.quitRequested()) {
        MikaQuit.resume(this)
    }
    flags.toggleCapsule()
    return isCapsuleEnabled()
}

/**
 * Programmatically dispatches a phone-control / agent task to [TaskService], creating a new Mika
 * conversation when [conversationId] is null. Returns the conversation ID running the task.
 */
fun MikaApp.launchPhoneTask(goal: String, conversationId: String? = null): String {
    val trimmed = goal.trim()
    require(trimmed.isNotEmpty()) { "Phone task goal must not be blank" }
    MikaQuit.resume(this)
    val targetConversationId = conversationId ?: ConversationStore.shared(this).newConversation()
    TaskService.start(this, trimmed, targetConversationId)
    return targetConversationId
}

/** Sends a live steering instruction to a running Mika phone task. */
fun MikaApp.steerPhoneTask(conversationId: String, messageId: String, text: String) {
    val trimmed = text.trim()
    if (trimmed.isEmpty()) return
    TaskService.start(this, trimmed, conversationId, messageId)
}

/** Cancels the active Mika phone task (or the specific task in [conversationId] if provided). */
fun MikaApp.cancelPhoneTask(conversationId: String? = null) {
    if (conversationId == null) {
        TaskService.cancel(this)
    } else {
        val runKey = taskState.run(conversationId)?.key ?: return
        TaskService.cancel(this, runKey)
    }
}

/** Answers an `ask_user` prompt for an active Mika task. */
fun MikaApp.answerPhoneTask(text: String, conversationId: String? = null) {
    val runKey = conversationId?.let { taskState.run(it)?.key }
    TaskService.answer(this, text, runKey)
}

/** Approves a pending irreversible-action confirmation request. */
fun MikaApp.approvePhoneTask(requestKey: Int, conversationId: String? = null) {
    val runKey = conversationId?.let { taskState.run(it)?.key }
    TaskService.approve(this, requestKey, runKey)
}

/** Declines a pending irreversible-action confirmation request. */
fun MikaApp.declinePhoneTask(requestKey: Int, conversationId: String? = null) {
    val runKey = conversationId?.let { taskState.run(it)?.key }
    TaskService.decline(this, requestKey, runKey)
}

/** Opens Mika's standalone [MainActivity] (for deep links, Teach mode, or internal handoffs). */
fun Context.openMikaStandaloneActivity(
    conversationId: String? = null,
    openTeach: Boolean = false,
    teachGoal: String? = null,
) {
    val intent = Intent(this, MainActivity::class.java).apply {
        if (this@openMikaStandaloneActivity !is Activity) addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP)
        if (conversationId != null) putExtra(MainActivity.EXTRA_OPEN_CONVERSATION, conversationId)
        if (openTeach) putExtra(MainActivity.EXTRA_OPEN_TEACH, true)
        if (teachGoal != null) putExtra(MainActivity.EXTRA_TEACH_GOAL, teachGoal)
    }
    startActivity(intent)
}
