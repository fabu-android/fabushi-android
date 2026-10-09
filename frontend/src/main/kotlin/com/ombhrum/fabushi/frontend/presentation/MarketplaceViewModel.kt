package com.ombhrum.fabushi

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.ombhrum.fabushi.androidpreload.deeplink.AndroidDeepLink
import com.ombhrum.fabushi.androidpreload.deeplink.AndroidPresentationDeepLink
import com.ombhrum.fabushi.androidpreload.deeplink.AuthCompletionStatus
import com.ombhrum.fabushi.androidpreload.runtime.AndroidCoordinatorPort
import kotlinx.coroutines.Dispatchers
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
)

data class PluginVariableField(
    val key: String,
    val label: String,
    val placeholder: String,
    val isRequired: Boolean,
    val isSecret: Boolean,
    val defaultValue: String? = null,
    val hint: String? = null,
)

data class MarketplacePlugin(
    val pluginId: String,
    val displayName: String,
    val description: String,
    val latestVersion: String?,
    val sourceRef: String? = null,
    val tools: List<MiniAppToolContract> = emptyList(),
    val variablesSchemaJson: String? = null,
    val variableFields: List<PluginVariableField> = emptyList(),
    val teamConfiguredVariables: Boolean = false,
)

data class PluginVariableRequest(
    val plugin: MarketplacePlugin,
    val fields: List<PluginVariableField>,
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
    val variableRequest: PluginVariableRequest? = null,
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
    private val coordinator: AndroidCoordinatorPort = CoordinatorClient.presentation()
    private val miniApps = MiniAppPlatformBridge(coordinator)
    private val mutableState = MutableStateFlow(MarketplaceUiState())
    val state: StateFlow<MarketplaceUiState> = mutableState.asStateFlow()
    private var featureEventSubscription: AutoCloseable? = null

    init {
        featureEventSubscription = coordinator.addFeatureEventListener { event ->
            viewModelScope.launch { handleChatEvent(event) }
        }
        val onboardingComplete = application.getSharedPreferences("fabushi.mobile", 0).getBoolean("onboarding-complete", false)
        mutableState.value = mutableState.value.copy(onboardingStep = if (onboardingComplete) 3 else 0)
        initializeAuth()
    }

    fun initializeAuth() {
        mutableState.value = mutableState.value.copy(authResolved = false)
        viewModelScope.launch {
            runCatching {
                withContext(Dispatchers.IO) { coordinator.authStatus() }
            }.onSuccess { result ->
                val user = result.optJSONObject("user")
                mutableState.value = mutableState.value.copy(
                    authResolved = true,
                    loggedIn = result.optBoolean("loggedIn"),
                    accountName = user?.optString("nickname").orEmpty().ifBlank { user?.optString("username").orEmpty().ifBlank { user?.optString("email").orEmpty().ifBlank { "Fabushi" } } },
                    accountEmail = user?.optString("email").orEmpty(),
                )
                if (mutableState.value.loggedIn) {
                    restoreChatTranscript()
                    refresh()
                }
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
                withContext(Dispatchers.IO) { coordinator.authBrowserStart() }
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
                withContext(Dispatchers.IO) { coordinator.authBrowserReopen( JSONObject().put("attemptId", attemptId)) }
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
            runCatching { withContext(Dispatchers.IO) { coordinator.authBrowserCancel( JSONObject().put("attemptId", attemptId)) } }
            mutableState.value = mutableState.value.copy(browserLoginAttemptId = null, browserLoginUrl = null, loginBusy = false, message = "登录授权已取消")
        }
    }

    fun setQuery(value: String) {
        mutableState.value = mutableState.value.copy(query = value)
    }

    fun handleDeepLink(link: AndroidPresentationDeepLink) {
        when (link) {
            is AndroidDeepLink.AuthCompletion -> {
                mutableState.value = mutableState.value.copy(
                    message = when (link.status) {
                        AuthCompletionStatus.COMPLETED -> "登录授权已完成，正在同步账号状态"
                        AuthCompletionStatus.CANCELLED -> "登录授权已取消"
                        AuthCompletionStatus.FAILED -> "登录授权失败"
                    },
                )
                if (link.status == AuthCompletionStatus.COMPLETED) {
                    completeBrowserLogin(link.attemptId)
                }
            }
            is AndroidDeepLink.Info -> Unit
            is AndroidDeepLink.Agent -> {
                mutableState.value = mutableState.value.copy(
                    message = "已接收智能体链接：${link.agentId}",
                )
            }
            is AndroidDeepLink.AppSection -> {
                mutableState.value = mutableState.value.copy(
                    message = "已接收应用链接：${link.section}",
                )
            }
        }
    }

    private fun completeBrowserLogin(attemptId: String) {
        viewModelScope.launch {
            runCatching {
                withContext(Dispatchers.IO) {
                    coordinator.authBrowserPoll(
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
                        restoreChatTranscript()
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
                runCatching { withContext(Dispatchers.IO) { coordinator.featureInterrupt( JSONObject().put("operationId", operationId)) } }
            }
            runCatching { withContext(Dispatchers.IO) { coordinator.authLogout() } }
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

    private fun restoreChatTranscript() {
        viewModelScope.launch {
            runCatching {
                withContext(Dispatchers.IO) { coordinator.transcriptSnapshot() }
            }.onSuccess { entries ->
                val restored = buildList {
                    for (index in 0 until entries.length()) {
                        val entry = entries.optJSONObject(index) ?: continue
                        if (entry.optString("kind") != "message") continue
                        val id = entry.optString("id").trim()
                        val text = entry.optString("content")
                        val role = when (entry.optString("role")) {
                            "user" -> MobileChatRole.USER
                            "assistant" -> MobileChatRole.ASSISTANT
                            else -> continue
                        }
                        if (id.isBlank()) continue
                        add(
                            MobileChatMessage(
                                id = id,
                                role = role,
                                text = text,
                                operationId = entry.optString("operationId").takeIf(String::isNotBlank),
                            ),
                        )
                    }
                }.distinctBy(MobileChatMessage::id)
                mutableState.value = mutableState.value.copy(
                    chatMessages = restored,
                    chatBusy = false,
                    activeOperationId = null,
                )
            }.onFailure { error ->
                mutableState.value = mutableState.value.copy(
                    message = "会话恢复失败：${error.message ?: error::class.java.simpleName}",
                )
            }
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
                    coordinator.featureExecute(
                        JSONObject().put("command", JSONObject().put("type", "chat.send").put("requestId", requestId).put("text", text).put("agentId", "mahayana-assistant").put("mode", "agent")),
                    )
                }
                val operationId = accepted.optString("operationId").ifBlank { requestId }
                mutableState.value = mutableState.value.copy(
                    activeOperationId = operationId,
                    chatMessages = mutableState.value.chatMessages + MobileChatMessage("thinking:$operationId", MobileChatRole.ASSISTANT, "", MobileChatEntryKind.THINKING, operationId, "正在思考", null, "running"),
                )
            }.onFailure { error -> mutableState.value = mutableState.value.copy(chatBusy = false, activeOperationId = null, message = "发送失败：${error.message ?: error::class.java.simpleName}") }
        }
    }

    fun stopChat() {
        val operationId = mutableState.value.activeOperationId ?: return
        viewModelScope.launch { runCatching { withContext(Dispatchers.IO) { coordinator.featureInterrupt( JSONObject().put("operationId", operationId)) } } }
    }

    private fun handleChatEvent(event: JSONObject) {
        val operationId = mutableState.value.activeOperationId ?: return
        if (!mutableState.value.chatBusy) return
        val eventOperationId = event.optString("operationId").ifBlank { operationId }
        when (event.optString("type")) {
            "operation.started" -> if (eventOperationId == operationId && mutableState.value.chatMessages.none { it.kind == MobileChatEntryKind.THINKING && it.operationId == operationId }) {
                appendChatMessage(MobileChatMessage("thinking:$operationId", MobileChatRole.ASSISTANT, "", MobileChatEntryKind.THINKING, operationId, event.optString("label").ifBlank { "正在思考" }, null, "running"))
            }
            "model.routed" -> if (eventOperationId == operationId) {
                appendChatAction(operationId, "model-route", if (event.optString("model") == "auto") "选择模型" else "模型：" + event.optString("model"), listOf(event.optString("provider"), event.optString("mode")).filter { it.isNotBlank() }.joinToString(" · "), "completed")
            }
            "agent.step" -> if (eventOperationId == operationId) {
                appendChatAction(operationId, event.optString("stepId").ifBlank { UUID.randomUUID().toString() }, event.optString("title").ifBlank { "助手动作" }, event.optString("detail").takeIf { it.isNotBlank() }, event.optString("status").ifBlank { "completed" })
            }
            "chat.message" -> if (eventOperationId == operationId && event.optString("role") == "assistant") {
                removeChatThinking(operationId)
                upsertAssistantMessage(operationId, event.optString("text"), append = false)
            }
            "chat.delta" -> if (eventOperationId == operationId) {
                removeChatThinking(operationId)
                upsertAssistantMessage(operationId, event.optString("delta"), append = true)
            }
            "operation.completed", "operation.interrupted" -> if (eventOperationId == operationId) {
                removeChatThinking(operationId)
                settleChatActions(operationId, if (event.optString("type") == "operation.completed") "completed" else "failed")
                mutableState.value = mutableState.value.copy(chatBusy = false, activeOperationId = null)
            }
            "operation.failed" -> if (eventOperationId == operationId) {
                removeChatThinking(operationId)
                settleChatActions(operationId, "failed")
                mutableState.value = mutableState.value.copy(chatBusy = false, activeOperationId = null, message = event.optString("message").ifBlank { "本次任务失败" })
            }
        }
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
                    coordinator.marketplaceBrowse(
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
                                    variablesSchemaJson = item.optJSONObject("variablesSchema")?.toString(),
                                    variableFields = item.optJSONObject("variablesSchema")
                                        ?.let(coordinator::pluginVariableFields)
                                        .toPluginVariableFields(),
                                    teamConfiguredVariables = item.optBoolean(
                                        "teamConfiguredVariables",
                                        item.optBoolean("hasTeamConfiguredVariables", false),
                                    ),
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
        if (plugin.variableFields.isNotEmpty() && !plugin.teamConfiguredVariables) {
            mutableState.value = mutableState.value.copy(
                variableRequest = PluginVariableRequest(plugin, plugin.variableFields),
                installingPluginId = null,
                message = "请配置 " + plugin.displayName + " 的连接变量",
            )
            return
        }
        installConfigured(plugin)
    }

    fun cancelPluginVariables() {
        val pluginId = mutableState.value.variableRequest?.plugin?.pluginId ?: return
        mutableState.value = mutableState.value.copy(
            variableRequest = null,
            installingPluginId = null,
            message = pluginId + " 配置已取消",
        )
    }

    fun submitPluginVariables(values: Map<String, String>) {
        val request = mutableState.value.variableRequest ?: return
        val plugin = request.plugin
        val schema = plugin.variablesSchemaJson?.let(::JSONObject) ?: JSONObject()
        mutableState.value = mutableState.value.copy(
            variableRequest = null,
            installingPluginId = plugin.pluginId,
            message = "正在保护并保存 " + plugin.pluginId + " 的变量…",
        )
        viewModelScope.launch {
            runCatching {
                withContext(Dispatchers.IO) {
                    val payloadValues = JSONObject()
                    values.forEach { (key, value) -> payloadValues.put(key, value) }
                    coordinator.pluginVariablesConfigure(
                        JSONObject()
                            .put("pluginId", plugin.pluginId)
                            .put("schema", schema)
                            .put("values", payloadValues)
                            .put("teamConfigured", false),
                    )
                }
            }.onSuccess {
                installConfigured(plugin)
            }.onFailure { error ->
                mutableState.value = mutableState.value.copy(
                    installingPluginId = null,
                    variableRequest = PluginVariableRequest(plugin, plugin.variableFields),
                    message = "变量配置未保存：" + (error.message ?: error::class.java.simpleName),
                )
            }
        }
    }

    private fun installConfigured(plugin: MarketplacePlugin) {
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
                    val metadata = coordinator.marketplaceRelease(
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
                    val installed = coordinator.pluginInstall(
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
                        coordinator.pluginPermissionGrant(
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
                    val compatibility = coordinator.pluginCompatibility(
                        JSONObject().put("pluginId", pluginId),
                    )
                    check(compatibility.optBoolean("portableCompatible")) {
                        "插件不满足移动端 portable runtime 约束"
                    }
                    coordinator.runtimeStart(
                        JSONObject().put("pluginId", pluginId),
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
            coordinator.pluginUiDocument(
                JSONObject().put("pluginId", pluginId),
            ).optString("html").takeIf { it.isNotBlank() }
        }.getOrNull()
    }

    fun callRuntimeToolJson(
        pluginId: String,
        name: String,
        argumentsJson: String,
        requestId: String,
    ): String {
        require(Regex("^[A-Za-z0-9_.-]{1,128}$").matches(name)) { "Invalid WebMCP tool name" }
        require(requestId.length in 8..256 && requestId.none(Char::isWhitespace)) {
            "Invalid stable WebMCP request id"
        }
        val arguments = JSONObject(argumentsJson.ifBlank { "{}" })
        val result = coordinator.runtimeCallValue(
            JSONObject()
                .put("pluginId", pluginId)
                .put("tool", name)
                .put("requestId", requestId)
                .put("arguments", arguments),
        )
        return result.toJsonString()
    }

    fun cancelRuntimeToolCall(requestId: String) {
        if (requestId.isBlank()) return
        runCatching {
            coordinator.runtimeCancel(
                JSONObject()
                    .put("requestId", requestId)
                    .put("reason", "Mini App load disposed"),
            )
        }
    }

    override fun onCleared() {
        featureEventSubscription?.close()
        featureEventSubscription = null
        super.onCleared()
    }
}

private fun JSONArray?.toPluginVariableFields(): List<PluginVariableField> = buildList {
    val array = this@toPluginVariableFields ?: return@buildList
    for (index in 0 until array.length()) {
        val field = array.optJSONObject(index) ?: continue
        val key = field.optString("key").trim()
        if (key.isBlank()) continue
        add(
            PluginVariableField(
                key = key,
                label = field.optString("label", key).ifBlank { key },
                placeholder = field.optString("placeholder", key).ifBlank { key },
                isRequired = field.optBoolean("isRequired"),
                isSecret = field.optBoolean("isSecret"),
                defaultValue = field.optString("defaultValue").takeIf(String::isNotBlank),
                hint = field.optString("hint").takeIf(String::isNotBlank),
            ),
        )
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
