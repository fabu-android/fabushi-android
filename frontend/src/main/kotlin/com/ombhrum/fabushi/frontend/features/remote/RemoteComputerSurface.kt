package com.ombhrum.fabushi

import android.annotation.SuppressLint
import android.graphics.Bitmap
import android.net.Uri
import android.net.http.SslError
import android.webkit.CookieManager
import android.webkit.SslErrorHandler
import android.webkit.WebResourceError
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebSettings
import android.webkit.WebView
import android.webkit.WebViewClient
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

private const val REMOTE_COMPUTER_ORIGIN = "fabushi.ombhrum.com"
private const val REMOTE_COMPUTER_URL = "https://fabushi.ombhrum.com/remote-computer"

/**
 * Restricted browser surface for human-operated remote computer sessions.
 *
 * The native panel owns account-scoped list/pair/revoke through the typed Coordinator contract.
 * The hosted viewport deliberately receives no native bridge and no pairing/session/executor secret.
 */
@SuppressLint("SetJavaScriptEnabled")
@Composable
fun RemoteComputerSurface(onClose: () -> Unit) {
    val context = LocalContext.current
    val coordinator = remember { CoordinatorClient.presentation() }
    val scope = rememberCoroutineScope()

    var status by remember { mutableStateOf("正在连接我的电脑…") }
    var loading by remember { mutableStateOf(true) }
    var errorMessage by remember { mutableStateOf<String?>(null) }
    var reloadToken by remember { mutableStateOf(0) }

    var nativeState by remember { mutableStateOf(RemoteComputerNativeState.Empty) }
    var nativeBusy by remember { mutableStateOf(false) }
    var nativeError by remember { mutableStateOf<String?>(null) }
    var pairingCode by remember { mutableStateOf("") }
    var pairingLabel by remember { mutableStateOf("Fabushi Android") }

    suspend fun loadNativeState(): RemoteComputerNativeState = withContext(Dispatchers.IO) {
        RemoteComputerPresentationPolicy.parse(
            coordinator.remoteComputerList(),
            coordinator.remoteComputerPairingStatus(),
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

    fun isAllowedUrl(uri: Uri): Boolean =
        uri.scheme.equals("https", ignoreCase = true) &&
            uri.host.equals(REMOTE_COMPUTER_ORIGIN, ignoreCase = true) &&
            uri.userInfo == null &&
            (uri.port == -1 || uri.port == 443)

    val webView = remember {
        WebView(context).apply {
            settings.javaScriptEnabled = true
            settings.domStorageEnabled = true
            settings.databaseEnabled = false
            settings.allowFileAccess = false
            settings.allowContentAccess = false
            settings.javaScriptCanOpenWindowsAutomatically = false
            settings.setSupportMultipleWindows(false)
            settings.mixedContentMode = WebSettings.MIXED_CONTENT_NEVER_ALLOW
            settings.safeBrowsingEnabled = true
            settings.mediaPlaybackRequiresUserGesture = false
            settings.setGeolocationEnabled(false)
            settings.builtInZoomControls = true
            settings.displayZoomControls = false

            CookieManager.getInstance().setAcceptCookie(true)
            CookieManager.getInstance().setAcceptThirdPartyCookies(this, false)

            // This restricted viewport deliberately has no JavaScriptInterface, WebMessage bridge,
            // or injected Coordinator object. Pairing and control credentials remain native-only.
            webViewClient = object : WebViewClient() {
                override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean {
                    val uri = request.url
                    if (isAllowedUrl(uri)) return false
                    if (request.isForMainFrame) {
                        loading = false
                        status = "已阻止外部导航"
                        errorMessage = "远程电脑页面只允许访问 https://fabushi.ombhrum.com。"
                    }
                    return true
                }

                override fun onPageStarted(view: WebView, url: String?, favicon: Bitmap?) {
                    loading = true
                    errorMessage = null
                    status = "正在安全连接…"
                }

                override fun onPageFinished(view: WebView, url: String?) {
                    loading = false
                    if (errorMessage == null) status = "已安全连接"
                }

                override fun onReceivedSslError(view: WebView, handler: SslErrorHandler, error: SslError) {
                    handler.cancel()
                    loading = false
                    status = "安全连接失败"
                    errorMessage = "无法验证远程电脑服务的安全证书。"
                }

                override fun onReceivedError(
                    view: WebView,
                    request: WebResourceRequest,
                    error: WebResourceError,
                ) {
                    if (!request.isForMainFrame) return
                    loading = false
                    status = "连接失败"
                    errorMessage = error.description.toString().ifBlank { "无法加载远程电脑页面。" }
                }

                override fun onReceivedHttpError(
                    view: WebView,
                    request: WebResourceRequest,
                    errorResponse: WebResourceResponse,
                ) {
                    if (!request.isForMainFrame || errorResponse.statusCode < 400) return
                    loading = false
                    status = "连接失败"
                    errorMessage = "远程电脑服务返回 " + errorResponse.statusCode + "。"
                }
            }
        }
    }

    BackHandler {
        if (webView.canGoBack()) webView.goBack() else onClose()
    }

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
            if (loading || nativeBusy) {
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
                            )
                        }
                    }.onSuccess {
                        nativeState = it
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
        )

        errorMessage?.let { message ->
            Card(
                modifier = Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 4.dp)
                    .testTag(TestTags.RemoteComputerError),
                colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.errorContainer),
                shape = RoundedCornerShape(12.dp),
            ) {
                Column(
                    modifier = Modifier.padding(14.dp),
                    verticalArrangement = Arrangement.spacedBy(10.dp),
                ) {
                    Text("无法打开远程电脑", style = MaterialTheme.typography.titleSmall)
                    Text(message, style = MaterialTheme.typography.bodySmall)
                    Button(
                        onClick = {
                            errorMessage = null
                            loading = true
                            status = "正在重新连接…"
                            reloadToken += 1
                        },
                        modifier = Modifier.testTag(TestTags.RemoteComputerReload),
                    ) {
                        Text("重新加载")
                    }
                }
            }
        }

        AndroidView(
            factory = { webView },
            update = { view ->
                if (view.tag == reloadToken) return@AndroidView
                view.tag = reloadToken
                view.loadUrl(REMOTE_COMPUTER_URL)
            },
            modifier = Modifier.weight(1f).fillMaxWidth().testTag(TestTags.RemoteComputerWebView),
        )
    }

    DisposableEffect(webView) {
        onDispose {
            webView.stopLoading()
            webView.removeAllViews()
            webView.destroy()
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
    onPairingCodeChange: (String) -> Unit,
    onPairingLabelChange: (String) -> Unit,
    onRefresh: () -> Unit,
    onPair: () -> Unit,
    onRevoke: (RemoteComputerPairing) -> Unit,
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
