package com.ombhrum.fabushi

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext


internal data class ComputerRebuildProgressProjection(
    val kind: String,
    val title: String,
    val steps: List<String>,
    val activeIndex: Int,
    val progress: Double,
    val reconnectVariant: String? = null,
) {
    val active: Boolean get() = kind.isNotBlank()
}

internal object ComputerRebuildProgressPolicy {
    private val updateSteps = listOf(
        "准备中",
        "备份数据",
        "重建 Fabushi 电脑",
        "启动 Fabushi 电脑",
        "清理",
        "重新连接",
    )
    private val resetSteps = listOf(
        "准备中",
        "清除数据",
        "创建 Fabushi 电脑",
        "启动 Fabushi 电脑",
        "清理",
        "重新连接",
    )
    private val recoverSteps = listOf(
        "准备中",
        "重建 Fabushi 电脑",
        "启动 Fabushi 电脑",
        "重新连接",
    )

    fun project(status: JSONObject): ComputerRebuildProgressProjection? {
        val kind = status.optString("kind").trim()
        if (kind.isEmpty() || kind == "null") return null
        if (kind == "reconnecting") {
            val boxPhase = status.optString("boxPhase")
            val connected = status.optBoolean("connected", true)
            val variant = when {
                !connected -> "network"
                boxPhase == "pulling" || boxPhase == "off" || boxPhase == "sleeping" -> "restarting"
                else -> "checking"
            }
            return ComputerRebuildProgressProjection(
                kind = kind,
                title = when (variant) {
                    "network" -> "正在重新连接"
                    "restarting" -> "Fabushi 电脑正在重启"
                    else -> "正在检查连接"
                },
                steps = listOf("重新连接"),
                activeIndex = 0,
                progress = 0.0,
                reconnectVariant = variant,
            )
        }

        val steps = when (kind) {
            "reset" -> resetSteps
            "recover" -> recoverSteps
            else -> updateSteps
        }
        val phases = status.optJSONArray("migrationPhases")
        var activeIndex = 0
        var afterHealthyPhase = false
        if (phases != null) {
            for (index in 0 until phases.length()) {
                val phase = phases.optString(index)
                val mapped = when (kind) {
                    "reset" -> when (phase) {
                        "wiping" -> 1
                        "creating" -> 2
                        "moving" -> 3
                        "cleaning-up" -> if (afterHealthyPhase) 4 else 1
                        "backing-up" -> 0
                        "done" -> steps.lastIndex
                        else -> null
                    }
                    "recover" -> when (phase) {
                        "backing-up", "wiping", "creating" -> 1
                        "moving" -> 2
                        "cleaning-up" -> if (afterHealthyPhase) 3 else 1
                        "done" -> steps.lastIndex
                        else -> null
                    }
                    else -> when (phase) {
                        "backing-up" -> 1
                        "creating" -> 2
                        "moving" -> 3
                        "cleaning-up" -> if (afterHealthyPhase) 4 else 2
                        "done" -> steps.lastIndex
                        else -> null
                    }
                }
                if (mapped != null) activeIndex = maxOf(activeIndex, mapped)
                if (phase != "cleaning-up") afterHealthyPhase = true
            }
        }

        val boxPhase = status.optString("boxPhase")
        val boxIndex = when {
            status.optBoolean("terminalMigration", false) -> steps.lastIndex
            boxPhase == "pulling" -> when (kind) {
                "reset" -> minOf(3, steps.lastIndex)
                "recover" -> minOf(2, steps.lastIndex)
                else -> minOf(2, steps.lastIndex)
            }
            status.optBoolean("leftHealthy", false) &&
                (boxPhase == "running" || boxPhase == "local") -> steps.lastIndex
            else -> 0
        }
        activeIndex = maxOf(activeIndex, boxIndex).coerceIn(0, steps.lastIndex)

        val pullPercent = status.optDouble("pullPercent", Double.NaN)
            .takeIf { it.isFinite() && it in 0.0..100.0 }
        val pullProgress = if (kind == "update" && boxPhase == "pulling" && pullPercent != null) {
            pullPercent / 100.0
        } else {
            0.0
        }
        val progress = ((activeIndex + pullProgress) / steps.size.toDouble()).coerceIn(0.0, 1.0)
        val title = when (kind) {
            "reset" -> "正在重置 Fabushi 电脑"
            "recover" -> "正在恢复 Fabushi 电脑"
            else -> "正在更新 Fabushi 电脑"
        }
        return ComputerRebuildProgressProjection(
            kind = kind,
            title = title,
            steps = steps,
            activeIndex = activeIndex,
            progress = progress,
        )
    }
}

/**
 * Restricted browser surface for human-operated remote computer sessions.
 *
 * The native panel owns account-scoped list/pair/revoke through the typed Coordinator contract.
 * The hosted viewport deliberately receives no native bridge and no pairing/session/executor secret.
 */
@Composable
fun RemoteComputerSurface(onClose: () -> Unit) {
    val context = LocalContext.current
    val coordinator = remember { CoordinatorClient.presentation() }
    val scope = rememberCoroutineScope()

    var status by remember { mutableStateOf("原生远端视图待连接") }

    var nativeState by remember { mutableStateOf(RemoteComputerNativeState.Empty) }
    var nativeBusy by remember { mutableStateOf(false) }
    var nativeError by remember { mutableStateOf<String?>(null) }
    var rebuildMessage by remember { mutableStateOf<String?>(null) }
    var rebuildProgress by remember { mutableStateOf<ComputerRebuildProgressProjection?>(null) }
    var pairingCode by remember { mutableStateOf("") }
    var pairingLabel by remember { mutableStateOf("Fabushi Android") }

    suspend fun loadNativeState(): RemoteComputerNativeState = withContext(Dispatchers.IO) {
        val initialSession = coordinator.remoteComputerSessionStatus()
        val settledSession = if (initialSession.optBoolean("createOutcomeUnknown", false)) {
            // A create response may have been lost after the server committed the session.
            // Reconcile the stable request identity; never resend create or replay user input.
            coordinator.remoteComputerSessionReconcile()
        } else {
            initialSession
        }
        RemoteComputerPresentationPolicy.parse(
            coordinator.remoteComputerList(),
            coordinator.remoteComputerPairingStatus(),
            settledSession,
        )
    }

    fun refreshNativeState() {
        if (nativeBusy) return
        scope.launch {
            nativeBusy = true
            runCatching { loadNativeState() }
                .onSuccess {
                    nativeState = it
                    nativeError = null
                }
                .onFailure {
                    nativeError = "无法刷新已配对电脑，请检查登录状态和网络后重试。"
                }
            nativeBusy = false
        }
    }

    LaunchedEffect(coordinator) {
        nativeBusy = true
        runCatching { loadNativeState() }
            .onSuccess {
                nativeState = it
                nativeError = null
            }
            .onFailure {
                nativeError = "无法读取已配对电脑。"
            }
        runCatching {
            withContext(Dispatchers.IO) {
                ComputerRebuildProgressPolicy.project(coordinator.computerRebuildStatus())
            }
        }.onSuccess { rebuildProgress = it }
        nativeBusy = false
    }

    LaunchedEffect(coordinator) {
        while (true) {
            runCatching {
                withContext(Dispatchers.IO) {
                    ComputerRebuildProgressPolicy.project(coordinator.computerRebuildStatus())
                }
            }.onSuccess { rebuildProgress = it }
            delay(1_500)
        }
    }

    BackHandler { onClose() }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(MaterialTheme.colorScheme.background)
            .windowInsetsPadding(WindowInsets.safeDrawing)
            .testTag(TestTags.RemoteComputerSurface),
    ) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(12.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Button(onClick = onClose, modifier = Modifier.testTag(TestTags.RemoteComputerClose)) {
                Text("返回")
            }
            Column(modifier = Modifier.weight(1f)) {
                Text("我的电脑", style = MaterialTheme.typography.titleMedium)
                Text(
                    status,
                    style = MaterialTheme.typography.labelSmall,
                    modifier = Modifier.testTag(TestTags.RemoteComputerStatus),
                )
            }
            if (nativeBusy) {
                CircularProgressIndicator(
                    modifier = Modifier.size(20.dp).testTag(TestTags.RemoteComputerLoading),
                    strokeWidth = 2.dp,
                )
            }
        }

        RemoteComputerNativePanel(
            state = nativeState,
            busy = nativeBusy,
            error = nativeError,
            pairingCode = pairingCode,
            pairingLabel = pairingLabel,
            onPairingCodeChange = { pairingCode = it.take(24) },
            onPairingLabelChange = { pairingLabel = it.take(80) },
            onRefresh = ::refreshNativeState,
            onPair = pair@{
                val code = RemoteComputerPresentationPolicy.normalizePairingCode(pairingCode)
                    ?: return@pair
                val label = RemoteComputerPresentationPolicy.normalizePairingLabel(pairingLabel)
                    ?: return@pair
                if (nativeBusy) return@pair
                scope.launch {
                    nativeBusy = true
                    runCatching {
                        withContext(Dispatchers.IO) {
                            coordinator.remoteComputerPair(code, label)
                            RemoteComputerPresentationPolicy.parse(
                                coordinator.remoteComputerList(),
                                coordinator.remoteComputerPairingStatus(),
                                coordinator.remoteComputerSessionStatus(),
                            )
                        }
                    }.onSuccess {
                        nativeState = it
                        pairingCode = ""
                        nativeError = null
                    }.onFailure {
                        nativeError = "配对失败。请确认电脑端显示的配对码仍有效。"
                    }
                    nativeBusy = false
                }
            },
            rebuildMessage = rebuildMessage,
            rebuildProgress = rebuildProgress,
            onRebuild = rebuild@{ force ->
                if (nativeBusy) return@rebuild
                scope.launch {
                    nativeBusy = true
                    runCatching {
                        withContext(Dispatchers.IO) {
                            coordinator.computerRebuildRequest(
                                preserveData = !force,
                                forceRecreate = force,
                            )
                        }
                    }.onSuccess { response ->
                        rebuildProgress = runCatching {
                            withContext(Dispatchers.IO) {
                                ComputerRebuildProgressPolicy.project(coordinator.computerRebuildStatus())
                            }
                        }.getOrNull()
                        rebuildMessage = if (response.optBoolean("accepted", false)) {
                            if (force) "强制重建已接受；正在等待迁移完成。" else "电脑环境更新已接受；正在等待迁移完成。"
                        } else {
                            response.optString("reason").ifBlank { "后端未启动重建。" }
                        }
                        nativeError = null
                    }.onFailure {
                        rebuildMessage = null
                        nativeError = "无法启动电脑重建；状态已按结果未知处理，不会自动重复请求。"
                    }
                    nativeBusy = false
                }
            },
            onRevoke = revoke@{ pairing ->
                if (nativeBusy) return@revoke
                scope.launch {
                    nativeBusy = true
                    runCatching {
                        withContext(Dispatchers.IO) {
                            coordinator.remoteComputerRevoke(pairing.deviceId, pairing.clientId)
                            RemoteComputerPresentationPolicy.parse(
                                coordinator.remoteComputerList(),
                                coordinator.remoteComputerPairingStatus(),
                                coordinator.remoteComputerSessionStatus(),
                            )
                        }
                    }.onSuccess {
                        nativeState = it
                        status = "原生远端视图已开始协商"
                        nativeError = null
                    }.onFailure {
                        // Revoke is locally fail-closed before the remote mutation. Keep the UI
                        // conservative and reload native truth rather than assuming server success.
                        nativeError = "本机授权已关闭；服务器撤销状态将在下次刷新时重新核对。"
                        runCatching { loadNativeState() }.onSuccess { nativeState = it }
                    }
                    nativeBusy = false
                }
            },
            onSessionStart = sessionStart@{ pairing ->
                if (nativeBusy) return@sessionStart
                scope.launch {
                    nativeBusy = true
                    runCatching {
                        withContext(Dispatchers.IO) {
                            coordinator.remoteComputerSessionCreate(pairing.deviceId)
                            loadNativeState()
                        }
                    }.onSuccess {
                        nativeState = it
                        nativeError = null
                    }.onFailure {
                        nativeError = "无法建立控制会话；不会自动重发可能已创建的请求，请刷新核对。"
                        runCatching { loadNativeState() }.onSuccess { nativeState = it }
                    }
                    nativeBusy = false
                }
            },
            onSessionConnect = sessionConnect@{ session ->
                if (nativeBusy) return@sessionConnect
                scope.launch {
                    nativeBusy = true
                    runCatching {
                        withContext(Dispatchers.IO) {
                            coordinator.remoteComputerDataPlaneConnect(
                                session.deviceId,
                                session.sessionId,
                            )
                            loadNativeState()
                        }
                    }.onSuccess {
                        nativeState = it
                        status = "原生远端视图正在协商"
                        nativeError = null
                    }.onFailure {
                        status = "原生远端视图连接失败"
                        nativeError = "原生远端显示/输入通道未能建立；保持 fail-closed，不回退到网页控制。"
                        runCatching { loadNativeState() }.onSuccess { nativeState = it }
                    }
                    nativeBusy = false
                }
            },
            onSessionReconcile = sessionReconcile@{ session ->
                if (nativeBusy) return@sessionReconcile
                scope.launch {
                    nativeBusy = true
                    runCatching {
                        withContext(Dispatchers.IO) {
                            val drained = coordinator.remoteComputerSignalDrain(
                                session.deviceId,
                                session.sessionId,
                                session.lastAcknowledgedSignalId,
                            )
                            val lastSignalId = drained.getLong("lastSignalId")
                            if (lastSignalId > session.lastAcknowledgedSignalId) {
                                coordinator.remoteComputerSignalAcknowledge(
                                    session.deviceId,
                                    session.sessionId,
                                    lastSignalId,
                                )
                            }
                            loadNativeState()
                        }
                    }.onSuccess {
                        nativeState = it
                        nativeError = null
                    }.onFailure {
                        nativeError = "会话核对失败；不会重放输入或远端副作用。"
                        runCatching { loadNativeState() }.onSuccess { nativeState = it }
                    }
                    nativeBusy = false
                }
            },
            onHumanTakeover = takeover@{ session, active ->
                if (nativeBusy) return@takeover
                scope.launch {
                    nativeBusy = true
                    runCatching {
                        withContext(Dispatchers.IO) {
                            coordinator.remoteComputerHumanTakeover(
                                session.deviceId,
                                session.sessionId,
                                session.viewportRevision,
                                active,
                            )
                            loadNativeState()
                        }
                    }.onSuccess {
                        nativeState = it
                        nativeError = null
                    }.onFailure {
                        nativeError = "控制租约已变化，请刷新后再切换人工接管。"
                        runCatching { loadNativeState() }.onSuccess { nativeState = it }
                    }
                    nativeBusy = false
                }
            },
            onSessionClose = sessionClose@{ session ->
                if (nativeBusy) return@sessionClose
                scope.launch {
                    nativeBusy = true
                    runCatching {
                        withContext(Dispatchers.IO) {
                            coordinator.remoteComputerSessionClose(session.deviceId, session.sessionId)
                            loadNativeState()
                        }
                    }.onSuccess {
                        nativeState = it
                        nativeError = null
                    }.onFailure {
                        nativeError = "结束会话的服务端结果未知；本机会保持 outcome-unknown 并要求核对，不会自动重复关闭。"
                        runCatching { loadNativeState() }.onSuccess { nativeState = it }
                    }
                    nativeBusy = false
                }
            },
        )

        AndroidView(
            factory = { coordinator.remoteComputerViewportView(it) },
            modifier = Modifier.weight(1f).fillMaxWidth().testTag(TestTags.RemoteComputerWebView),
        )
    }

    DisposableEffect(coordinator) {
        onDispose {
            runCatching { coordinator.remoteComputerDataPlaneDisconnect() }
        }
    }

}

@Composable
private fun RemoteComputerNativePanel(
    state: RemoteComputerNativeState,
    busy: Boolean,
    error: String?,
    pairingCode: String,
    pairingLabel: String,
    rebuildMessage: String?,
    rebuildProgress: ComputerRebuildProgressProjection?,
    onPairingCodeChange: (String) -> Unit,
    onPairingLabelChange: (String) -> Unit,
    onRefresh: () -> Unit,
    onPair: () -> Unit,
    onRebuild: (force: Boolean) -> Unit,
    onRevoke: (RemoteComputerPairing) -> Unit,
    onSessionStart: (RemoteComputerPairing) -> Unit,
    onSessionConnect: (RemoteComputerSessionProjection) -> Unit,
    onSessionReconcile: (RemoteComputerSessionProjection) -> Unit,
    onHumanTakeover: (RemoteComputerSessionProjection, Boolean) -> Unit,
    onSessionClose: (RemoteComputerSessionProjection) -> Unit,
) {
    Card(
        modifier = Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 4.dp),
        shape = RoundedCornerShape(12.dp),
    ) {
        Column(
            modifier = Modifier.fillMaxWidth().padding(14.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                Column(modifier = Modifier.weight(1f)) {
                    Text("原生配对", style = MaterialTheme.typography.titleSmall)
                    Text(
                        "配对令牌由 Android 安全存储持有，不会传给远程网页。",
                        style = MaterialTheme.typography.bodySmall,
                    )
                }
                OutlinedButton(onClick = onRefresh, enabled = !busy) {
                    Text("刷新")
                }
            }

            error?.let {
                Text(it, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall)
            }
            rebuildMessage?.let {
                Text(it, style = MaterialTheme.typography.bodySmall)
            }

            rebuildProgress?.let { projection ->
                Card(
                    modifier = Modifier.fillMaxWidth(),
                    colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceVariant),
                ) {
                    Column(
                        modifier = Modifier.fillMaxWidth().padding(12.dp),
                        verticalArrangement = Arrangement.spacedBy(6.dp),
                    ) {
                        Text(projection.title, style = MaterialTheme.typography.labelMedium)
                        if (projection.reconnectVariant == null) {
                            LinearProgressIndicator(
                                progress = { projection.progress.toFloat() },
                                modifier = Modifier.fillMaxWidth(),
                            )
                            projection.steps.forEachIndexed { index, label ->
                                val prefix = when {
                                    index < projection.activeIndex -> "✓"
                                    index == projection.activeIndex -> "•"
                                    else -> "○"
                                }
                                Text("$prefix $label", style = MaterialTheme.typography.bodySmall)
                            }
                        } else {
                            Text(
                                when (projection.reconnectVariant) {
                                    "network" -> "网络连接中断，正在安全重连。"
                                    "restarting" -> "远端电脑正在启动，输入不会自动重放。"
                                    else -> "正在核对远端电脑连接状态。"
                                },
                                style = MaterialTheme.typography.bodySmall,
                            )
                        }
                    }
                }
            }

            Card(
                modifier = Modifier.fillMaxWidth(),
                shape = RoundedCornerShape(10.dp),
            ) {
                Column(
                    modifier = Modifier.fillMaxWidth().padding(12.dp),
                    verticalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    Text("Fabushi 云电脑维护", style = MaterialTheme.typography.labelMedium)
                    Text(
                        "更新会保留数据；强制重建会清理当前云电脑环境。请求一旦结果未知不会自动重发。",
                        style = MaterialTheme.typography.bodySmall,
                    )
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        OutlinedButton(onClick = { onRebuild(false) }, enabled = !busy) {
                            Text("更新电脑环境")
                        }
                        OutlinedButton(onClick = { onRebuild(true) }, enabled = !busy) {
                            Text("强制重建")
                        }
                    }
                }
            }

            val pairing = state.pairing
            if (pairing == null) {
                OutlinedTextField(
                    value = pairingCode,
                    onValueChange = onPairingCodeChange,
                    label = { Text("12 位配对码") },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth(),
                )
                OutlinedTextField(
                    value = pairingLabel,
                    onValueChange = onPairingLabelChange,
                    label = { Text("这台 Android 的名称") },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth(),
                )
                Button(
                    onClick = onPair,
                    enabled = !busy &&
                        RemoteComputerPresentationPolicy.normalizePairingCode(pairingCode) != null &&
                        RemoteComputerPresentationPolicy.normalizePairingLabel(pairingLabel) != null,
                ) {
                    Text("配对电脑")
                }
            } else {
                val pairedDevice = state.computers.firstOrNull { it.deviceId == pairing.deviceId }
                Text(
                    "已配对：" + (pairedDevice?.label ?: pairing.deviceId),
                    style = MaterialTheme.typography.bodyMedium,
                )
                Text(
                    "客户端 " + pairing.clientId + " · account epoch " + pairing.accountEpoch,
                    style = MaterialTheme.typography.bodySmall,
                )
                OutlinedButton(onClick = { onRevoke(pairing) }, enabled = !busy) {
                    Text("撤销本机授权")
                }

                val session = state.session
                if (!session.stored) {
                    Button(onClick = { onSessionStart(pairing) }, enabled = !busy) {
                        Text("建立控制会话")
                    }
                } else {
                    Card(modifier = Modifier.fillMaxWidth(), shape = RoundedCornerShape(10.dp)) {
                        Column(
                            modifier = Modifier.fillMaxWidth().padding(12.dp),
                            verticalArrangement = Arrangement.spacedBy(8.dp),
                        ) {
                            Text("原生控制会话", style = MaterialTheme.typography.labelMedium)
                            Text(
                                "状态 " + session.lifecycle +
                                    " · viewport r" + session.viewportRevision +
                                    (if (session.selectedRoute.isBlank()) "" else " · " + session.selectedRoute),
                                style = MaterialTheme.typography.bodySmall,
                            )
                            Text(
                                "信令游标 " + session.lastAcknowledgedSignalId +
                                    "/" + session.highestDrainedSignalId +
                                    (if (session.reconcileRequired) " · 需要核对" else ""),
                                style = MaterialTheme.typography.bodySmall,
                            )
                            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                OutlinedButton(
                                    onClick = { onSessionConnect(session) },
                                    enabled = !busy && session.lifecycle !in setOf("closing", "outcome_unknown"),
                                ) {
                                    Text("连接控制会话")
                                }
                                OutlinedButton(
                                    onClick = { onSessionReconcile(session) },
                                    enabled = !busy,
                                ) {
                                    Text("核对信令")
                                }
                            }
                            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                OutlinedButton(
                                    onClick = { onHumanTakeover(session, !session.humanTakeover) },
                                    enabled = !busy && session.lifecycle !in setOf("closing", "outcome_unknown"),
                                ) {
                                    Text(if (session.humanTakeover) "结束人工接管" else "人工接管")
                                }
                                OutlinedButton(
                                    onClick = { onSessionClose(session) },
                                    enabled = !busy && session.lifecycle != "closing",
                                ) {
                                    Text("结束会话")
                                }
                            }
                            Text(
                                "控制凭据与 ICE/TURN 只保存在 Android Coordinator/Keystore；原生 viewport/input 由唯一 data-plane owner 持有，Presentation 不接触凭据。",
                                style = MaterialTheme.typography.bodySmall,
                            )
                        }
                    }
                }
            }

            if (state.computers.isEmpty()) {
                Text("当前账户没有可用电脑。", style = MaterialTheme.typography.bodySmall)
            } else {
                Text("账户中的电脑", style = MaterialTheme.typography.labelMedium)
                state.computers.take(4).forEach { computer ->
                    val online = if (computer.online) "在线" else "离线"
                    val sessions = if (computer.activeSessionCount > 0) {
                        " · " + computer.activeSessionCount + " 个活动会话"
                    } else {
                        ""
                    }
                    Text(
                        computer.label + " · " +
                            computer.platform.ifBlank { "unknown" } + " · " + online + sessions,
                        style = MaterialTheme.typography.bodySmall,
                    )
                }
                if (state.computers.size > 4) {
                    Text(
                        "另有 " + (state.computers.size - 4) + " 台电脑",
                        style = MaterialTheme.typography.bodySmall,
                    )
                }
            }
        }
    }
}
