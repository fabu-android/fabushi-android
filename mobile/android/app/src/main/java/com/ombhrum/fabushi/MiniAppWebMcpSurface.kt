package com.ombhrum.fabushi

import android.annotation.SuppressLint
import android.app.AlertDialog
import android.graphics.Bitmap
import android.os.Handler
import android.os.Looper
import android.webkit.JavascriptInterface
import android.webkit.WebResourceRequest
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import com.ombhrum.fabushi.core.MahayanaHost
import org.json.JSONArray
import org.json.JSONObject
import java.net.URLEncoder
import java.nio.charset.StandardCharsets
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean

private const val WEB_MCP_ORIGIN = "fabushi.ombhrum.com"
private const val LOCAL_WEB_MCP_ORIGIN = "miniapp.local.fabushi.invalid"

internal fun isAllowedMiniAppUrl(url: String): Boolean {
    val uri = runCatching { java.net.URI(url.trim()) }.getOrNull() ?: return false
    if (!uri.scheme.equals("https", ignoreCase = true)) return false
    if (uri.userInfo != null) return false
    val host = uri.host?.lowercase(java.util.Locale.ROOT) ?: return false
    if (host != WEB_MCP_ORIGIN && host != LOCAL_WEB_MCP_ORIGIN) return false
    return uri.port == -1 || uri.port == 443
}

internal fun isLocalMiniAppUrl(url: String?): Boolean {
    if (url.isNullOrBlank() || !isAllowedMiniAppUrl(url)) return false
    val host = runCatching { java.net.URI(url.trim()).host?.lowercase(java.util.Locale.ROOT) }.getOrNull()
    return host == LOCAL_WEB_MCP_ORIGIN
}

internal class MiniAppLifecycleGate {
    private var _localPageLoaded = false
    private var _lifecycleActive = true
    private var _disposed = false

    val localPageLoaded: Boolean
        get() = synchronized(this) { _localPageLoaded }
    val lifecycleActive: Boolean
        get() = synchronized(this) { _lifecycleActive }
    val localDocumentActive: Boolean
        get() = synchronized(this) { !_disposed && _lifecycleActive && _localPageLoaded }
    val disposed: Boolean
        get() = synchronized(this) { _disposed }

    fun onLocalHtmlLoaded() {
        synchronized(this) {
            if (_disposed) return
            _localPageLoaded = true
        }
    }

    fun onHostedUrlLoaded() {
        synchronized(this) {
            _localPageLoaded = false
        }
    }

    fun onPageStarted(url: String?) {
        synchronized(this) {
            _localPageLoaded = !_disposed && isLocalMiniAppUrl(url)
        }
    }

    fun onLifecycleEvent(event: Lifecycle.Event) {
        synchronized(this) {
            when (event) {
                Lifecycle.Event.ON_START, Lifecycle.Event.ON_RESUME -> {
                    if (_disposed) return
                    _lifecycleActive = true
                }
                Lifecycle.Event.ON_PAUSE, Lifecycle.Event.ON_STOP -> {
                    // A page that is no longer foregrounded loses its native
                    // execution grant. Long work must be resumed through the
                    // Host task owner after the page returns.
                    _lifecycleActive = false
                }
                Lifecycle.Event.ON_DESTROY -> {
                    _disposed = true
                    _lifecycleActive = false
                    _localPageLoaded = false
                }
                else -> Unit
            }
        }
    }

    fun onDispose() {
        synchronized(this) {
            _disposed = true
            _lifecycleActive = false
            _localPageLoaded = false
        }
    }

    fun canDispatchHostEvent(): Boolean = synchronized(this) {
        !_disposed && _lifecycleActive && _localPageLoaded
    }

    fun dispatchHostEvent(event: JSONObject, deliver: (JSONObject) -> Unit): Boolean {
        if (!canDispatchHostEvent()) return false
        val projected = miniAppEventProjection(event)
        if (projected.length() == 0) return false
        if (!canDispatchHostEvent()) return false
        deliver(projected)
        return true
    }
}

private fun requestNativeApproval(context: android.content.Context, tool: MiniAppToolContract): Boolean {
    val decided = AtomicBoolean(false)
    val allowed = AtomicBoolean(false)
    val latch = CountDownLatch(1)
    val posted = runCatching {
        Handler(Looper.getMainLooper()).post {
            try {
                val warning = if (tool.approval == "destructive") {
                    "该操作可能产生破坏性修改。"
                } else {
                    "该操作会修改小程序或后台状态。"
                }
                AlertDialog.Builder(context)
                    .setTitle("允许 WebMCP 调用 ${tool.name}？")
                    .setMessage("${tool.description}\n\n$warning")
                    .setPositiveButton("允许") { _, _ ->
                        if (decided.compareAndSet(false, true)) {
                            allowed.set(true)
                            latch.countDown()
                        }
                    }
                    .setNegativeButton("取消") { _, _ ->
                        if (decided.compareAndSet(false, true)) latch.countDown()
                    }
                    .setOnCancelListener {
                        if (decided.compareAndSet(false, true)) latch.countDown()
                    }
                    .setOnDismissListener {
                        if (decided.compareAndSet(false, true)) latch.countDown()
                    }
                    .show()
            } catch (_: Throwable) {
                if (decided.compareAndSet(false, true)) latch.countDown()
            }
        }
    }.getOrDefault(false)
    if (!posted) {
        if (decided.compareAndSet(false, true)) latch.countDown()
        return false
    }
    latch.await(2, TimeUnit.MINUTES)
    return allowed.get()
}

internal class MiniAppNativeWebMcpBridge(
    private val plugin: MarketplacePlugin,
    private val context: android.content.Context,
    private val localDocumentActive: () -> Boolean,
    private val callRuntimeToolJson: (pluginId: String, name: String, argumentsJson: String) -> String,
    private val approvalPrompt: (MiniAppToolContract) -> Boolean = { tool -> requestNativeApproval(context, tool) },
) {
    private val tools = plugin.tools.associateBy { it.name }

    @JavascriptInterface
    fun callTool(name: String, argumentsJson: String): String {
        if (!localDocumentActive()) {
            return JSONObject()
                .put("ok", false)
                .put("error", "Native WebMCP bridge is available only to the local MiniApp document")
                .toString()
        }
        // Discovery is answered by the host itself: it reveals only what this MiniApp
        // was already granted, and works before any tool is declared.
        if (name == CAPABILITY_PROBE_TOOL) {
            return JSONObject()
                .put("ok", true)
                .put("result", miniAppCapabilityAnswer(plugin.tools))
                .toString()
        }
        val tool = tools[name]
            ?: return JSONObject().put("ok", false).put("error", "Tool is not in this MiniApp contract").toString()
        if (tool.approval != "none") {
            val approved = runCatching { approvalPrompt(tool) }.getOrDefault(false)
            if (!approved) {
                return JSONObject().put("ok", false).put("error", "用户取消了 WebMCP Tool 调用").toString()
            }
            if (!localDocumentActive()) {
                return JSONObject().put("ok", false).put("error", "Local MiniApp document is not active").toString()
            }
        }
        return runCatching {
            val resultJson = callRuntimeToolJson(plugin.pluginId, name, argumentsJson)
            "{\"ok\":true,\"result\":$resultJson}"
        }.getOrElse { error ->
            JSONObject()
                .put("ok", false)
                .put("error", error.message ?: "WebMCP runtime call failed")
                .toString()
        }
    }
}

private const val CAPABILITY_PROBE_TOOL = "offline.capabilities"
private val CAPABILITY_PREFIXES = setOf(
    "offline", "selection", "transfer", "scan", "state", "config", "platform",
    "hotspot", "update", "diagnostics", "compat",
)

/** What this host granted to the MiniApp, derived from its declared tool contract. */
internal fun miniAppCapabilityDomains(tools: Collection<MiniAppToolContract>): List<String> = tools
    .map { it.name.substringBefore('.').lowercase() }
    .filter { it in CAPABILITY_PREFIXES }
    .distinct()
    .sorted()

internal fun miniAppCapabilityAnswer(tools: Collection<MiniAppToolContract>): JSONObject {
    val capabilities = miniAppCapabilityDomains(tools)
    return JSONObject()
        .put("contract", "fabushi.miniapp.tools.v1")
        .put("platform", "android")
        .put("capabilities", JSONArray(capabilities))
        .put("targetPools", JSONArray())
}

internal fun toolContractJson(plugin: MarketplacePlugin): String {
    val rows = JSONArray()
    // The probe is part of the injected contract so the page can discover what it was
    // granted before calling anything; callTool answers it without a native runtime hop.
    rows.put(
        JSONObject()
            .put("name", CAPABILITY_PROBE_TOOL)
            .put("title", CAPABILITY_PROBE_TOOL)
            .put("description", "Report which capabilities this host granted to the MiniApp.")
            .put("readOnlyHint", true)
            .put("annotations", JSONObject().put("readOnlyHint", true))
            .put("inputSchema", JSONObject().put("type", "object").put("properties", JSONObject())),
    )
    for (tool in plugin.tools) {
        val readOnly = tool.approval == "none"
        val row = JSONObject()
            .put("name", tool.name)
            .put("title", tool.title ?: tool.name)
            .put("description", tool.description)
            .put("readOnlyHint", readOnly)
            .put("annotations", JSONObject().put("readOnlyHint", readOnly))
        tool.inputSchemaJson?.let { schema ->
            runCatching { row.put("inputSchema", JSONObject(schema)) }
        }
        rows.put(row)
    }
    return rows.toString()
}

/** UI-safe Fabushi account projection. Account/session credentials never enter the WebView. */
internal fun miniAppAccountProjection(auth: JSONObject): JSONObject {
    val projection = JSONObject().put("loggedIn", auth.optBoolean("loggedIn", false))
    val source = auth.optJSONObject("user") ?: JSONObject()
    val user = JSONObject()
    for (key in listOf("id", "userId", "username", "nickname", "email", "avatar", "role", "membership")) {
        if (!source.has(key) || source.isNull(key)) continue
        when (val value = source.opt(key)) {
            is String, is Number, is Boolean -> user.put(key, value)
        }
    }
    projection.put("user", user)
    return projection
}

/** FeatureHost events are projected as scalar UI state only; nested credentials/results are dropped. */
internal fun miniAppEventProjection(event: JSONObject): JSONObject {
    val output = JSONObject()
    val allowed = listOf(
        "type", "timestamp", "requestId", "operationId", "miniAppId", "pluginId", "agentId",
        "stepId", "title", "detail", "status", "role", "text", "delta", "label", "tool",
        "progress", "current", "total", "message",
    )
    for (key in allowed) {
        if (!event.has(key) || event.isNull(key)) continue
        when (val value = event.opt(key)) {
            is String, is Number, is Boolean -> output.put(key, value)
        }
    }
    return output
}

private fun escapeJsonForHtmlScript(json: String): String =
    json
        .replace("<", "\\u003c")
        .replace("\u2028", "\\u2028")
        .replace("\u2029", "\\u2029")

internal fun injectLocalWebMcp(html: String, plugin: MarketplacePlugin, account: JSONObject): String {
    val tools = escapeJsonForHtmlScript(toolContractJson(plugin))
    val safeAccount = escapeJsonForHtmlScript(account.toString())
    val safePluginId = escapeJsonForHtmlScript(JSONObject.quote(plugin.pluginId))
    val bootstrap = """
        <script>
        (function(){
          const definitions=$tools;
          const account=$safeAccount;
          const localTools=new Map();
          const controllers=[];
          function publicTool(tool){const copy={...tool};delete copy.execute;return copy;}
          function nativeCall(name,input){
            const raw=window.FabushiWebMcpNative.callTool(name,JSON.stringify(input||{}));
            const payload=JSON.parse(raw);
            if(!payload.ok)throw new Error(payload.error||'WebMCP runtime call failed');
            return payload.result;
          }
          function register(item){
            const tool={name:item.name,title:item.title||item.name,description:item.description||item.name,inputSchema:item.inputSchema||{type:'object',properties:{}},annotations:{readOnlyHint:item.readOnlyHint===true},execute:(input)=>nativeCall(item.name,input)};
            localTools.set(tool.name,tool);
            if(document.modelContext&&typeof document.modelContext.registerTool==='function'){
              const controller=new AbortController();controllers.push(controller);
              Promise.resolve(document.modelContext.registerTool(tool,{signal:controller.signal})).catch(()=>{});
            }
          }
          for(const item of definitions)register(item);
          const host={
            version:1,
            pluginId:$safePluginId,
            account,
            lastEvent:null,
            getAccount:()=>account,
            pushEvent:(event)=>{
              host.lastEvent=event;
              window.dispatchEvent(new CustomEvent('fabushi:host-event',{detail:event}));
            }
          };
          Object.defineProperty(window,'__fabushiMiniAppHost',{configurable:true,value:host});
          Object.defineProperty(window,'__fabushiWebMcp',{configurable:true,value:{version:1,list:()=>Array.from(localTools.values()).map(publicTool),call:async(name,input={})=>{const tool=localTools.get(name);if(!tool)throw new Error('Unknown WebMCP tool: '+name);return tool.execute(input);}}});
          window.addEventListener('pagehide',()=>{for(const controller of controllers)controller.abort();},{once:true});
          window.dispatchEvent(new CustomEvent('fabushi:miniapp-auth',{detail:account}));
          window.dispatchEvent(new CustomEvent('fabushi:webmcp-ready',{detail:{pluginId:$safePluginId,tools:definitions.map(t=>t.name)}}));
        })();
        </script>
    """.trimIndent()
    return if (html.contains("</head>", ignoreCase = true)) {
        html.replaceFirst(Regex("</head>", RegexOption.IGNORE_CASE), Regex.escapeReplacement("$bootstrap</head>"))
    } else {
        "$bootstrap$html"
    }
}

@SuppressLint("SetJavaScriptEnabled")
@Composable
fun MiniAppWebMcpSurface(
    plugin: MarketplacePlugin,
    loadLocalHtml: suspend (pluginId: String) -> String?,
    callRuntimeToolJson: (pluginId: String, name: String, argumentsJson: String) -> String,
    onClose: () -> Unit,
) {
    val context = LocalContext.current
    val lifecycleOwner = LocalLifecycleOwner.current
    val host = remember(plugin.pluginId) { MahayanaHost(context.applicationContext) }
    var status by remember(plugin.pluginId) { mutableStateOf("正在解析本地 WebMCP…") }
    var localHtml by remember(plugin.pluginId) { mutableStateOf<String?>(null) }
    var accountProjection by remember(plugin.pluginId) { mutableStateOf(JSONObject().put("loggedIn", false).put("user", JSONObject())) }
    var sourceResolved by remember(plugin.pluginId) { mutableStateOf(false) }
    val gate = remember(plugin.pluginId) { MiniAppLifecycleGate() }
    val encodedId = URLEncoder.encode(plugin.pluginId, StandardCharsets.UTF_8.toString())
    val hostedUrl = "https://fabushi.ombhrum.com/miniapps/$encodedId/"

    LaunchedEffect(plugin.pluginId) {
        accountProjection = runCatching { miniAppAccountProjection(host.request("feature.auth.status")) }
            .getOrElse { JSONObject().put("loggedIn", false).put("user", JSONObject()) }
        localHtml = loadLocalHtml(plugin.pluginId)
        sourceResolved = true
        status = if (localHtml != null) {
            if (accountProjection.optBoolean("loggedIn")) "正在加载本地 WebMCP · Fabushi 已登录" else "正在加载本地 WebMCP…"
        } else {
            "正在加载 Hosted WebMCP…"
        }
    }

    BackHandler(onBack = onClose)

    Column(modifier = Modifier.fillMaxSize().testTag("miniapp-webmcp-surface")) {
        Row(modifier = Modifier.fillMaxWidth().padding(12.dp)) {
            Button(onClick = onClose, modifier = Modifier.testTag("miniapp-webmcp-close")) {
                Text("返回")
            }
            Column(modifier = Modifier.padding(start = 12.dp)) {
                Text(plugin.displayName, style = MaterialTheme.typography.titleMedium)
                Text(status, style = MaterialTheme.typography.labelSmall, modifier = Modifier.testTag("miniapp-webmcp-status"))
            }
        }

        val webView = remember(plugin.pluginId) {
            WebView(context).apply {
                settings.javaScriptEnabled = true
                settings.domStorageEnabled = true
                settings.allowFileAccess = false
                settings.allowContentAccess = false
                settings.javaScriptCanOpenWindowsAutomatically = false
                settings.setSupportMultipleWindows(false)
                addJavascriptInterface(
                    MiniAppNativeWebMcpBridge(
                        plugin = plugin,
                        context = context,
                        localDocumentActive = { gate.localDocumentActive },
                        callRuntimeToolJson = callRuntimeToolJson,
                    ),
                    "FabushiWebMcpNative",
                )
                webViewClient = object : WebViewClient() {
                    override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean {
                        val url = request.url.toString()
                        val allowed = isAllowedMiniAppUrl(url)
                        if (!allowed && request.isForMainFrame) {
                            gate.onPageStarted(url)
                        }
                        return !allowed
                    }

                    override fun onPageStarted(view: WebView, url: String?, favicon: Bitmap?) {
                        gate.onPageStarted(url)
                        status = "正在加载 WebMCP…"
                    }

                    override fun onPageFinished(view: WebView, url: String?) {
                        val probe = """
                            (() => {
                              const tools = window.__fabushiWebMcp?.list?.() || [];
                              return JSON.stringify({ ready: tools.length > 0, tools: tools.map(t => t.name), loggedIn: window.__fabushiMiniAppHost?.account?.loggedIn === true });
                            })()
                        """.trimIndent()
                        view.evaluateJavascript(probe) { value ->
                            status = if (value.contains("\\\"ready\\\":true")) {
                                if (url?.contains(LOCAL_WEB_MCP_ORIGIN) == true) {
                                    if (value.contains("\\\"loggedIn\\\":true")) "本地 WebMCP 已连接 · Fabushi 已登录" else "本地 WebMCP 已连接"
                                } else {
                                    "WebMCP 已连接"
                                }
                            } else {
                                "WebMCP 页面已打开"
                            }
                        }
                    }
                }
            }
        }

        val eventListener = remember(plugin.pluginId, webView) {
            host.addFeatureEventListener { event ->
                gate.dispatchHostEvent(event) { projected ->
                    val script = "window.__fabushiMiniAppHost?.pushEvent(JSON.parse(${JSONObject.quote(projected.toString())}));"
                    Handler(Looper.getMainLooper()).post {
                        if (gate.canDispatchHostEvent()) runCatching { webView.evaluateJavascript(script, null) }
                    }
                }
            }
        }

        AndroidView(
            factory = { webView },
            update = { view ->
                if (!sourceResolved) return@AndroidView
                val local = localHtml
                val desiredTag = if (local != null) "local:${plugin.pluginId}:${accountProjection.optBoolean("loggedIn")}" else "hosted:${plugin.pluginId}"
                if (view.tag == desiredTag) return@AndroidView
                view.tag = desiredTag
                if (local != null) {
                    gate.onLocalHtmlLoaded()
                    val baseUrl = "https://$LOCAL_WEB_MCP_ORIGIN/miniapps/$encodedId/"
                    view.loadDataWithBaseURL(
                        baseUrl,
                        injectLocalWebMcp(local, plugin, accountProjection),
                        "text/html",
                        "utf-8",
                        null,
                    )
                } else {
                    gate.onHostedUrlLoaded()
                    view.loadUrl(hostedUrl)
                }
            },
            modifier = Modifier.fillMaxSize().testTag("miniapp-webmcp-webview"),
        )

        DisposableEffect(webView, eventListener, host, lifecycleOwner) {
            val lifecycleObserver = LifecycleEventObserver { _, event ->
                gate.onLifecycleEvent(event)
            }
            lifecycleOwner.lifecycle.addObserver(lifecycleObserver)
            onDispose {
                lifecycleOwner.lifecycle.removeObserver(lifecycleObserver)
                eventListener.close()
                gate.onDispose()
                webView.stopLoading()
                webView.removeJavascriptInterface("FabushiWebMcpNative")
                webView.loadUrl("about:blank")
                webView.removeAllViews()
                webView.destroy()
                host.close()
            }
        }
    }
}
