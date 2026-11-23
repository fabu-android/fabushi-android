package com.ombhrum.fabushi

import android.annotation.SuppressLint
import android.graphics.Bitmap
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
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.viewinterop.AndroidView
import com.ombhrum.fabushi.telegram.LocalTelegramPalette
import java.net.URI
import java.util.Locale

private val ALLOWED_REMOTE_COMPUTER_HOSTS = setOf(
    "fabushi.ombhrum.com",
    "remote.fabushi.ombhrum.com",
)
private const val REMOTE_COMPUTER_URL = "https://fabushi.ombhrum.com/remote-computer"

/**
 * Pure-JVM URL allowlist validator for [RemoteComputerSurface].
 * Permits only HTTPS URLs on exact allowlisted hosts without userinfo or non-443 ports.
 */
internal fun isAllowedRemoteComputerUrl(url: String): Boolean {
    val trimmed = url.trim()
    if (trimmed.isEmpty()) return false
    val uri = runCatching { URI(trimmed) }.getOrNull() ?: return false
    if (!uri.scheme.equals("https", ignoreCase = true)) return false
    if (uri.userInfo != null || uri.rawUserInfo != null) return false
    val host = uri.host?.lowercase(Locale.ROOT) ?: return false
    if (host !in ALLOWED_REMOTE_COMPUTER_HOSTS) return false
    return uri.port == -1 || uri.port == 443
}

/**
 * Restricted browser surface for human-operated remote computer sessions.
 *
 * This intentionally does not register a native bridge or inject JavaScript. The hosted
 * application retains regular browser JavaScript/WebRTC support without receiving arbitrary
 * access to the native Fabushi runtime.
 */
@SuppressLint("SetJavaScriptEnabled")
@Composable
fun RemoteComputerSurface(
    onClose: () -> Unit,
    embedded: Boolean = false,
) {
    val context = LocalContext.current
    val palette = LocalTelegramPalette.current
    var status by remember { mutableStateOf("正在连接我的电脑…") }
    var loading by remember { mutableStateOf(true) }
    var errorMessage by remember { mutableStateOf<String?>(null) }
    var reloadToken by remember { mutableStateOf(0) }

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

            // This restricted surface deliberately has no native bridge.
            webViewClient = object : WebViewClient() {
                override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean {
                    val target = request.url?.toString().orEmpty()
                    if (isAllowedRemoteComputerUrl(target)) return false
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
                    errorMessage = "远程电脑服务返回 ${errorResponse.statusCode}。"
                }
            }
        }
    }

    if (!embedded) {
        BackHandler {
            if (webView.canGoBack()) webView.goBack() else onClose()
        }
    }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(palette.appBackground)
            .let { base -> if (embedded) base else base.windowInsetsPadding(WindowInsets.safeDrawing) }
            .testTag(TestTags.RemoteComputerSurface),
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .background(palette.topBarBackground)
                .padding(horizontal = 12.dp, vertical = 10.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Surface(
                shape = RoundedCornerShape(16.dp),
                color = palette.inputBg,
                modifier = Modifier
                    .clip(RoundedCornerShape(16.dp))
                    .clickable(onClick = onClose)
                    .testTag(TestTags.RemoteComputerClose),
            ) {
                Text(
                    text = if (embedded) "收起" else "返回",
                    color = palette.topBarActionColor,
                    fontSize = 13.sp,
                    fontWeight = FontWeight.SemiBold,
                    modifier = Modifier.padding(horizontal = 12.dp, vertical = 7.dp),
                )
            }
            Column(modifier = Modifier.weight(1f)) {
                Text(
                    "实时远程桌面",
                    color = palette.topBarContent,
                    fontSize = 15.sp,
                    fontWeight = FontWeight.SemiBold,
                )
                Text(
                    status,
                    color = palette.secondaryText,
                    fontSize = 11.sp,
                    modifier = Modifier.testTag(TestTags.RemoteComputerStatus),
                )
            }
            if (loading) {
                CircularProgressIndicator(
                    modifier = Modifier
                        .size(20.dp)
                        .testTag(TestTags.RemoteComputerLoading),
                    strokeWidth = 2.dp,
                    color = palette.primaryAccent,
                )
            }
        }

        errorMessage?.let { message ->
            Card(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 12.dp, vertical = 6.dp)
                    .testTag(TestTags.RemoteComputerError),
                colors = CardDefaults.cardColors(containerColor = palette.cardBg),
                shape = RoundedCornerShape(12.dp),
            ) {
                Column(
                    modifier = Modifier.padding(14.dp),
                    verticalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    Text(
                        "无法打开远程电脑",
                        style = MaterialTheme.typography.titleSmall,
                        color = palette.destructiveRed,
                        fontWeight = FontWeight.SemiBold,
                    )
                    Text(message, style = MaterialTheme.typography.bodySmall, color = palette.primaryText)
                    Button(
                        onClick = {
                            errorMessage = null
                            loading = true
                            status = "正在重新连接…"
                            reloadToken += 1
                        },
                        colors = ButtonDefaults.buttonColors(
                            containerColor = palette.primaryAccent,
                            contentColor = Color.White,
                        ),
                        shape = RoundedCornerShape(10.dp),
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
                if (isAllowedRemoteComputerUrl(REMOTE_COMPUTER_URL)) {
                    view.loadUrl(REMOTE_COMPUTER_URL)
                }
            },
            modifier = Modifier
                .fillMaxSize()
                .testTag(TestTags.RemoteComputerWebView),
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
