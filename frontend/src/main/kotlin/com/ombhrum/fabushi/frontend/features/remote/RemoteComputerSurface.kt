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
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

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
    var pairingCode by remember { mutableStateOf("") }
    var pairingLabel by remember { mutableStateOf("Fabushi Android") }

    suspend fun loadNativeState(): RemoteComputerNativeState = withContext(Dispatchers.IO) {
        RemoteComputerPresentationPolicy.parse(
            coordinator.remoteComputerList(),
            coordinator.remoteComputerPairingStatus(),
            coordinator.remoteComputerSessionStatus(),
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
        nativeBusy = false
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
