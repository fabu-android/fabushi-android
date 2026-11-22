package com.ombhrum.fabushi

import android.app.Application
import android.net.Uri
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.ombhrum.fabushi.core.MahayanaHost
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject
import java.util.UUID

data class MiniAppToolContract(
    val name: String,
    val description: String,
    val approval: String,
    val title: String? = null,
    val inputSchemaJson: String? = null,
)

data class MarketplacePlugin(
    val pluginId: String,
    val displayName: String,
    val description: String,
    val latestVersion: String?,
    val sourceRef: String? = null,
    val tools: List<MiniAppToolContract> = emptyList(),
)

data class PermissionRequest(
    val pluginId: String,
    val runtime: String,
    val permissions: List<String>,
)

enum class MobileChatRole { USER, ASSISTANT }
enum class MobileChatEntryKind { MESSAGE, ACTION, THINKING, MINI_APP }

data class MobileChatMessage(
    val id: String,
    val role: MobileChatRole,
    val text: String,
    val kind: MobileChatEntryKind = MobileChatEntryKind.MESSAGE,
    val operationId: String? = null,
    val actionTitle: String? = null,
    val actionDetail: String? = null,
    val actionStatus: String? = null,
    val miniAppName: String? = null,
    val miniAppDescription: String? = null,
    val streaming: Boolean = false,
)

data class MarketplaceUiState(
    val loading: Boolean = false,
    val installingPluginId: String? = null,
    val query: String = "",
    val message: String = "Mahayana Rust Host 已启动",
    val plugins: List<MarketplacePlugin> = emptyList(),
    val permissionRequest: PermissionRequest? = null,
    val authResolved: Boolean = false,
    val loggedIn: Boolean = false,
    val accountName: String = "Fabushi",
    val accountEmail: String = "",
    val onboardingStep: Int = 0,
    val browserLoginAttemptId: String? = null,
    val browserLoginUrl: String? = null,
    val browserLaunchNonce: Long = 0,
    val loginBusy: Boolean = false,
    val loginError: String? = null,
    val chatDraft: String = "",
    val chatMessages: List<MobileChatMessage> = emptyList(),
    val chatBusy: Boolean = false,
    val activeOperationId: String? = null,
)

class MarketplaceViewModel(application: Application) : AndroidViewModel(application) {
    private val host = MahayanaHost(application)
    private val miniApps = MiniAppPlatformBridge(host)
    private val mutableState = MutableStateFlow(MarketplaceUiState())
    val state: StateFlow<MarketplaceUiState> = mutableState.asStateFlow()

    init {
        val onboardingComplete = application.getSharedPreferences("fabushi.mobile", 0).getBoolean("onboarding-complete", false)
        mutableState.value = mutableState.value.copy(onboardingStep = if (onboardingComplete) 3 else 0)
        initializeAuth()
    }

    fun initializeAuth() {
        mutableState.value = mutableState.value.copy(authResolved = false)
        viewModelScope.launch {
            runCatching {
                withContext(Dispatchers.IO) { host.request("feature.auth.status") }
            }.onSuccess { result ->
                val user = result.optJSONObject("user")
                mutableState.value = mutableState.value.copy(
                    authResolved = true,
                    loggedIn = result.optBoolean("loggedIn"),
                    accountName = user?.optString("nickname").orEmpty().ifBlank { user?.optString("username").orEmpty().ifBlank { user?.optString("email").orEmpty().ifBlank { "Fabushi" } } },
                    accountEmail = user?.optString("email").orEmpty(),
                )
                if (mutableState.value.loggedIn) refresh()
            }.onFailure { error ->
                mutableState.value = mutableState.value.copy(authResolved = true, message = "账号状态加载失败：${error.message ?: error::class.java.simpleName}")
            }
        }
    }

    fun advanceOnboarding() {
        val next = (mutableState.value.onboardingStep + 1).coerceAtMost(3)
        mutableState.value = mutableState.value.copy(onboardingStep = next)
        if (next == 3) getApplication<Application>().getSharedPreferences("fabushi.mobile", 0).edit().putBoolean("onboarding-complete", true).apply()
    }

    fun skipOnboarding() {
        mutableState.value = mutableState.value.copy(onboardingStep = 3)
        getApplication<Application>().getSharedPreferences("fabushi.mobile", 0).edit().putBoolean("onboarding-complete", true).apply()
    }

    fun retreatOnboarding() {
        mutableState.value = mutableState.value.copy(onboardingStep = (mutableState.value.onboardingStep - 1).coerceAtLeast(0))
    }

    fun beginBrowserLogin() {
        if (mutableState.value.loginBusy) return
        mutableState.value = mutableState.value.copy(loginBusy = true, loginError = null)
        viewModelScope.launch {
            runCatching {
                withContext(Dispatchers.IO) { host.request("feature.auth.browserStart") }
            }.onSuccess { result ->
                val attemptId = result.optString("attemptId")
                val loginUrl = result.optString("loginUrl").ifBlank { result.optString("authorizationUrl") }
                check(attemptId.isNotBlank() && loginUrl.isNotBlank()) { "登录地址无效" }
                mutableState.value = mutableState.value.copy(
                    loginBusy = false,
                    browserLoginAttemptId = attemptId,
                    browserLoginUrl = loginUrl,
                    browserLaunchNonce = mutableState.value.browserLaunchNonce + 1,
                    message = "登录页面已打开",
                )
                if (loginUrl.startsWith("about:blank#fabushi-test-browser-login")) {
                    completeBrowserLogin(attemptId)
                }
            }.onFailure { error ->
                mutableState.value = mutableState.value.copy(loginBusy = false, loginError = error.message ?: error::class.java.simpleName)
            }
        }
    }

    fun reopenBrowserLogin() {
        val attemptId = mutableState.value.browserLoginAttemptId ?: return
        viewModelScope.launch {
            runCatching {
                withContext(Dispatchers.IO) { host.request("feature.auth.browserReopen", JSONObject().put("attemptId", attemptId)) }
            }.onSuccess { result ->
                val loginUrl = result.optString("loginUrl").ifBlank { result.optString("authorizationUrl") }
                val resolvedUrl = loginUrl.ifBlank { mutableState.value.browserLoginUrl.orEmpty() }
                if (resolvedUrl.isNotBlank()) {
                    mutableState.value = mutableState.value.copy(
                        browserLoginUrl = resolvedUrl,
                        browserLaunchNonce = mutableState.value.browserLaunchNonce + 1,
                    )
                    if (resolvedUrl.startsWith("about:blank#fabushi-test-browser-login")) {
                        completeBrowserLogin(attemptId)
                    }
                }
            }.onFailure { error -> mutableState.value = mutableState.value.copy(loginError = error.message ?: error::class.java.simpleName) }
        }
    }

    fun cancelBrowserLogin() {
        val attemptId = mutableState.value.browserLoginAttemptId ?: return
        viewModelScope.launch {
            runCatching { withContext(Dispatchers.IO) { host.request("feature.auth.browserCancel", JSONObject().put("attemptId", attemptId)) } }
            mutableState.value = mutableState.value.copy(browserLoginAttemptId = null, browserLoginUrl = null, loginBusy = false, message = "登录授权已取消")
        }
    }

    fun setQuery(value: String) {
        mutableState.value = mutableState.value.copy(query = value)
    }

    fun handleDeepLink(uri: Uri) {
        if (uri.scheme != "fabushi" || uri.userInfo != null || uri.port != -1) return
        val hostName = uri.host?.lowercase().orEmpty()
        val path = uri.pathSegments.filter { it.isNotBlank() }
        when (hostName) {
            "auth" -> {
                if (path.firstOrNull() != "complete" || path.size != 1) return
                val allowedNames = setOf("attemptId", "status")
                if (uri.queryParameterNames.any { it !in allowedNames }) return
                val attemptIds = uri.getQueryParameters("attemptId")
                val statuses = uri.getQueryParameters("status")
                if (attemptIds.size != 1 || statuses.size > 1) return
                val attemptId = attemptIds.single()
                val status = statuses.singleOrNull()?.lowercase() ?: "completed"
                if (!Regex("^[A-Za-z0-9_-]{8,96}$").matches(attemptId)) return
                if (status !in setOf("completed", "cancelled", "failed")) return
                mutableState.value = mutableState.value.copy(
                    message = when (status) {
                        "completed" -> "登录授权已完成，正在同步账号状态"
                        "cancelled" -> "登录授权已取消"
                        else -> "登录授权失败"
                    },
                )
                if (status == "completed") completeBrowserLogin(attemptId)
            }
            "agent" -> {
                val agentId = path.firstOrNull().orEmpty().take(200)
                if (agentId.isNotBlank()) {
                    mutableState.value = mutableState.value.copy(message = "已接收智能体链接：$agentId")
                }
            }
            "settings", "feedback", "about", "widgets", "onboarding" -> {
                mutableState.value = mutableState.value.copy(message = "已接收应用链接：$hostName")
            }
        }
    }

    private fun completeBrowserLogin(attemptId: String) {
        viewModelScope.launch {
            runCatching {
                withContext(Dispatchers.IO) {
                    host.request(
                        "feature.auth.browserPoll",
                        JSONObject().put("attemptId", attemptId),
                    )
                }
            }.onSuccess { result ->
                when (result.optString("status")) {
                    "completed" -> {
                        val auth = result.optJSONObject("auth")
                        val user = auth?.optJSONObject("user")
                        mutableState.value = mutableState.value.copy(
                            authResolved = true,
                            loggedIn = auth?.optBoolean("loggedIn", true) ?: true,
                            accountName = user?.optString("nickname").orEmpty().ifBlank { user?.optString("username").orEmpty().ifBlank { user?.optString("email").orEmpty().ifBlank { "Fabushi" } } },
                            accountEmail = user?.optString("email").orEmpty(),
                            browserLoginAttemptId = null,
                            browserLoginUrl = null,
                            loginError = null,
                            message = "登录成功，账号状态已同步",
                        )
                        refresh()
                    }
                    "cancelled" -> mutableState.value = mutableState.value.copy(message = "登录授权已取消")
                    "failed" -> mutableState.value = mutableState.value.copy(message = "登录授权失败")
                    else -> mutableState.value = mutableState.value.copy(message = "登录结果尚未可用，请返回浏览器重试")
                }
            }.onFailure { error ->
                mutableState.value = mutableState.value.copy(
                    message = "登录状态同步失败：${error.message ?: error::class.java.simpleName}",
                )
            }
        }
    }

    fun logout() {
        val operationId = mutableState.value.activeOperationId
        viewModelScope.launch {
            if (!operationId.isNullOrBlank()) {
                runCatching { withContext(Dispatchers.IO) { host.request("feature.interrupt", JSONObject().put("operationId", operationId)) } }
            }
            runCatching { withContext(Dispatchers.IO) { host.request("feature.auth.logout") } }
                .onSuccess { result ->
                    val user = result.optJSONObject("user")
                    mutableState.value = mutableState.value.copy(
                        authResolved = true,
                        loggedIn = result.optBoolean("loggedIn", false),
                        accountName = user?.optString("nickname").orEmpty().ifBlank { "Fabushi" },
                        accountEmail = user?.optString("email").orEmpty(),
                        chatMessages = emptyList(),
                        activeOperationId = null,
                        chatBusy = false,
                        message = "已退出登录",
                    )
                }
                .onFailure { error -> mutableState.value = mutableState.value.copy(message = "退出登录失败：${error.message ?: error::class.java.simpleName}") }
        }
    }

    fun setChatDraft(value: String) {
        mutableState.value = mutableState.value.copy(chatDraft = value)
    }

    fun sendChat() {
        val current = mutableState.value
        val text = current.chatDraft.trim()
        if (text.isBlank() || !current.loggedIn || current.chatBusy) return
        val requestId = "android-chat-${UUID.randomUUID()}"
        mutableState.value = current.copy(
            chatDraft = "",
            chatBusy = true,
            chatMessages = current.chatMessages + MobileChatMessage(requestId, MobileChatRole.USER, text),
        )
        viewModelScope.launch {
            runCatching {
                val accepted = withContext(Dispatchers.IO) {
                    host.request(
                        "feature.execute",
                        JSONObject().put("command", JSONObject().put("type", "chat.send").put("requestId", requestId).put("text", text).put("agentId", "mahayana-assistant").put("mode", "agent")),
                    )
                }
                val operationId = accepted.optString("operationId").ifBlank { requestId }
                mutableState.value = mutableState.value.copy(
                    activeOperationId = operationId,
                    chatMessages = mutableState.value.chatMessages + MobileChatMessage("thinking:$operationId", MobileChatRole.ASSISTANT, "", MobileChatEntryKind.THINKING, operationId, "正在思考", null, "running"),
                )
                pumpChatEvents(operationId)
            }.onFailure { error -> mutableState.value = mutableState.value.copy(chatBusy = false, activeOperationId = null, message = "发送失败：${error.message ?: error::class.java.simpleName}") }
        }
    }

    fun stopChat() {
        val operationId = mutableState.value.activeOperationId ?: return
        viewModelScope.launch { runCatching { withContext(Dispatchers.IO) { host.request("feature.interrupt", JSONObject().put("operationId", operationId)) } } }
    }

    private suspend fun pumpChatEvents(operationId: String) {
        repeat(1800) {
            if (!mutableState.value.chatBusy) return
            val event = runCatching { withContext(Dispatchers.IO) { host.request("feature.receive") } }.getOrElse {
                mutableState.value = mutableState.value.copy(chatBusy = false, activeOperationId = null, message = "消息流中断：${it.message ?: it::class.java.simpleName}")
                return
            }
            val eventOperationId = event.optString("operationId").ifBlank { operationId }
            when (event.optString("type")) {
                "operation.started" -> if (eventOperationId == operationId && mutableState.value.chatMessages.none { it.kind == MobileChatEntryKind.THINKING && it.operationId == operationId }) {
                    appendChatMessage(MobileChatMessage("thinking:$operationId", MobileChatRole.ASSISTANT, "", MobileChatEntryKind.THINKING, operationId, event.optString("label").ifBlank { "正在思考" }, null, "running"))
                }
                "model.routed" -> if (eventOperationId == operationId) {
                    appendChatAction(operationId, "model-route", if (event.optString("model") == "auto") "选择模型" else "模型：${event.optString("model")}", listOf(event.optString("provider"), event.optString("mode")).filter { it.isNotBlank() }.joinToString(" · "), "completed")
                }
                "agent.step" -> if (eventOperationId == operationId) {
                    appendChatAction(operationId, event.optString("stepId").ifBlank { UUID.randomUUID().toString() }, event.optString("title").ifBlank { "助手动作" }, event.optString("detail").takeIf { it.isNotBlank() }, event.optString("status").ifBlank { "completed" })
                }
                "chat.message" -> if (eventOperationId == operationId) {
                    if (event.optString("role") == "assistant") {
                        removeChatThinking(operationId)
                        upsertAssistantMessage(operationId, event.optString("text"), append = false)
                    }
                }
                "chat.delta" -> if (event.optString("operationId") == operationId) {
                    removeChatThinking(operationId)
                    upsertAssistantMessage(operationId, event.optString("delta"), append = true)
                }
                "operation.completed", "operation.interrupted" -> if (eventOperationId == operationId) {
                    removeChatThinking(operationId)
                    settleChatActions(operationId, if (event.optString("type") == "operation.completed") "completed" else "failed")
                    mutableState.value = mutableState.value.copy(chatBusy = false, activeOperationId = null)
                    return
                }
                "operation.failed" -> if (eventOperationId == operationId) {
                    removeChatThinking(operationId)
                    settleChatActions(operationId, "failed")
                    mutableState.value = mutableState.value.copy(chatBusy = false, activeOperationId = null, message = event.optString("message").ifBlank { "本次任务失败" })
                    return
                }
            }
            delay(80)
        }
        if (mutableState.value.chatBusy) mutableState.value = mutableState.value.copy(message = "任务仍在后台运行，稍后会继续同步事件")
    }

    private fun appendChatMessage(entry: MobileChatMessage) {
        mutableState.value = mutableState.value.copy(chatMessages = mutableState.value.chatMessages.filterNot { it.id == entry.id } + entry)
    }

    private fun removeChatThinking(operationId: String) {
        mutableState.value = mutableState.value.copy(chatMessages = mutableState.value.chatMessages.filterNot { it.kind == MobileChatEntryKind.THINKING && it.operationId == operationId })
    }

    private fun settleChatActions(operationId: String, status: String) {
        mutableState.value = mutableState.value.copy(
            chatMessages = mutableState.value.chatMessages.map { entry ->
                if (entry.kind == MobileChatEntryKind.ACTION && entry.operationId == operationId && entry.actionStatus == "running") {
                    entry.copy(actionStatus = status)
                } else entry
            },
        )
    }

    private fun appendChatAction(operationId: String, stepId: String, title: String, detail: String?, status: String) {
        val id = "action:$operationId:$stepId"
        appendChatMessage(MobileChatMessage(id, MobileChatRole.ASSISTANT, "", MobileChatEntryKind.ACTION, operationId, title, detail, status))
    }

    private fun upsertAssistantMessage(operationId: String, text: String, append: Boolean) {
        if (text.isBlank()) return
        val current = mutableState.value.chatMessages
        val index = current.indexOfLast { it.kind == MobileChatEntryKind.MESSAGE && it.role == MobileChatRole.ASSISTANT && it.operationId == operationId }
        val next = if (index >= 0) {
            current.toMutableList().also { list -> list[index] = list[index].copy(text = if (append) list[index].text + text else text) }
        } else current + MobileChatMessage("assistant:$operationId", MobileChatRole.ASSISTANT, text, operationId = operationId)
        mutableState.value = mutableState.value.copy(chatMessages = next)
    }

    fun refresh() {
        val query = mutableState.value.query.trim()
        mutableState.value = mutableState.value.copy(loading = true)
        viewModelScope.launch {
            runCatching {
                withContext(Dispatchers.IO) {
                    host.request(
                        "feature.marketplace.browse",
                        JSONObject().put("query", query.ifBlank { JSONObject.NULL }).put("platform", "android"),
                    )
                }
            }.onSuccess { result ->
                val plugins = result.optJSONArray("plugins")
                val items = buildList {
                    if (plugins != null) {
                        for (index in 0 until plugins.length()) {
                            val item = plugins.optJSONObject(index) ?: continue
                            val pluginId = item.optString("pluginId")
                            if (pluginId.isBlank()) continue
                            val commands = item.optJSONObject("source")?.optJSONArray("commands")
                                ?: item.optJSONArray("commands")
                            val install = item.optJSONObject("install")
                                ?: item.optJSONObject("releaseManifest")?.optJSONObject("install")
                            val sourceRef = install?.optJSONObject("source")?.optString("sourceRef")
                                ?.takeIf(String::isNotBlank)
                            add(
                                MarketplacePlugin(
                                    pluginId = pluginId,
                                    displayName = item.optString("displayName", pluginId),
                                    description = item.optString("description", "无描述"),
                                    latestVersion = item.optString("latestVersion").takeIf(String::isNotBlank),
                                    sourceRef = sourceRef,
                                    tools = commands.toToolContracts(),
                                ),
                            )
                        }
                    }
                }
                mutableState.value = mutableState.value.copy(
                    loading = false,
                    message = "原生 Android · Rust Host 已连接",
                    plugins = items,
                )
            }.onFailure { error ->
                mutableState.value = mutableState.value.copy(
                    loading = false,
                    message = "市场加载失败：${error.message ?: error::class.java.simpleName}",
                )
            }
        }
    }

    fun install(plugin: MarketplacePlugin) {
        val version = plugin.latestVersion
        if (version.isNullOrBlank()) {
            mutableState.value = mutableState.value.copy(message = "${plugin.pluginId} 没有可安装版本")
            return
        }
        mutableState.value = mutableState.value.copy(
            installingPluginId = plugin.pluginId,
            message = "正在安装 ${plugin.pluginId}@$version…",
        )
        viewModelScope.launch {
            runCatching {
                withContext(Dispatchers.IO) {
                    val metadata = host.request(
                        "feature.marketplace.release",
                        JSONObject().put("pluginId", plugin.pluginId).put("version", version),
                    )
                    val release = metadata.optJSONObject("releaseManifest")
                        ?: error("marketplace release has no releaseManifest")
                    val install = metadata.optJSONObject("install")
                        ?: release.optJSONObject("install")
                        ?: error("marketplace release has no unified install contract")
                    check(install.optString("protocol") == "fabushi.marketplace.install.v1") {
                        "marketplace release has an unsupported install contract"
                    }
                    check(install.optString("strategy") == "github-immutable") {
                        "marketplace release is not pinned to GitHub"
                    }
                    val source = install.optJSONObject("source")
                        ?: error("marketplace release has no GitHub source")
                    check(source.optString("sourceRef").isNotBlank() && !source.optBoolean("marketplaceHostsPackage")) {
                        "marketplace release is missing an immutable GitHub source"
                    }
                    val installed = host.request(
                        "feature.plugin.install",
                        JSONObject().put("release", release).put("platform", "android"),
                    )
                    val installedPluginId = installed.optString("pluginId", plugin.pluginId)
                    miniApps.confirmInstalledMiniApp(installedPluginId)
                    installed
                }
            }.onSuccess { installed ->
                val pluginId = installed.optString("pluginId", plugin.pluginId)
                val runtime = installed.optString("runtime")
                val permissions = installed.optJSONArray("requestedPermissions").toStringList()
                if (permissions.isEmpty()) {
                    startPortableRuntime(pluginId, runtime)
                } else {
                    mutableState.value = mutableState.value.copy(
                        installingPluginId = null,
                        permissionRequest = PermissionRequest(pluginId, runtime, permissions),
                        message = "$pluginId 请求 ${permissions.size} 项权限",
                    )
                }
            }.onFailure { error ->
                mutableState.value = mutableState.value.copy(
                    installingPluginId = null,
                    message = "安装未完成：${error.message ?: error::class.java.simpleName}",
                )
            }
        }
    }

    fun approvePermissions() {
        val request = mutableState.value.permissionRequest ?: return
        mutableState.value = mutableState.value.copy(
            permissionRequest = null,
            installingPluginId = request.pluginId,
            message = "正在授权 ${request.pluginId}…",
        )
        viewModelScope.launch {
            runCatching {
                withContext(Dispatchers.IO) {
                    for (permission in request.permissions) {
                        host.request(
                            "plugin.permission.grant",
                            JSONObject().put("pluginId", request.pluginId).put("permission", permission),
                        )
                    }
                }
            }.onSuccess {
                startPortableRuntime(request.pluginId, request.runtime)
            }.onFailure { error ->
                mutableState.value = mutableState.value.copy(
                    installingPluginId = null,
                    message = "授权失败：${error.message ?: error::class.java.simpleName}",
                )
            }
        }
    }

    fun denyPermissions() {
        val pluginId = mutableState.value.permissionRequest?.pluginId ?: return
        mutableState.value = mutableState.value.copy(
            permissionRequest = null,
            installingPluginId = null,
            message = "$pluginId 已安装，但权限未授权",
        )
    }

    private fun startPortableRuntime(pluginId: String, runtime: String) {
        if (runtime !in setOf("deepseek-js", "javascript", "cordis-js")) {
            mutableState.value = mutableState.value.copy(
                installingPluginId = null,
                message = "$pluginId 已安装 · $runtime",
            )
            return
        }
        viewModelScope.launch {
            runCatching {
                withContext(Dispatchers.IO) {
                    val compatibility = host.request(
                        "plugin.compatibility",
                        JSONObject().put("pluginId", pluginId),
                    )
                    check(compatibility.optBoolean("portableCompatible")) {
                        "插件不满足移动端 portable runtime 约束"
                    }
                    host.request(
                        "runtime.start",
                        JSONObject().put("pluginId", pluginId).put("config", JSONObject()),
                    )
                }
            }.onSuccess {
                mutableState.value = mutableState.value.copy(
                    installingPluginId = null,
                    message = "$pluginId 已安装并启动 · $runtime",
                )
            }.onFailure { error ->
                mutableState.value = mutableState.value.copy(
                    installingPluginId = null,
                    message = "$pluginId 已安装但启动失败：${error.message ?: error::class.java.simpleName}",
                )
            }
        }
    }

    suspend fun loadLocalMiniAppHtml(pluginId: String): String? = withContext(Dispatchers.IO) {
        runCatching {
            host.request(
                "feature.plugin.uiDocument",
                JSONObject().put("pluginId", pluginId),
            ).optString("html").takeIf { it.isNotBlank() }
        }.getOrNull()
    }

    fun callRuntimeToolJson(pluginId: String, name: String, argumentsJson: String): String {
        require(Regex("^[A-Za-z0-9_.-]{1,128}$").matches(name)) { "Invalid WebMCP tool name" }
        val arguments = JSONObject(argumentsJson.ifBlank { "{}" })
        val result = host.requestValue(
            "runtime.call",
            JSONObject()
                .put("pluginId", pluginId)
                .put("name", name)
                .put("arguments", arguments),
        )
        return result.toJsonString()
    }

    override fun onCleared() {
        host.close()
        super.onCleared()
    }
}

private fun JSONArray?.toStringList(): List<String> = buildList {
    val array = this@toStringList ?: return@buildList
    for (index in 0 until array.length()) {
        val value = array.optString(index).trim()
        if (value.isNotEmpty()) add(value)
    }
}

private fun JSONArray?.toToolContracts(): List<MiniAppToolContract> = buildList {
    val array = this@toToolContracts ?: return@buildList
    for (index in 0 until array.length()) {
        val command = array.optJSONObject(index) ?: continue
        val name = command.optString("tool", command.optString("name")).trim()
        if (!Regex("^[A-Za-z0-9_.-]{1,128}$").matches(name)) continue
        add(
            MiniAppToolContract(
                name = name,
                description = command.optString("description", name).ifBlank { name },
                approval = command.optString("approval", "none"),
                title = command.optString("title", name).ifBlank { name },
                inputSchemaJson = command.optJSONObject("inputSchema")?.toString(),
            ),
        )
    }
}

private fun Any?.toJsonString(): String = when (this) {
    null, JSONObject.NULL -> "null"
    is JSONObject, is JSONArray -> toString()
    is Number, is Boolean -> toString()
    is String -> JSONObject.quote(this)
    else -> JSONObject.quote(toString())
}
