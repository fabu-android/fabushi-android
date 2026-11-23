package com.ombhrum.fabushi.telegram

import android.Manifest
import android.annotation.SuppressLint
import android.app.Application
import android.content.pm.PackageManager
import android.media.AudioFormat
import android.media.AudioRecord
import android.media.MediaPlayer
import android.media.MediaRecorder
import android.media.PlaybackParams
import android.os.Build
import androidx.core.content.ContextCompat
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.ombhrum.fabushi.ChatReaction
import com.ombhrum.fabushi.ConversationKind
import com.ombhrum.fabushi.MessagingUiState
import com.ombhrum.fabushi.NativeVoiceRecorder
import com.ombhrum.fabushi.core.MahayanaHost
import com.ombhrum.fabushi.mika.MikaChannelCardEvent
import com.ombhrum.fabushi.mika.TelegramMikaChannel
import java.io.File
import java.math.BigDecimal
import java.math.RoundingMode
import java.net.URLDecoder
import java.net.URLEncoder
import java.time.Instant
import java.util.Base64
import java.util.UUID
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicLong
import kotlin.random.Random
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import org.json.JSONArray
import org.json.JSONObject

const val DEFAULT_DESKTOP_SERVER_URL = "http://34.46.77.133:8080"

enum class TelegramSubScreen {
    HOME,
    CHAT,
    CONTACTS,
    CALLS_HISTORY,
    BOTFATHER,
    GLOBAL_DHARMA,
    TON_WALLET,
    HERMES_STUDIO,
    MAGPIE_GATEWAY,
    CLOUD_COMPUTER,
    MIKA_WORKSPACE,
    SETTINGS,
}

enum class TelegramSettingsPage(val title: String, val subtitle: String) {
    OVERVIEW("设置", "账号、外观、安全与网络"),
    PROFILE_BIO("个人资料与简介", "头像、昵称、@用户名、个人签名"),
    CHAT_THEMES("聊天设置与主题外观", "日间/夜间主题、气泡圆角、字号、涂鸦壁纸"),
    PRIVACY_SECURITY("隐私与安全", "本地密码锁、生物识别、两步验证、端到端密钥"),
    NOTIFICATIONS_SOUNDS("通知与声音", "私聊、群组、频道提醒与震动反馈"),
    DATA_STORAGE("数据与存储", "缓存清理、自动下载媒体策略"),
    CHAT_FOLDERS("聊天分组文件夹", "自定义顶部滑动标签页与过滤规则"),
    DEVICES_SESSIONS("已登录设备与扫码配对", "桌面端 QR 登录与活跃会话管理"),
    POWER_SAVING("省电与动效管理", "低电量阈值、贴纸与壁纸动态效果"),
    LANGUAGE_TRANSLATE("语言与即时翻译", "多语言界面与聊天消息一键翻译"),
    STARS_WALLET("Telegram Stars 与 TON 钱包", "Stars 余额、礼物藏品与链上资产"),
    AI_PROVIDERS("AI 模型与供应商", "配置 OpenAI / Anthropic / Gemini / DeepSeek 等 API Key"),
    PROXY_SERVER("代理与服务器节点", "gpui_telegram_server 同步端点与 SOCKS5/MTProto"),
}

data class CallHistoryEntry(
    val id: String,
    val peerTitle: String,
    val conversationId: String,
    val isVideo: Boolean,
    val outgoing: Boolean,
    val missed: Boolean = false,
    val timeLabel: String,
    val durationLabel: String,
)

data class TelegramUiState(
    val subScreen: TelegramSubScreen = TelegramSubScreen.HOME,
    val settingsPage: TelegramSettingsPage = TelegramSettingsPage.OVERVIEW,
    val isAuthenticated: Boolean = false,
    val offlineLocalMode: Boolean = false,
    val isAuthenticating: Boolean = false,
    val authError: String? = null,
    val pendingTwoFactorChallengeToken: String? = null,
    val pendingTwoFactorHint: String? = null,
    val alipayAttemptId: String? = null,
    val alipayLoginUrl: String? = null,
    val alipayStatusText: String? = null,
    val emailCodeSentNotice: String? = null,
    val serverWsConnected: Boolean = false,
    val syncStatusLabel: String = "信息",
    val serverEndpoint: String = DEFAULT_DESKTOP_SERVER_URL,
    val proxyEnabled: Boolean = false,
    val proxyHost: String = "127.0.0.1:1080",
    val uiLanguage: String = "zh-CN",
    val showTranslateButton: Boolean = true,
    val autoTranslateChannels: Boolean = false,
    val powerSavingThreshold: Int = 20,
    val inAppSoundsEnabled: Boolean = true,
    val storageCacheMb: Double = 0.0,
    val twoFactorEnabled: Boolean = false,
    val themeSettings: TelegramThemeSettings = TelegramThemeSettings(),
    val passcodeConfigured: Boolean = false,
    val biometricUnlockEnabled: Boolean = true,
    val isLocked: Boolean = false,
    val passcodeError: String? = null,
    val accounts: List<TelegramAccount> = emptyList(),
    val activeAccountId: String = "",
    val stories: List<TelegramStoryItem> = emptyList(),
    val viewingStoryId: String? = null,
    val folderTabs: List<TelegramFolderTab> = TelegramFolderFilter.DEFAULT_BUILT_IN_TABS,
    val selectedFolderId: String = "all",
    val archiveExpanded: Boolean = false,
    val globalSearchActive: Boolean = false,
    val globalSearchQuery: String = "",
    val globalSearchFilter: GlobalSearchFilter = GlobalSearchFilter.CHATS,
    val selectedChatIds: Set<String> = emptySet(),
    val chats: List<TelegramChatItem> = emptyList(),
    val activeChatId: String? = null,
    val activeTopicId: String? = null,
    val topicsByChat: Map<String, List<TelegramForumTopic>> = emptyMap(),
    val messagesByChat: Map<String, List<TelegramMessageItem>> = emptyMap(),
    val startedBotChatIds: Set<String> = emptySet(),
    val pinnedBannerIndexByChat: Map<String, Int> = emptyMap(),
    val inChatSearchActive: Boolean = false,
    val inChatSearchQuery: String = "",
    val replyTarget: TelegramMessageItem? = null,
    val editTarget: TelegramMessageItem? = null,
    val viewingMediaMessage: TelegramMessageItem? = null,
    val secretChatDialogChatId: String? = null,
    val channelStatsSnapshot: ChannelStatisticsSnapshot? = null,
    val discussionSheetMessage: TelegramMessageItem? = null,
    val discussionCommentsByMessage: Map<String, List<TelegramMessageItem>> = emptyMap(),
    val stickerPacks: List<StickerPackModel> = emptyList(),
    val gifCatalog: List<GifItemModel> = emptyList(),
    val slashCommands: List<SlashCommandSuggestion> = emptyList(),
    val recordingState: ActiveRecordingState = ActiveRecordingState(),
    val audioPlayback: FloatingAudioPlaybackState = FloatingAudioPlaybackState(),
    val callState: TelegramCallUiState = TelegramCallUiState(),
    val callHistory: List<CallHistoryEntry> = emptyList(),
    val talkOrbState: TalkModeOrbState = TalkModeOrbState(),
    val tonWallet: TonWalletUiState = TonWalletUiState(),
    val globalDharma: GlobalDharmaUiState = GlobalDharmaUiState(),
    val hermesStudio: HermesStudioUiState = HermesStudioUiState(),
    val magpieGateway: MagpieGatewayUiState = MagpieGatewayUiState(),
    val cloudComputer: CloudComputerUiState = CloudComputerUiState(),
    val codingAgentBots: List<CodingAgentBotItem> = emptyList(),
    val fabushiBots: List<FabushiBotItem> = emptyList(),
    val codingAgentProbes: List<CodingAgentProbeItem> = emptyList(),
    val serverSessions: List<ServerSessionItem> = emptyList(),
    val serverContacts: List<ServerContactItem> = emptyList(),
    val serverSearchResults: List<TelegramMessageItem> = emptyList(),
    val lastCreatedBotToken: String? = null,
    val botComputerBindings: Map<String, String> = emptyMap(),
    val lastBotProfileJson: String? = null,
    val userVault: UserVaultUiState = UserVaultUiState(),
    val miniAppPip: MiniAppPipSessionUi = MiniAppPipSessionUi(),
    val mikaSnapshot: MikaBridgeUiSnapshot = MikaBridgeUiSnapshot(),
    val composerAttachmentSheetOpen: Boolean = false,
    val toastBanner: String? = null,
) {
    val activeAccount: TelegramAccount?
        get() = accounts.firstOrNull { it.id == activeAccountId } ?: accounts.firstOrNull()

    val activeChat: TelegramChatItem?
        get() = activeChatId?.let { id -> chats.firstOrNull { it.id == id } }
}

class TelegramViewModel(application: Application) : AndroidViewModel(application) {
    private val appContext = getApplication<Application>()
    private val prefs = appContext.getSharedPreferences("fabushi.telegram.prefs", 0)
    private val generationFence = GenerationFence(1L)
    private val rustAccountGeneration = AtomicLong(1L)
    private val voiceRecorder = NativeVoiceRecorder(appContext)
    private val wsHttpClient by lazy { OkHttpClient.Builder().build() }
    private val wsLifecycleLock = Any()

    @Volatile private var serverWebSocket: WebSocket? = null
    @Volatile private var wsEndpoint: String? = null
    @Volatile private var wsToken: String? = null
    @Volatile private var wsShouldReconnect: Boolean = false
    @Volatile private var wsReconnectAttempt: Int = 0
    @Volatile private var wsReconnectJob: Job? = null
    @Volatile private var alipayAutoPollJob: Job? = null
    @Volatile private var currentServerUserId: Long? = null
    private val wsConnectCounter = AtomicInteger(0)
    private val mikaStepCounter = AtomicInteger(0)
    private val chatPtsMap = ConcurrentHashMap<Long, Long>()
    private val typingTimeoutJobs = ConcurrentHashMap<String, Job>()

    // 5-Rule Anti-Flicker In-Flight Mutation Guards (Tokenized CAS)
    private val inFlightMutationSeq = AtomicLong(0L)
    private val inFlightPinChats = ConcurrentHashMap<String, Pair<Long, Boolean>>()
    private val inFlightMuteChats = ConcurrentHashMap<String, Pair<Long, Boolean>>()
    private val inFlightArchiveChats = ConcurrentHashMap<String, Pair<Long, Boolean>>()
    private val inFlightMessagePins = ConcurrentHashMap<String, Pair<Long, Set<String>>>()
    private val inFlightReactions = ConcurrentHashMap<String, Long>()

    // Per-domain monotonic mutation epochs (prevents stale refresh overwrites across all 10 server domains)
    private val sessionsEpoch = AtomicLong(0L)
    private val contactsEpoch = AtomicLong(0L)
    private val foldersEpoch = AtomicLong(0L)
    private val computersEpoch = AtomicLong(0L)
    private val assistEpoch = AtomicLong(0L)
    private val remoteSessionsEpoch = AtomicLong(0L)
    private val mcpGrantsEpoch = AtomicLong(0L)
    private val botsEpoch = AtomicLong(0L)
    private val profileEpoch = AtomicLong(0L)
    private val chatsEpoch = AtomicLong(0L)
    private val chatMessagesEpoch = ConcurrentHashMap<String, AtomicLong>()
    private val chatPinsEpoch = ConcurrentHashMap<String, AtomicLong>()
    private val chatDraftEpoch = ConcurrentHashMap<String, AtomicLong>()

    private fun chatMessagesEpochFor(chatId: String): AtomicLong =
        chatMessagesEpoch.computeIfAbsent(chatId) { AtomicLong(0L) }

    private fun chatPinsEpochFor(chatId: String): AtomicLong =
        chatPinsEpoch.computeIfAbsent(chatId) { AtomicLong(0L) }

    private fun chatDraftEpochFor(chatId: String): AtomicLong =
        chatDraftEpoch.computeIfAbsent(chatId) { AtomicLong(0L) }

    internal fun currentGenerationForTest(): Long = generationFence.currentGeneration

    companion object {
        internal val RESERVED_FOLDER_TAB_IDS: Set<String> = setOf(
            "all", "personal", "unread", "pinned", "bots", "groups", "channels",
        )
    }

    internal var wsBaseBackoffMs: Long = 1_000L
    internal val wsConnectCount: Int get() = wsConnectCounter.get()

    private val mahayanaHost: MahayanaHost? by lazy {
        runCatching { MahayanaHost(appContext) }.getOrNull()
    }

    private var mikaBridge: MikaBridgeCallbacks? = null
    private var mikaEventsJob: Job? = null
    private var actorId: String = "actor:local"
    private var recordingTimerJob: Job? = null
    private var audioProgressJob: Job? = null
    private var callTimerJob: Job? = null
    private var activeMediaPlayer: MediaPlayer? = null
    private val cachedVoiceBlobs = ConcurrentHashMap<String, ByteArray>()

    private val _state = MutableStateFlow(buildCleanInitialState())
    val state: StateFlow<TelegramUiState> = _state.asStateFlow()

    init {
        restorePersistedSettings()
        refreshAllFromHost()
    }

    // ------------------------------------------------------------------------
    // Fail-Closed Host RPC Dispatch & Unauthorized Session Handler
    // ------------------------------------------------------------------------

    private fun hostRequestResult(method: String, params: JSONObject = JSONObject()): Result<JSONObject> {
        val host = mahayanaHost ?: return Result.failure(IllegalStateException("原生 Rust Host 未初始化"))
        return host.requestResult(method, params)
    }

    private fun isUnauthorizedError(err: Throwable?): Boolean {
        val msg = err?.message.orEmpty()
        return msg.contains("HTTP 401") ||
            (msg.contains("401") && (msg.contains("Unauthorized", ignoreCase = true) || msg.contains("invalid token", ignoreCase = true)))
    }

    private fun handleUnauthorizedSession(reason: String, expectedGen: Long? = null) {
        if (expectedGen != null) {
            if (!generationFence.compareAndAdvance(expectedGen)) return
        } else {
            generationFence.advance()
        }
        disconnectServerWebSocket()
        clearInFlightGuards()
        chatPtsMap.clear()
        currentServerUserId = null
        prefs.edit().putBoolean("offline_local_mode", false).apply()
        // Synchronous local SQLite session clear (<1ms, zero network I/O) before emitting signed-out UI state
        hostRequestResult("telegram.server.logout")
        _state.update { prev ->
            buildCleanInitialState().copy(
                serverEndpoint = prev.serverEndpoint,
                themeSettings = prev.themeSettings,
                uiLanguage = prev.uiLanguage,
                showTranslateButton = prev.showTranslateButton,
                autoTranslateChannels = prev.autoTranslateChannels,
                powerSavingThreshold = prev.powerSavingThreshold,
                inAppSoundsEnabled = prev.inAppSoundsEnabled,
                passcodeConfigured = prev.passcodeConfigured,
                biometricUnlockEnabled = prev.biometricUnlockEnabled,
                storageCacheMb = prev.storageCacheMb,
                mikaSnapshot = prev.mikaSnapshot,
                isAuthenticated = false,
                offlineLocalMode = false,
                syncStatusLabel = "未登录",
                authError = reason.ifBlank { "登录会话已失效，请重新登录" },
                toastBanner = "会话已过期 (401)，请重新登录",
            )
        }
    }

    private fun clearInFlightGuards() {
        inFlightPinChats.clear()
        inFlightMuteChats.clear()
        inFlightArchiveChats.clear()
        inFlightMessagePins.clear()
        inFlightReactions.clear()
    }

    fun attachMikaBridge(callbacks: MikaBridgeCallbacks?) {
        mikaBridge = callbacks
        mikaEventsJob?.cancel()
        mikaEventsJob = null
        if (callbacks != null) {
            val snap = callbacks.currentSnapshot()
            _state.update { cur ->
                val updatedNodes = cur.cloudComputer.nodes.map { node ->
                    if (node.id == "local-android-mika") node.copy(online = snap.available) else node
                }
                cur.copy(
                    mikaSnapshot = snap,
                    cloudComputer = cur.cloudComputer.copy(nodes = updatedNodes),
                )
            }
            val eventFlow = callbacks.channelEvents()
            if (eventFlow != null) {
                mikaEventsJob = viewModelScope.launch {
                    eventFlow.collect { event ->
                        applyMikaChannelCardEvent(event)
                    }
                }
            }
        }
    }

    private fun applyMikaChannelCardEvent(event: MikaChannelCardEvent) {
        val chatId = event.threadId.ifBlank { _state.value.activeChatId ?: return }
        val cardId = "mika-evt-${event.deliveryId}"
        val status = when (event.kind) {
            MikaChannelCardEvent.Kind.ASK_CONFIRM,
            MikaChannelCardEvent.Kind.ASK_INPUT,
            MikaChannelCardEvent.Kind.ASK_TAKEOVER -> "waiting_confirmation"
            MikaChannelCardEvent.Kind.RESULT -> "completed"
            else -> "running"
        }
        val card = TelegramMessageItem(
            id = cardId,
            conversationId = chatId,
            senderId = "mika-local-agent",
            senderName = "Mika 手机操控智能体",
            text = event.text,
            contentType = if (event.kind == MikaChannelCardEvent.Kind.IMAGE) "photo" else "mikaTask",
            outgoing = false,
            time = nowTimeLabel(),
            deliveryState = "sent",
            mikaTaskId = event.threadId,
            mikaTaskStatus = status,
            mikaStepLogs = listOf(event.text),
            mikaAskId = event.askId,
            mikaRequiresConfirmation = event.kind == MikaChannelCardEvent.Kind.ASK_CONFIRM,
            inlineKeyboard = if (event.inlineButtons.isNotEmpty()) listOf(InlineKeyboardRow(event.inlineButtons)) else emptyList(),
            mediaFileName = event.imageName,
            mediaSizeBytes = (event.imageSize ?: 0L).toInt().coerceAtLeast(0),
        )
        appendMessageToChat(chatId, card, clearDraft = false)
        refreshMikaSnapshot()
    }

    fun refreshMikaSnapshot() {
        val callbacks = mikaBridge ?: return
        val snap = callbacks.currentSnapshot()
        _state.update { cur ->
            val updatedNodes = cur.cloudComputer.nodes.map { node ->
                if (node.id == "local-android-mika") node.copy(online = snap.available) else node
            }
            cur.copy(
                mikaSnapshot = snap,
                cloudComputer = cur.cloudComputer.copy(nodes = updatedNodes),
            )
        }
    }

    fun clearToastBanner() {
        _state.update { it.copy(toastBanner = null) }
    }

    fun showToastBanner(message: String) {
        postBanner(message)
    }

    private fun postBanner(message: String) {
        _state.update { it.copy(toastBanner = message) }
    }

    private fun normalizeLanguageCode(raw: String?): String = when (raw?.trim()?.lowercase()) {
        "en", "en-us", "english" -> "en-US"
        "zh", "zh-cn", "简体中文", null, "" -> "zh-CN"
        else -> raw.trim()
    }

    private fun restorePersistedSettings() {
        // K-F24: Remove any legacy plaintext passcode stored in SharedPreferences immediately.
        if (prefs.contains("passcode_pin")) {
            prefs.edit().remove("passcode_pin").apply()
        }
        val passcodeEnabled = prefs.getBoolean("passcode_enabled", false)
        val presetWire = prefs.getString("theme_preset", TelegramThemePreset.APPLE_DAY.wire)
        val preset = TelegramThemePreset.entries.firstOrNull { it.wire == presetWire } ?: TelegramThemePreset.APPLE_DAY
        val wallpaperWire = prefs.getString("wallpaper_style", TelegramWallpaperStyle.SOLID_MINIMAL.wire)
        val wallpaper = TelegramWallpaperStyle.entries.firstOrNull { it.wire == wallpaperWire } ?: TelegramWallpaperStyle.SOLID_MINIMAL
        val savedEndpoint = prefs.getString("server_endpoint", DEFAULT_DESKTOP_SERVER_URL)
            ?.takeIf {
                it.isNotBlank() &&
                    it != "https://telegram-sync.ombhrum.com" &&
                    it != "https://telegram.fabushi.ombhrum.com"
            }
            ?: DEFAULT_DESKTOP_SERVER_URL
        val savedOffline = prefs.getBoolean("offline_local_mode", false)
        val savedLanguage = normalizeLanguageCode(prefs.getString("ui_language", "zh-CN"))
        val savedShowTranslate = prefs.getBoolean("show_translate_button", true)
        val savedAutoTranslate = prefs.getBoolean("auto_translate_channels", false)
        val savedPowerThreshold = prefs.getInt("power_saving_threshold", 20).coerceIn(5, 80)
        val savedInAppSounds = prefs.getBoolean("in_app_sounds_enabled", true)
        val actualCacheMb = measureLocalMediaCacheMb()

        _state.update { current ->
            current.copy(
                serverEndpoint = savedEndpoint,
                offlineLocalMode = savedOffline,
                uiLanguage = savedLanguage,
                showTranslateButton = savedShowTranslate,
                autoTranslateChannels = savedAutoTranslate,
                powerSavingThreshold = savedPowerThreshold,
                inAppSoundsEnabled = savedInAppSounds,
                storageCacheMb = actualCacheMb,
                passcodeConfigured = passcodeEnabled,
                isLocked = passcodeEnabled,
                themeSettings = current.themeSettings.copy(
                    preset = preset,
                    customAccentArgb = prefs.getLong("custom_accent", 0xFF007AFFL),
                    customDark = prefs.getBoolean("custom_dark", false),
                    bubbleCornerRadiusDp = prefs.getInt("bubble_radius", 18).coerceIn(4, 18),
                    messageFontSizeSp = prefs.getInt("font_size", 15).coerceIn(12, 22),
                    wallpaperStyle = wallpaper,
                    wallpaperDimPercent = prefs.getInt("wallpaper_dim", 0).coerceIn(0, 80),
                ),
            )
        }
    }

    fun measureLocalMediaCacheMb(): Double {
        val root = runCatching { appContext.cacheDir }.getOrNull() ?: return 0.0
        if (!root.exists()) return 0.0
        var totalBytes = 0L
        runCatching {
            root.walkTopDown().forEach { f ->
                if (f.isFile) totalBytes += f.length()
            }
        }
        return totalBytes.toDouble() / (1024.0 * 1024.0)
    }

    fun refreshAllFromHost() {
        val capturedGen = generationFence.currentGeneration
        _state.update { it.copy(syncStatusLabel = "同步中…") }
        viewModelScope.launch {
            var completedUpdate = false
            try {
                withContext(Dispatchers.IO) {
                    val prefEndpoint = prefs.getString("server_endpoint", null)
                        ?.takeIf {
                            it.isNotBlank() &&
                                it != "https://telegram-sync.ombhrum.com" &&
                                it != "https://telegram.fabushi.ombhrum.com"
                        }
                    if (prefEndpoint != null) {
                        hostRequestResult(
                            "telegram.server.configureEndpoint",
                            JSONObject().put("endpoint", prefEndpoint),
                        )
                    }

                    val serverSession = hostRequestResult("telegram.server.session").getOrNull()
                    val hasServerToken = serverSession?.optBoolean("authenticated", false) == true
                    val sessionToken = serverSession?.optNonNullString("token").orEmpty()
                    val sessionEndpoint = prefEndpoint
                        ?: serverSession?.optNonNullString("endpoint")?.ifBlank { null }
                        ?: _state.value.serverEndpoint

                    var liveAuthenticated = false
                    var liveUserId: Long? = null
                    var liveUsername: String? = null
                    var liveFirstName: String? = null
                    var liveBio = ""
                    var livePhone = ""
                    var unauthorizedReason: String? = null
                    var serverChatsParsed: List<TelegramChatItem>? = null
                    var serverFoldersParsed: List<TelegramFolderTab>? = null
                    var serverContactsParsed: List<ServerContactItem>? = null
                    var serverSessionsParsed: List<ServerSessionItem>? = null
                    var serverBotsParsed: List<FabushiBotItem>? = null
                    var serverComputersParsed: List<ComputerNodeItem>? = null
                    var serverAssistResp: JSONObject? = null
                    var serverRemoteSessionsParsed: List<RemoteSessionItem>? = null
                    var serverMcpGrantsResp: JSONObject? = null

                    var capturedProfileEpoch = profileEpoch.get()
                    var capturedChatsEpoch = chatsEpoch.get()
                    var capturedFoldersEpoch = foldersEpoch.get()
                    var capturedContactsEpoch = contactsEpoch.get()
                    var capturedSessionsEpoch = sessionsEpoch.get()
                    var capturedBotsEpoch = botsEpoch.get()
                    var capturedComputersEpoch = computersEpoch.get()
                    var capturedAssistEpoch = assistEpoch.get()
                    var capturedRemoteSessionsEpoch = remoteSessionsEpoch.get()
                    var capturedMcpGrantsEpoch = mcpGrantsEpoch.get()

                    if (hasServerToken && sessionToken.isNotBlank()) {
                        capturedProfileEpoch = profileEpoch.get()
                        val meResult = hostRequestResult("telegram.server.me")
                        val meResp = meResult.getOrNull()
                        val meErr = meResult.exceptionOrNull()
                        if (meResp != null) {
                            liveAuthenticated = true
                            liveUserId = meResp.optLong("user_id").takeIf { it > 0L }
                                ?: meResp.optLong("id").takeIf { it > 0L }
                                ?: serverSession?.optLong("userId")?.takeIf { it > 0L }
                            liveUsername = meResp.optNullableString("username")
                                ?: serverSession?.optNullableString("username")
                            liveFirstName = meResp.optNullableString("first_name")
                                ?: serverSession?.optNullableString("firstName")
                            liveBio = meResp.optNonNullString("bio", "")
                            livePhone = meResp.optNonNullString("phone", "")

                            capturedChatsEpoch = chatsEpoch.get()
                            val chatsResult = hostRequestResult("telegram.server.listChats")
                            if (isUnauthorizedError(chatsResult.exceptionOrNull())) {
                                unauthorizedReason = chatsResult.exceptionOrNull()?.message ?: "HTTP 401"
                                liveAuthenticated = false
                            } else {
                                chatsResult.getOrNull()?.let { serverChatsParsed = parseServerChats(it) }
                            }

                            if (liveAuthenticated) {
                                capturedFoldersEpoch = foldersEpoch.get()
                                hostRequestResult("telegram.server.listFolders").getOrNull()?.let {
                                    serverFoldersParsed = parseServerFolders(it)
                                }
                                capturedContactsEpoch = contactsEpoch.get()
                                hostRequestResult("telegram.server.listContacts").getOrNull()?.let {
                                    serverContactsParsed = parseServerContacts(it)
                                }
                                capturedSessionsEpoch = sessionsEpoch.get()
                                hostRequestResult("telegram.server.listSessions").getOrNull()?.let {
                                    serverSessionsParsed = parseServerSessions(it)
                                }
                                capturedBotsEpoch = botsEpoch.get()
                                val myBotsResp = hostRequestResult("telegram.server.listMyBots").getOrNull()
                                val fabushiBotsResp = hostRequestResult("telegram.server.listFabushiBots").getOrNull()
                                if (myBotsResp != null || fabushiBotsResp != null) {
                                    serverBotsParsed = mergeServerBots(myBotsResp, fabushiBotsResp)
                                }
                                capturedComputersEpoch = computersEpoch.get()
                                hostRequestResult("telegram.server.listComputers").getOrNull()?.let {
                                    serverComputersParsed = parseServerComputers(it)
                                }
                                capturedAssistEpoch = assistEpoch.get()
                                serverAssistResp = hostRequestResult("telegram.server.assistInfo").getOrNull()
                                capturedRemoteSessionsEpoch = remoteSessionsEpoch.get()
                                hostRequestResult("telegram.server.listRemoteSessions").getOrNull()?.let {
                                    serverRemoteSessionsParsed = parseRemoteSessions(it)
                                }
                                capturedMcpGrantsEpoch = mcpGrantsEpoch.get()
                                serverMcpGrantsResp = hostRequestResult("telegram.server.listMcpGrants").getOrNull()
                            }
                        } else if (isUnauthorizedError(meErr)) {
                            unauthorizedReason = meErr?.message ?: "HTTP 401: Unauthorized"
                            liveAuthenticated = false
                        } else {
                            // Offline or transient network error: keep stored identity without fabricating chats
                            liveAuthenticated = true
                            liveUserId = serverSession?.optLong("userId")?.takeIf { it > 0L }
                            liveUsername = serverSession?.optNullableString("username")
                            liveFirstName = serverSession?.optNullableString("firstName")
                        }
                    }

                    if (!generationFence.isCurrent(capturedGen)) return@withContext

                    if (unauthorizedReason != null) {
                        handleUnauthorizedSession(unauthorizedReason, capturedGen)
                        completedUpdate = true
                        return@withContext
                    }

                    val access = hostRequestResult(
                        "feature.messaging.access.issue",
                        JSONObject()
                            .put("deviceId", "android:native")
                            .put("sessionId", "account-session:android-native")
                            .put("scopes", JSONArray(listOf("messaging", "calls", "blobsRead", "blobsWrite", "payments", "miniApps"))),
                    ).getOrNull()
                    access?.optNonNullString("actorId")?.takeIf { it.isNotBlank() }?.let { actorId = it }

                    val accountsResp = hostRequestResult("telegram.auth.accountsList").getOrNull()
                    accountsResp?.optLong("generation")?.takeIf { it > 0L }?.let { rustAccountGeneration.set(it) }
                    val walletResp = hostRequestResult("wallet.status").getOrNull()
                    val walletCollectiblesResp = hostRequestResult("wallet.collectibles").getOrNull()
                    val dharmaStatusResp = hostRequestResult("fabushiMiniapp.status").getOrNull()
                    val dharmaBootstrapResp = hostRequestResult("fabushiMiniapp.bootstrap").getOrNull()
                    val dharmaCountriesResp = hostRequestResult("fabushiMiniapp.countries").getOrNull()
                    val magpieProvidersResp = hostRequestResult("magpie.providers").getOrNull()
                    val hermesSessionsResp = hostRequestResult("hermes.sessions").getOrNull()
                    val hermesMemoryResp = hostRequestResult("hermes.memoryGet").getOrNull()
                    val hermesSkillsResp = hostRequestResult("hermes.skillsList").getOrNull()
                    val hermesCronResp = hostRequestResult("hermes.cronList").getOrNull()
                    val hermesKanbanResp = hostRequestResult("hermes.kanbanBoard").getOrNull()
                    val hermesApprovalsResp = hostRequestResult("hermes.approvalsList").getOrNull()
                    val streamResp = hostRequestResult("remote.getStreamSettings").getOrNull()
                    val actionBusCatsResp = hostRequestResult("remote.actionBusCategories").getOrNull()
                    val localNodesResp = hostRequestResult("computer.listNodes").getOrNull()
                    val mikaResp = hostRequestResult("mika.bridgeStatus").getOrNull()
                    val vaultUid = liveUserId?.let { "user:$it" } ?: _state.value.activeAccountId.ifBlank { "local" }
                    val vaultEnvResp = hostRequestResult(
                        "userVault.getEnvelope",
                        JSONObject().put("userId", vaultUid),
                    ).getOrNull()

                    if (!generationFence.isCurrent(capturedGen)) return@withContext
                    currentServerUserId = liveUserId

                    if (liveAuthenticated && sessionToken.isNotBlank()) {
                        connectServerWebSocket(sessionEndpoint, sessionToken, capturedGen)
                    } else {
                        disconnectServerWebSocket()
                    }

                    _state.update { prev ->
                        val displayName = liveFirstName?.ifBlank { null } ?: liveUsername?.ifBlank { null }
                        val resolvedAccount = if (liveAuthenticated && !displayName.isNullOrBlank()) {
                            TelegramAccount(
                                id = liveUserId?.let { "user:$it" } ?: "user:${liveUsername ?: "me"}",
                                slotIndex = 0,
                                displayName = displayName,
                                username = liveUsername?.let { if (it.startsWith("@")) it else "@$it" } ?: "",
                                phone = livePhone,
                                bio = liveBio,
                                isActive = true,
                                isPremium = false,
                                statusText = "",
                            )
                        } else null

                        val persistedAccounts = accountsResp?.optJSONArray("accounts")?.let { arr ->
                            buildList {
                                for (i in 0 until arr.length()) {
                                    val a = arr.optJSONObject(i) ?: continue
                                    val aid = a.optNonNullString("accountId").ifBlank { continue }
                                    val uname = a.optNonNullString("username").trim().let {
                                        if (it.isBlank()) "" else if (it.startsWith("@")) it else "@$it"
                                    }
                                    add(
                                        TelegramAccount(
                                            id = aid,
                                            slotIndex = i,
                                            displayName = a.optNonNullString("displayName").ifBlank { uname.trimStart('@').ifBlank { aid } },
                                            username = uname,
                                            phone = a.optNonNullString("phone", ""),
                                            bio = "",
                                            isActive = a.optBoolean("active", false),
                                            isPremium = false,
                                            statusText = "",
                                        ),
                                    )
                                }
                            }
                        }.orEmpty()

                        val resolvedAccounts = when {
                            resolvedAccount != null -> {
                                val extras = persistedAccounts.filterNot {
                                    it.id == resolvedAccount.id ||
                                        (resolvedAccount.username.isNotBlank() && it.username.equals(resolvedAccount.username, ignoreCase = true))
                                }
                                listOf(resolvedAccount) + extras.mapIndexed { idx, acc -> acc.copy(slotIndex = idx + 1, isActive = false) }
                            }
                            persistedAccounts.isNotEmpty() -> persistedAccounts
                            else -> emptyList()
                        }
                        val resolvedActiveAccountId = resolvedAccount?.id
                            ?: persistedAccounts.firstOrNull { it.isActive }?.id
                            ?: persistedAccounts.firstOrNull()?.id
                            ?: ""

                        val applyProfile = profileEpoch.get() == capturedProfileEpoch ||
                            prev.accounts.isEmpty() ||
                            prev.isAuthenticated != liveAuthenticated
                        var next = prev.copy(
                            isAuthenticated = liveAuthenticated,
                            serverEndpoint = sessionEndpoint,
                            syncStatusLabel = if (liveAuthenticated) "信息" else "信息（离线）",
                            accounts = if (applyProfile) resolvedAccounts else prev.accounts,
                            activeAccountId = if (applyProfile) resolvedActiveAccountId else prev.activeAccountId,
                            storageCacheMb = measureLocalMediaCacheMb(),
                        )

                        if (serverChatsParsed != null) {
                            if (chatsEpoch.get() == capturedChatsEpoch) {
                                val merged = mergeServerChats(next.chats, serverChatsParsed!!)
                                next = next.copy(chats = sortTelegramChats(merged))
                            }
                        } else if (!liveAuthenticated && !prev.offlineLocalMode) {
                            next = next.copy(chats = emptyList(), messagesByChat = emptyMap())
                        }

                        if (serverFoldersParsed != null && foldersEpoch.get() == capturedFoldersEpoch) {
                            next = next.copy(
                                folderTabs = TelegramFolderFilter.DEFAULT_BUILT_IN_TABS + serverFoldersParsed!!,
                            )
                        }
                        if (serverContactsParsed != null && contactsEpoch.get() == capturedContactsEpoch) {
                            next = next.copy(serverContacts = serverContactsParsed!!)
                        }
                        if (serverSessionsParsed != null && sessionsEpoch.get() == capturedSessionsEpoch) {
                            next = next.copy(serverSessions = serverSessionsParsed!!)
                        }
                        if (serverBotsParsed != null && botsEpoch.get() == capturedBotsEpoch) {
                            val codingFromBots = serverBotsParsed!!.mapNotNull { it.codingAgent }
                            next = next.copy(
                                fabushiBots = serverBotsParsed!!,
                                codingAgentBots = codingFromBots,
                                chats = syncFabushiBotsToChats(next.chats, serverBotsParsed!!),
                            )
                        } else if (next.fabushiBots.isNotEmpty() && next.chats.isNotEmpty()) {
                            next = next.copy(
                                chats = syncFabushiBotsToChats(next.chats, next.fabushiBots),
                            )
                        }

                        // Contract #4: wallet.status -> hasWallet, balanceTon (String), address, starsBalance
                        if (walletResp != null) {
                            val hasWallet = walletResp.optBoolean("hasWallet", false)
                            val balanceTonStr = walletResp.optNonNullString("balanceTon", "")
                            val balanceTon = balanceTonStr.toDoubleOrNull()
                                ?: (walletResp.optLong("balanceNanotons", 0L).toDouble() / 1_000_000_000.0)
                            val collectibles = parseWalletCollectibles(walletCollectiblesResp)
                            next = next.copy(
                                tonWallet = next.tonWallet.copy(
                                    initialized = hasWallet,
                                    addressFriendly = if (hasWallet) walletResp.optNonNullString("address", next.tonWallet.addressFriendly) else "",
                                    tonBalance = if (hasWallet) balanceTon else 0.0,
                                    starsBalance = walletResp.optLong("starsBalance", 0L),
                                    collectibles = collectibles,
                                ),
                            )
                        }

                        // Contract #12: fabushiMiniapp.{bootstrap,status,countries}
                        if (dharmaBootstrapResp != null || dharmaStatusResp != null || dharmaCountriesResp != null) {
                            val profile = dharmaBootstrapResp?.optJSONObject("profile")
                            val dharmaState = dharmaStatusResp?.optJSONObject("dharmaState")
                            val parsedCountries = parseDharmaCountries(dharmaCountriesResp)
                            val totalOfferings = profile?.optLong("total_transfer_count", -1L)?.takeIf { it >= 0L }
                                ?: dharmaState?.optLong("totalOfferings", next.globalDharma.totalGlobalOfferings)
                                ?: next.globalDharma.totalGlobalOfferings
                            val meritPoints = dharmaState?.optLong("meritPoints", next.globalDharma.myMeritPoints)
                                ?: next.globalDharma.myMeritPoints
                            val wheelSpins = dharmaState?.optLong("wheelRotations", next.globalDharma.prayerWheelSpins)
                                ?: next.globalDharma.prayerWheelSpins
                            val practiceMins = profile?.optInt("total_practice_minutes", -1)?.takeIf { it >= 0 }
                                ?: dharmaState?.optInt("totalPracticeMinutes", next.globalDharma.practiceMinutesTotal)
                                ?: next.globalDharma.practiceMinutesTotal
                            next = next.copy(
                                globalDharma = next.globalDharma.copy(
                                    countries = if (parsedCountries.isNotEmpty()) parsedCountries else next.globalDharma.countries,
                                    totalGlobalOfferings = totalOfferings,
                                    myMeritPoints = meritPoints,
                                    prayerWheelSpins = wheelSpins,
                                    practiceMinutesTotal = practiceMins,
                                ),
                            )
                        }

                        // Contract #2: magpie.providers hydration
                        if (magpieProvidersResp != null) {
                            next = next.copy(magpieGateway = applyMagpieProvidersResponse(next.magpieGateway, magpieProvidersResp))
                        }

                        // Hermes hydration (zero fake defaults)
                        if (hermesSessionsResp != null || hermesMemoryResp != null || hermesSkillsResp != null ||
                            hermesCronResp != null || hermesKanbanResp != null || hermesApprovalsResp != null
                        ) {
                            next = next.copy(
                                hermesStudio = parseHermesStudioState(
                                    current = next.hermesStudio,
                                    sessionsResp = hermesSessionsResp,
                                    memoryResp = hermesMemoryResp,
                                    skillsResp = hermesSkillsResp,
                                    cronResp = hermesCronResp,
                                    kanbanResp = hermesKanbanResp,
                                    approvalsResp = hermesApprovalsResp,
                                ),
                            )
                        }

                        // Mika status (never lie available=true when bridge is absent)
                        val mikaAvailable = mikaBridge?.currentSnapshot()?.available
                            ?: mikaResp?.optBoolean("available", false)
                            ?: false
                        if (mikaBridge == null) {
                            next = next.copy(mikaSnapshot = next.mikaSnapshot.copy(available = mikaAvailable))
                        }

                        // Contract #16: Computers (real server computers + genuine local-android-mika)
                        var nextComputer = next.cloudComputer
                        if (computersEpoch.get() == capturedComputersEpoch) {
                            val mergedNodes = mergeComputerNodes(
                                localNodesResp = localNodesResp,
                                serverNodes = serverComputersParsed,
                                localMikaOnline = mikaAvailable,
                            )
                            nextComputer = nextComputer.copy(
                                nodes = mergedNodes,
                                selectedNodeId = nextComputer.selectedNodeId.takeIf { sel ->
                                    sel.isNotBlank() && mergedNodes.any { it.id == sel }
                                } ?: mergedNodes.firstOrNull()?.id.orEmpty(),
                            )
                        }
                        if (serverAssistResp != null && serverAssistResp.has("assist_id") && assistEpoch.get() == capturedAssistEpoch) {
                            nextComputer = nextComputer.copy(
                                assistConfig = RemoteAssistConfigUi(
                                    assistId = serverAssistResp.optNonNullString("assist_id", ""),
                                    allowAssist = serverAssistResp.optBoolean("allow_assist", false),
                                    codeMode = serverAssistResp.optNonNullString("code_mode", "temporary"),
                                    currentCode = serverAssistResp.optNonNullString("current_code", ""),
                                ),
                            )
                        }
                        if (streamResp != null && streamResp.has("codec")) {
                            nextComputer = nextComputer.copy(
                                streamSettings = RemoteStreamSettingsUi(
                                    codec = streamResp.optNonNullString("codec", "auto"),
                                    targetFps = streamResp.optInt("target_fps", 60),
                                    quality = streamResp.optNonNullString("quality", "auto"),
                                    colorMode = streamResp.optNonNullString("color_mode", "yuv420"),
                                    hdr = streamResp.optBoolean("hdr", false),
                                    audioEnabled = streamResp.optBoolean("audio_enabled", true),
                                    micPassthrough = streamResp.optBoolean("mic_passthrough", false),
                                    privacyScreen = streamResp.optBoolean("privacy_screen", false),
                                    displayIndex = streamResp.optInt("display_index", 0),
                                ),
                            )
                        }
                        if (actionBusCatsResp != null) {
                            nextComputer = nextComputer.copy(
                                actionBusCategoryCount = actionBusCatsResp.optInt("total", nextComputer.actionBusCategoryCount),
                            )
                        }
                        if (serverRemoteSessionsParsed != null && remoteSessionsEpoch.get() == capturedRemoteSessionsEpoch) {
                            nextComputer = nextComputer.copy(remoteSessions = serverRemoteSessionsParsed!!)
                        }
                        if (serverMcpGrantsResp != null && mcpGrantsEpoch.get() == capturedMcpGrantsEpoch) {
                            nextComputer = nextComputer.copy(mcpGrants = parseMcpGrants(serverMcpGrantsResp))
                        }
                        next = next.copy(cloudComputer = nextComputer)

                        val envObj = vaultEnvResp?.optJSONObject("envelope")
                        if (envObj != null) {
                            next = next.copy(
                                userVault = next.userVault.copy(
                                    initialized = true,
                                    kdfAlg = envObj.optNonNullString("kdf_alg", "split_key_v2"),
                                    keyVersion = envObj.optInt("key_version", 2),
                                ),
                            )
                        }
                        next
                    }
                    completedUpdate = true
                }
            } finally {
                if (!completedUpdate && generationFence.isCurrent(capturedGen)) {
                    _state.update { prev ->
                        if (prev.syncStatusLabel == "同步中…") {
                            prev.copy(syncStatusLabel = if (prev.isAuthenticated) "信息" else "信息（离线）")
                        } else prev
                    }
                }
            }
        }
    }

    /**
     * Source-compatibility bridge retained for any caller passing [MessagingUiState].
     * Duplicate MessagingViewModel has been retired from MainActivity (K-F26).
     */
    fun syncExternalMessagingState(external: MessagingUiState) {
        if (external.conversations.isEmpty()) return
        _state.update { current ->
            val mergedChats = current.chats.toMutableList()
            external.conversations.forEach { summary ->
                if (mergedChats.none { it.id == summary.id }) {
                    mergedChats.add(
                        TelegramChatItem(
                            id = summary.id,
                            title = summary.title,
                            subtitle = summary.description.ifBlank { summary.kind.label },
                            preview = summary.preview,
                            time = summary.time,
                            kind = summary.kind,
                            unreadCount = summary.unreadCount,
                            isPinned = summary.isPinned,
                            isMuted = summary.isMuted,
                            isArchived = summary.isArchived,
                            markedUnread = summary.markedUnread,
                            isSecretChat = summary.kind == ConversationKind.SECRET,
                            pinnedMessageIds = summary.pinnedMessageIds,
                            folderIds = summary.folderIds,
                            rawSummary = summary,
                        ),
                    )
                }
            }
            current.copy(chats = sortTelegramChats(mergedChats))
        }
    }

    // ------------------------------------------------------------------------
    // Navigation, Multi-Account & Rust Argon2id Passcode Gate (Contract #8, #9)
    // ------------------------------------------------------------------------

    fun navigateTo(screen: TelegramSubScreen, settingsPage: TelegramSettingsPage = TelegramSettingsPage.OVERVIEW) {
        if (screen == TelegramSubScreen.MIKA_WORKSPACE || screen == TelegramSubScreen.CLOUD_COMPUTER) refreshMikaSnapshot()
        if (screen == TelegramSubScreen.CONTACTS && _state.value.isAuthenticated) refreshContacts()
        if (screen == TelegramSubScreen.BOTFATHER && _state.value.isAuthenticated) refreshServerBots()
        if (screen == TelegramSubScreen.CLOUD_COMPUTER && _state.value.isAuthenticated) {
            refreshComputers()
            refreshRemoteAssist()
            refreshRemoteSessions()
            refreshMcpGrants()
        }
        if (screen == TelegramSubScreen.SETTINGS) {
            if (_state.value.isAuthenticated) refreshSessions()
            val measuredCache = measureLocalMediaCacheMb()
            _state.update { it.copy(storageCacheMb = measuredCache) }
        }
        _state.update {
            it.copy(
                subScreen = screen,
                settingsPage = settingsPage,
                selectedChatIds = emptySet(),
                composerAttachmentSheetOpen = false,
            )
        }
    }

    fun openSettingsPage(page: TelegramSettingsPage) {
        if (page == TelegramSettingsPage.DEVICES_SESSIONS && _state.value.isAuthenticated) {
            refreshSessions()
        }
        if (page == TelegramSettingsPage.DATA_STORAGE || page == TelegramSettingsPage.OVERVIEW) {
            val measuredCache = measureLocalMediaCacheMb()
            _state.update { it.copy(storageCacheMb = measuredCache) }
        }
        _state.update { it.copy(subScreen = TelegramSubScreen.SETTINGS, settingsPage = page) }
    }

    fun setComposerAttachmentSheetOpen(open: Boolean) {
        _state.update { it.copy(composerAttachmentSheetOpen = open) }
    }

    fun switchAccount(accountId: String) {
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                val expectedGen = rustAccountGeneration.get()
                hostRequestResult(
                    "telegram.auth.accountSwitch",
                    JSONObject()
                        .put("accountId", accountId)
                        .put("expectedGeneration", expectedGen),
                )
            }
            if (result.isFailure) {
                postBanner("切换账号失败：${result.exceptionOrNull()?.message ?: "状态已过期"}")
                return@launch
            }
            val resp = result.getOrNull()
            rustAccountGeneration.set(resp?.optLong("generation", rustAccountGeneration.get() + 1L) ?: (rustAccountGeneration.get() + 1L))
            generationFence.advance()
            _state.update { current ->
                val target = current.accounts.firstOrNull { it.id == accountId }
                val label = target?.let { "${it.displayName} (${it.username})" } ?: accountId
                current.copy(
                    activeAccountId = accountId,
                    accounts = current.accounts.map { it.copy(isActive = it.id == accountId) },
                    toastBanner = "已切换至账号：$label",
                )
            }
        }
    }

    fun addAccount(displayName: String, phone: String, username: String) {
        val cleanName = displayName.trim()
        val cleanUser = username.trim().let { if (it.isBlank()) "" else if (it.startsWith("@")) it else "@$it" }
        val cleanPhone = phone.trim() // K-F18: NEVER fabricate "+86 138 0000 100x"
        if (cleanName.isBlank() && cleanUser.isBlank()) {
            postBanner("请输入有效的账号名称或用户名")
            return
        }
        if (_state.value.accounts.size >= 4) {
            postBanner("Telegram Android 最多支持同时登录 4 个账号")
            return
        }
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                val addRes = hostRequestResult(
                    "telegram.auth.accountAdd",
                    JSONObject()
                        .put("displayName", cleanName.ifBlank { cleanUser.trimStart('@') })
                        .put("phone", cleanPhone)
                        .put("username", cleanUser),
                )
                if (addRes.isSuccess) {
                    hostRequestResult("telegram.auth.accountsList").getOrNull()
                        ?.optLong("generation")
                        ?.takeIf { it > 0L }
                        ?.let { rustAccountGeneration.set(it) }
                }
                addRes
            }
            result.onSuccess { resp ->
                val accId = resp.optNonNullString("accountId").ifBlank { "acc-${UUID.randomUUID().toString().take(8)}" }
                generationFence.advance()
                _state.update { current ->
                    val newAcc = TelegramAccount(
                        id = accId,
                        slotIndex = current.accounts.size,
                        displayName = cleanName.ifBlank { cleanUser.trimStart('@') },
                        username = cleanUser,
                        phone = cleanPhone,
                        bio = "",
                        isActive = true,
                        isPremium = false,
                        statusText = "",
                    )
                    current.copy(
                        activeAccountId = newAcc.id,
                        accounts = current.accounts.map { it.copy(isActive = false) } + newAcc,
                        toastBanner = "已添加账号 ${newAcc.displayName}",
                    )
                }
            }.onFailure { err ->
                postBanner("添加账号失败：${err.message ?: "请先登录服务器账号"}")
            }
        }
    }

    fun configurePasscode(passcode: String) {
        val clean = passcode.trim()
        if (clean.length < 4) {
            postBanner("密码锁至少需要 4 位字符")
            return
        }
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                hostRequestResult(
                    "telegram.auth.passcodeSet",
                    JSONObject().put("passcode", clean),
                )
            }
            result.onSuccess {
                // K-F24: Never store plaintext PIN in SharedPreferences; store only non-secret boolean flag
                prefs.edit()
                    .remove("passcode_pin")
                    .putBoolean("passcode_enabled", true)
                    .apply()
                _state.update {
                    it.copy(
                        passcodeConfigured = true,
                        isLocked = false,
                        passcodeError = null,
                        toastBanner = "本地密码锁已通过 Argon2id 安全保存",
                    )
                }
            }.onFailure { err ->
                postBanner("设置密码锁失败：${err.message ?: "未知错误"}")
            }
        }
    }

    fun disablePasscode() {
        prefs.edit()
            .remove("passcode_pin")
            .putBoolean("passcode_enabled", false)
            .apply()
        _state.update {
            it.copy(
                passcodeConfigured = false,
                isLocked = false,
                passcodeError = null,
                toastBanner = "本地密码锁已关闭",
            )
        }
    }

    fun lockAppNow() {
        if (!_state.value.passcodeConfigured) {
            // Never auto-set "0000"
            postBanner("请先在隐私与安全设置中配置密码锁")
        } else {
            _state.update { it.copy(isLocked = true, passcodeError = null) }
        }
    }

    fun verifyPasscode(passcodeInput: String): Boolean {
        val result = hostRequestResult(
            "telegram.auth.passcodeVerify",
            JSONObject().put("passcode", passcodeInput),
        )
        val ok = result.getOrNull()?.optBoolean("valid", false) == true
        _state.update {
            if (ok) it.copy(isLocked = false, passcodeError = null)
            else it.copy(passcodeError = result.exceptionOrNull()?.message ?: "密码不正确，请重试")
        }
        return ok
    }

    fun unlockViaBiometricSuccess() {
        _state.update { it.copy(isLocked = false, passcodeError = null) }
    }

    // ------------------------------------------------------------------------
    // Theme, Appearance, Cache, Profile, Sessions & Persisted Settings
    // ------------------------------------------------------------------------

    fun updateThemeSettings(transform: (TelegramThemeSettings) -> TelegramThemeSettings) {
        _state.update { current ->
            val updated = transform(current.themeSettings).let { raw ->
                raw.copy(
                    bubbleCornerRadiusDp = raw.bubbleCornerRadiusDp.coerceIn(4, 18),
                    messageFontSizeSp = raw.messageFontSizeSp.coerceIn(12, 22),
                    wallpaperDimPercent = raw.wallpaperDimPercent.coerceIn(0, 80),
                )
            }
            prefs.edit()
                .putString("theme_preset", updated.preset.wire)
                .putLong("custom_accent", updated.customAccentArgb)
                .putBoolean("custom_dark", updated.customDark)
                .putInt("bubble_radius", updated.bubbleCornerRadiusDp)
                .putInt("font_size", updated.messageFontSizeSp)
                .putString("wallpaper_style", updated.wallpaperStyle.wire)
                .putInt("wallpaper_dim", updated.wallpaperDimPercent)
                .apply()
            current.copy(themeSettings = updated)
        }
    }

    fun toggleQuickNightMode() {
        updateThemeSettings { current ->
            val isApple = current.preset in setOf(
                TelegramThemePreset.APPLE_DAY,
                TelegramThemePreset.APPLE_NIGHT,
                TelegramThemePreset.APPLE_TINTED,
            )
            val nextPreset = when {
                isApple && current.isDarkEffective -> TelegramThemePreset.APPLE_DAY
                isApple -> TelegramThemePreset.APPLE_NIGHT
                current.isDarkEffective -> TelegramThemePreset.DAY_BLUE
                else -> TelegramThemePreset.NIGHT
            }
            current.copy(preset = nextPreset, customDark = nextPreset.isDark)
        }
    }

    fun configureServerEndpoint(endpoint: String, proxyEnabled: Boolean, proxyHost: String) {
        val cleanEndpoint = endpoint.trim().trimEnd('/').ifBlank { DEFAULT_DESKTOP_SERVER_URL }
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                hostRequestResult(
                    "telegram.server.configureEndpoint",
                    JSONObject()
                        .put("endpoint", cleanEndpoint)
                        .put("proxyEnabled", proxyEnabled)
                        .put("proxyHost", proxyHost),
                )
            }
            result.onSuccess {
                prefs.edit().putString("server_endpoint", cleanEndpoint).apply()
                _state.update {
                    it.copy(
                        serverEndpoint = cleanEndpoint,
                        proxyEnabled = proxyEnabled,
                        proxyHost = proxyHost,
                        toastBanner = "服务器同步节点已更新：$cleanEndpoint",
                    )
                }
            }.onFailure { err ->
                postBanner("更新服务器节点失败：${err.message ?: "无效地址"}")
            }
        }
    }

    fun clearLocalMediaCache() {
        cachedVoiceBlobs.clear()
        runCatching {
            val cacheRoot = appContext.cacheDir
            cacheRoot?.listFiles()?.forEach { child ->
                child.deleteRecursively()
            }
        }
        val remainingMb = measureLocalMediaCacheMb()
        _state.update {
            it.copy(
                storageCacheMb = remainingMb,
                toastBanner = "媒体与音频缓存已清理释放",
            )
        }
    }

    fun updateProfile(displayName: String, username: String, bio: String) {
        val cleanName = displayName.trim()
        val cleanUsernameRaw = username.trim().trimStart('@')
        val cleanBio = bio.trim()
        val cleanUserFormatted = if (cleanUsernameRaw.isBlank()) "" else "@$cleanUsernameRaw"
        profileEpoch.incrementAndGet()
        var prevActiveAccount: TelegramAccount? = null

        _state.update { current ->
            prevActiveAccount = current.accounts.firstOrNull { it.id == current.activeAccountId }
            current.copy(
                accounts = current.accounts.map { acc ->
                    if (acc.id == current.activeAccountId) {
                        acc.copy(
                            displayName = cleanName.ifBlank { acc.displayName },
                            username = cleanUserFormatted.ifBlank { acc.username },
                            bio = cleanBio,
                        )
                    } else acc
                },
            )
        }

        if (_state.value.isAuthenticated) {
            val capturedGen = generationFence.currentGeneration
            viewModelScope.launch {
                val payload = JSONObject()
                if (cleanName.isNotBlank()) payload.put("firstName", cleanName)
                if (cleanUsernameRaw.isNotBlank()) payload.put("username", cleanUsernameRaw)
                payload.put("bio", cleanBio)
                val result = withContext(Dispatchers.IO) {
                    hostRequestResult("telegram.server.updateProfile", payload)
                }
                if (!generationFence.isCurrent(capturedGen)) return@launch
                result.onSuccess { resp ->
                    val srvFirst = resp.optNonNullString("first_name").ifBlank { cleanName }
                    val srvUser = resp.optNonNullString("username").ifBlank { cleanUsernameRaw }
                    val srvBio = resp.optNonNullString("bio", cleanBio)
                    _state.update { current ->
                        current.copy(
                            accounts = current.accounts.map { acc ->
                                if (acc.id == current.activeAccountId) {
                                    acc.copy(
                                        displayName = srvFirst.ifBlank { acc.displayName },
                                        username = if (srvUser.isNotBlank()) "@${srvUser.trimStart('@')}" else acc.username,
                                        bio = srvBio,
                                    )
                                } else acc
                            },
                            toastBanner = "个人资料已保存同步",
                        )
                    }
                }.onFailure { err ->
                    if (isUnauthorizedError(err)) {
                        handleUnauthorizedSession(err.message ?: "HTTP 401", capturedGen)
                    } else {
                        val rollback = prevActiveAccount
                        _state.update { current ->
                            val restored = if (rollback != null) {
                                current.accounts.map { acc -> if (acc.id == rollback.id) rollback else acc }
                            } else current.accounts
                            current.copy(accounts = restored, toastBanner = "更新个人资料失败：${err.message}")
                        }
                    }
                }
            }
        } else {
            postBanner("个人资料已在本地更新")
        }
    }

    fun refreshSessions() {
        if (!_state.value.isAuthenticated) return
        val capturedGen = generationFence.currentGeneration
        val capturedEpoch = sessionsEpoch.get()
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                hostRequestResult("telegram.server.listSessions")
            }
            if (!generationFence.isCurrent(capturedGen)) return@launch
            result.onSuccess { resp ->
                val parsed = parseServerSessions(resp)
                _state.update { cur ->
                    if (sessionsEpoch.get() != capturedEpoch) cur else cur.copy(serverSessions = parsed)
                }
            }.onFailure { err ->
                if (isUnauthorizedError(err)) {
                    handleUnauthorizedSession(err.message ?: "HTTP 401", capturedGen)
                } else {
                    postBanner("获取登录设备列表失败：${err.message}")
                }
            }
        }
    }

    fun revokeSession(sessionId: String) {
        if (sessionId.isBlank() || !_state.value.isAuthenticated) return
        val capturedGen = generationFence.currentGeneration
        sessionsEpoch.incrementAndGet()
        var removedItem: ServerSessionItem? = null
        _state.update { cur ->
            removedItem = cur.serverSessions.firstOrNull { it.sessionId == sessionId }
            cur.copy(serverSessions = cur.serverSessions.filterNot { s -> s.sessionId == sessionId })
        }
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                hostRequestResult("telegram.server.revokeSession", JSONObject().put("sessionId", sessionId))
            }
            if (!generationFence.isCurrent(capturedGen)) return@launch
            result.onSuccess {
                postBanner("已注销设备会话")
            }.onFailure { err ->
                if (isUnauthorizedError(err)) {
                    handleUnauthorizedSession(err.message ?: "HTTP 401", capturedGen)
                } else {
                    val itemToRestore = removedItem
                    _state.update { cur ->
                        val nextSessions = if (itemToRestore != null && cur.serverSessions.none { it.sessionId == itemToRestore.sessionId }) {
                            cur.serverSessions + itemToRestore
                        } else cur.serverSessions
                        cur.copy(serverSessions = nextSessions, toastBanner = "注销会话失败：${err.message}")
                    }
                }
            }
        }
    }

    fun revokeOtherSessions() {
        if (!_state.value.isAuthenticated) return
        val capturedGen = generationFence.currentGeneration
        sessionsEpoch.incrementAndGet()
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                hostRequestResult("telegram.server.revokeOtherSessions")
            }
            if (!generationFence.isCurrent(capturedGen)) return@launch
            result.onSuccess {
                _state.update { cur ->
                    cur.copy(
                        serverSessions = cur.serverSessions.filter { it.isCurrent },
                        toastBanner = "已注销所有其他设备会话",
                    )
                }
            }.onFailure { err ->
                if (isUnauthorizedError(err)) {
                    handleUnauthorizedSession(err.message ?: "HTTP 401", capturedGen)
                } else {
                    postBanner("批量注销失败：${err.message}")
                }
            }
        }
    }

    fun updateLanguageAndTranslate(language: String, showTranslateButton: Boolean, autoTranslate: Boolean) {
        val normalized = normalizeLanguageCode(language)
        prefs.edit()
            .putString("ui_language", normalized)
            .putBoolean("show_translate_button", showTranslateButton)
            .putBoolean("auto_translate_channels", autoTranslate)
            .apply()
        _state.update {
            it.copy(
                uiLanguage = normalized,
                showTranslateButton = showTranslateButton,
                autoTranslateChannels = autoTranslate,
                toastBanner = "语言与翻译偏好已更新 ($normalized)",
            )
        }
    }

    fun updatePowerAndNotificationPrefs(powerThreshold: Int, inAppSounds: Boolean) {
        val bounded = powerThreshold.coerceIn(5, 80)
        prefs.edit()
            .putInt("power_saving_threshold", bounded)
            .putBoolean("in_app_sounds_enabled", inAppSounds)
            .apply()
        _state.update {
            it.copy(powerSavingThreshold = bounded, inAppSoundsEnabled = inAppSounds)
        }
    }

    // ------------------------------------------------------------------------
    // Stories, Folders, Global Search & Server-Persisted Anti-Flicker Chat Flags
    // ------------------------------------------------------------------------

    fun openStory(storyId: String) {
        viewModelScope.launch(Dispatchers.IO) {
            hostRequestResult("telegram.messaging.viewStory", JSONObject().put("storyId", storyId))
        }
        _state.update { current ->
            current.copy(
                viewingStoryId = storyId,
                stories = current.stories.map { if (it.id == storyId) it.copy(viewed = true, viewCount = it.viewCount + 1) else it },
            )
        }
    }

    fun closeStory() {
        _state.update { it.copy(viewingStoryId = null) }
    }

    fun createStory(headline: String, caption: String, gradientIndex: Int) {
        val cleanHeadline = headline.trim()
        val cleanCaption = caption.trim()
        if (cleanHeadline.isBlank() && cleanCaption.isBlank()) return
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                hostRequestResult(
                    "telegram.messaging.createStory",
                    JSONObject().put("headline", cleanHeadline).put("caption", cleanCaption),
                )
            }
            result.onSuccess { resp ->
                val storyId = resp.optNonNullString("storyId").ifBlank { "story-${UUID.randomUUID().toString().take(8)}" }
                val newStory = TelegramStoryItem(
                    id = storyId,
                    authorId = actorId,
                    authorName = _state.value.activeAccount?.displayName ?: "我的动态",
                    avatarSeed = actorId,
                    headline = cleanHeadline,
                    caption = cleanCaption,
                    gradientIndex = gradientIndex,
                    isMine = true,
                    viewed = false,
                    viewCount = 0,
                    reactionsCount = 0,
                )
                _state.update { it.copy(stories = listOf(newStory) + it.stories, toastBanner = "24 小时快拍动态已发布") }
            }.onFailure { err ->
                postBanner("发布动态失败：${err.message}")
            }
        }
    }

    fun selectFolder(folderId: String) {
        _state.update { it.copy(selectedFolderId = folderId) }
    }

    fun toggleArchiveExpanded() {
        _state.update { it.copy(archiveExpanded = !it.archiveExpanded) }
    }

    fun refreshFolders() {
        if (!_state.value.isAuthenticated) return
        val capturedGen = generationFence.currentGeneration
        val capturedEpoch = foldersEpoch.get()
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) { hostRequestResult("telegram.server.listFolders") }
            if (!generationFence.isCurrent(capturedGen)) return@launch
            result.onSuccess { resp ->
                val custom = parseServerFolders(resp)
                _state.update { cur ->
                    if (foldersEpoch.get() != capturedEpoch) cur
                    else cur.copy(folderTabs = TelegramFolderFilter.DEFAULT_BUILT_IN_TABS + custom)
                }
            }.onFailure { err ->
                if (isUnauthorizedError(err)) handleUnauthorizedSession(err.message ?: "HTTP 401", capturedGen)
            }
        }
    }

    fun upsertCustomFolder(
        title: String,
        includeContacts: Boolean,
        includeBots: Boolean,
        includeGroups: Boolean,
        includeChannels: Boolean,
        excludeMuted: Boolean,
    ) {
        val clean = title.trim().ifBlank { return }
        foldersEpoch.incrementAndGet()
        if (_state.value.isAuthenticated) {
            val capturedGen = generationFence.currentGeneration
            viewModelScope.launch {
                val payload = JSONObject()
                    .put("title", clean)
                    .put("includeContacts", includeContacts)
                    .put("includeBots", includeBots)
                    .put("includeGroups", includeGroups)
                    .put("includeChannels", includeChannels)
                    .put("excludeMuted", excludeMuted)
                    .put("excludeRead", false)
                    .put("excludeArchived", true)
                val result = withContext(Dispatchers.IO) {
                    hostRequestResult("telegram.server.saveFolder", payload)
                }
                if (!generationFence.isCurrent(capturedGen)) return@launch
                result.onSuccess { resp ->
                    val folderObj = resp.optJSONObject("folder") ?: resp
                    val rawFolderId = folderObj.optLong("id").takeIf { it > 0L }?.toString()
                        ?: folderObj.optNullableString("id")
                        ?: "folder-${UUID.randomUUID().toString().take(6)}"
                    val folderId = if (rawFolderId.lowercase() in RESERVED_FOLDER_TAB_IDS) {
                        "custom-$rawFolderId"
                    } else rawFolderId
                    val folder = TelegramFolderTab(
                        id = folderId,
                        title = folderObj.optNonNullString("title", clean).ifBlank { clean },
                        icon = folderObj.optNullableString("icon"),
                        isBuiltIn = false,
                        includeContacts = includeContacts,
                        includeBots = includeBots,
                        includeGroups = includeGroups,
                        includeChannels = includeChannels,
                        excludeMuted = excludeMuted,
                    )
                    _state.update { cur ->
                        val updated = cur.folderTabs.filterNot { it.id == folder.id } + folder
                        cur.copy(folderTabs = updated, selectedFolderId = folder.id, toastBanner = "已保存分组「${folder.title}」")
                    }
                }.onFailure { err ->
                    if (isUnauthorizedError(err)) {
                        handleUnauthorizedSession(err.message ?: "HTTP 401", capturedGen)
                    } else {
                        postBanner("保存分组失败：${err.message}")
                    }
                }
            }
            return
        }

        val folder = TelegramFolderTab(
            id = "folder-${UUID.randomUUID().toString().take(6)}",
            title = clean,
            isBuiltIn = false,
            includeContacts = includeContacts,
            includeBots = includeBots,
            includeGroups = includeGroups,
            includeChannels = includeChannels,
            excludeMuted = excludeMuted,
        )
        _state.update { it.copy(folderTabs = it.folderTabs + folder, selectedFolderId = folder.id, toastBanner = "已创建分组「$clean」") }
    }

    fun deleteCustomFolder(folderId: String) {
        val numericId = folderId.toLongOrNull()
        foldersEpoch.incrementAndGet()
        var removedTab: TelegramFolderTab? = null
        var prevSelected = "all"
        _state.update { current ->
            removedTab = current.folderTabs.firstOrNull { !it.isBuiltIn && it.id == folderId }
            prevSelected = current.selectedFolderId
            val updated = current.folderTabs.filterNot { !it.isBuiltIn && it.id == folderId }
            current.copy(
                folderTabs = updated,
                selectedFolderId = if (current.selectedFolderId == folderId) "all" else current.selectedFolderId,
            )
        }
        if (_state.value.isAuthenticated && numericId != null) {
            val capturedGen = generationFence.currentGeneration
            viewModelScope.launch {
                val result = withContext(Dispatchers.IO) {
                    hostRequestResult("telegram.server.deleteFolder", JSONObject().put("folderId", numericId))
                }
                if (!generationFence.isCurrent(capturedGen)) return@launch
                result.onFailure { err ->
                    if (isUnauthorizedError(err)) {
                        handleUnauthorizedSession(err.message ?: "HTTP 401", capturedGen)
                    } else {
                        val restore = removedTab
                        _state.update { cur ->
                            val nextTabs = if (restore != null && cur.folderTabs.none { it.id == restore.id }) {
                                cur.folderTabs + restore
                            } else cur.folderTabs
                            cur.copy(
                                folderTabs = nextTabs,
                                selectedFolderId = if (prevSelected == folderId) folderId else cur.selectedFolderId,
                                toastBanner = "删除分组失败：${err.message}",
                            )
                        }
                    }
                }
            }
        }
    }

    fun setGlobalSearch(active: Boolean, query: String = "", filter: GlobalSearchFilter = GlobalSearchFilter.CHATS) {
        _state.update {
            it.copy(
                globalSearchActive = active,
                globalSearchQuery = query,
                globalSearchFilter = filter,
                serverSearchResults = if (!active || query.isBlank()) emptyList() else it.serverSearchResults,
            )
        }
        if (active && query.isNotBlank()) {
            searchServerMessages(query, chatId = null, category = filter.name.lowercase())
        }
    }

    fun searchServerMessages(query: String, chatId: String? = null, category: String? = null) {
        val clean = query.trim()
        if (clean.isBlank()) {
            _state.update { it.copy(serverSearchResults = emptyList()) }
            return
        }
        if (!_state.value.isAuthenticated) return
        val capturedGen = generationFence.currentGeneration
        viewModelScope.launch {
            val params = JSONObject().put("query", clean)
            chatId?.toLongOrNull()?.let { params.put("chatId", it) }
            category?.takeIf { it.isNotBlank() }?.let { params.put("category", it) }
            val result = withContext(Dispatchers.IO) {
                hostRequestResult("telegram.server.searchMessages", params)
            }
            if (!generationFence.isCurrent(capturedGen)) return@launch
            result.onSuccess { resp ->
                val arr = resp.optJSONArray("messages")
                    ?: resp.optJSONArray("results")
                    ?: resp.optJSONArray("hits")
                    ?: resp.optJSONArray("value")
                    ?: JSONArray()
                val myUid = currentServerUserId
                val parsed = buildList {
                    for (i in 0 until arr.length()) {
                        val obj = arr.optJSONObject(i) ?: continue
                        val cid = obj.optLong("chat_id").takeIf { it > 0L }?.toString()
                            ?: chatId
                            ?: continue
                        parseServerMessageJson(obj, cid, myUid, readOutboxMaxId = 0L)?.let(::add)
                    }
                }
                _state.update { it.copy(serverSearchResults = parsed) }
            }.onFailure { err ->
                if (isUnauthorizedError(err)) handleUnauthorizedSession(err.message ?: "HTTP 401", capturedGen)
            }
        }
    }

    fun toggleChatSelection(chatId: String) {
        _state.update { current ->
            val next = if (chatId in current.selectedChatIds) current.selectedChatIds - chatId else current.selectedChatIds + chatId
            current.copy(selectedChatIds = next)
        }
    }

    fun clearChatSelection() {
        _state.update { it.copy(selectedChatIds = emptySet()) }
    }

    /**
     * Anti-Flicker Rule 1–5:
     * Single-apply local update + inFlightPinChats guard + server POST /api/v1/chats/{id}/pin +
     * authoritative reconciliation on success + atomic rollback on failure. NEVER bitwise-OR isPinned.
     */
    fun togglePinChat(chatId: String) {
        val target = _state.value.chats.firstOrNull { it.id == chatId } ?: return
        chatsEpoch.incrementAndGet()
        val prevPinned = target.isPinned
        val nextPinned = !prevPinned
        val numericId = chatId.toLongOrNull()

        if (_state.value.isAuthenticated && numericId != null) {
            val entry = inFlightMutationSeq.incrementAndGet() to nextPinned
            inFlightPinChats[chatId] = entry
            applyLocalChatFlags(chatId, isPinned = nextPinned)
            val capturedGen = generationFence.currentGeneration
            viewModelScope.launch {
                val result = withContext(Dispatchers.IO) {
                    hostRequestResult(
                        "telegram.server.pinChat",
                        JSONObject().put("chatId", numericId).put("pinned", nextPinned),
                    )
                }
                if (result.isFailure && isUnauthorizedError(result.exceptionOrNull())) {
                    inFlightPinChats.remove(chatId, entry)
                    handleUnauthorizedSession(result.exceptionOrNull()?.message ?: "HTTP 401", capturedGen)
                    return@launch
                }
                if (!inFlightPinChats.remove(chatId, entry)) return@launch
                if (!generationFence.isCurrent(capturedGen)) return@launch
                result.onSuccess { resp ->
                    val authoritativePinned = resp.optBoolean("is_pinned", nextPinned)
                    val authoritativeMuted = if (resp.has("is_muted")) resp.optBoolean("is_muted") else null
                    val authoritativeArchived = if (resp.has("is_archived")) resp.optBoolean("is_archived") else null
                    applyLocalChatFlags(
                        chatId = chatId,
                        isPinned = authoritativePinned,
                        isMuted = authoritativeMuted,
                        isArchived = authoritativeArchived,
                        clearMuteUntil = authoritativeMuted == false,
                    )
                }.onFailure { err ->
                    applyLocalChatFlags(chatId, isPinned = prevPinned)
                    postBanner("置顶会话失败：${err.message ?: "网络异常"}")
                }
            }
        } else {
            applyLocalChatFlags(chatId, isPinned = nextPinned)
        }
    }

    fun toggleArchiveChat(chatId: String) {
        val target = _state.value.chats.firstOrNull { it.id == chatId } ?: return
        chatsEpoch.incrementAndGet()
        val prevArchived = target.isArchived
        val nextArchived = !prevArchived
        val numericId = chatId.toLongOrNull()

        if (_state.value.isAuthenticated && numericId != null) {
            val entry = inFlightMutationSeq.incrementAndGet() to nextArchived
            inFlightArchiveChats[chatId] = entry
            applyLocalChatFlags(chatId, isArchived = nextArchived)
            val capturedGen = generationFence.currentGeneration
            viewModelScope.launch {
                val result = withContext(Dispatchers.IO) {
                    hostRequestResult(
                        "telegram.server.archiveChat",
                        JSONObject().put("chatId", numericId).put("archived", nextArchived),
                    )
                }
                if (result.isFailure && isUnauthorizedError(result.exceptionOrNull())) {
                    inFlightArchiveChats.remove(chatId, entry)
                    handleUnauthorizedSession(result.exceptionOrNull()?.message ?: "HTTP 401", capturedGen)
                    return@launch
                }
                if (!inFlightArchiveChats.remove(chatId, entry)) return@launch
                if (!generationFence.isCurrent(capturedGen)) return@launch
                result.onSuccess { resp ->
                    val authoritative = resp.optBoolean("is_archived", nextArchived)
                    applyLocalChatFlags(chatId, isArchived = authoritative)
                    postBanner(if (authoritative) "已归档「${target.title}」" else "已从归档移出「${target.title}」")
                }.onFailure { err ->
                    applyLocalChatFlags(chatId, isArchived = prevArchived)
                    postBanner("归档操作失败：${err.message ?: "网络异常"}")
                }
            }
        } else {
            applyLocalChatFlags(chatId, isArchived = nextArchived)
            postBanner(if (nextArchived) "已归档「${target.title}」" else "已从归档移出「${target.title}」")
        }
    }

    fun toggleMuteChat(chatId: String) {
        val target = _state.value.chats.firstOrNull { it.id == chatId } ?: return
        chatsEpoch.incrementAndGet()
        val prevMuted = target.isMuted
        val prevMuteUntilMs = target.muteUntilMs
        val nextMuted = !prevMuted
        val numericId = chatId.toLongOrNull()

        if (_state.value.isAuthenticated && numericId != null) {
            val entry = inFlightMutationSeq.incrementAndGet() to nextMuted
            inFlightMuteChats[chatId] = entry
            applyLocalChatFlags(chatId, isMuted = nextMuted, clearMuteUntil = !nextMuted)
            val capturedGen = generationFence.currentGeneration
            viewModelScope.launch {
                val result = withContext(Dispatchers.IO) {
                    hostRequestResult(
                        "telegram.server.muteChat",
                        JSONObject().put("chatId", numericId).put("muted", nextMuted),
                    )
                }
                if (result.isFailure && isUnauthorizedError(result.exceptionOrNull())) {
                    inFlightMuteChats.remove(chatId, entry)
                    handleUnauthorizedSession(result.exceptionOrNull()?.message ?: "HTTP 401", capturedGen)
                    return@launch
                }
                if (!inFlightMuteChats.remove(chatId, entry)) return@launch
                if (!generationFence.isCurrent(capturedGen)) return@launch
                result.onSuccess { resp ->
                    val authoritative = resp.optBoolean("is_muted", nextMuted)
                    val muteUntil = if (authoritative && resp.has("mute_until_ms") && !resp.isNull("mute_until_ms")) {
                        resp.optLong("mute_until_ms")
                    } else null
                    applyLocalChatFlags(
                        chatId = chatId,
                        isMuted = authoritative,
                        muteUntilMs = muteUntil,
                        clearMuteUntil = !authoritative || muteUntil == null,
                    )
                }.onFailure { err ->
                    applyLocalChatFlags(
                        chatId = chatId,
                        isMuted = prevMuted,
                        muteUntilMs = prevMuteUntilMs,
                        clearMuteUntil = prevMuteUntilMs == null,
                    )
                    postBanner("静音操作失败：${err.message ?: "网络异常"}")
                }
            }
        } else {
            applyLocalChatFlags(chatId, isMuted = nextMuted, clearMuteUntil = !nextMuted)
        }
    }

    private fun applyLocalChatFlags(
        chatId: String,
        isPinned: Boolean? = null,
        isMuted: Boolean? = null,
        isArchived: Boolean? = null,
        muteUntilMs: Long? = null,
        clearMuteUntil: Boolean = false,
    ) {
        _state.update { current ->
            val updated = current.chats.map { chat ->
                if (chat.id == chatId) {
                    val nextMuted = isMuted ?: chat.isMuted
                    val nextMuteUntil = when {
                        clearMuteUntil || isMuted == false -> null
                        muteUntilMs != null -> muteUntilMs
                        else -> chat.muteUntilMs
                    }
                    chat.copy(
                        isPinned = isPinned ?: chat.isPinned,
                        isMuted = nextMuted,
                        isArchived = isArchived ?: chat.isArchived,
                        muteUntilMs = nextMuteUntil,
                    )
                } else chat
            }
            current.copy(chats = sortTelegramChats(updated), selectedChatIds = emptySet())
        }
    }

    fun toggleUnreadChat(chatId: String) {
        val target = _state.value.chats.firstOrNull { it.id == chatId } ?: return
        val markingRead = target.unreadCount > 0 || target.markedUnread
        _state.update { current ->
            val updated = current.chats.map { chat ->
                if (chat.id == chatId) {
                    if (markingRead) chat.copy(unreadCount = 0, markedUnread = false)
                    else chat.copy(markedUnread = true)
                } else chat
            }
            current.copy(chats = updated, selectedChatIds = emptySet())
        }
        if (markingRead && _state.value.isAuthenticated && chatId.toLongOrNull() != null) {
            markChatRead(chatId)
        }
    }

    fun markChatRead(chatId: String, maxId: Long? = null) {
        val numericChatId = chatId.toLongOrNull() ?: return
        if (!_state.value.isAuthenticated) return
        val resolvedMaxId = maxId
            ?: _state.value.messagesByChat[chatId]?.mapNotNull { it.id.toLongOrNull() }?.maxOrNull()
            ?: _state.value.chats.firstOrNull { it.id == chatId }?.readInboxMaxId?.takeIf { it > 0L }
            ?: 0L
        if (resolvedMaxId <= 0L) return
        chatsEpoch.incrementAndGet()
        val capturedGen = generationFence.currentGeneration
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                hostRequestResult(
                    "telegram.server.markRead",
                    JSONObject().put("chatId", numericChatId).put("maxId", resolvedMaxId),
                )
            }
            if (!generationFence.isCurrent(capturedGen)) return@launch
            result.onSuccess { resp ->
                val unread = resp.optInt("unread_count", 0)
                val readInbox = resp.optLong("read_inbox_max_id", resolvedMaxId)
                _state.update { cur ->
                    cur.copy(
                        chats = cur.chats.map { c ->
                            if (c.id == chatId) c.copy(unreadCount = unread, markedUnread = false, readInboxMaxId = readInbox) else c
                        },
                    )
                }
            }.onFailure { err ->
                if (isUnauthorizedError(err)) handleUnauthorizedSession(err.message ?: "HTTP 401", capturedGen)
            }
        }
    }

    // ------------------------------------------------------------------------
    // Opening & Creating Conversations (Group, Channel, Secret Chat, Direct)
    // ------------------------------------------------------------------------

    fun openChat(
        chatId: String,
        topicId: String? = null,
        provisionalTitle: String? = null,
        provisionalUsername: String? = null,
        provisionalIsBot: Boolean = false,
        provisionalAvatarShape: String? = null,
        provisionalAvatarColor: String? = null,
        provisionalPeerUserId: Long? = null,
        provisionalBio: String? = null,
    ) {
        val targetChat = _state.value.chats.firstOrNull { it.id == chatId }
        val hadUnread = (targetChat?.unreadCount ?: 0) > 0
        val nowMs = System.currentTimeMillis()
        _state.update { current ->
            val exists = current.chats.any { it.id == chatId }
            val updatedChats = if (exists) {
                current.chats.map {
                    if (it.id == chatId) {
                        it.copy(
                            unreadCount = 0,
                            markedUnread = false,
                            isBot = it.isBot || provisionalIsBot,
                            avatarShape = it.avatarShape ?: provisionalAvatarShape,
                            avatarColor = it.avatarColor ?: provisionalAvatarColor,
                            peerUserId = it.peerUserId ?: provisionalPeerUserId,
                            peerBio = it.peerBio ?: provisionalBio,
                            username = it.username ?: provisionalUsername,
                        )
                    } else it
                }
            } else {
                val cleanHandle = provisionalUsername?.trim()?.trimStart('@')?.takeIf { it.isNotBlank() }
                val fallbackTitle = provisionalTitle?.trim()?.takeIf { it.isNotBlank() }
                    ?: cleanHandle
                    ?: if (provisionalIsBot) "Bot" else "Chat #$chatId"
                val provisional = TelegramChatItem(
                    id = chatId,
                    title = fallbackTitle,
                    subtitle = cleanHandle?.let { "@$it" } ?: "",
                    preview = provisionalBio?.trim().orEmpty(),
                    time = nowTimeLabel(nowMs),
                    kind = ConversationKind.DIRECT,
                    isBot = provisionalIsBot,
                    username = cleanHandle?.let { "@$it" },
                    avatarShape = provisionalAvatarShape,
                    avatarColor = provisionalAvatarColor,
                    peerUserId = provisionalPeerUserId,
                    peerBio = provisionalBio?.trim()?.takeIf { it.isNotEmpty() },
                    lastMessageAtMs = nowMs,
                )
                sortTelegramChats(listOf(provisional) + current.chats)
            }
            current.copy(
                subScreen = TelegramSubScreen.CHAT,
                activeChatId = chatId,
                activeTopicId = topicId,
                chats = updatedChats,
                inChatSearchActive = false,
                inChatSearchQuery = "",
                replyTarget = null,
                editTarget = null,
                selectedChatIds = emptySet(),
            )
        }
        val numericId = chatId.toLongOrNull()
        if (_state.value.isAuthenticated && numericId != null) {
            fetchServerMessagesForChat(chatId, numericId, markReadAfterLoad = hadUnread)
            fetchPinnedMessages(chatId, numericId)
            fetchServerDraft(chatId)
        }
    }

    /**
     * Desktop-aligned resilient bot entry (`R5 / AC-5`):
     * Clicking ANY bot (owned, official, Mini App with `companionChatId == null`, or newly created)
     * opens its conversation immediately without silent no-ops or bouncing back to HOME.
     */
    fun openBotChat(bot: FabushiBotItem) {
        val currentChats = _state.value.chats
        val existing = currentChats.firstOrNull { ch ->
            (bot.companionChatId != null && ch.id == bot.companionChatId.toString()) ||
                (bot.botUserId > 0L && ch.peerUserId == bot.botUserId) ||
                matchesBotUsername(ch.username, bot.username)
        }
        if (existing != null) {
            openChat(
                chatId = existing.id,
                provisionalTitle = bot.displayName.ifBlank { existing.title },
                provisionalUsername = bot.username.ifBlank { existing.username },
                provisionalIsBot = true,
                provisionalAvatarShape = bot.avatarShape.ifBlank { existing.avatarShape },
                provisionalAvatarColor = bot.avatarColor.ifBlank { existing.avatarColor },
                provisionalPeerUserId = bot.botUserId.takeIf { it > 0L } ?: existing.peerUserId,
                provisionalBio = bot.description.ifBlank { existing.peerBio },
            )
            return
        }
        val cid = bot.companionChatId?.takeIf { it > 0L }?.toString()
        if (cid != null) {
            openChat(
                chatId = cid,
                provisionalTitle = bot.displayName.ifBlank { bot.username },
                provisionalUsername = bot.username,
                provisionalIsBot = true,
                provisionalAvatarShape = bot.avatarShape.ifBlank { null },
                provisionalAvatarColor = bot.avatarColor.ifBlank { null },
                provisionalPeerUserId = bot.botUserId.takeIf { it > 0L },
                provisionalBio = bot.description.ifBlank { null },
            )
            return
        }
        openBotByUsername(
            username = bot.username,
            displayName = bot.displayName.ifBlank { bot.username },
            description = bot.description,
            botUserId = bot.botUserId.takeIf { it > 0L },
            avatarShape = bot.avatarShape.ifBlank { null },
            avatarColor = bot.avatarColor.ifBlank { null },
        )
    }

    /**
     * Opens a bot chat by `@username` (e.g., `@BotFather` or an official bot with `companionChatId == null`).
     * Immediately opens a provisional chat (<16ms UI response) and idempotently materializes the real
     * server DM via `POST /api/v1/contacts/add` (`is_bot = true`), migrating the chat ID transparently.
     */
    fun openBotByUsername(
        username: String,
        displayName: String = "",
        description: String = "",
        botUserId: Long? = null,
        avatarShape: String? = null,
        avatarColor: String? = null,
    ) {
        val clean = username.trim().trimStart('@')
        if (clean.isBlank()) return
        val existing = _state.value.chats.firstOrNull { ch ->
            (botUserId != null && botUserId > 0L && ch.peerUserId == botUserId) ||
                matchesBotUsername(ch.username, clean)
        }
        if (existing != null) {
            openChat(
                chatId = existing.id,
                provisionalTitle = displayName.ifBlank { existing.title },
                provisionalUsername = clean,
                provisionalIsBot = true,
                provisionalAvatarShape = avatarShape ?: existing.avatarShape,
                provisionalAvatarColor = avatarColor ?: existing.avatarColor,
                provisionalPeerUserId = botUserId ?: existing.peerUserId,
                provisionalBio = description.ifBlank { existing.peerBio },
            )
            return
        }

        val provisionalId = "bot:${clean.lowercase(java.util.Locale.ROOT)}"
        val title = displayName.trim().ifBlank { clean }
        openChat(
            chatId = provisionalId,
            provisionalTitle = title,
            provisionalUsername = clean,
            provisionalIsBot = true,
            provisionalAvatarShape = avatarShape,
            provisionalAvatarColor = avatarColor,
            provisionalPeerUserId = botUserId,
            provisionalBio = description.trim().ifBlank { null },
        )

        if (!_state.value.isAuthenticated) return
        val capturedGen = generationFence.currentGeneration
        chatsEpoch.incrementAndGet()
        viewModelScope.launch {
            val payload = JSONObject()
                .put("username", clean)
                .put("firstName", title)
                .put("isBot", true)
                .put("kind", "bot")
            val result = withContext(Dispatchers.IO) {
                hostRequestResult("telegram.server.addContact", payload)
            }
            if (!generationFence.isCurrent(capturedGen)) return@launch
            result.onSuccess { resp ->
                val resolvedChatId = resp.optLong("chat_id").takeIf { it > 0L }?.toString()
                    ?: resp.optJSONObject("chat")?.optLong("id")?.takeIf { it > 0L }?.toString()
                    ?: return@onSuccess
                val nowMs = System.currentTimeMillis()
                _state.update { cur ->
                    val existingReal = cur.chats.firstOrNull { it.id == resolvedChatId }
                    val provChat = cur.chats.firstOrNull { it.id == provisionalId }
                    val migratedChat = (existingReal ?: provChat ?: TelegramChatItem(
                        id = resolvedChatId,
                        title = title,
                        subtitle = "@$clean",
                        preview = description.trim(),
                        time = nowTimeLabel(nowMs),
                        kind = ConversationKind.DIRECT,
                        isBot = true,
                        username = "@$clean",
                        lastMessageAtMs = nowMs,
                    )).copy(
                        id = resolvedChatId,
                        title = title.ifBlank { existingReal?.title ?: clean },
                        isBot = true,
                        username = "@$clean",
                        avatarShape = avatarShape ?: existingReal?.avatarShape ?: provChat?.avatarShape,
                        avatarColor = avatarColor ?: existingReal?.avatarColor ?: provChat?.avatarColor,
                        peerUserId = botUserId ?: existingReal?.peerUserId ?: provChat?.peerUserId,
                        peerBio = description.trim().ifBlank { existingReal?.peerBio ?: provChat?.peerBio },
                    )
                    val remaining = cur.chats.filterNot { it.id == provisionalId || it.id == resolvedChatId }
                    val nextMsgs = cur.messagesByChat.toMutableMap().apply {
                        val provMsgs = remove(provisionalId)
                        if (!provMsgs.isNullOrEmpty() && get(resolvedChatId).isNullOrEmpty()) {
                            put(resolvedChatId, provMsgs.map { it.copy(conversationId = resolvedChatId) })
                        }
                    }
                    val nextStarted = if (provisionalId in cur.startedBotChatIds) {
                        (cur.startedBotChatIds - provisionalId) + resolvedChatId
                    } else cur.startedBotChatIds
                    val nextBots = cur.fabushiBots.map { b ->
                        if (matchesBotUsername(b.username, clean) && b.companionChatId == null) {
                            b.copy(companionChatId = resolvedChatId.toLongOrNull())
                        } else b
                    }
                    cur.copy(
                        chats = sortTelegramChats(listOf(migratedChat) + remaining),
                        messagesByChat = nextMsgs,
                        startedBotChatIds = nextStarted,
                        fabushiBots = nextBots,
                        activeChatId = if (cur.activeChatId == provisionalId) resolvedChatId else cur.activeChatId,
                    )
                }
                resolvedChatId.toLongOrNull()?.let { numId ->
                    fetchServerMessagesForChat(resolvedChatId, numId, markReadAfterLoad = false)
                    fetchPinnedMessages(resolvedChatId, numId)
                    fetchServerDraft(resolvedChatId)
                }
            }.onFailure { err ->
                if (isUnauthorizedError(err)) {
                    handleUnauthorizedSession(err.message ?: "HTTP 401", capturedGen)
                } else {
                    postBanner("连接机器人 @$clean 失败：${err.message ?: "网络异常"}")
                }
            }
        }
    }

    /**
     * Desktop-aligned bot pre-start activation (`R5 / AC-6`):
     * Marks the bot chat as started and sends `/start`.
     */
    fun startBotChat(chatId: String) {
        _state.update { it.copy(startedBotChatIds = it.startedBotChatIds + chatId) }
        val active = _state.value.chats.firstOrNull { it.id == chatId }
        if (chatId.startsWith("bot:") && active != null && _state.value.isAuthenticated) {
            val clean = active.username?.trimStart('@').orEmpty().ifBlank { chatId.removePrefix("bot:") }
            val capturedGen = generationFence.currentGeneration
            viewModelScope.launch {
                val resp = withContext(Dispatchers.IO) {
                    hostRequestResult(
                        "telegram.server.addContact",
                        JSONObject()
                            .put("username", clean)
                            .put("firstName", active.title.ifBlank { clean })
                            .put("isBot", true)
                            .put("kind", "bot"),
                    )
                }.getOrNull()
                if (!generationFence.isCurrent(capturedGen)) return@launch
                val realId = resp?.optLong("chat_id")?.takeIf { it > 0L }?.toString()
                    ?: resp?.optJSONObject("chat")?.optLong("id")?.takeIf { it > 0L }?.toString()
                if (realId != null) {
                    _state.update { cur ->
                        val updated = cur.chats.map { if (it.id == chatId) it.copy(id = realId) else it }
                        cur.copy(
                            chats = updated,
                            startedBotChatIds = (cur.startedBotChatIds - chatId) + realId,
                            activeChatId = if (cur.activeChatId == chatId) realId else cur.activeChatId,
                        )
                    }
                    sendComposerMessage(realId, "/start", silent = false, scheduledAtMs = null, overrideTtlSeconds = null)
                } else {
                    sendComposerMessage(chatId, "/start", silent = false, scheduledAtMs = null, overrideTtlSeconds = null)
                }
            }
        } else {
            sendComposerMessage(chatId, "/start", silent = false, scheduledAtMs = null, overrideTtlSeconds = null)
        }
    }

    private fun syncFabushiBotsToChats(
        chats: List<TelegramChatItem>,
        bots: List<FabushiBotItem>,
    ): List<TelegramChatItem> {
        if (chats.isEmpty() || bots.isEmpty()) return chats
        return chats.map { ch ->
            val matched = bots.firstOrNull { b ->
                (b.companionChatId != null && ch.id == b.companionChatId.toString()) ||
                    (b.botUserId > 0L && ch.peerUserId == b.botUserId) ||
                    matchesBotUsername(ch.username, b.username)
            }
            if (matched != null) {
                ch.copy(
                    isBot = true,
                    avatarShape = matched.avatarShape.ifBlank { ch.avatarShape },
                    avatarColor = matched.avatarColor.ifBlank { ch.avatarColor },
                    botExecutionState = matched.executionState.ifBlank { ch.botExecutionState },
                    peerUserId = matched.botUserId.takeIf { it > 0L } ?: ch.peerUserId,
                    peerBio = matched.description.ifBlank { ch.peerBio },
                    username = ch.username ?: matched.username.takeIf { it.isNotBlank() }?.let { "@${it.trimStart('@')}" },
                )
            } else ch
        }
    }

    fun closeChat() {
        val snapshot = _state.value
        val closingChatId = snapshot.activeChatId
        val closingDraft = closingChatId?.let { cid -> snapshot.chats.firstOrNull { it.id == cid }?.draftText }
        val closingReplyId = snapshot.replyTarget
            ?.takeIf { it.conversationId == closingChatId }
            ?.id
            ?.toLongOrNull()
        _state.update {
            it.copy(
                subScreen = TelegramSubScreen.HOME,
                activeChatId = null,
                activeTopicId = null,
                inChatSearchActive = false,
                inChatSearchQuery = "",
                replyTarget = null,
                editTarget = null,
                composerAttachmentSheetOpen = false,
            )
        }
        if (closingChatId != null && snapshot.isAuthenticated && closingChatId.toLongOrNull() != null) {
            if (!closingDraft.isNullOrBlank()) {
                saveServerDraft(closingChatId, closingDraft, closingReplyId)
            } else {
                clearServerDraft(closingChatId)
            }
        }
    }

    fun openSavedMessages() {
        val existing = _state.value.chats.firstOrNull {
            it.kind == ConversationKind.SAVED_MESSAGES || it.id == "saved-messages"
        }
        if (existing != null) {
            openChat(existing.id)
            return
        }
        if (_state.value.isAuthenticated) {
            createNewGroupOrChannel(ConversationKind.SAVED_MESSAGES, "收藏夹", "")
            return
        }
        val saved = TelegramChatItem(
            id = "saved-messages",
            title = "收藏夹 (Saved Messages)",
            subtitle = "个人笔记与文件箱",
            preview = "",
            time = nowTimeLabel(),
            kind = ConversationKind.SAVED_MESSAGES,
            isPinned = true,
            lastMessageAtMs = System.currentTimeMillis(),
        )
        _state.update { cur ->
            cur.copy(chats = sortTelegramChats(listOf(saved) + cur.chats))
        }
        openChat("saved-messages")
    }

    fun createNewGroupOrChannel(kind: ConversationKind, title: String, description: String, forumEnabled: Boolean = false) {
        val cleanTitle = title.trim().ifBlank { return }
        chatsEpoch.incrementAndGet()
        if (_state.value.isAuthenticated) {
            val capturedGen = generationFence.currentGeneration
            viewModelScope.launch {
                val payload = JSONObject()
                    .put("title", cleanTitle)
                    .put("kind", kind.wire)
                    .put("about", description.trim())
                    .put("isForum", forumEnabled && kind == ConversationKind.GROUP)
                val result = withContext(Dispatchers.IO) {
                    hostRequestResult("telegram.server.createChat", payload)
                }
                if (!generationFence.isCurrent(capturedGen)) return@launch
                result.onSuccess { resp ->
                    val chatObj = resp.optJSONObject("chat") ?: resp
                    val serverId = chatObj.optLong("chat_id").takeIf { it > 0L }?.toString()
                        ?: chatObj.optLong("id").takeIf { it > 0L }?.toString()
                    if (serverId == null) {
                        postBanner("创建会话失败：服务器未返回有效会话 ID")
                        return@onSuccess
                    }
                    val parsed = parseServerChatObject(chatObj) ?: TelegramChatItem(
                        id = serverId,
                        title = cleanTitle,
                        subtitle = kind.label,
                        preview = description.trim(),
                        time = nowTimeLabel(),
                        kind = kind,
                        isForum = forumEnabled && kind == ConversationKind.GROUP,
                        lastMessageAtMs = System.currentTimeMillis(),
                    )
                    _state.update { current ->
                        current.copy(
                            chats = sortTelegramChats(listOf(parsed) + current.chats.filterNot { it.id == serverId }),
                            subScreen = TelegramSubScreen.CHAT,
                            activeChatId = serverId,
                            toastBanner = "已创建「${parsed.title}」",
                        )
                    }
                }.onFailure { err ->
                    if (isUnauthorizedError(err)) {
                        handleUnauthorizedSession(err.message ?: "HTTP 401", capturedGen)
                    } else {
                        postBanner("创建会话失败：${err.message ?: "网络异常"}")
                    }
                }
            }
            return
        }

        // Offline local mode fallback (protocol v2 SQLite)
        val id = "${kind.wire}-${UUID.randomUUID().toString().take(8)}"
        val now = System.currentTimeMillis()
        val newChat = TelegramChatItem(
            id = id,
            title = cleanTitle,
            subtitle = kind.label,
            preview = description.trim(),
            time = nowTimeLabel(now),
            kind = kind,
            isForum = forumEnabled && kind == ConversationKind.GROUP,
            lastMessageAtMs = now,
        )
        viewModelScope.launch(Dispatchers.IO) {
            val convJson = JSONObject()
                .put("id", id).put("kind", kind.wire).put("title", cleanTitle)
                .put("description", description.ifBlank { JSONObject.NULL })
                .put("participants", JSONArray().put(JSONObject().put("actorId", actorId).put("role", "owner").put("joinedAtMs", now).put("mutedUntilMs", JSONObject.NULL)))
                .put("ownerId", actorId).put("unreadCount", 0).put("mentionCount", 0)
                .put("pinnedMessageIds", JSONArray()).put("folderIds", JSONArray())
                .put("archived", false).put("pinned", false).put("markedUnread", false)
                .put("createdAtMs", now).put("updatedAtMs", now)
            sendProtocolV2Command(JSONObject().put("type", "createConversation").put("conversation", convJson))
        }
        _state.update { current ->
            current.copy(
                chats = sortTelegramChats(listOf(newChat) + current.chats),
                subScreen = TelegramSubScreen.CHAT,
                activeChatId = id,
            )
        }
    }

    /**
     * Contract #11: telegram.e2ee.createSecretChat expects {peerId, chatId, deviceId?}
     */
    fun createSecretChat(peerName: String) {
        val cleanPeer = peerName.trim().ifBlank { return }
        val id = "secret-${UUID.randomUUID().toString().take(8)}"
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                hostRequestResult("telegram.e2ee.initDevice")
                hostRequestResult(
                    "telegram.e2ee.createSecretChat",
                    JSONObject()
                        .put("peerId", cleanPeer)
                        .put("chatId", id),
                )
            }
            result.onSuccess { resp ->
                val actualChatId = resp.optNonNullString("chatId").ifBlank { id }
                val fpObj = resp.optJSONObject("safetyFingerprint")
                val hexFp = fpObj?.optNonNullString("hex_fingerprint").orEmpty()
                val emojiDisplay = fpObj?.optNonNullString("emoji_display").orEmpty()
                val now = System.currentTimeMillis()
                val secretChat = TelegramChatItem(
                    id = actualChatId,
                    title = "🔒 $cleanPeer",
                    subtitle = "端到端加密聊天 · X3DH + Double Ratchet",
                    preview = emojiDisplay.ifBlank { "端到端密钥协商已完成" },
                    time = nowTimeLabel(now),
                    kind = ConversationKind.SECRET,
                    isSecretChat = true,
                    secretFingerprintSeed = hexFp.ifBlank { actualChatId },
                    ttlSeconds = 86_400,
                    online = false,
                    lastMessageAtMs = now,
                )
                val initMsg = TelegramMessageItem(
                    id = "msg-sec-$actualChatId",
                    conversationId = actualChatId,
                    senderId = "system",
                    senderName = "E2EE 安全引擎",
                    text = "🔐 已建立端到端加密通道 (${resp.optNonNullString("protocol", "X3DH + Double Ratchet")})\n安全校验表情：$emojiDisplay",
                    outgoing = false,
                    time = nowTimeLabel(now),
                    timestampMs = now,
                    deliveryState = "sent",
                    ttlSeconds = 86_400,
                )
                _state.update { current ->
                    current.copy(
                        chats = sortTelegramChats(listOf(secretChat) + current.chats),
                        messagesByChat = current.messagesByChat + (actualChatId to listOf(initMsg)),
                        subScreen = TelegramSubScreen.CHAT,
                        activeChatId = actualChatId,
                    )
                }
            }.onFailure { err ->
                postBanner("建立加密聊天失败：${err.message ?: "未知错误"}")
            }
        }
    }

    fun openSecretChatFingerprintDialog(chatId: String?) {
        _state.update { it.copy(secretChatDialogChatId = chatId) }
    }

    fun markSecretChatVerified(chatId: String) {
        _state.update { current ->
            current.copy(
                chats = current.chats.map { if (it.id == chatId) it.copy(secretVerified = true) else it },
                secretChatDialogChatId = null,
                toastBanner = "已验证端到端加密会话指纹",
            )
        }
    }

    fun selectForumTopic(topicId: String?) {
        _state.update { it.copy(activeTopicId = topicId) }
    }

    /**
     * Contract #10: telegram.messaging.createTopic expects {conversationId, title, iconEmoji}
     */
    fun createForumTopic(conversationId: String, title: String, emoji: String) {
        val clean = title.trim().ifBlank { return }
        val iconEmoji = emoji.ifBlank { "💡" }
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                hostRequestResult(
                    "telegram.messaging.createTopic",
                    JSONObject()
                        .put("conversationId", conversationId)
                        .put("title", clean)
                        .put("iconEmoji", iconEmoji),
                )
            }
            result.onSuccess { resp ->
                val topicId = resp.optNonNullString("topicId").ifBlank { "topic-${UUID.randomUUID().toString().take(6)}" }
                val topic = TelegramForumTopic(
                    id = topicId,
                    conversationId = conversationId,
                    title = resp.optNonNullString("title", clean),
                    iconEmoji = resp.optNonNullString("iconEmoji", iconEmoji),
                    lastPreview = "",
                )
                _state.update { current ->
                    val list = current.topicsByChat[conversationId].orEmpty() + topic
                    current.copy(
                        topicsByChat = current.topicsByChat + (conversationId to list),
                        activeTopicId = topic.id,
                    )
                }
            }.onFailure { err ->
                postBanner("创建话题失败：${err.message ?: "未知错误"}")
            }
        }
    }

    // ------------------------------------------------------------------------
    // Rich Messaging, Cloud Drafts, Optimistic Send + Retry, Pins, Reactions & Polls
    // ------------------------------------------------------------------------

    fun updateDraft(conversationId: String, draft: String) {
        chatDraftEpochFor(conversationId).incrementAndGet()
        _state.update { current ->
            current.copy(
                chats = current.chats.map { if (it.id == conversationId) it.copy(draftText = draft) else it },
            )
        }
    }

    fun saveServerDraft(conversationId: String, text: String, replyToId: Long? = null) {
        updateDraft(conversationId, text)
        val numericId = conversationId.toLongOrNull() ?: return
        if (!_state.value.isAuthenticated) return
        val capturedGen = generationFence.currentGeneration
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                if (text.isBlank()) {
                    hostRequestResult("telegram.server.clearDraft", JSONObject().put("chatId", numericId))
                } else {
                    val payload = JSONObject().put("chatId", numericId).put("text", text)
                    replyToId?.let { payload.put("replyToMsgId", it) }
                    hostRequestResult("telegram.server.setDraft", payload)
                }
            }
            if (!generationFence.isCurrent(capturedGen)) return@launch
            result.onFailure { err ->
                if (isUnauthorizedError(err)) handleUnauthorizedSession(err.message ?: "HTTP 401", capturedGen)
            }
        }
    }

    fun clearServerDraft(conversationId: String) {
        updateDraft(conversationId, "")
        val numericId = conversationId.toLongOrNull() ?: return
        if (!_state.value.isAuthenticated) return
        val capturedGen = generationFence.currentGeneration
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                hostRequestResult("telegram.server.clearDraft", JSONObject().put("chatId", numericId))
            }
            if (!generationFence.isCurrent(capturedGen)) return@launch
            result.onFailure { err ->
                if (isUnauthorizedError(err)) handleUnauthorizedSession(err.message ?: "HTTP 401", capturedGen)
            }
        }
    }

    fun fetchServerDraft(conversationId: String) {
        val numericId = conversationId.toLongOrNull() ?: return
        if (!_state.value.isAuthenticated) return
        val capturedGen = generationFence.currentGeneration
        val capturedDraftEpoch = chatDraftEpochFor(conversationId).get()
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                hostRequestResult("telegram.server.getDraft", JSONObject().put("chatId", numericId))
            }
            if (!generationFence.isCurrent(capturedGen)) return@launch
            result.onSuccess { resp ->
                val draftObj = resp.optJSONObject("draft")
                val draftText = draftObj?.optNonNullString("text").orEmpty()
                if (draftText.isNotEmpty()) {
                    _state.update { cur ->
                        if (chatDraftEpochFor(conversationId).get() != capturedDraftEpoch) {
                            return@update cur
                        }
                        cur.copy(
                            chats = cur.chats.map { c ->
                                if (c.id == conversationId && c.draftText.isEmpty()) c.copy(draftText = draftText) else c
                            },
                        )
                    }
                }
            }.onFailure { err ->
                if (isUnauthorizedError(err)) handleUnauthorizedSession(err.message ?: "HTTP 401", capturedGen)
            }
        }
    }

    fun setReplyTarget(message: TelegramMessageItem?) {
        _state.update { it.copy(replyTarget = message, editTarget = null) }
    }

    fun setEditTarget(message: TelegramMessageItem?) {
        _state.update { current ->
            val convId = message?.conversationId ?: current.activeChatId
            val updatedChats = if (convId != null && message != null) {
                current.chats.map { if (it.id == convId) it.copy(draftText = message.text) else it }
            } else current.chats
            current.copy(editTarget = message, replyTarget = null, chats = updatedChats)
        }
    }

    fun setInChatSearch(active: Boolean, query: String = "") {
        val convId = _state.value.activeChatId
        _state.update { it.copy(inChatSearchActive = active, inChatSearchQuery = query) }
        if (active && query.isNotBlank() && convId != null) {
            if (_state.value.isAuthenticated && convId.toLongOrNull() != null) {
                searchServerMessages(query, chatId = convId)
            } else {
                viewModelScope.launch(Dispatchers.IO) {
                    hostRequestResult("telegram.messaging.searchMessages", JSONObject().put("conversationId", convId).put("query", query))
                }
            }
        }
    }

    fun sendComposerMessage(
        conversationId: String,
        rawText: String,
        silent: Boolean = false,
        scheduledAtMs: Long? = null,
        overrideTtlSeconds: Int? = null,
    ) {
        val trimmed = rawText.trim()
        if (trimmed.isEmpty()) return

        // If Mika is waiting for an `ask_user` input on this thread, answer it directly!
        val bridge = mikaBridge
        if (bridge != null && bridge.answerAsk(trimmed, conversationId, null)) {
            updateDraft(conversationId, "")
            return
        }

        // Editing branch -> real server PUT /api/v1/chats/{id}/messages/{mid} with rollback on failure
        val editing = _state.value.editTarget
        if (editing != null && editing.conversationId == conversationId) {
            chatMessagesEpochFor(conversationId).incrementAndGet()
            val (plain, entities) = TelegramEntitiesEngine.parseMarkdownAndAutoEntities(trimmed)
            val prevMsg = editing
            updateMessageInChat(conversationId, editing.id) {
                it.copy(text = plain, entities = entities, edited = true)
            }
            _state.update { current ->
                current.copy(
                    editTarget = null,
                    chats = current.chats.map { if (it.id == conversationId) it.copy(draftText = "") else it },
                )
            }
            val numericChatId = conversationId.toLongOrNull()
            val numericMsgId = editing.id.toLongOrNull()
            if (_state.value.isAuthenticated && numericChatId != null && numericMsgId != null) {
                val capturedGen = generationFence.currentGeneration
                viewModelScope.launch {
                    val result = withContext(Dispatchers.IO) {
                        hostRequestResult(
                            "telegram.server.editMessage",
                            JSONObject()
                                .put("chatId", numericChatId)
                                .put("messageId", numericMsgId)
                                .put("text", plain)
                                .put("entities", serializeEntities(entities)),
                        )
                    }
                    if (!generationFence.isCurrent(capturedGen)) return@launch
                    result.onFailure { err ->
                        if (isUnauthorizedError(err)) {
                            handleUnauthorizedSession(err.message ?: "HTTP 401", capturedGen)
                        } else {
                            updateMessageInChat(conversationId, editing.id) { prevMsg }
                            postBanner("编辑消息失败：${err.message ?: "网络异常"}")
                        }
                    }
                }
            } else {
                viewModelScope.launch(Dispatchers.IO) {
                    sendProtocolV2Command(
                        JSONObject()
                            .put("type", "editMessage")
                            .put("conversationId", conversationId)
                            .put("messageId", editing.id)
                            .put("content", JSONObject().put("type", "text").put("data", JSONObject().put("text", JSONObject().put("text", plain).put("entities", serializeEntities(entities))))),
                    )
                }
            }
            return
        }

        val currentChat = _state.value.chats.firstOrNull { it.id == conversationId }
        val (plainText, entities) = TelegramEntitiesEngine.parseMarkdownAndAutoEntities(trimmed)
        val reply = _state.value.replyTarget?.takeIf { it.conversationId == conversationId }
        val replyHeader = reply?.let {
            TelegramReplyHeader(
                replyToMessageId = it.id,
                senderName = it.senderName,
                quoteText = it.text.take(96),
            )
        }
        val effectiveTtl = overrideTtlSeconds ?: currentChat?.ttlSeconds?.takeIf { it > 0 }
        val nowMs = System.currentTimeMillis()
        val clientNonce = UUID.randomUUID().toString()
        val msgId = "msg-${clientNonce.take(10)}"
        val isChannel = currentChat?.kind == ConversationKind.CHANNEL

        // K-F22: Optimistic outgoing message ALWAYS starts as "sending" (NEVER "read"!)
        val newMsg = TelegramMessageItem(
            id = msgId,
            conversationId = conversationId,
            senderId = currentServerUserId?.toString() ?: actorId,
            senderName = _state.value.activeAccount?.displayName ?: "我",
            text = plainText,
            entities = entities,
            contentType = "text",
            outgoing = true,
            time = if (scheduledAtMs != null) "🕒 定时 ${nowTimeLabel(scheduledAtMs)}" else nowTimeLabel(nowMs),
            timestampMs = nowMs,
            deliveryState = "sending",
            silent = silent,
            scheduledAtMs = scheduledAtMs,
            ttlSeconds = effectiveTtl,
            topicId = _state.value.activeTopicId,
            replyHeader = replyHeader,
            viewsCount = if (isChannel) 1 else null,
            commentsCount = if (isChannel) 0 else null,
            clientNonce = clientNonce,
        )

        chatMessagesEpochFor(conversationId).incrementAndGet()
        chatsEpoch.incrementAndGet()
        appendMessageToChat(conversationId, newMsg, clearDraft = true)
        if (_state.value.isAuthenticated && conversationId.toLongOrNull() != null) {
            clearServerDraft(conversationId)
        }
        dispatchOutgoingMessage(conversationId, currentChat, newMsg)
        handleSlashNavigationAndLocalCommands(conversationId, plainText)
    }

    fun retryFailedMessage(conversationId: String, messageId: String) {
        val msg = _state.value.messagesByChat[conversationId]?.firstOrNull { it.id == messageId } ?: return
        if (msg.deliveryState != "failed") return
        chatMessagesEpochFor(conversationId).incrementAndGet()
        val currentChat = _state.value.chats.firstOrNull { it.id == conversationId }
        val retrying = msg.copy(
            deliveryState = "sending",
            clientNonce = msg.clientNonce ?: UUID.randomUUID().toString(),
        )
        updateMessageInChat(conversationId, messageId) { retrying }
        updateChatLastDeliveryState(conversationId, "sending")
        dispatchOutgoingMessage(conversationId, currentChat, retrying)
    }

    private fun dispatchOutgoingMessage(
        conversationId: String,
        currentChat: TelegramChatItem?,
        msg: TelegramMessageItem,
    ) {
        val capturedGen = generationFence.currentGeneration
        viewModelScope.launch {
            val numericChatId = conversationId.toLongOrNull()
            if (_state.value.isAuthenticated && numericChatId != null && currentChat?.isSecretChat != true) {
                val payload = JSONObject()
                    .put("chatId", numericChatId)
                    .put("text", msg.text)
                    .put("entities", serializeEntities(msg.entities))
                    .put("silent", msg.silent)
                msg.clientNonce?.let { payload.put("clientNonce", it) }
                msg.replyHeader?.replyToMessageId?.toLongOrNull()?.let { payload.put("replyTo", it) }
                msg.topicId?.toLongOrNull()?.let { payload.put("topicId", it) }
                msg.scheduledAtMs?.let { payload.put("scheduledAtMs", it) }

                val result = withContext(Dispatchers.IO) {
                    hostRequestResult("telegram.server.sendMessage", payload)
                }
                if (!generationFence.isCurrent(capturedGen)) return@launch
                result.onSuccess { resp ->
                    val serverMsgId = resp.optLong("message_id").takeIf { it > 0L }?.toString()
                        ?: resp.optLong("id").takeIf { it > 0L }?.toString()
                        ?: msg.id
                    val serverDateMs = resp.optLong("date_ms").takeIf { it > 0L } ?: msg.timestampMs
                    val pts = resp.optLong("pts", 0L)
                    if (pts > 0L) chatPtsMap[numericChatId] = maxOf(chatPtsMap[numericChatId] ?: 0L, pts)
                    _state.update { cur ->
                        val existingList = cur.messagesByChat[conversationId].orEmpty()
                        val alreadyHasServerId = serverMsgId != msg.id && existingList.any { it.id == serverMsgId }
                        val updatedList = if (alreadyHasServerId) {
                            existingList.filterNot { it.id == msg.id }.map { existing ->
                                if (existing.id == serverMsgId) {
                                    existing.copy(
                                        deliveryState = if (existing.deliveryState == "read") "read" else "sent",
                                        timestampMs = serverDateMs,
                                    )
                                } else existing
                            }
                        } else {
                            existingList.map { existing ->
                                if (existing.id == msg.id) {
                                    existing.copy(
                                        id = serverMsgId,
                                        deliveryState = if (existing.deliveryState == "read") "read" else "sent",
                                        timestampMs = serverDateMs,
                                        time = nowTimeLabel(serverDateMs),
                                    )
                                } else existing
                            }
                        }
                        val updatedChats = cur.chats.map { c ->
                            if (c.id == conversationId) {
                                c.copy(
                                    lastDeliveryState = if (c.lastDeliveryState == "read") "read" else "sent",
                                    lastMessageAtMs = maxOf(c.lastMessageAtMs, serverDateMs),
                                    time = nowTimeLabel(maxOf(c.lastMessageAtMs, serverDateMs)),
                                )
                            } else c
                        }
                        cur.copy(
                            messagesByChat = cur.messagesByChat + (conversationId to updatedList),
                            chats = sortTelegramChats(updatedChats),
                        )
                    }
                }.onFailure { err ->
                    if (isUnauthorizedError(err)) {
                        handleUnauthorizedSession(err.message ?: "HTTP 401", capturedGen)
                    } else {
                        updateMessageInChat(conversationId, msg.id) { it.copy(deliveryState = "failed") }
                        updateChatLastDeliveryState(conversationId, "failed")
                        postBanner("发送失败：${err.message ?: "网络异常"}")
                    }
                }
            } else if (currentChat?.isSecretChat == true) {
                // Contract #11: telegram.e2ee.sendEncrypted expects {chatId, plaintext, mode}
                val result = withContext(Dispatchers.IO) {
                    hostRequestResult(
                        "telegram.e2ee.sendEncrypted",
                        JSONObject()
                            .put("chatId", conversationId)
                            .put("plaintext", msg.text)
                            .put("mode", "secret_chat"),
                    )
                }
                if (!generationFence.isCurrent(capturedGen)) return@launch
                result.onSuccess {
                    updateMessageInChat(conversationId, msg.id) { it.copy(deliveryState = "sent") }
                    updateChatLastDeliveryState(conversationId, "sent")
                }.onFailure { err ->
                    updateMessageInChat(conversationId, msg.id) { it.copy(deliveryState = "failed") }
                    updateChatLastDeliveryState(conversationId, "failed")
                    postBanner("端到端加密发送失败：${err.message ?: "未知错误"}")
                }
            } else {
                val result = withContext(Dispatchers.IO) {
                    sendProtocolV2Command(
                        JSONObject()
                            .put("type", "sendMessage")
                            .put("conversationId", conversationId)
                            .put("clientMessageId", msg.id)
                            .put(
                                "content",
                                JSONObject().put("type", "text").put(
                                    "data",
                                    JSONObject().put("text", JSONObject().put("text", msg.text).put("entities", serializeEntities(msg.entities))),
                                ),
                            )
                            .put("replyToMessageId", msg.replyHeader?.replyToMessageId ?: JSONObject.NULL)
                            .put("threadRootMessageId", msg.topicId ?: JSONObject.NULL)
                            .put("scheduledAtMs", msg.scheduledAtMs ?: JSONObject.NULL)
                            .put("silent", msg.silent)
                            .put("protectedContent", false),
                    )
                }
                if (!generationFence.isCurrent(capturedGen)) return@launch
                result.onSuccess {
                    updateMessageInChat(conversationId, msg.id) { it.copy(deliveryState = "sent") }
                    updateChatLastDeliveryState(conversationId, "sent")
                }.onFailure { err ->
                    updateMessageInChat(conversationId, msg.id) { it.copy(deliveryState = "failed") }
                    updateChatLastDeliveryState(conversationId, "failed")
                    postBanner("本地发送失败：${err.message ?: "未知错误"}")
                }
            }
        }
    }

    private fun updateChatLastDeliveryState(conversationId: String, deliveryState: String) {
        _state.update { cur ->
            cur.copy(
                chats = cur.chats.map { c ->
                    if (c.id == conversationId) c.copy(lastDeliveryState = deliveryState) else c
                },
            )
        }
    }

    private fun handleSlashNavigationAndLocalCommands(conversationId: String, plainText: String) {
        when {
            plainText.startsWith("/wallet") -> navigateTo(TelegramSubScreen.TON_WALLET)
            plainText.startsWith("/dharma") -> navigateTo(TelegramSubScreen.GLOBAL_DHARMA)
            plainText.startsWith("/hermes") -> navigateTo(TelegramSubScreen.HERMES_STUDIO)
            plainText.startsWith("/magpie") -> navigateTo(TelegramSubScreen.MAGPIE_GATEWAY)
            plainText.startsWith("/computer") -> navigateTo(TelegramSubScreen.CLOUD_COMPUTER)
            plainText.startsWith("/newbot") -> navigateTo(TelegramSubScreen.BOTFATHER)
            plainText.startsWith("/mika") -> {
                val taskPrompt = plainText.removePrefix("/mika").trim()
                if (taskPrompt.isNotBlank()) {
                    runWithMikaFromChat(conversationId, taskPrompt)
                } else {
                    navigateTo(TelegramSubScreen.MIKA_WORKSPACE)
                }
            }
        }
    }

    /**
     * Contract #18: mika.registerTaskEvent expects {taskId, stepIndex, phase, summary}.
     * Zero dead "bot-mika-phone" IDs and zero fake delay(450) "ready" fallback when mikaBridge == null.
     */
    fun runWithMikaFromChat(conversationId: String, taskPrompt: String) {
        val cleanPrompt = taskPrompt.trim().ifBlank { return }
        val taskId = "mika-task-${UUID.randomUUID().toString().take(8)}"
        val cardId = "msg-$taskId"
        val bridge = mikaBridge

        if (bridge == null) {
            val unavailableCard = TelegramMessageItem(
                id = cardId,
                conversationId = conversationId,
                senderId = "mika-local-agent",
                senderName = "Mika 手机操控智能体",
                text = "Mika 本机操控服务当前不可用，请确认在完整的 Fabushi Android 运行环境中开启相关权限。",
                contentType = "mikaTask",
                outgoing = false,
                time = nowTimeLabel(),
                deliveryState = "failed",
                mikaTaskId = taskId,
                mikaTaskStatus = "unavailable",
                mikaStepLogs = listOf("Mika HostChannel 未连接"),
            )
            appendMessageToChat(conversationId, unavailableCard, clearDraft = false)
            postBanner("Mika 手机操控桥接不可用")
            return
        }

        val taskCard = TelegramMessageItem(
            id = cardId,
            conversationId = conversationId,
            senderId = "mika-local-agent",
            senderName = "Mika 手机操控智能体",
            text = "🤖 已受理本机操控任务：「$cleanPrompt」",
            contentType = "mikaTask",
            outgoing = false,
            time = nowTimeLabel(),
            deliveryState = "sent",
            mikaTaskId = taskId,
            mikaTaskStatus = "running",
            mikaStepLogs = listOf("已投递至 Mika HostChannel"),
            inlineKeyboard = listOf(
                InlineKeyboardRow(
                    listOf(
                        InlineKeyboardButtonModel("open-mika", "📱 打开 Mika 操控中心", InlineKeyboardButtonKind.MIKA_ACTION, "open_workspace"),
                        InlineKeyboardButtonModel("stop-mika", "⏹ 紧急停止", InlineKeyboardButtonKind.MIKA_ACTION, "stop_task"),
                    ),
                ),
            ),
        )
        appendMessageToChat(conversationId, taskCard, clearDraft = false)

        viewModelScope.launch(Dispatchers.IO) {
            hostRequestResult(
                "mika.registerTaskEvent",
                JSONObject()
                    .put("taskId", taskId)
                    .put("stepIndex", mikaStepCounter.incrementAndGet())
                    .put("phase", "started")
                    .put("summary", cleanPrompt),
            )
        }

        bridge.submitPhoneTask(cleanPrompt, conversationId) { stepUpdate ->
            updateMessageInChat(conversationId, cardId) { existing ->
                existing.copy(
                    mikaStepLogs = (existing.mikaStepLogs + stepUpdate).takeLast(8),
                    mikaTaskStatus = when {
                        stepUpdate.contains("完成") || stepUpdate.contains("Done", ignoreCase = true) -> "completed"
                        stepUpdate.contains("失败") || stepUpdate.contains("异常") || stepUpdate.contains("取消") -> "failed"
                        stepUpdate.contains("待确认") || stepUpdate.contains("接管") -> "waiting_confirmation"
                        else -> "running"
                    },
                )
            }
        }
        refreshMikaSnapshot()
    }

    fun resolveMikaConfirmation(approved: Boolean, threadId: String? = null, askId: String? = null) {
        val bridge = mikaBridge
        if (bridge == null) {
            postBanner("Mika 手机操控桥接不可用")
            return
        }
        bridge.resolveConfirmation(approved, threadId, askId)
        refreshMikaSnapshot()
        postBanner(if (approved) "已批准 Mika 操作确认" else "已拒绝 Mika 操作请求")
    }

    fun answerMikaInput(text: String, threadId: String? = null, askId: String? = null): Boolean {
        val bridge = mikaBridge ?: return false
        val ok = bridge.answerAsk(text, threadId, askId)
        if (ok) refreshMikaSnapshot()
        return ok
    }

    fun toggleReaction(conversationId: String, messageId: String, emoji: String) {
        val reactionKey = "$conversationId:$messageId"
        val prevMsg = _state.value.messagesByChat[conversationId]?.firstOrNull { it.id == messageId } ?: return
        val prevReactions = prevMsg.reactions
        chatMessagesEpochFor(conversationId).incrementAndGet()

        val token = inFlightMutationSeq.incrementAndGet()
        inFlightReactions[reactionKey] = token
        updateMessageInChat(conversationId, messageId) { msg ->
            val existing = msg.reactions.firstOrNull { it.reaction == emoji }
            val updated = when {
                existing == null -> msg.reactions + ChatReaction(emoji, 1, true)
                existing.chosenByMe && existing.count <= 1 -> msg.reactions.filterNot { it.reaction == emoji }
                existing.chosenByMe -> msg.reactions.map { if (it.reaction == emoji) it.copy(count = it.count - 1, chosenByMe = false) else it }
                else -> msg.reactions.map { if (it.reaction == emoji) it.copy(count = it.count + 1, chosenByMe = true) else it }
            }
            msg.copy(reactions = updated)
        }

        val numericChatId = conversationId.toLongOrNull()
        val numericMsgId = messageId.toLongOrNull()
        if (_state.value.isAuthenticated && numericChatId != null && numericMsgId != null) {
            val capturedGen = generationFence.currentGeneration
            viewModelScope.launch {
                val result = withContext(Dispatchers.IO) {
                    hostRequestResult(
                        "telegram.server.sendReaction",
                        JSONObject()
                            .put("chatId", numericChatId)
                            .put("messageId", numericMsgId)
                            .put("emoji", emoji)
                            .put("action", "toggle"),
                    )
                }
                if (result.isFailure && isUnauthorizedError(result.exceptionOrNull())) {
                    inFlightReactions.remove(reactionKey, token)
                    handleUnauthorizedSession(result.exceptionOrNull()?.message ?: "HTTP 401", capturedGen)
                    return@launch
                }
                if (!inFlightReactions.remove(reactionKey, token)) return@launch
                if (!generationFence.isCurrent(capturedGen)) return@launch
                result.onSuccess { resp ->
                    val arr = resp.optJSONArray("reactions")
                    if (arr != null) {
                        val authoritative = parseReactionsArray(arr)
                        updateMessageInChat(conversationId, messageId) { it.copy(reactions = authoritative) }
                    }
                }.onFailure { err ->
                    updateMessageInChat(conversationId, messageId) { it.copy(reactions = prevReactions) }
                    postBanner("表情回应失败：${err.message ?: "网络异常"}")
                }
            }
        } else {
            inFlightReactions.remove(reactionKey, token)
        }
    }

    fun fetchPinnedMessages(chatId: String, numericId: Long) {
        val capturedGen = generationFence.currentGeneration
        val capturedPinsEpoch = chatPinsEpochFor(chatId).get()
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                hostRequestResult("telegram.server.listPinnedMessages", JSONObject().put("chatId", numericId))
            }
            if (!generationFence.isCurrent(capturedGen)) return@launch
            result.onSuccess { resp ->
                if (inFlightMessagePins.containsKey(chatId) || chatPinsEpochFor(chatId).get() != capturedPinsEpoch) {
                    return@onSuccess
                }
                val pinIds = parsePinnedMessageIds(resp)
                applyPinnedMessageIdsToChat(chatId, pinIds)
            }.onFailure { err ->
                if (isUnauthorizedError(err)) handleUnauthorizedSession(err.message ?: "HTTP 401", capturedGen)
            }
        }
    }

    fun togglePinMessage(conversationId: String, messageId: String) {
        chatPinsEpochFor(conversationId).incrementAndGet()
        chatsEpoch.incrementAndGet()
        val curState = _state.value
        val chat = curState.chats.firstOrNull { it.id == conversationId }
        val msgs = curState.messagesByChat[conversationId].orEmpty()
        val target = msgs.firstOrNull { it.id == messageId }
        val prevPins = chat?.pinnedMessageIds.orEmpty()
        val isCurrentlyPinned = target?.pinned == true || messageId in prevPins
        val nextPinned = !isCurrentlyPinned
        val optimisticPins = if (nextPinned) (prevPins + messageId).distinct() else prevPins.filterNot { it == messageId }

        val numericChatId = conversationId.toLongOrNull()
        val numericMsgId = messageId.toLongOrNull()
        if (curState.isAuthenticated && numericChatId != null && numericMsgId != null) {
            val entry = inFlightMutationSeq.incrementAndGet() to optimisticPins.toSet()
            inFlightMessagePins[conversationId] = entry
            applyPinnedMessageIdsToChat(conversationId, optimisticPins)
            val capturedGen = generationFence.currentGeneration
            viewModelScope.launch {
                val method = if (nextPinned) "telegram.server.pinMessage" else "telegram.server.unpinMessage"
                val result = withContext(Dispatchers.IO) {
                    hostRequestResult(
                        method,
                        JSONObject().put("chatId", numericChatId).put("messageId", numericMsgId),
                    )
                }
                if (result.isFailure && isUnauthorizedError(result.exceptionOrNull())) {
                    inFlightMessagePins.remove(conversationId, entry)
                    handleUnauthorizedSession(result.exceptionOrNull()?.message ?: "HTTP 401", capturedGen)
                    return@launch
                }
                if (!inFlightMessagePins.remove(conversationId, entry)) return@launch
                if (!generationFence.isCurrent(capturedGen)) return@launch
                result.onSuccess { resp ->
                    val serverPins = parsePinnedMessageIds(resp)
                    applyPinnedMessageIdsToChat(conversationId, serverPins)
                    postBanner(if (nextPinned) "消息已置顶" else "已取消置顶")
                }.onFailure { err ->
                    applyPinnedMessageIdsToChat(conversationId, prevPins)
                    postBanner("置顶消息失败：${err.message ?: "无权限或网络异常"}")
                }
            }
        } else {
            applyPinnedMessageIdsToChat(conversationId, optimisticPins)
            postBanner(if (nextPinned) "消息已置顶" else "已取消置顶")
        }
    }

    private fun applyPinnedMessageIdsToChat(chatId: String, pinnedIds: List<String>) {
        val pinSet = pinnedIds.toSet()
        _state.update { current ->
            val updatedMsgs = current.messagesByChat[chatId]?.map { m ->
                m.copy(pinned = m.id in pinSet)
            }
            val updatedChats = current.chats.map { c ->
                if (c.id == chatId) c.copy(pinnedMessageIds = pinnedIds) else c
            }
            current.copy(
                chats = updatedChats,
                messagesByChat = if (updatedMsgs != null) current.messagesByChat + (chatId to updatedMsgs) else current.messagesByChat,
            )
        }
    }

    fun cyclePinnedBanner(conversationId: String): String? {
        val chat = _state.value.chats.firstOrNull { it.id == conversationId } ?: return null
        val pins = chat.pinnedMessageIds
        if (pins.isEmpty()) return null
        val currentIdx = _state.value.pinnedBannerIndexByChat[conversationId] ?: 0
        val nextIdx = (currentIdx + 1) % pins.size
        _state.update { it.copy(pinnedBannerIndexByChat = it.pinnedBannerIndexByChat + (conversationId to nextIdx)) }
        return pins[nextIdx]
    }

    fun deleteMessage(conversationId: String, messageId: String, forEveryone: Boolean) {
        chatMessagesEpochFor(conversationId).incrementAndGet()
        chatPinsEpochFor(conversationId).incrementAndGet()
        var removedMessage: TelegramMessageItem? = null
        var wasPinned = false
        _state.update { current ->
            val currentMsgs = current.messagesByChat[conversationId].orEmpty()
            removedMessage = currentMsgs.firstOrNull { it.id == messageId }
            wasPinned = current.chats.firstOrNull { it.id == conversationId }?.pinnedMessageIds?.contains(messageId) == true
            val updated = currentMsgs.filterNot { it.id == messageId }
            val updatedChats = current.chats.map { c ->
                if (c.id == conversationId) c.copy(pinnedMessageIds = c.pinnedMessageIds.filterNot { it == messageId }) else c
            }
            current.copy(
                messagesByChat = current.messagesByChat + (conversationId to updated),
                chats = updatedChats,
            )
        }
        val numericChatId = conversationId.toLongOrNull()
        val numericMsgId = messageId.toLongOrNull()
        if (_state.value.isAuthenticated && numericChatId != null && numericMsgId != null) {
            val capturedGen = generationFence.currentGeneration
            viewModelScope.launch {
                val result = withContext(Dispatchers.IO) {
                    hostRequestResult(
                        "telegram.server.deleteMessage",
                        JSONObject()
                            .put("chatId", numericChatId)
                            .put("messageId", numericMsgId)
                            .put("revoke", forEveryone),
                    )
                }
                if (!generationFence.isCurrent(capturedGen)) return@launch
                result.onSuccess {
                    postBanner(if (forEveryone) "已为双方撤回消息" else "已删除消息")
                }.onFailure { err ->
                    if (isUnauthorizedError(err)) {
                        handleUnauthorizedSession(err.message ?: "HTTP 401", capturedGen)
                    } else {
                        val restoreMsg = removedMessage
                        val restorePin = wasPinned
                        _state.update { current ->
                            val existingMsgs = current.messagesByChat[conversationId].orEmpty()
                            val restoredMsgs = if (restoreMsg != null && existingMsgs.none { it.id == restoreMsg.id }) {
                                (existingMsgs + restoreMsg).sortedWith(
                                    compareBy<TelegramMessageItem> { it.timestampMs }
                                        .thenBy { it.id.toLongOrNull() ?: Long.MAX_VALUE },
                                )
                            } else {
                                existingMsgs
                            }
                            val restoredChats = if (restorePin) {
                                current.chats.map { c ->
                                    if (c.id == conversationId && messageId !in c.pinnedMessageIds) {
                                        c.copy(pinnedMessageIds = c.pinnedMessageIds + messageId)
                                    } else c
                                }
                            } else {
                                current.chats
                            }
                            current.copy(
                                messagesByChat = current.messagesByChat + (conversationId to restoredMsgs),
                                chats = restoredChats,
                                toastBanner = "删除消息失败：${err.message ?: "网络异常"}",
                            )
                        }
                    }
                }
            }
        } else {
            viewModelScope.launch(Dispatchers.IO) {
                sendProtocolV2Command(
                    JSONObject()
                        .put("type", "deleteMessages")
                        .put("conversationId", conversationId)
                        .put("messageIds", JSONArray().put(messageId))
                        .put("forEveryone", forEveryone),
                )
            }
            postBanner(if (forEveryone) "已撤回消息" else "已删除消息")
        }
    }

    fun forwardMessage(sourceMessage: TelegramMessageItem, targetConversationId: String) {
        val targetChat = _state.value.chats.firstOrNull { it.id == targetConversationId } ?: return
        chatMessagesEpochFor(targetConversationId).incrementAndGet()
        chatsEpoch.incrementAndGet()
        val nowMs = System.currentTimeMillis()
        val clientNonce = UUID.randomUUID().toString()
        val forwarded = sourceMessage.copy(
            id = "msg-fwd-${clientNonce.take(8)}",
            conversationId = targetConversationId,
            outgoing = true,
            time = nowTimeLabel(nowMs),
            timestampMs = nowMs,
            deliveryState = "sending",
            forwardHeader = TelegramForwardHeader(originName = sourceMessage.senderName, dateLabel = sourceMessage.time),
            pinned = false,
            reactions = emptyList(),
            clientNonce = clientNonce,
        )
        appendMessageToChat(targetConversationId, forwarded, clearDraft = false)
        val targetNumId = targetConversationId.toLongOrNull()
        val srcChatNumId = sourceMessage.conversationId.toLongOrNull()
        val srcMsgNumId = sourceMessage.id.toLongOrNull()
        if (_state.value.isAuthenticated && targetNumId != null) {
            val capturedGen = generationFence.currentGeneration
            viewModelScope.launch {
                val payload = JSONObject()
                    .put("chatId", targetNumId)
                    .put("text", sourceMessage.text)
                    .put("clientNonce", clientNonce)
                if (srcChatNumId != null && srcMsgNumId != null) {
                    payload.put("forwardFromChatId", srcChatNumId).put("forwardFromMsgId", srcMsgNumId)
                }
                val result = withContext(Dispatchers.IO) {
                    hostRequestResult("telegram.server.sendMessage", payload)
                }
                if (!generationFence.isCurrent(capturedGen)) return@launch
                result.onSuccess { resp ->
                    val srvId = resp.optLong("message_id").takeIf { it > 0L }?.toString() ?: forwarded.id
                    updateMessageInChat(targetConversationId, forwarded.id) { it.copy(id = srvId, deliveryState = "sent") }
                    postBanner("已转发至「${targetChat.title}」")
                }.onFailure { err ->
                    updateMessageInChat(targetConversationId, forwarded.id) { it.copy(deliveryState = "failed") }
                    postBanner("转发失败：${err.message ?: "网络异常"}")
                }
            }
        } else {
            updateMessageInChat(targetConversationId, forwarded.id) { it.copy(deliveryState = "sent") }
            postBanner("已转发至「${targetChat.title}」")
        }
    }

    /**
     * K-F20: NEVER prepend fake "【即时中文译文】" / "[Instant Translation · EN] " strings!
     * Route through real Magpie LLM (magpie.chatComplete) and fail honestly when unconfigured.
     */
    fun translateMessage(conversationId: String, messageId: String) {
        val target = _state.value.messagesByChat[conversationId]?.firstOrNull { it.id == messageId } ?: return
        if (target.translatedText != null) {
            updateMessageInChat(conversationId, messageId) { it.copy(translatedText = null) }
            return
        }
        val targetLang = if (_state.value.uiLanguage.startsWith("en", ignoreCase = true)) "English" else "Simplified Chinese"
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                hostRequestResult(
                    "magpie.chatComplete",
                    JSONObject().put(
                        "messages",
                        JSONArray()
                            .put(JSONObject().put("role", "system").put("content", "Translate the user message into $targetLang. Output only the translation."))
                            .put(JSONObject().put("role", "user").put("content", target.text)),
                    ),
                )
            }
            result.onSuccess { resp ->
                val reply = resp.optNonNullString("reply")
                    .ifBlank { resp.optNonNullString("content") }
                    .ifBlank { resp.optNonNullString("text") }
                    .trim()
                if (reply.isNotBlank()) {
                    updateMessageInChat(conversationId, messageId) { it.copy(translatedText = reply) }
                } else {
                    postBanner("翻译服务未返回有效译文")
                }
            }.onFailure { err ->
                postBanner("翻译服务未配置或调用失败：${err.message ?: "请先在 Magpie 网关配置 API Key"}")
            }
        }
    }

    fun sendSticker(conversationId: String, emoji: String, packTitle: String) {
        sendComposerMessage(conversationId, "$emoji")
    }

    fun sendGif(conversationId: String, gif: GifItemModel) {
        sendComposerMessage(conversationId, "${gif.previewGlyph} ${gif.title}")
    }

    fun sendPollOrQuiz(
        conversationId: String,
        question: String,
        options: List<String>,
        isAnonymous: Boolean,
        multipleAnswers: Boolean,
        isQuiz: Boolean,
        correctIndex: Int,
        explanation: String?,
    ) {
        val cleanOpts = options.map { it.trim() }.filter { it.isNotEmpty() }
        if (question.isBlank() || cleanOpts.size < 2) return
        chatMessagesEpochFor(conversationId).incrementAndGet()
        chatsEpoch.incrementAndGet()
        val numericChatId = conversationId.toLongOrNull()
        if (_state.value.isAuthenticated && numericChatId != null) {
            val capturedGen = generationFence.currentGeneration
            viewModelScope.launch {
                val payload = JSONObject()
                    .put("chatId", numericChatId)
                    .put("question", question.trim())
                    .put("options", JSONArray(cleanOpts))
                    .put("isAnonymous", isAnonymous)
                    .put("multipleChoice", multipleAnswers)
                    .put("isQuiz", isQuiz)
                    .put("clientNonce", UUID.randomUUID().toString())
                if (isQuiz) payload.put("correctOptionId", correctIndex.coerceIn(0, cleanOpts.lastIndex))
                explanation?.trim()?.takeIf { it.isNotBlank() }?.let { payload.put("solution", it) }

                val result = withContext(Dispatchers.IO) {
                    hostRequestResult("telegram.server.createPoll", payload)
                }
                if (!generationFence.isCurrent(capturedGen)) return@launch
                result.onSuccess { resp ->
                    val pollObj = resp.optJSONObject("poll")
                    val pollId = pollObj?.optNonNullString("id")?.ifBlank { null }
                        ?: pollObj?.optLong("id")?.takeIf { it > 0L }?.toString()
                        ?: resp.optNonNullString("poll_id").ifBlank { null }
                    val msgId = resp.optLong("message_id").takeIf { it > 0L }?.toString()
                        ?: "msg-poll-${UUID.randomUUID().toString().take(8)}"
                    val parsedPoll = parsePollObject(pollObj) ?: buildLocalPollModel(question, cleanOpts, isAnonymous, multipleAnswers, isQuiz, correctIndex, explanation)
                    val nowMs = System.currentTimeMillis()
                    appendMessageToChat(
                        conversationId,
                        TelegramMessageItem(
                            id = msgId,
                            conversationId = conversationId,
                            senderId = currentServerUserId?.toString() ?: actorId,
                            senderName = _state.value.activeAccount?.displayName ?: "我",
                            text = "📊 ${parsedPoll.question}",
                            contentType = "poll",
                            outgoing = true,
                            time = nowTimeLabel(nowMs),
                            timestampMs = nowMs,
                            deliveryState = "sent",
                            poll = parsedPoll,
                            pollId = pollId,
                        ),
                        clearDraft = false,
                    )
                }.onFailure { err ->
                    if (isUnauthorizedError(err)) {
                        handleUnauthorizedSession(err.message ?: "HTTP 401", capturedGen)
                    } else {
                        postBanner("创建投票失败：${err.message ?: "网络异常"}")
                    }
                }
            }
            return
        }

        val poll = buildLocalPollModel(question, cleanOpts, isAnonymous, multipleAnswers, isQuiz, correctIndex, explanation)
        appendMessageToChat(
            conversationId,
            TelegramMessageItem(
                id = "msg-poll-${UUID.randomUUID().toString().take(8)}",
                conversationId = conversationId,
                senderId = actorId,
                senderName = "我",
                text = "📊 ${poll.question}",
                contentType = "poll",
                outgoing = true,
                time = nowTimeLabel(),
                deliveryState = "sent",
                poll = poll,
            ),
            clearDraft = false,
        )
    }

    private fun buildLocalPollModel(
        question: String,
        cleanOpts: List<String>,
        isAnonymous: Boolean,
        multipleAnswers: Boolean,
        isQuiz: Boolean,
        correctIndex: Int,
        explanation: String?,
    ): TelegramPollModel {
        val correctId = if (isQuiz) "opt-${correctIndex.coerceIn(0, cleanOpts.lastIndex) + 1}" else null
        return TelegramPollModel(
            question = question.trim(),
            options = cleanOpts.mapIndexed { idx, label ->
                val id = "opt-${idx + 1}"
                TelegramPollOption(id = id, text = label, voterCount = 0, chosen = false, isCorrect = id == correctId)
            },
            isAnonymous = isAnonymous,
            multipleAnswers = multipleAnswers,
            isQuiz = isQuiz,
            correctOptionId = correctId,
            explanation = explanation?.trim()?.ifBlank { null },
        )
    }

    fun votePoll(conversationId: String, messageId: String, optionId: String) {
        val prevMsg = _state.value.messagesByChat[conversationId]?.firstOrNull { it.id == messageId } ?: return
        val prevPoll = prevMsg.poll ?: return
        if (prevPoll.isQuiz && prevPoll.hasVoted) return

        val optionIdx = prevPoll.options.indexOfFirst { it.id == optionId }
        if (optionIdx < 0) return

        chatMessagesEpochFor(conversationId).incrementAndGet()
        val optimisticOptions = prevPoll.options.map { opt ->
            when {
                opt.id == optionId && !opt.chosen -> opt.copy(chosen = true, voterCount = opt.voterCount + 1)
                opt.id == optionId && opt.chosen && !prevPoll.isQuiz -> opt.copy(chosen = false, voterCount = (opt.voterCount - 1).coerceAtLeast(0))
                !prevPoll.multipleAnswers && opt.chosen -> opt.copy(chosen = false, voterCount = (opt.voterCount - 1).coerceAtLeast(0))
                else -> opt
            }
        }
        updateMessageInChat(conversationId, messageId) { it.copy(poll = prevPoll.copy(options = optimisticOptions)) }

        val serverPollId = prevMsg.pollId
        if (_state.value.isAuthenticated && !serverPollId.isNullOrBlank()) {
            val capturedGen = generationFence.currentGeneration
            val chosenIndices = optimisticOptions.mapIndexedNotNull { idx, opt -> if (opt.chosen) idx else null }
            viewModelScope.launch {
                val result = withContext(Dispatchers.IO) {
                    hostRequestResult(
                        "telegram.server.votePoll",
                        JSONObject()
                            .put("pollId", serverPollId)
                            .put("optionIds", JSONArray(chosenIndices)),
                    )
                }
                if (!generationFence.isCurrent(capturedGen)) return@launch
                result.onSuccess { resp ->
                    val pollObj = resp.optJSONObject("poll") ?: resp
                    parsePollObject(pollObj)?.let { updated ->
                        updateMessageInChat(conversationId, messageId) { it.copy(poll = updated) }
                    }
                }.onFailure { err ->
                    if (isUnauthorizedError(err)) {
                        handleUnauthorizedSession(err.message ?: "HTTP 401", capturedGen)
                    } else {
                        updateMessageInChat(conversationId, messageId) { it.copy(poll = prevPoll) }
                        postBanner("投票提交失败：${err.message ?: "网络异常"}")
                    }
                }
            }
        }
    }

    fun hasRecordAudioPermission(): Boolean =
        ContextCompat.checkSelfPermission(appContext, Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED

    fun canSendCurrentLocation(): Boolean {
        val fine = ContextCompat.checkSelfPermission(appContext, Manifest.permission.ACCESS_FINE_LOCATION) == PackageManager.PERMISSION_GRANTED
        val coarse = ContextCompat.checkSelfPermission(appContext, Manifest.permission.ACCESS_COARSE_LOCATION) == PackageManager.PERMISSION_GRANTED
        return fine || coarse
    }

    fun sendLocation(conversationId: String, latitude: Double, longitude: Double, venueTitle: String = "实时共享位置") {
        if (!canSendCurrentLocation()) {
            postBanner("未授予位置权限，无法发送定位信息")
            return
        }
        if (latitude !in -90.0..90.0 || longitude !in -180.0..180.0 || (latitude == 0.0 && longitude == 0.0)) {
            postBanner("无效的经纬度坐标")
            return
        }
        chatMessagesEpochFor(conversationId).incrementAndGet()
        chatsEpoch.incrementAndGet()
        val nowMs = System.currentTimeMillis()
        val clientNonce = UUID.randomUUID().toString()
        val localMsgId = "msg-loc-${clientNonce.take(8)}"
        val numericChatId = conversationId.toLongOrNull()
        val isAuthedServerChat = _state.value.isAuthenticated && numericChatId != null
        val displayText = "📍 $venueTitle (${"%.5f".format(latitude)}, ${"%.5f".format(longitude)})"
        appendMessageToChat(
            conversationId,
            TelegramMessageItem(
                id = localMsgId,
                conversationId = conversationId,
                senderId = currentServerUserId?.toString() ?: actorId,
                senderName = _state.value.activeAccount?.displayName ?: "我",
                text = displayText,
                contentType = "location",
                outgoing = true,
                time = nowTimeLabel(nowMs),
                timestampMs = nowMs,
                deliveryState = if (isAuthedServerChat) "sending" else "sent",
                latitude = latitude,
                longitude = longitude,
                venueTitle = venueTitle,
                clientNonce = clientNonce,
            ),
            clearDraft = false,
        )
        if (isAuthedServerChat && numericChatId != null) {
            val capturedGen = generationFence.currentGeneration
            viewModelScope.launch {
                val contentJson = JSONObject()
                    .put("type", "location")
                    .put(
                        "data",
                        JSONObject()
                            .put("latitude", latitude)
                            .put("longitude", longitude)
                            .put("title", venueTitle),
                    )
                val payload = JSONObject()
                    .put("chatId", numericChatId)
                    .put("text", displayText)
                    .put("content", contentJson)
                    .put("clientNonce", clientNonce)
                val result = withContext(Dispatchers.IO) {
                    hostRequestResult("telegram.server.sendMessage", payload)
                }
                if (!generationFence.isCurrent(capturedGen)) return@launch
                result.onSuccess { resp ->
                    val serverMsgId = resp.optLong("message_id").takeIf { it > 0L }?.toString()
                        ?: resp.optLong("id").takeIf { it > 0L }?.toString()
                        ?: localMsgId
                    val serverDateMs = resp.optLong("date_ms").takeIf { it > 0L } ?: nowMs
                    reconcileOutgoingMessageSent(conversationId, localMsgId, serverMsgId, serverDateMs)
                }.onFailure { err ->
                    if (isUnauthorizedError(err)) {
                        handleUnauthorizedSession(err.message ?: "HTTP 401", capturedGen)
                    } else {
                        updateMessageInChat(conversationId, localMsgId) { it.copy(deliveryState = "failed") }
                        postBanner("发送位置失败：${err.message ?: "网络异常"}")
                    }
                }
            }
        }
    }

    fun sendContactCard(conversationId: String, name: String, phone: String) {
        val cleanName = name.trim()
        if (cleanName.isBlank()) return
        val cleanPhone = phone.trim().ifBlank { null }
        chatMessagesEpochFor(conversationId).incrementAndGet()
        chatsEpoch.incrementAndGet()
        val nowMs = System.currentTimeMillis()
        val clientNonce = UUID.randomUUID().toString()
        val localMsgId = "msg-ctc-${clientNonce.take(8)}"
        val numericChatId = conversationId.toLongOrNull()
        val isAuthedServerChat = _state.value.isAuthenticated && numericChatId != null
        val displayText = if (cleanPhone != null) "👤 $cleanName ($cleanPhone)" else "👤 $cleanName"
        appendMessageToChat(
            conversationId,
            TelegramMessageItem(
                id = localMsgId,
                conversationId = conversationId,
                senderId = currentServerUserId?.toString() ?: actorId,
                senderName = _state.value.activeAccount?.displayName ?: "我",
                text = displayText,
                contentType = "contact",
                outgoing = true,
                time = nowTimeLabel(nowMs),
                timestampMs = nowMs,
                deliveryState = if (isAuthedServerChat) "sending" else "sent",
                contactName = cleanName,
                contactPhone = cleanPhone,
                clientNonce = clientNonce,
            ),
            clearDraft = false,
        )
        if (isAuthedServerChat && numericChatId != null) {
            val capturedGen = generationFence.currentGeneration
            viewModelScope.launch {
                val dataObj = JSONObject().put("display_name", cleanName)
                cleanPhone?.let { dataObj.put("phone", it) }
                val contentJson = JSONObject()
                    .put("type", "contact")
                    .put("data", dataObj)
                val payload = JSONObject()
                    .put("chatId", numericChatId)
                    .put("text", displayText)
                    .put("content", contentJson)
                    .put("clientNonce", clientNonce)
                val result = withContext(Dispatchers.IO) {
                    hostRequestResult("telegram.server.sendMessage", payload)
                }
                if (!generationFence.isCurrent(capturedGen)) return@launch
                result.onSuccess { resp ->
                    val serverMsgId = resp.optLong("message_id").takeIf { it > 0L }?.toString()
                        ?: resp.optLong("id").takeIf { it > 0L }?.toString()
                        ?: localMsgId
                    val serverDateMs = resp.optLong("date_ms").takeIf { it > 0L } ?: nowMs
                    reconcileOutgoingMessageSent(conversationId, localMsgId, serverMsgId, serverDateMs)
                }.onFailure { err ->
                    if (isUnauthorizedError(err)) {
                        handleUnauthorizedSession(err.message ?: "HTTP 401", capturedGen)
                    } else {
                        updateMessageInChat(conversationId, localMsgId) { it.copy(deliveryState = "failed") }
                        postBanner("发送联系人名片失败：${err.message ?: "网络异常"}")
                    }
                }
            }
        }
    }

    private fun reconcileOutgoingMessageSent(
        conversationId: String,
        localMsgId: String,
        serverMsgId: String,
        serverDateMs: Long,
        mediaBlobId: String? = null,
        mediaThumbnailDataUrl: String? = null,
    ) {
        _state.update { cur ->
            val existingList = cur.messagesByChat[conversationId].orEmpty()
            val alreadyHasServerId = serverMsgId != localMsgId && existingList.any { it.id == serverMsgId }
            val localItem = existingList.firstOrNull { it.id == localMsgId }
            val updatedList = if (alreadyHasServerId) {
                val localWasRead = localItem?.deliveryState == "read"
                existingList.filterNot { it.id == localMsgId }.map { existing ->
                    if (existing.id == serverMsgId) {
                        existing.copy(
                            deliveryState = if (existing.deliveryState == "read" || localWasRead) "read" else "sent",
                            timestampMs = serverDateMs,
                            mediaBlobId = existing.mediaBlobId ?: mediaBlobId ?: localItem?.mediaBlobId,
                            mediaThumbnailDataUrl = existing.mediaThumbnailDataUrl ?: mediaThumbnailDataUrl ?: localItem?.mediaThumbnailDataUrl,
                        )
                    } else existing
                }
            } else {
                existingList.map { existing ->
                    if (existing.id == localMsgId) {
                        existing.copy(
                            id = serverMsgId,
                            deliveryState = if (existing.deliveryState == "read") "read" else "sent",
                            timestampMs = serverDateMs,
                            time = nowTimeLabel(serverDateMs),
                            mediaBlobId = mediaBlobId ?: existing.mediaBlobId,
                            mediaThumbnailDataUrl = mediaThumbnailDataUrl ?: existing.mediaThumbnailDataUrl,
                        )
                    } else existing
                }
            }
            cur.copy(messagesByChat = cur.messagesByChat + (conversationId to updatedList))
        }
    }

    fun sendMediaAttachment(conversationId: String, fileName: String, mimeType: String, bytes: ByteArray) {
        if (bytes.isEmpty()) {
            postBanner("附件内容为空")
            return
        }
        chatMessagesEpochFor(conversationId).incrementAndGet()
        chatsEpoch.incrementAndGet()
        val localBlobId = "blob-${UUID.randomUUID()}"
        cachedVoiceBlobs[localBlobId] = bytes
        val contentType = when {
            mimeType.startsWith("image/") -> "photo"
            mimeType.startsWith("video/") -> "video"
            mimeType.startsWith("audio/") -> "audio"
            else -> "document"
        }
        val nowMs = System.currentTimeMillis()
        val clientNonce = UUID.randomUUID().toString()
        val msgId = "msg-med-${clientNonce.take(8)}"
        val optimistic = TelegramMessageItem(
            id = msgId,
            conversationId = conversationId,
            senderId = currentServerUserId?.toString() ?: actorId,
            senderName = _state.value.activeAccount?.displayName ?: "我",
            text = fileName,
            contentType = contentType,
            outgoing = true,
            time = nowTimeLabel(nowMs),
            timestampMs = nowMs,
            deliveryState = "sending",
            mediaFileName = fileName,
            mediaBlobId = localBlobId,
            mediaMimeType = mimeType,
            mediaSizeBytes = bytes.size,
            clientNonce = clientNonce,
        )
        appendMessageToChat(conversationId, optimistic, clearDraft = false)

        val numericChatId = conversationId.toLongOrNull()
        if (_state.value.isAuthenticated && numericChatId != null) {
            val capturedGen = generationFence.currentGeneration
            viewModelScope.launch {
                val b64 = Base64.getEncoder().encodeToString(bytes)
                val uploadRes = withContext(Dispatchers.IO) {
                    hostRequestResult(
                        "telegram.server.uploadMedia",
                        JSONObject()
                            .put("dataBase64", b64)
                            .put("filename", fileName)
                            .put("mimeType", mimeType)
                            .put("chatId", numericChatId),
                    )
                }
                if (!generationFence.isCurrent(capturedGen)) return@launch
                val uploaded = uploadRes.getOrNull()
                if (uploaded == null) {
                    val err = uploadRes.exceptionOrNull()
                    if (isUnauthorizedError(err)) {
                        handleUnauthorizedSession(err?.message ?: "HTTP 401", capturedGen)
                    } else {
                        updateMessageInChat(conversationId, msgId) { it.copy(deliveryState = "failed") }
                        postBanner("上传附件失败：${err?.message ?: "网络异常"}")
                    }
                    return@launch
                }
                val fileId = uploaded.optNonNullString("file_id").ifBlank { uploaded.optNonNullString("media_id") }
                val thumb = uploaded.optNullableString("thumbnail_data_url")
                if (fileId.isNotBlank()) cachedVoiceBlobs[fileId] = bytes
                val sendRes = withContext(Dispatchers.IO) {
                    hostRequestResult(
                        "telegram.server.sendMessage",
                        JSONObject()
                            .put("chatId", numericChatId)
                            .put("text", fileName)
                            .put("mediaId", fileId)
                            .put("clientNonce", clientNonce),
                    )
                }
                if (!generationFence.isCurrent(capturedGen)) return@launch
                sendRes.onSuccess { resp ->
                    val serverMsgId = resp.optLong("message_id").takeIf { it > 0L }?.toString()
                        ?: resp.optLong("id").takeIf { it > 0L }?.toString()
                        ?: msgId
                    val serverDateMs = resp.optLong("date_ms").takeIf { it > 0L } ?: nowMs
                    reconcileOutgoingMessageSent(
                        conversationId = conversationId,
                        localMsgId = msgId,
                        serverMsgId = serverMsgId,
                        serverDateMs = serverDateMs,
                        mediaBlobId = fileId.ifBlank { localBlobId },
                        mediaThumbnailDataUrl = thumb,
                    )
                }.onFailure { err ->
                    if (isUnauthorizedError(err)) {
                        handleUnauthorizedSession(err.message ?: "HTTP 401", capturedGen)
                    } else {
                        updateMessageInChat(conversationId, msgId) { it.copy(deliveryState = "failed") }
                        postBanner("发送媒体消息失败：${err.message ?: "网络异常"}")
                    }
                }
            }
        } else {
            updateMessageInChat(conversationId, msgId) { it.copy(deliveryState = "sent") }
        }
    }

    fun openMediaViewer(message: TelegramMessageItem?) {
        _state.update { it.copy(viewingMediaMessage = message) }
    }

    fun loadMediaBlob(blobId: String, sizeBytes: Int, onResult: (Result<ByteArray>) -> Unit) {
        val local = cachedVoiceBlobs[blobId]
        if (local != null) {
            onResult(Result.success(local))
            return
        }
        viewModelScope.launch {
            val res = runCatching {
                withContext(Dispatchers.IO) {
                    if (_state.value.isAuthenticated) {
                        val srvRes = hostRequestResult("telegram.server.downloadMedia", JSONObject().put("fileId", blobId))
                        srvRes.getOrNull()?.let { dl ->
                            val b64 = dl.optNonNullString("dataBase64").ifBlank { dl.optNonNullString("data_base64") }
                            if (b64.isNotBlank()) {
                                val decoded = Base64.getDecoder().decode(b64)
                                cachedVoiceBlobs[blobId] = decoded
                                return@withContext decoded
                            }
                        }
                    }
                    if (sizeBytes <= 0) error("媒体文件不可用")
                    val host = mahayanaHost ?: error("媒体缓存为空")
                    val output = java.io.ByteArrayOutputStream(sizeBytes.coerceAtLeast(1))
                    var offset = 0
                    while (offset < sizeBytes) {
                        val reqLen = minOf(512 * 1024, sizeBytes - offset)
                        val chunkResp = host.request("feature.messaging.blob.read", JSONObject().put("blobId", blobId).put("offset", offset).put("length", reqLen))
                        val chunk = Base64.getDecoder().decode(chunkResp.optNonNullString("dataBase64"))
                        check(chunk.isNotEmpty()) { "Empty media chunk" }
                        output.write(chunk)
                        offset += chunk.size
                    }
                    output.toByteArray()
                }
            }
            onResult(res)
        }
    }

    fun clickInlineButton(
        conversationId: String,
        button: InlineKeyboardButtonModel,
        onOpenUrl: (String) -> Unit = {},
        onOpenMiniAppId: (String) -> Unit = {},
        messageId: String? = null,
    ) {
        when (button.kind) {
            InlineKeyboardButtonKind.URL -> onOpenUrl(button.payload)
            InlineKeyboardButtonKind.MINI_APP -> {
                if (button.payload == "global-dharma" || button.payload.isBlank()) {
                    navigateTo(TelegramSubScreen.GLOBAL_DHARMA)
                } else {
                    onOpenMiniAppId(button.payload)
                }
            }
            InlineKeyboardButtonKind.SWITCH_INLINE -> {
                updateDraft(conversationId, "@${button.payload.ifBlank { "MikaPhoneBot " }}")
            }
            InlineKeyboardButtonKind.MIKA_ACTION -> {
                when {
                    button.payload == "open_workspace" -> navigateTo(TelegramSubScreen.MIKA_WORKSPACE)
                    button.payload == "stop_task" -> {
                        mikaBridge?.stopActiveTask()
                        postBanner("已向 Mika 手机智能体发送停止信号")
                    }
                    button.payload.startsWith(TelegramMikaChannel.ACTION_CONFIRM_APPROVE_PREFIX) -> {
                        val (threadId, askId) = parseMikaConfirmPayload(
                            button.payload.removePrefix(TelegramMikaChannel.ACTION_CONFIRM_APPROVE_PREFIX),
                            fallbackThreadId = conversationId,
                        )
                        resolveMikaConfirmation(approved = true, threadId = threadId, askId = askId)
                    }
                    button.payload.startsWith(TelegramMikaChannel.ACTION_CONFIRM_DECLINE_PREFIX) -> {
                        val (threadId, askId) = parseMikaConfirmPayload(
                            button.payload.removePrefix(TelegramMikaChannel.ACTION_CONFIRM_DECLINE_PREFIX),
                            fallbackThreadId = conversationId,
                        )
                        resolveMikaConfirmation(approved = false, threadId = threadId, askId = askId)
                    }
                    else -> runWithMikaFromChat(conversationId, button.payload)
                }
            }
            InlineKeyboardButtonKind.CALLBACK -> {
                when (button.payload) {
                    "open:botfather" -> navigateTo(TelegramSubScreen.BOTFATHER)
                    "open:wallet" -> navigateTo(TelegramSubScreen.TON_WALLET)
                    "open:hermes" -> navigateTo(TelegramSubScreen.HERMES_STUDIO)
                    "open:magpie" -> navigateTo(TelegramSubScreen.MAGPIE_GATEWAY)
                    "open:computer" -> navigateTo(TelegramSubScreen.CLOUD_COMPUTER)
                    else -> {
                        val numericChatId = conversationId.toLongOrNull()
                        val resolvedMsgId = messageId?.toLongOrNull()
                            ?: _state.value.messagesByChat[conversationId]?.lastOrNull { msg ->
                                msg.inlineKeyboard.any { row -> row.buttons.any { it.id == button.id && it.payload == button.payload } }
                            }?.id?.toLongOrNull()
                        if (!_state.value.isAuthenticated || numericChatId == null || resolvedMsgId == null) {
                            postBanner("回调按钮需要连接服务器会话")
                            return
                        }
                        val capturedGen = generationFence.currentGeneration
                        viewModelScope.launch {
                            val result = withContext(Dispatchers.IO) {
                                hostRequestResult(
                                    "telegram.server.sendCallbackQuery",
                                    JSONObject()
                                        .put("chatId", numericChatId)
                                        .put("messageId", resolvedMsgId)
                                        .put("callbackData", button.payload),
                                )
                            }
                            if (!generationFence.isCurrent(capturedGen)) return@launch
                            result.onSuccess { resp ->
                                val toast = resp.optNonNullString("toast").ifBlank { resp.optNonNullString("message") }
                                val updatedText = resp.optNullableString("updated_text")
                                val updatedKb = parseInlineKeyboard(resp.optJSONArray("inline_keyboard"))
                                if (updatedText != null || updatedKb != null) {
                                    chatMessagesEpochFor(conversationId).incrementAndGet()
                                    updateMessageInChat(conversationId, resolvedMsgId.toString()) { msg ->
                                        val (plain, ents) = if (updatedText != null) {
                                            TelegramEntitiesEngine.parseMarkdownAndAutoEntities(updatedText)
                                        } else msg.text to msg.entities
                                        msg.copy(
                                            text = plain,
                                            entities = ents,
                                            inlineKeyboard = updatedKb ?: msg.inlineKeyboard,
                                        )
                                    }
                                }
                                if (toast.isNotBlank()) postBanner(toast)
                            }.onFailure { err ->
                                if (isUnauthorizedError(err)) {
                                    handleUnauthorizedSession(err.message ?: "HTTP 401", capturedGen)
                                } else {
                                    postBanner("机器人回调失败：${err.message ?: "网络异常"}")
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fun setChatTtl(conversationId: String, ttlSeconds: Int) {
        chatsEpoch.incrementAndGet()
        var prevTtl = 0
        _state.update { current ->
            prevTtl = current.chats.firstOrNull { it.id == conversationId }?.ttlSeconds ?: 0
            current.copy(
                chats = current.chats.map { if (it.id == conversationId) it.copy(ttlSeconds = ttlSeconds) else it },
            )
        }
        val numericChatId = conversationId.toLongOrNull()
        if (_state.value.isAuthenticated && numericChatId != null) {
            val capturedGen = generationFence.currentGeneration
            viewModelScope.launch {
                val result = withContext(Dispatchers.IO) {
                    hostRequestResult(
                        "telegram.server.setChatTtl",
                        JSONObject().put("chatId", numericChatId).put("ttlSeconds", ttlSeconds.toLong()),
                    )
                }
                if (!generationFence.isCurrent(capturedGen)) return@launch
                result.onSuccess {
                    postBanner(if (ttlSeconds <= 0) "已关闭自动销毁计时器" else "已设置消息自动销毁计时器")
                }.onFailure { err ->
                    if (isUnauthorizedError(err)) {
                        handleUnauthorizedSession(err.message ?: "HTTP 401", capturedGen)
                    } else {
                        _state.update { current ->
                            current.copy(
                                chats = current.chats.map { if (it.id == conversationId) it.copy(ttlSeconds = prevTtl) else it },
                                toastBanner = "设置自动销毁失败：${err.message ?: "网络异常"}",
                            )
                        }
                    }
                }
            }
        } else {
            postBanner(if (ttlSeconds <= 0) "已关闭自动销毁计时器" else "已设置消息自动销毁计时器")
        }
    }

    fun clearChatHistory(conversationId: String) {
        chatMessagesEpochFor(conversationId).incrementAndGet()
        chatPinsEpochFor(conversationId).incrementAndGet()
        chatsEpoch.incrementAndGet()
        var clearedMsgs: List<TelegramMessageItem> = emptyList()
        var clearedPins: List<String> = emptyList()
        _state.update { current ->
            clearedMsgs = current.messagesByChat[conversationId].orEmpty()
            clearedPins = current.chats.firstOrNull { it.id == conversationId }?.pinnedMessageIds.orEmpty()
            current.copy(
                messagesByChat = current.messagesByChat + (conversationId to emptyList()),
                chats = current.chats.map { c ->
                    if (c.id == conversationId) c.copy(pinnedMessageIds = emptyList()) else c
                },
            )
        }
        val numericChatId = conversationId.toLongOrNull()
        if (_state.value.isAuthenticated && numericChatId != null) {
            val capturedGen = generationFence.currentGeneration
            viewModelScope.launch {
                val result = withContext(Dispatchers.IO) {
                    hostRequestResult("telegram.server.clearChat", JSONObject().put("chatId", numericChatId))
                }
                if (!generationFence.isCurrent(capturedGen)) return@launch
                result.onSuccess {
                    postBanner("会话聊天记录已清空")
                }.onFailure { err ->
                    if (isUnauthorizedError(err)) {
                        handleUnauthorizedSession(err.message ?: "HTTP 401", capturedGen)
                    } else {
                        val restoreMsgs = clearedMsgs
                        val restorePins = clearedPins
                        _state.update { current ->
                            val existingMsgs = current.messagesByChat[conversationId].orEmpty()
                            val existingIds = existingMsgs.mapTo(HashSet()) { it.id }
                            val mergedMsgs = (existingMsgs + restoreMsgs.filterNot { it.id in existingIds })
                                .sortedWith(
                                    compareBy<TelegramMessageItem> { it.timestampMs }
                                        .thenBy { it.id.toLongOrNull() ?: Long.MAX_VALUE },
                                )
                            val updatedChats = if (restorePins.isEmpty()) {
                                current.chats
                            } else {
                                current.chats.map { c ->
                                    if (c.id == conversationId) {
                                        c.copy(pinnedMessageIds = (restorePins + c.pinnedMessageIds).distinct())
                                    } else c
                                }
                            }
                            current.copy(
                                messagesByChat = current.messagesByChat + (conversationId to mergedMsgs),
                                chats = updatedChats,
                                toastBanner = "清空记录失败：${err.message ?: "网络异常"}",
                            )
                        }
                    }
                }
            }
        } else {
            postBanner("会话聊天记录已清空")
        }
    }

    fun leaveChat(conversationId: String) {
        chatsEpoch.incrementAndGet()
        val numericChatId = conversationId.toLongOrNull()
        var removedChat: TelegramChatItem? = null
        _state.update { current ->
            removedChat = current.chats.firstOrNull { it.id == conversationId }
            current.copy(
                chats = current.chats.filterNot { it.id == conversationId },
                activeChatId = if (current.activeChatId == conversationId) null else current.activeChatId,
                subScreen = if (current.activeChatId == conversationId) TelegramSubScreen.HOME else current.subScreen,
            )
        }
        if (_state.value.isAuthenticated && numericChatId != null) {
            val capturedGen = generationFence.currentGeneration
            viewModelScope.launch {
                val result = withContext(Dispatchers.IO) {
                    hostRequestResult("telegram.server.leaveChat", JSONObject().put("chatId", numericChatId))
                }
                if (!generationFence.isCurrent(capturedGen)) return@launch
                result.onSuccess {
                    postBanner("已退出该会话")
                }.onFailure { err ->
                    if (isUnauthorizedError(err)) {
                        handleUnauthorizedSession(err.message ?: "HTTP 401", capturedGen)
                    } else {
                        val restore = removedChat
                        _state.update { cur ->
                            val nextChats = if (restore != null && cur.chats.none { it.id == restore.id }) {
                                sortTelegramChats(cur.chats + restore)
                            } else cur.chats
                            cur.copy(chats = nextChats, toastBanner = "退出会话失败：${err.message ?: "网络异常"}")
                        }
                    }
                }
            }
        }
    }

    fun exportChatHistory(conversationId: String, format: String): File? {
        val chat = _state.value.chats.firstOrNull { it.id == conversationId } ?: return null
        val messages = _state.value.messagesByChat[conversationId].orEmpty()
        return runCatching {
            val dir = File(appContext.cacheDir, "fabushi-media").apply { mkdirs() }
            val ext = if (format.equals("html", ignoreCase = true)) "html" else "json"
            val outFile = File(dir, "telegram-export-${chat.id}.$ext")
            val content = if (ext == "json") {
                val arr = JSONArray()
                messages.forEach { m ->
                    arr.put(
                        JSONObject()
                            .put("id", m.id)
                            .put("sender", m.senderName)
                            .put("time", m.time)
                            .put("type", m.contentType)
                            .put("text", m.text),
                    )
                }
                JSONObject().put("chatTitle", chat.title).put("messageCount", messages.size).put("messages", arr).toString(2)
            } else {
                buildString {
                    append("<!doctype html><html><head><meta charset='utf-8'><title>${chat.title}</title></head><body>")
                    append("<h2>${chat.title} · 导出记录 (${messages.size} 条)</h2><hr/>")
                    messages.forEach { m ->
                        append("<p><b>${m.senderName}</b> <small>[${m.time}]</small><br/>${m.text}</p>")
                    }
                    append("</body></html>")
                }
            }
            outFile.writeText(content)
            _state.update { it.copy(storageCacheMb = measureLocalMediaCacheMb(), toastBanner = "已导出 ${messages.size} 条记录至 ${outFile.name}") }
            outFile
        }.getOrNull()
    }

    /**
     * K-F13: Query real server /api/v1/chats/{id}/statistics or fixed Rust telegram.messaging.channelStats.
     * NEVER fabricate 12,840 subscribers or fake growth bars!
     */
    fun openChannelStats(conversationId: String?) {
        if (conversationId == null) {
            _state.update { it.copy(channelStatsSnapshot = null) }
            return
        }
        val capturedGen = generationFence.currentGeneration
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                val numericId = conversationId.toLongOrNull()
                if (_state.value.isAuthenticated && numericId != null) {
                    val srv = hostRequestResult("telegram.server.channelStats", JSONObject().put("chatId", numericId))
                    if (srv.isSuccess) srv else hostRequestResult("telegram.messaging.channelStats", JSONObject().put("conversationId", conversationId))
                } else {
                    hostRequestResult("telegram.messaging.channelStats", JSONObject().put("conversationId", conversationId))
                }
            }
            if (!generationFence.isCurrent(capturedGen)) return@launch
            result.onSuccess { resp ->
                val barsArr = resp.optJSONArray("dailyViewsBars") ?: resp.optJSONArray("daily_views")
                val bars = buildList {
                    if (barsArr != null) {
                        for (i in 0 until barsArr.length()) add(barsArr.optInt(i, 0))
                    }
                }
                val subs = resp.optInt("subscribers", resp.optInt("member_count", 0))
                val avgViews = resp.optInt("avgViewsPerPost", resp.optInt("totalMessages", 0))
                _state.update {
                    it.copy(
                        channelStatsSnapshot = ChannelStatisticsSnapshot(
                            conversationId = conversationId,
                            subscribers = subs,
                            growth7dPercent = resp.optDouble("growth7dPercent", 0.0),
                            notificationsEnabledPercent = resp.optDouble("notificationsEnabledPercent", 0.0),
                            avgViewsPerPost = avgViews,
                            sharesPerPost = resp.optInt("sharesPerPost", 0),
                            reactionsPerPost = resp.optInt("reactionsPerPost", 0),
                            dailyViewsBars = bars,
                        ),
                    )
                }
            }.onFailure { err ->
                _state.update { it.copy(channelStatsSnapshot = null, toastBanner = "频道统计暂不可用：${err.message ?: "无统计数据"}") }
            }
        }
    }

    fun openDiscussionSheet(message: TelegramMessageItem?) {
        _state.update { it.copy(discussionSheetMessage = message) }
    }

    fun postDiscussionComment(rootMessage: TelegramMessageItem, commentText: String) {
        val clean = commentText.trim().ifBlank { return }
        val comment = TelegramMessageItem(
            id = "cmt-${UUID.randomUUID().toString().take(8)}",
            conversationId = rootMessage.conversationId,
            senderId = currentServerUserId?.toString() ?: actorId,
            senderName = _state.value.activeAccount?.displayName ?: "我",
            text = clean,
            outgoing = true,
            time = nowTimeLabel(),
            deliveryState = "sent",
        )
        _state.update { current ->
            val existing = current.discussionCommentsByMessage[rootMessage.id].orEmpty() + comment
            val updatedChatMsgs = current.messagesByChat[rootMessage.conversationId].orEmpty().map {
                if (it.id == rootMessage.id) it.copy(commentsCount = (it.commentsCount ?: 0) + 1) else it
            }
            current.copy(
                discussionCommentsByMessage = current.discussionCommentsByMessage + (rootMessage.id to existing),
                messagesByChat = current.messagesByChat + (rootMessage.conversationId to updatedChatMsgs),
            )
        }
    }

    // ------------------------------------------------------------------------
    // Voice Waveform & Round Video Note Recording (Real RECORD_AUDIO Guard, K-F23)
    // ------------------------------------------------------------------------

    fun toggleComposerRecordMode() {
        _state.update { current ->
            val nextMode = if (current.recordingState.mode == ComposerRecordMode.VOICE_MIC) {
                ComposerRecordMode.ROUND_VIDEO_NOTE
            } else {
                ComposerRecordMode.VOICE_MIC
            }
            current.copy(recordingState = current.recordingState.copy(mode = nextMode))
        }
    }

    @SuppressLint("MissingPermission")
    fun startVoiceOrVideoRecording() {
        if (_state.value.recordingState.isRecording) return
        val hasAudioPerm = ContextCompat.checkSelfPermission(appContext, Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED
        if (!hasAudioPerm) {
            // K-F23: Refuse recording cleanly when RECORD_AUDIO is denied; zero synthetic waveform
            postBanner("未授予麦克风权限 (RECORD_AUDIO)，无法录制语音消息")
            return
        }
        if (_state.value.recordingState.mode == ComposerRecordMode.VOICE_MIC) {
            runCatching { voiceRecorder.start() }
        }
        val startedAt = System.currentTimeMillis()
        _state.update {
            it.copy(
                recordingState = it.recordingState.copy(
                    isRecording = true,
                    lockedHandsFree = false,
                    elapsedMs = 0L,
                    dragOffsetX = 0f,
                    dragOffsetY = 0f,
                    liveBars = emptyList(),
                ),
            )
        }
        recordingTimerJob?.cancel()
        recordingTimerJob = viewModelScope.launch {
            var audioRecord: AudioRecord? = null
            val minBuf = runCatching {
                AudioRecord.getMinBufferSize(16_000, AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT)
            }.getOrDefault(0)
            if (minBuf > 0 && !voiceRecorder.isRecording) {
                runCatching {
                    audioRecord = AudioRecord(MediaRecorder.AudioSource.MIC, 16_000, AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT, minBuf * 2)
                    audioRecord?.startRecording()
                }
            }
            val pcmBuf = ShortArray(512)
            try {
                while (isActive && _state.value.recordingState.isRecording) {
                    val elapsed = System.currentTimeMillis() - startedAt
                    val bar = if (audioRecord != null && audioRecord?.recordingState == AudioRecord.RECORDSTATE_RECORDING) {
                        val read = audioRecord?.read(pcmBuf, 0, pcmBuf.size) ?: 0
                        TelegramWaveformCodec.computeRmsBar31(pcmBuf, read)
                    } else {
                        1
                    }
                    _state.update { current ->
                        val nextBars = (current.recordingState.liveBars + bar).takeLast(100)
                        current.copy(recordingState = current.recordingState.copy(elapsedMs = elapsed, liveBars = nextBars))
                    }
                    delay(90)
                }
            } finally {
                runCatching { audioRecord?.stop() }
                runCatching { audioRecord?.release() }
            }
        }
    }

    fun updateRecordingDrag(dx: Float, dy: Float) {
        if (!_state.value.recordingState.isRecording) return
        if (dx < -140f) {
            cancelRecording()
            return
        }
        if (dy < -100f && !_state.value.recordingState.lockedHandsFree) {
            _state.update { it.copy(recordingState = it.recordingState.copy(lockedHandsFree = true, dragOffsetX = 0f, dragOffsetY = 0f)) }
            return
        }
        _state.update { it.copy(recordingState = it.recordingState.copy(dragOffsetX = dx, dragOffsetY = dy)) }
    }

    fun cancelRecording() {
        recordingTimerJob?.cancel()
        if (voiceRecorder.isRecording) voiceRecorder.cancel()
        _state.update { it.copy(recordingState = ActiveRecordingState(mode = it.recordingState.mode), toastBanner = "已取消录制") }
    }

    fun finishAndSendRecording(conversationId: String) {
        val snap = _state.value.recordingState
        if (!snap.isRecording) return
        recordingTimerJob?.cancel()
        val recordedBytes = if (voiceRecorder.isRecording) {
            voiceRecorder.stop().getOrNull()?.bytes
        } else null

        val durationSec = ((snap.elapsedMs / 1000L).toInt()).coerceAtLeast(1)
        val bars100 = TelegramWaveformCodec.normalizeTo100Bars(snap.liveBars, seedHint = conversationId)
        val blobId = "voice-${UUID.randomUUID()}"
        if (recordedBytes != null && recordedBytes.isNotEmpty()) {
            cachedVoiceBlobs[blobId] = recordedBytes
        }

        val isRoundVideo = snap.mode == ComposerRecordMode.ROUND_VIDEO_NOTE
        val nowMs = System.currentTimeMillis()
        val msg = TelegramMessageItem(
            id = "msg-rec-${UUID.randomUUID().toString().take(8)}",
            conversationId = conversationId,
            senderId = currentServerUserId?.toString() ?: actorId,
            senderName = _state.value.activeAccount?.displayName ?: "我",
            text = if (isRoundVideo) "⭕ 圆形视频消息 (${durationSec}s)" else "🎙 语音消息 (${durationSec}s)",
            contentType = if (isRoundVideo) "videoNote" else "voice",
            outgoing = true,
            time = nowTimeLabel(nowMs),
            timestampMs = nowMs,
            deliveryState = "sent",
            waveformBars = bars100,
            durationSeconds = durationSec,
            mediaBlobId = blobId,
            mediaFileName = if (isRoundVideo) "$blobId.mp4" else "$blobId.m4a",
            mediaMimeType = if (isRoundVideo) "video/mp4" else "audio/mp4",
            mediaSizeBytes = recordedBytes?.size ?: 0,
        )
        appendMessageToChat(conversationId, msg, clearDraft = false)
        _state.update { it.copy(recordingState = ActiveRecordingState(mode = snap.mode)) }
    }

    fun playOrToggleAudioMessage(message: TelegramMessageItem, startFraction: Float? = null) {
        val currentAudio = _state.value.audioPlayback
        if (currentAudio.active && currentAudio.messageId == message.id && startFraction == null) {
            val nextPlaying = !currentAudio.isPlaying
            runCatching {
                if (nextPlaying) activeMediaPlayer?.start() else activeMediaPlayer?.pause()
            }
            _state.update { it.copy(audioPlayback = it.audioPlayback.copy(isPlaying = nextPlaying)) }
            if (nextPlaying) launchAudioTicker() else audioProgressJob?.cancel()
            return
        }

        stopAudioPlayback()
        val duration = message.durationSeconds.coerceAtLeast(1)
        val initialFraction = (startFraction ?: 0f).coerceIn(0f, 0.99f)
        val blobBytes = message.mediaBlobId?.let { cachedVoiceBlobs[it] }
        if (blobBytes != null && blobBytes.isNotEmpty()) {
            runCatching {
                val dir = File(appContext.cacheDir, "fabushi-voice-playback").apply { mkdirs() }
                val f = File(dir, "play-${message.id}.m4a").apply { writeBytes(blobBytes) }
                activeMediaPlayer = MediaPlayer().apply {
                    setDataSource(f.absolutePath)
                    prepare()
                    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M) {
                        playbackParams = PlaybackParams().setSpeed(_state.value.audioPlayback.speedMultiplier)
                    }
                    seekTo((duration * 1000 * initialFraction).toInt())
                    setOnCompletionListener { stopAudioPlayback() }
                    start()
                }
            }
        }
        _state.update {
            it.copy(
                audioPlayback = FloatingAudioPlaybackState(
                    active = true,
                    messageId = message.id,
                    conversationId = message.conversationId,
                    title = if (message.contentType == "voice") "语音消息 · ${message.senderName}" else (message.mediaFileName ?: "音频播放"),
                    subtitle = message.time,
                    isPlaying = true,
                    progressFraction = initialFraction,
                    durationSeconds = duration,
                    speedMultiplier = it.audioPlayback.speedMultiplier,
                ),
            )
        }
        launchAudioTicker()
    }

    fun seekAudioPlayback(fraction: Float) {
        val bounded = fraction.coerceIn(0f, 0.99f)
        runCatching {
            activeMediaPlayer?.let { mp ->
                if (mp.duration > 0) mp.seekTo((mp.duration * bounded).toInt())
            }
        }
        _state.update { it.copy(audioPlayback = it.audioPlayback.copy(progressFraction = bounded)) }
    }

    fun cycleAudioSpeed() {
        val speeds = listOf(1.0f, 1.5f, 2.0f, 0.5f)
        val cur = _state.value.audioPlayback.speedMultiplier
        val next = speeds[(speeds.indexOfFirst { kotlin.math.abs(it - cur) < 0.05f } + 1) % speeds.size]
        runCatching {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M) {
                activeMediaPlayer?.let { mp ->
                    if (mp.isPlaying) mp.playbackParams = PlaybackParams().setSpeed(next)
                }
            }
        }
        _state.update { it.copy(audioPlayback = it.audioPlayback.copy(speedMultiplier = next)) }
    }

    fun stopAudioPlayback() {
        audioProgressJob?.cancel()
        runCatching { activeMediaPlayer?.stop() }
        runCatching { activeMediaPlayer?.release() }
        activeMediaPlayer = null
        _state.update { it.copy(audioPlayback = FloatingAudioPlaybackState(speedMultiplier = it.audioPlayback.speedMultiplier)) }
    }

    private fun launchAudioTicker() {
        audioProgressJob?.cancel()
        audioProgressJob = viewModelScope.launch {
            while (isActive && _state.value.audioPlayback.active && _state.value.audioPlayback.isPlaying) {
                val cur = _state.value.audioPlayback
                val mp = activeMediaPlayer
                val nextFrac = if (mp != null && runCatching { mp.duration }.getOrDefault(0) > 0) {
                    (mp.currentPosition.toFloat() / mp.duration.toFloat()).coerceIn(0f, 1f)
                } else {
                    val step = (0.1f * cur.speedMultiplier) / cur.durationSeconds.coerceAtLeast(1).toFloat()
                    cur.progressFraction + step
                }
                if (nextFrac >= 1f) {
                    stopAudioPlayback()
                    break
                }
                _state.update { it.copy(audioPlayback = it.audioPlayback.copy(progressFraction = nextFrac)) }
                delay(100)
            }
        }
    }

    // ------------------------------------------------------------------------
    // VoIP Calls & VAD (Contract #19, K-F14: Honest Fail-Closed — Zero Fake Calls/VAD)
    // ------------------------------------------------------------------------

    fun startCall(conversationId: String, peerTitle: String, video: Boolean, groupCall: Boolean = false) {
        callTimerJob?.cancel()
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                hostRequestResult(
                    if (groupCall) "telegram.calls.joinGroupCall" else "telegram.calls.startCall",
                    JSONObject().put("conversationId", conversationId).put("video", video),
                )
            }
            result.onFailure { err ->
                _state.update {
                    it.copy(
                        callState = TelegramCallUiState(phase = CallPhase.IDLE),
                        toastBanner = "实时语音/视频通话暂不可用：${err.message ?: "CALL_TRANSPORT_UNAVAILABLE"}",
                    )
                }
            }.onSuccess { resp ->
                val callId = resp.optNonNullString("callId").ifBlank {
                    _state.update { it.copy(callState = TelegramCallUiState(), toastBanner = "CALL_TRANSPORT_UNAVAILABLE") }
                    return@onSuccess
                }
                val emojis = CallsEmojiFingerprint.deriveCallEmojis("$callId:$conversationId:$actorId")
                _state.update {
                    it.copy(
                        callState = TelegramCallUiState(
                            phase = CallPhase.OUTGOING_RINGING,
                            callId = callId,
                            conversationId = conversationId,
                            peerTitle = peerTitle,
                            isGroupCall = groupCall,
                            isVideoCall = video,
                            emojiFingerprint = emojis,
                            participants = emptyList(),
                        ),
                    )
                }
            }
        }
    }

    fun toggleCallMute() {
        _state.update { it.copy(callState = it.callState.copy(isMuted = !it.callState.isMuted)) }
    }

    fun toggleCallVideo() {
        _state.update { it.copy(callState = it.callState.copy(isVideoCall = !it.callState.isVideoCall)) }
    }

    fun toggleCallScreenShare() {
        postBanner("实时通话屏幕共享暂不可用 (CALL_TRANSPORT_UNAVAILABLE)")
    }

    fun toggleCallSpeaker() {
        _state.update { it.copy(callState = it.callState.copy(isSpeakerOn = !it.callState.isSpeakerOn)) }
    }

    fun setCallMinimized(minimized: Boolean) {
        _state.update { it.copy(callState = it.callState.copy(isMinimized = minimized)) }
    }

    fun endCall() {
        callTimerJob?.cancel()
        val ended = _state.value.callState
        if (ended.phase == CallPhase.IDLE) return
        viewModelScope.launch(Dispatchers.IO) {
            hostRequestResult("telegram.calls.endCall", JSONObject().put("callId", ended.callId))
        }
        _state.update { it.copy(callState = TelegramCallUiState()) }
    }

    fun setTalkModeOrbVisible(visible: Boolean) {
        _state.update {
            it.copy(
                talkOrbState = it.talkOrbState.copy(
                    visible = visible,
                    phase = TalkVadPhase.LISTENING,
                    vadLatencyMs = 0,
                    rmsLevel = 0f,
                ),
            )
        }
    }

    fun triggerTalkModeBargeIn() {
        _state.update {
            it.copy(
                talkOrbState = it.talkOrbState.copy(
                    phase = TalkVadPhase.LISTENING,
                    vadLatencyMs = 0,
                    assistantReply = "",
                ),
                toastBanner = "端侧实时语音通话通道未就绪，请传入真实 PCM 音频帧",
            )
        }
    }

    fun submitTalkModeUtterance(utterance: String) {
        val clean = utterance.trim().ifBlank { return }
        _state.update {
            it.copy(
                talkOrbState = it.talkOrbState.copy(
                    phase = TalkVadPhase.AI_THINKING,
                    userTranscript = clean,
                    vadLatencyMs = 0,
                ),
            )
        }
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                hostRequestResult(
                    "magpie.chatComplete",
                    JSONObject().put(
                        "messages",
                        JSONArray().put(JSONObject().put("role", "user").put("content", clean)),
                    ),
                )
            }
            result.onSuccess { resp ->
                val reply = resp.optNonNullString("reply")
                    .ifBlank { resp.optNonNullString("content") }
                    .ifBlank { resp.optNonNullString("text") }
                _state.update {
                    it.copy(
                        talkOrbState = it.talkOrbState.copy(
                            phase = TalkVadPhase.AI_SPEAKING,
                            assistantReply = reply,
                        ),
                    )
                }
            }.onFailure { err ->
                _state.update {
                    it.copy(
                        talkOrbState = it.talkOrbState.copy(
                            phase = TalkVadPhase.LISTENING,
                            assistantReply = "",
                        ),
                        toastBanner = "语音对话不可用：${err.message ?: "请先配置 Magpie API Key"}",
                    )
                }
            }
        }
    }

    /**
     * Contract #19: Real PCM feed to telegram.calls.vadFeedPcm ({pcmSamples, ttsPlaying}).
     */
    fun feedVadPcm(samples: IntArray, ttsPlaying: Boolean = false): JSONObject? {
        val arr = JSONArray()
        samples.forEach { arr.put(it) }
        val result = hostRequestResult(
            "telegram.calls.vadFeedPcm",
            JSONObject().put("pcmSamples", arr).put("ttsPlaying", ttsPlaying),
        )
        return result.getOrNull()?.also { resp ->
            _state.update { cur ->
                cur.copy(
                    talkOrbState = cur.talkOrbState.copy(
                        vadLatencyMs = resp.optInt("detectionLatencyMs", 0),
                        rmsLevel = resp.optDouble("peakRms", 0.0).toFloat(),
                        phase = if (resp.optBoolean("bargeInTriggered", false)) TalkVadPhase.USER_SPEAKING else cur.talkOrbState.phase,
                    ),
                )
            }
        }
    }

    // ------------------------------------------------------------------------
    // TON Wallet (Contracts #4, #5, #6, #7, K-F15: Real Ed25519 v4r2 + Fail-Closed)
    // ------------------------------------------------------------------------

    fun createNewTonWallet() {
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                hostRequestResult("wallet.createWallet", JSONObject().put("label", "Fabushi TON Wallet"))
            }
            result.onSuccess { resp ->
                val address = resp.optNonNullString("address").ifBlank { resp.optNonNullString("addressNonBounceable") }
                val mnemonicArr = resp.optJSONArray("mnemonic")
                val words = buildList {
                    if (mnemonicArr != null) {
                        for (i in 0 until mnemonicArr.length()) {
                            mnemonicArr.optNonNullString(i).takeIf { it.isNotBlank() }?.let(::add)
                        }
                    }
                }
                val balanceTon = resp.optLong("balanceNanotons", 0L).toDouble() / 1_000_000_000.0
                _state.update {
                    it.copy(
                        tonWallet = it.tonWallet.copy(
                            initialized = address.isNotBlank(),
                            addressFriendly = address,
                            mnemonicWords = words,
                            tonBalance = balanceTon,
                            starsBalance = resp.optLong("starsBalance", 0L),
                            collectibles = emptyList(),
                            statusBanner = "已通过 Rust Ed25519 + BIP39 创建 v4r2 钱包",
                        ),
                    )
                }
            }.onFailure { err ->
                postBanner("创建 TON 钱包失败：${err.message ?: "未知错误"}")
            }
        }
    }

    fun importTonMnemonic(wordsRaw: String) {
        val words = wordsRaw.trim().split(Regex("\\s+")).filter { it.isNotBlank() }
        if (words.size != 24) {
            postBanner("请输入完整的 24 个 BIP39 助记词（当前 ${words.size} 个）")
            return
        }
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                hostRequestResult("wallet.importMnemonic", JSONObject().put("mnemonic", words.joinToString(" ")))
            }
            result.onSuccess { resp ->
                val address = resp.optNonNullString("address").ifBlank { resp.optNonNullString("addressNonBounceable") }
                val balanceTon = resp.optLong("balanceNanotons", 0L).toDouble() / 1_000_000_000.0
                _state.update {
                    it.copy(
                        tonWallet = it.tonWallet.copy(
                            initialized = address.isNotBlank(),
                            addressFriendly = address,
                            mnemonicWords = words,
                            tonBalance = balanceTon,
                            starsBalance = resp.optLong("starsBalance", 0L),
                            statusBanner = "24 词助记词钱包导入成功",
                        ),
                    )
                }
            }.onFailure { err ->
                postBanner("导入助记词失败：${err.message ?: "无效助记词"}")
            }
        }
    }

    /**
     * Contract #5: wallet.sendTon expects {to, amountNanotons: Long, comment, broadcast: true}.
     */
    fun sendTonTransfer(recipient: String, amountTon: Double, memo: String) {
        val cleanRecipient = recipient.trim()
        if (cleanRecipient.isBlank() || !amountTon.isFinite() || amountTon <= 0.0) {
            postBanner("请检查收款地址与转账金额")
            return
        }
        val amountNanotons = runCatching {
            BigDecimal(amountTon.toString())
                .multiply(BigDecimal(1_000_000_000L))
                .setScale(0, RoundingMode.DOWN)
                .longValueExact()
        }.getOrNull()
        if (amountNanotons == null || amountNanotons <= 0L) {
            postBanner("无效的转账金额（超出允许范围）")
            return
        }
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                hostRequestResult(
                    "wallet.sendTon",
                    JSONObject()
                        .put("to", cleanRecipient)
                        .put("amountNanotons", amountNanotons)
                        .put("comment", memo)
                        .put("broadcast", true),
                )
            }
            result.onSuccess { resp ->
                val newBalanceTon = resp.optLong("newBalanceNanotons", 0L).toDouble() / 1_000_000_000.0
                val tx = TonTransactionItem(
                    id = resp.optNonNullString("txId").ifBlank { "tx-${UUID.randomUUID().toString().take(8)}" },
                    direction = "out",
                    title = "发送 TON",
                    peerAddress = cleanRecipient,
                    amountLabel = "-%.4f TON".format(amountTon),
                    asset = "TON",
                    timestampLabel = nowTimeLabel(),
                    memo = memo,
                    bocHash = resp.optNonNullString("bocHashHex", ""),
                )
                _state.update { cur ->
                    cur.copy(
                        tonWallet = cur.tonWallet.copy(
                            tonBalance = newBalanceTon,
                            transactions = listOf(tx) + cur.tonWallet.transactions,
                            statusBanner = "TON 交易已签名广播",
                        ),
                    )
                }
            }.onFailure { err ->
                _state.update { cur ->
                    cur.copy(
                        tonWallet = cur.tonWallet.copy(statusBanner = "转账失败：${err.message}"),
                        toastBanner = "TON 转账失败：${err.message ?: "余额不足或网络异常"}",
                    )
                }
            }
        }
    }

    fun swapTonToUsdt(tonAmount: Double) {
        // K-F15: Zero fake 5.42 USDT rate or STON.fi fabrication!
        _state.update { cur ->
            cur.copy(
                tonWallet = cur.tonWallet.copy(statusBanner = "链上 DEX 闪兑暂未接入真实合约执行"),
                toastBanner = "链上 DEX 闪兑暂未接入真实合约执行",
            )
        }
    }

    fun stakeTon(tonAmount: Double) {
        // K-F15: Zero fake staking mutation!
        _state.update { cur ->
            cur.copy(
                tonWallet = cur.tonWallet.copy(statusBanner = "链上流动性质押暂未接入真实合约执行"),
                toastBanner = "链上流动性质押暂未接入真实合约执行",
            )
        }
    }

    fun emulateTonConnectUri(uri: String) {
        // Contract #7: Never fabricate fake Fabushi Dharma Merit Vault preview or debit balance
        val clean = uri.trim()
        if (clean.isBlank()) {
            postBanner("TonConnect URI 为空")
            return
        }
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                hostRequestResult("wallet.tonConnectApprove", JSONObject().put("manifestUrl", clean))
            }
            result.onFailure { err ->
                _state.update { cur ->
                    cur.copy(
                        tonWallet = cur.tonWallet.copy(
                            pendingTonConnect = null,
                            statusBanner = "TonConnect 暂不可用：${err.message ?: "TONCONNECT_UNAVAILABLE"}",
                        ),
                        toastBanner = "TonConnect 暂不可用：${err.message ?: "TONCONNECT_UNAVAILABLE"}",
                    )
                }
            }
        }
    }

    fun resolveTonConnect(approve: Boolean) {
        if (!approve) {
            _state.update { cur ->
                cur.copy(
                    tonWallet = cur.tonWallet.copy(
                        pendingTonConnect = null,
                        statusBanner = "已拒绝 TonConnect 请求",
                    ),
                )
            }
            return
        }
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                hostRequestResult("wallet.tonConnectApprove", JSONObject())
            }
            result.onFailure { err ->
                _state.update { cur ->
                    cur.copy(
                        tonWallet = cur.tonWallet.copy(
                            pendingTonConnect = null,
                            statusBanner = "TonConnect 签名失败：${err.message ?: "TONCONNECT_UNAVAILABLE"}",
                        ),
                        toastBanner = "TonConnect 不可用：${err.message ?: "TONCONNECT_UNAVAILABLE"}",
                    )
                }
            }
        }
    }

    // ------------------------------------------------------------------------
    // Global Dharma MiniApp (Contract #12, K-F16: Real concap.csv & Fail-Closed AI)
    // ------------------------------------------------------------------------

    fun selectDharmaCountry(code: String) {
        _state.update { it.copy(globalDharma = it.globalDharma.copy(selectedCountryCode = code)) }
    }

    fun offerDharmaToCountry(targetCode: String, blessingNote: String) {
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                hostRequestResult(
                    "fabushiMiniapp.offerDharma",
                    JSONObject().put("countryCode", targetCode).put("count", 1),
                )
            }
            result.onSuccess {
                _state.update { cur ->
                    val target = cur.globalDharma.countries.firstOrNull { it.code == targetCode }
                    val arc = target?.let {
                        OfferingArc(
                            id = "arc-${System.nanoTime()}",
                            fromLat = 35.86,
                            fromLon = 104.19,
                            toLat = it.latitude,
                            toLon = it.longitude,
                            createdAtMs = System.currentTimeMillis(),
                            label = blessingNote.ifBlank { it.nameZh },
                        )
                    }
                    val updatedCountries = cur.globalDharma.countries.map {
                        if (it.code == targetCode) it.copy(offeringsCount = it.offeringsCount + 1, lampLitByMe = true) else it
                    }
                    cur.copy(
                        globalDharma = cur.globalDharma.copy(
                            countries = updatedCountries,
                            selectedCountryCode = targetCode,
                            activeArcs = if (arc != null) (cur.globalDharma.activeArcs + arc).takeLast(8) else cur.globalDharma.activeArcs,
                            totalGlobalOfferings = cur.globalDharma.totalGlobalOfferings + 1,
                        ),
                        toastBanner = "已向 ${target?.nameZh ?: targetCode} 供养祈福心灯",
                    )
                }
            }.onFailure { err ->
                postBanner("供养心灯失败：${err.message ?: "未知错误"}")
            }
        }
    }

    fun recordPrayerWheelRevolution(revolutionsDelta: Int = 1) {
        if (revolutionsDelta <= 0) return
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                hostRequestResult("fabushiMiniapp.spinPrayerWheel", JSONObject().put("rotations", revolutionsDelta))
            }
            result.onSuccess { resp ->
                _state.update { cur ->
                    val d = cur.globalDharma
                    cur.copy(
                        globalDharma = d.copy(
                            prayerWheelSpins = resp.optLong("wheelRotations", d.prayerWheelSpins + revolutionsDelta),
                            mantraCount = d.mantraCount + revolutionsDelta * 8L,
                            myMeritPoints = resp.optLong("totalMeritPoints", d.myMeritPoints),
                        ),
                    )
                }
            }.onFailure { err ->
                postBanner("转经轮记录失败：${err.message}")
            }
        }
    }

    fun tapWoodenFish() {
        _state.update { cur ->
            val d = cur.globalDharma
            cur.copy(globalDharma = d.copy(woodenFishHits = d.woodenFishHits + 1))
        }
    }

    fun completePracticeMinutes(minutes: Int) {
        if (minutes <= 0) return
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                hostRequestResult("fabushiMiniapp.recordPractice", JSONObject().put("durationSeconds", minutes.toLong() * 60L))
            }
            result.onSuccess { resp ->
                _state.update { cur ->
                    val d = cur.globalDharma
                    cur.copy(
                        globalDharma = d.copy(
                            practiceMinutesTotal = d.practiceMinutesTotal + minutes,
                            myMeritPoints = resp.optLong("totalMeritPoints", d.myMeritPoints),
                        ),
                        toastBanner = "已记录 $minutes 分钟禅修共修",
                    )
                }
            }.onFailure { err ->
                postBanner("记录共修失败：${err.message}")
            }
        }
    }

    fun askDharmaAi(question: String) {
        val clean = question.trim().ifBlank { return }
        _state.update { cur ->
            cur.copy(globalDharma = cur.globalDharma.copy(aiDharmaMessages = cur.globalDharma.aiDharmaMessages + (true to clean)))
        }
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                hostRequestResult("fabushiMiniapp.aiDharmaChat", JSONObject().put("prompt", clean))
            }
            result.onSuccess { resp ->
                val answer = resp.optNonNullString("reply").trim()
                if (answer.isNotBlank()) {
                    _state.update { cur ->
                        cur.copy(globalDharma = cur.globalDharma.copy(aiDharmaMessages = cur.globalDharma.aiDharmaMessages + (false to answer)))
                    }
                } else {
                    postBanner("AI 法义问答未返回内容")
                }
            }.onFailure { err ->
                // K-F16: ZERO Heart Sutra fallback!
                postBanner("AI 问答不可用：${err.message ?: "请先在 Magpie 网关配置 API Key"}")
            }
        }
    }

    // ------------------------------------------------------------------------
    // Real Bots, Fabushi AI Bots & Coding Agents (F10, K-F2..K-F4)
    // ------------------------------------------------------------------------

    fun refreshServerBots() {
        if (!_state.value.isAuthenticated) return
        val capturedGen = generationFence.currentGeneration
        val capturedEpoch = botsEpoch.get()
        viewModelScope.launch {
            val (myBots, fabushiBots) = withContext(Dispatchers.IO) {
                hostRequestResult("telegram.server.listMyBots").getOrNull() to
                    hostRequestResult("telegram.server.listFabushiBots").getOrNull()
            }
            if (!generationFence.isCurrent(capturedGen)) return@launch
            if (myBots != null || fabushiBots != null) {
                val merged = mergeServerBots(myBots, fabushiBots)
                _state.update { cur ->
                    if (botsEpoch.get() != capturedEpoch) cur
                    else cur.copy(
                        fabushiBots = merged,
                        codingAgentBots = merged.mapNotNull { it.codingAgent },
                        chats = syncFabushiBotsToChats(cur.chats, merged),
                    )
                }
            }
        }
    }

    fun createServerBot(
        displayName: String,
        username: String,
        description: String = "",
        avatarShape: String? = null,
        avatarColor: String? = null,
    ) {
        val cleanName = displayName.trim()
        val cleanUsername = username.trim().trimStart('@')
        if (cleanName.isBlank() || cleanUsername.isBlank()) {
            postBanner("请输入有效的机器人名称和用户名")
            return
        }
        if (!_state.value.isAuthenticated) {
            postBanner("请先登录服务器账号后再创建机器人")
            return
        }
        botsEpoch.incrementAndGet()
        val capturedGen = generationFence.currentGeneration
        viewModelScope.launch {
            val payload = JSONObject()
                .put("displayName", cleanName)
                .put("username", cleanUsername)
                .put("description", description.trim())
            avatarShape?.let { payload.put("avatarShape", it) }
            avatarColor?.let { payload.put("avatarColor", it) }
            val result = withContext(Dispatchers.IO) {
                hostRequestResult("telegram.server.createBot", payload)
            }
            if (!generationFence.isCurrent(capturedGen)) return@launch
            result.onSuccess { resp ->
                val token = resp.optNonNullString("bot_token").ifBlank { resp.optNonNullString("token") }.ifBlank { null }
                val botObj = resp.optJSONObject("bot") ?: resp
                val botItem = parseFabushiBotDto(botObj, isFabushi = false)
                _state.update { cur ->
                    cur.copy(
                        fabushiBots = if (botItem != null) listOf(botItem) + cur.fabushiBots.filterNot { it.botUserId == botItem.botUserId } else cur.fabushiBots,
                        lastCreatedBotToken = token,
                        toastBanner = "机器人 $cleanName (@$cleanUsername) 已由服务器创建",
                    )
                }
                refreshAllFromHost()
            }.onFailure { err ->
                if (isUnauthorizedError(err)) {
                    handleUnauthorizedSession(err.message ?: "HTTP 401", capturedGen)
                } else {
                    postBanner("创建机器人失败：${err.message ?: "服务器拒绝请求"}")
                }
            }
        }
    }

    fun createCompanionBot(name: String, description: String, handleHint: String = "") {
        createFabushiBot(displayName = name, username = handleHint, description = description)
    }

    fun createFabushiBot(
        displayName: String,
        username: String = "",
        description: String = "",
        avatarShape: String? = null,
        avatarColor: String? = null,
        preferredModel: String? = null,
        approvalMode: String? = null,
        codingAgent: JSONObject? = null,
        openAfterCreate: Boolean = false,
    ) {
        val cleanName = displayName.trim()
        if (cleanName.isBlank()) return
        if (!_state.value.isAuthenticated) {
            postBanner("请先登录服务器账号后再创建 Fabushi 智能体")
            return
        }
        botsEpoch.incrementAndGet()
        chatsEpoch.incrementAndGet()
        val capturedGen = generationFence.currentGeneration
        viewModelScope.launch {
            val payload = JSONObject()
                .put("displayName", cleanName)
                .put("description", description.trim())
            username.trim().trimStart('@').takeIf { it.isNotBlank() }?.let { payload.put("username", it) }
            avatarShape?.let { payload.put("avatarShape", it) }
            avatarColor?.let { payload.put("avatarColor", it) }
            preferredModel?.let { payload.put("preferredModel", it) }
            approvalMode?.let { payload.put("approvalMode", it) }
            codingAgent?.let { payload.put("codingAgent", it) }

            val result = withContext(Dispatchers.IO) {
                hostRequestResult("telegram.server.createFabushiBot", payload)
            }
            if (!generationFence.isCurrent(capturedGen)) return@launch
            result.onSuccess { resp ->
                val token = resp.optNonNullString("bot_token").ifBlank { resp.optNonNullString("token") }.ifBlank { null }
                val botObj = resp.optJSONObject("bot") ?: resp
                val parsedItem = parseFabushiBotDto(botObj, isFabushi = true)
                val companionChatId = resp.optLong("companion_chat_id").takeIf { it > 0L }
                    ?: botObj.optLong("companion_chat_id").takeIf { it > 0L }
                    ?: parsedItem?.companionChatId
                val botItem = parsedItem?.copy(companionChatId = companionChatId ?: parsedItem.companionChatId)
                _state.update { cur ->
                    val nextBots = if (botItem != null) {
                        listOf(botItem) + cur.fabushiBots.filterNot { it.botUserId == botItem.botUserId }
                    } else cur.fabushiBots
                    val nextCoding = nextBots.mapNotNull { it.codingAgent }
                    val nextChats = if (companionChatId != null && botItem != null && cur.chats.none { it.id == companionChatId.toString() }) {
                        val nowMs = System.currentTimeMillis()
                        sortTelegramChats(
                            listOf(
                                TelegramChatItem(
                                    id = companionChatId.toString(),
                                    title = botItem.displayName,
                                    subtitle = "@${botItem.username.trimStart('@')}",
                                    preview = botItem.description,
                                    time = nowTimeLabel(nowMs),
                                    kind = ConversationKind.DIRECT,
                                    isBot = true,
                                    isCompanionGhost = true,
                                    online = false,
                                    username = "@${botItem.username.trimStart('@')}",
                                    botExecutionState = botItem.executionState,
                                    avatarShape = botItem.avatarShape,
                                    avatarColor = botItem.avatarColor,
                                    peerUserId = botItem.botUserId,
                                    peerBio = botItem.description.ifBlank { null },
                                    lastMessageAtMs = nowMs,
                                ),
                            ) + cur.chats,
                        )
                    } else cur.chats
                    cur.copy(
                        fabushiBots = nextBots,
                        codingAgentBots = nextCoding,
                        chats = nextChats,
                        lastCreatedBotToken = token,
                        toastBanner = "已创建智能体「${botItem?.displayName ?: cleanName}」",
                    )
                }
                if (openAfterCreate && botItem != null) {
                    openBotChat(botItem)
                }
            }.onFailure { err ->
                if (isUnauthorizedError(err)) {
                    handleUnauthorizedSession(err.message ?: "HTTP 401", capturedGen)
                } else {
                    postBanner("创建智能体失败：${err.message ?: "网络异常"}")
                }
            }
        }
    }

    fun patchFabushiBot(
        botId: String,
        displayName: String? = null,
        description: String? = null,
        avatarShape: String? = null,
        avatarColor: String? = null,
        preferredModel: String? = null,
        approvalMode: String? = null,
        codingAgent: JSONObject? = null,
        thinkingLevel: String? = null,
        toolsetPreset: String? = null,
    ) {
        if (!_state.value.isAuthenticated || botId.isBlank()) {
            postBanner("需要登录服务器账号才能更新智能体配置")
            return
        }
        botsEpoch.incrementAndGet()
        chatsEpoch.incrementAndGet()
        val capturedGen = generationFence.currentGeneration
        viewModelScope.launch {
            val payload = JSONObject().put("botId", botId)
            displayName?.let { payload.put("displayName", it) }
            description?.let { payload.put("description", it) }
            avatarShape?.let { payload.put("avatarShape", it) }
            avatarColor?.let { payload.put("avatarColor", it) }
            preferredModel?.let { payload.put("preferredModel", it) }
            approvalMode?.let { payload.put("approvalMode", it) }
            codingAgent?.let { payload.put("codingAgent", it) }
            thinkingLevel?.let { payload.put("thinkingLevel", it) }
            toolsetPreset?.let { payload.put("toolsetPreset", it) }
            val result = withContext(Dispatchers.IO) {
                hostRequestResult("telegram.server.patchFabushiBot", payload)
            }
            if (!generationFence.isCurrent(capturedGen)) return@launch
            result.onSuccess { resp ->
                val botObj = resp.optJSONObject("bot") ?: resp
                val updated = parseFabushiBotDto(botObj, isFabushi = true)
                _state.update { cur ->
                    val nextBots = if (updated != null) {
                        val matchesBot: (FabushiBotItem) -> Boolean = {
                            it.botUserId == updated.botUserId || (updated.agentId.isNotBlank() && it.agentId == updated.agentId)
                        }
                        val exists = cur.fabushiBots.any(matchesBot)
                        if (exists) cur.fabushiBots.map { if (matchesBot(it)) updated else it }
                        else listOf(updated) + cur.fabushiBots
                    } else cur.fabushiBots
                    val nextChats = if (updated != null) {
                        cur.chats.map { ch ->
                            val matchesChat = (updated.companionChatId != null && ch.id == updated.companionChatId.toString()) ||
                                (ch.peerUserId != null && ch.peerUserId == updated.botUserId) ||
                                matchesBotUsername(ch.username, updated.username)
                            if (matchesChat) {
                                ch.copy(
                                    title = updated.displayName.ifBlank { ch.title },
                                    avatarShape = updated.avatarShape.ifBlank { ch.avatarShape },
                                    avatarColor = updated.avatarColor.ifBlank { ch.avatarColor },
                                )
                            } else ch
                        }
                    } else cur.chats
                    cur.copy(
                        chats = nextChats,
                        fabushiBots = nextBots,
                        codingAgentBots = nextBots.mapNotNull { it.codingAgent },
                        toastBanner = "智能体配置已更新",
                    )
                }
            }.onFailure { err ->
                if (isUnauthorizedError(err)) {
                    handleUnauthorizedSession(err.message ?: "HTTP 401", capturedGen)
                } else {
                    postBanner("更新智能体失败：${err.message ?: "无权限或网络异常"}")
                }
            }
        }
    }

    fun getBotProfile(target: String) {
        if (!_state.value.isAuthenticated || target.isBlank()) return
        val capturedGen = generationFence.currentGeneration
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                hostRequestResult("telegram.server.getBotProfile", JSONObject().put("target", target))
            }
            if (!generationFence.isCurrent(capturedGen)) return@launch
            result.onSuccess { resp ->
                _state.update { it.copy(lastBotProfileJson = resp.toString()) }
            }.onFailure { err ->
                if (isUnauthorizedError(err)) {
                    handleUnauthorizedSession(err.message ?: "HTTP 401", capturedGen)
                } else {
                    postBanner("获取机器人资料失败：${err.message}")
                }
            }
        }
    }

    fun botfatherNlCreate(
        prompt: String,
        avatarShape: String? = null,
        avatarColor: String? = null,
        openAfterCreate: Boolean = false,
    ) {
        val clean = prompt.trim()
        if (clean.isBlank()) return
        if (!_state.value.isAuthenticated) {
            postBanner("请先登录服务器账号后再使用 BotFather 创建机器人")
            return
        }
        botsEpoch.incrementAndGet()
        val capturedGen = generationFence.currentGeneration
        viewModelScope.launch {
            val payload = JSONObject().put("prompt", clean)
            avatarShape?.let { payload.put("avatarShape", it) }
            avatarColor?.let { payload.put("avatarColor", it) }
            val result = withContext(Dispatchers.IO) {
                hostRequestResult("telegram.server.botfatherNlCreate", payload)
            }
            if (!generationFence.isCurrent(capturedGen)) return@launch
            result.onSuccess { resp ->
                val token = resp.optNonNullString("bot_token").ifBlank { resp.optNonNullString("token") }.ifBlank { null }
                val botObj = resp.optJSONObject("bot") ?: resp
                val parsedItem = parseFabushiBotDto(botObj, isFabushi = true)
                val companionChatId = resp.optLong("companion_chat_id").takeIf { it > 0L }
                    ?: botObj.optLong("companion_chat_id").takeIf { it > 0L }
                    ?: parsedItem?.companionChatId
                val botItem = parsedItem?.copy(companionChatId = companionChatId ?: parsedItem.companionChatId)
                _state.update { cur ->
                    cur.copy(
                        fabushiBots = if (botItem != null) listOf(botItem) + cur.fabushiBots.filterNot { it.botUserId == botItem.botUserId } else cur.fabushiBots,
                        lastCreatedBotToken = token,
                        toastBanner = "BotFather 已创建机器人 ${botItem?.displayName.orEmpty()}",
                    )
                }
                refreshServerBots()
                if (openAfterCreate && botItem != null) {
                    openBotChat(botItem)
                }
            }.onFailure { err ->
                if (isUnauthorizedError(err)) {
                    handleUnauthorizedSession(err.message ?: "HTTP 401", capturedGen)
                } else {
                    postBanner("BotFather 创建失败：${err.message ?: "网络异常"}")
                }
            }
        }
    }

    fun createCodingAgentBot(
        provider: CodingProviderKindUi,
        name: String,
        handle: String,
        workspacePath: String,
        permissionMode: CodingPermissionModeUi,
        useWorktree: Boolean,
    ) {
        val cleanName = name.trim()
        val cleanWorkspace = workspacePath.trim().ifBlank { "~/workspace" }
        if (cleanName.isBlank()) return
        val codingObj = JSONObject()
            .put("provider_kind", provider.wire)
            .put("workspace_path", cleanWorkspace)
            .put("permission_mode", permissionMode.wire)
            .put("use_worktree", useWorktree)
        createFabushiBot(
            displayName = cleanName,
            username = handle,
            description = "${provider.label} · $cleanWorkspace",
            codingAgent = codingObj,
        )
    }

    fun cycleCodingAgentPermission(botId: String, nextMode: CodingPermissionModeUi) {
        patchFabushiBot(
            botId = botId,
            codingAgent = JSONObject().put("permission_mode", nextMode.wire),
        )
    }

    fun probeCodingAgents() {
        if (!_state.value.isAuthenticated) {
            postBanner("请先登录服务器账号后再探测编程智能体")
            return
        }
        val capturedGen = generationFence.currentGeneration
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                hostRequestResult("telegram.server.probeCodingAgents", JSONObject())
            }
            if (!generationFence.isCurrent(capturedGen)) return@launch
            result.onSuccess { resp ->
                val arr = resp.optJSONArray("probes")
                    ?: resp.optJSONArray("providers")
                    ?: resp.optJSONArray("agents")
                    ?: resp.optJSONArray("value")
                    ?: JSONArray()
                val probes = buildList {
                    for (i in 0 until arr.length()) {
                        val p = arr.optJSONObject(i) ?: continue
                        val wire = p.optNonNullString("provider_kind", p.optNonNullString("provider", ""))
                        val kind = CodingProviderKindUi.entries.firstOrNull { it.wire == wire } ?: continue
                        add(
                            CodingAgentProbeItem(
                                providerKind = kind,
                                installed = p.optBoolean("installed", false),
                                version = p.optNonNullString("version").ifBlank { null },
                                executablePath = p.optNonNullString("executable_path").ifBlank { null },
                            ),
                        )
                    }
                }
                _state.update { it.copy(codingAgentProbes = probes) }
            }.onFailure { err ->
                if (isUnauthorizedError(err)) {
                    handleUnauthorizedSession(err.message ?: "HTTP 401", capturedGen)
                } else {
                    postBanner("探测编程智能体失败：${err.message}")
                }
            }
        }
    }

    fun getBotComputer(botId: String) {
        if (!_state.value.isAuthenticated || botId.isBlank()) return
        val capturedGen = generationFence.currentGeneration
        val capturedEpoch = botsEpoch.get()
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                hostRequestResult("telegram.server.getBotComputer", JSONObject().put("botId", botId))
            }
            if (!generationFence.isCurrent(capturedGen)) return@launch
            result.onSuccess { resp ->
                val boundNode = resp.optNullableString("bound_node_id")
                _state.update { cur ->
                    if (botsEpoch.get() != capturedEpoch) cur
                    else {
                        val nextMap = if (boundNode != null) cur.botComputerBindings + (botId to boundNode) else cur.botComputerBindings - botId
                        cur.copy(
                            botComputerBindings = nextMap,
                            fabushiBots = cur.fabushiBots.map { b ->
                                if (b.botUserId.toString() == botId || b.agentId == botId) b.copy(boundNodeId = boundNode) else b
                            },
                        )
                    }
                }
            }.onFailure { err ->
                if (isUnauthorizedError(err)) handleUnauthorizedSession(err.message ?: "HTTP 401", capturedGen)
            }
        }
    }

    fun setBotComputer(botId: String, boundNodeId: String?, clearBoundNode: Boolean = false) {
        if (!_state.value.isAuthenticated || botId.isBlank()) {
            postBanner("请先登录服务器账号后再绑定远程计算节点")
            return
        }
        botsEpoch.incrementAndGet()
        val capturedGen = generationFence.currentGeneration
        viewModelScope.launch {
            val payload = JSONObject()
                .put("botId", botId)
                .put("clearBoundNode", clearBoundNode)
            if (!clearBoundNode && !boundNodeId.isNullOrBlank()) {
                payload.put("boundNodeId", boundNodeId)
            }
            val result = withContext(Dispatchers.IO) {
                hostRequestResult("telegram.server.setBotComputer", payload)
            }
            if (!generationFence.isCurrent(capturedGen)) return@launch
            result.onSuccess { resp ->
                val resolved = resp.optNullableString("bound_node_id") ?: if (clearBoundNode) null else boundNodeId
                _state.update { cur ->
                    val nextMap = if (resolved != null) cur.botComputerBindings + (botId to resolved) else cur.botComputerBindings - botId
                    cur.copy(
                        botComputerBindings = nextMap,
                        fabushiBots = cur.fabushiBots.map { b ->
                            if (b.botUserId.toString() == botId || b.agentId == botId) b.copy(boundNodeId = resolved) else b
                        },
                        toastBanner = if (resolved != null) "已绑定计算节点 $resolved" else "已解除计算节点绑定",
                    )
                }
            }.onFailure { err ->
                if (isUnauthorizedError(err)) {
                    handleUnauthorizedSession(err.message ?: "HTTP 401", capturedGen)
                } else {
                    postBanner("绑定计算节点失败：${err.message}")
                }
            }
        }
    }

    // ------------------------------------------------------------------------
    // Hermes Studio (Contracts #13, #14, #15, K-F12: Real Fail-Closed RPCs)
    // ------------------------------------------------------------------------

    fun selectHermesTab(tab: HermesStudioTab) {
        _state.update { it.copy(hermesStudio = it.hermesStudio.copy(activeTab = tab)) }
    }

    fun branchHermesSession(sourceId: String) {
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                hostRequestResult("hermes.branchSession", JSONObject().put("sessionId", sourceId))
            }
            result.onSuccess { resp ->
                val newId = resp.optNonNullString("sessionId").ifBlank { "hermes-${UUID.randomUUID().toString().take(6)}" }
                _state.update { cur ->
                    val src = cur.hermesStudio.sessions.firstOrNull { it.id == sourceId }
                    val branched = HermesSessionItem(
                        id = newId,
                        title = resp.optNonNullString("title").ifBlank { "${src?.title ?: "Hermes"} · 分叉" },
                        provider = src?.provider.orEmpty(),
                        model = src?.model.orEmpty(),
                        parentBranchId = sourceId,
                        turnCount = 0,
                        lastOutput = "",
                    )
                    cur.copy(
                        hermesStudio = cur.hermesStudio.copy(
                            sessions = listOf(branched) + cur.hermesStudio.sessions,
                            activeSessionId = branched.id,
                        ),
                        toastBanner = "已分叉新会话",
                    )
                }
            }.onFailure { err ->
                postBanner("分叉会话失败：${err.message}")
            }
        }
    }

    fun runHermesMoaPrompt(prompt: String) {
        val clean = prompt.trim().ifBlank { return }
        viewModelScope.launch {
            val payload = JSONObject().put("prompt", clean).put("moa", true)
            _state.value.hermesStudio.activeSessionId.takeIf { it.isNotBlank() }?.let { payload.put("sessionId", it) }
            val result = withContext(Dispatchers.IO) {
                hostRequestResult("hermes.sendTurn", payload)
            }
            result.onSuccess { resp ->
                val output = resp.optNonNullString("reply")
                    .ifBlank { resp.optNonNullString("output") }
                    .ifBlank { resp.optNonNullString("content") }
                _state.update { cur ->
                    val updatedSessions = cur.hermesStudio.sessions.map { s ->
                        if (s.id == cur.hermesStudio.activeSessionId) {
                            s.copy(turnCount = s.turnCount + 1, lastOutput = output)
                        } else s
                    }
                    cur.copy(hermesStudio = cur.hermesStudio.copy(sessions = updatedSessions))
                }
            }.onFailure { err ->
                // K-F12: ZERO fake [MoA 融合结论] fallback!
                postBanner("Hermes 推理失败：${err.message ?: "请先配置 Magpie API Key"}")
            }
        }
    }

    /**
     * Contract #13: hermes.memoryUpdate expects {target: "SOUL.md", content: markdown}.
     */
    fun updateHermesSoul(markdown: String) {
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                hostRequestResult(
                    "hermes.memoryUpdate",
                    JSONObject().put("target", "SOUL.md").put("content", markdown),
                )
            }
            result.onSuccess { resp ->
                val persisted = resp.optNonNullString("content", markdown)
                _state.update {
                    it.copy(
                        hermesStudio = it.hermesStudio.copy(soulMarkdown = persisted),
                        toastBanner = "SOUL.md 已持久化保存",
                    )
                }
            }.onFailure { err ->
                postBanner("保存 SOUL.md 失败：${err.message}")
            }
        }
    }

    fun toggleHermesSkill(skillId: String) {
        val target = _state.value.hermesStudio.skills.firstOrNull { it.id == skillId || it.name == skillId }
        if (target == null) {
            postBanner("技能不存在：$skillId")
            return
        }
        val nextEnabled = !target.enabled
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                if (target.category.equals("toolset", ignoreCase = true)) {
                    hostRequestResult(
                        "hermes.toolsetToggle",
                        JSONObject().put("id", target.id).put("enabled", nextEnabled),
                    )
                } else {
                    val existingBody = hostRequestResult("hermes.skillsList").getOrNull()
                        ?.optJSONArray("skills")
                        ?.let { arr ->
                            (0 until arr.length())
                                .asSequence()
                                .mapNotNull { arr.optJSONObject(it) }
                                .firstOrNull { it.optNonNullString("name") == target.name }
                                ?.optNonNullString("bodyMarkdown")
                        }
                        .orEmpty()
                        .ifBlank { "# ${target.name}\n\n${target.description.ifBlank { "Hermes skill" }}" }
                    hostRequestResult(
                        "hermes.skillUpsert",
                        JSONObject()
                            .put("name", target.name)
                            .put("description", target.description)
                            .put("bodyMarkdown", existingBody)
                            .put("enabled", nextEnabled),
                    )
                }
            }
            result.onSuccess {
                _state.update { cur ->
                    val skills = cur.hermesStudio.skills.map {
                        if (it.id == target.id) it.copy(enabled = nextEnabled) else it
                    }
                    cur.copy(hermesStudio = cur.hermesStudio.copy(skills = skills))
                }
            }.onFailure { err ->
                postBanner("切换技能失败：${err.message}")
            }
        }
    }

    /**
     * Contract #14: hermes.runSlashCommand expects {input: clean}. Zero fake uname / exit 0 · 14ms!
     */
    fun executeHermesTerminal(cmd: String) {
        val clean = cmd.trim().ifBlank { return }
        viewModelScope.launch {
            val payload = JSONObject().put("input", clean)
            _state.value.hermesStudio.activeSessionId.takeIf { it.isNotBlank() }?.let { payload.put("sessionId", it) }
            val result = withContext(Dispatchers.IO) {
                hostRequestResult("hermes.runSlashCommand", payload)
            }
            result.onSuccess { resp ->
                val out = resp.optNonNullString("summary").ifBlank { resp.toString() }
                _state.update { cur ->
                    cur.copy(hermesStudio = cur.hermesStudio.copy(terminalLines = (cur.hermesStudio.terminalLines + "$ $clean" + out).takeLast(24)))
                }
            }.onFailure { err ->
                val errLine = "error: ${err.message ?: "命令执行失败"}"
                _state.update { cur ->
                    cur.copy(
                        hermesStudio = cur.hermesStudio.copy(terminalLines = (cur.hermesStudio.terminalLines + "$ $clean" + errLine).takeLast(24)),
                        toastBanner = errLine,
                    )
                }
            }
        }
    }

    fun addHermesCronJob(title: String, expr: String, prompt: String) {
        val cleanTitle = title.trim()
        val cleanExpr = expr.trim()
        val cleanPrompt = prompt.trim().ifBlank { cleanTitle }
        if (cleanExpr.isBlank() || cleanPrompt.isBlank()) return
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                hostRequestResult(
                    "hermes.cronUpsert",
                    JSONObject()
                        .put("schedule", cleanExpr)
                        .put("prompt", cleanPrompt),
                )
            }
            result.onSuccess { resp ->
                val id = resp.optNonNullString("jobId").ifBlank { "cron-${UUID.randomUUID().toString().take(6)}" }
                val job = HermesCronItem(
                    id = id,
                    title = cleanTitle.ifBlank { cleanPrompt.take(40) },
                    scheduleExpr = resp.optNonNullString("schedule").ifBlank { cleanExpr },
                    prompt = resp.optNonNullString("prompt").ifBlank { cleanPrompt },
                    enabled = true,
                    lastRunStatus = "",
                )
                _state.update { cur -> cur.copy(hermesStudio = cur.hermesStudio.copy(cronJobs = listOf(job) + cur.hermesStudio.cronJobs)) }
            }.onFailure { err ->
                postBanner("创建定时任务失败：${err.message}")
            }
        }
    }

    fun advanceKanbanCard(cardId: String) {
        val card = _state.value.hermesStudio.kanbanCards.firstOrNull { it.id == cardId } ?: return
        val nextCols = KanbanColumn.entries
        val nextCol = nextCols[(card.column.ordinal + 1) % nextCols.size]
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                hostRequestResult(
                    "hermes.kanbanMove",
                    JSONObject().put("cardId", cardId).put("columnId", nextCol.toRustColumnId()),
                )
            }
            result.onSuccess {
                _state.update { cur ->
                    val cards = cur.hermesStudio.kanbanCards.map { if (it.id == cardId) it.copy(column = nextCol) else it }
                    cur.copy(hermesStudio = cur.hermesStudio.copy(kanbanCards = cards))
                }
            }.onFailure { err ->
                postBanner("更新看板卡片失败：${err.message}")
            }
        }
    }

    /**
     * Contract #15: hermes.approvalsDecide expects {approvalId, decision: "approved"|"denied"}.
     */
    fun decideHermesApproval(approvalId: String, approved: Boolean) {
        val decision = if (approved) "approved" else "denied"
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                hostRequestResult(
                    "hermes.approvalsDecide",
                    JSONObject().put("approvalId", approvalId).put("decision", decision),
                )
            }
            result.onSuccess {
                _state.update { cur ->
                    val items = cur.hermesStudio.approvals.map {
                        if (it.id == approvalId) it.copy(status = decision) else it
                    }
                    cur.copy(hermesStudio = cur.hermesStudio.copy(approvals = items))
                }
            }.onFailure { err ->
                postBanner("审批决策失败：${err.message}")
            }
        }
    }

    // ------------------------------------------------------------------------
    // Magpie Gateway (Contracts #2, #3, K-F11: Real Key Persistence + Keystore Sync)
    // ------------------------------------------------------------------------

    fun setMagpieStrategy(strategy: MagpieRoutingStrategy) {
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                hostRequestResult("magpie.setStrategy", JSONObject().put("strategy", strategy.wire))
            }
            result.onSuccess {
                _state.update { cur ->
                    cur.copy(
                        magpieGateway = cur.magpieGateway.copy(
                            strategy = strategy,
                            lastRouteExplanation = "${strategy.label} 已激活 → ${strategy.subtitle}",
                        ),
                    )
                }
            }.onFailure { err ->
                postBanner("切换路由策略失败：${err.message}")
            }
        }
    }

    fun configureMagpieProvider(presetId: String, apiKeyInput: String, baseUrl: String? = null) {
        val cleanKey = apiKeyInput.trim()
        if (cleanKey.isBlank()) {
            postBanner("API Key 不能为空")
            return
        }
        viewModelScope.launch {
            val payload = JSONObject()
                .put("presetId", presetId)
                .put("apiKey", cleanKey)
                .put("enabled", true)
            baseUrl?.trim()?.takeIf { it.isNotBlank() }?.let { payload.put("baseUrl", it) }

            val upsertResult = withContext(Dispatchers.IO) {
                hostRequestResult("magpie.upsertProvider", payload)
            }
            upsertResult.onSuccess {
                val providersResp = withContext(Dispatchers.IO) {
                    hostRequestResult("magpie.providers").getOrNull()
                }
                val presetMeta = _state.value.magpieGateway.presets.firstOrNull { it.id == presetId }
                if (presetMeta != null) {
                    mikaBridge?.syncMagpieProviderKey(
                        presetId = presetMeta.id,
                        displayName = presetMeta.name,
                        protocol = presetMeta.protocol,
                        baseUrl = baseUrl?.trim()?.ifBlank { null } ?: presetMeta.defaultBaseUrl,
                        defaultModel = presetMeta.flagshipModel,
                        apiKey = cleanKey,
                    )
                }
                _state.update { cur ->
                    val nextMagpie = if (providersResp != null) {
                        applyMagpieProvidersResponse(cur.magpieGateway, providersResp)
                    } else {
                        cur.magpieGateway.copy(
                            liveReady = true,
                            presets = cur.magpieGateway.presets.map { if (it.id == presetId) it.copy(configured = true, healthy = true) else it },
                        )
                    }
                    cur.copy(
                        magpieGateway = nextMagpie,
                        toastBanner = "已保存 $presetId 密钥并同步至本机安全存储",
                    )
                }
            }.onFailure { err ->
                postBanner("保存 $presetId 密钥失败：${err.message ?: "未知错误"}")
            }
        }
    }

    /**
     * Contract #3: magpie.importUri ({uri}) & magpie.importEnv ({envText}).
     */
    fun importMagpieEnvOrUri(rawText: String) {
        val clean = rawText.trim()
        if (clean.isBlank()) return
        val isUri = !clean.contains("\n") && Regex("^[A-Za-z][A-Za-z0-9+.-]*://").containsMatchIn(clean)
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                if (isUri) {
                    hostRequestResult("magpie.importUri", JSONObject().put("uri", clean))
                } else {
                    hostRequestResult("magpie.importEnv", JSONObject().put("envText", clean))
                }
            }
            result.onSuccess { resp ->
                val providersResp = withContext(Dispatchers.IO) {
                    hostRequestResult("magpie.providers").getOrNull()
                }
                val importedCount = resp.optInt("importedCount", if (resp.has("imported")) 1 else 0)
                _state.update { cur ->
                    val nextMagpie = if (providersResp != null) {
                        applyMagpieProvidersResponse(cur.magpieGateway, providersResp)
                    } else cur.magpieGateway
                    cur.copy(
                        magpieGateway = nextMagpie,
                        toastBanner = "已导入 $importedCount 个供应商通道",
                    )
                }
            }.onFailure { err ->
                postBanner("导入供应商配置失败：${err.message ?: "格式无效"}")
            }
        }
    }

    fun testMagpiePiiMask(sampleText: String) {
        val (masked, count) = MagpiePresetCatalog.maskSecretsAndPii(sampleText)
        _state.update { cur ->
            cur.copy(
                magpieGateway = cur.magpieGateway.copy(
                    lastMaskedPreview = masked,
                    piiRedactedCount = cur.magpieGateway.piiRedactedCount + count,
                ),
            )
        }
    }

    fun syncModelsDevCatalog() {
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                hostRequestResult("magpie.discoverModels", JSONObject().put("subscriptionKind", "google-vertex"))
            }
            result.onSuccess { res ->
                val models = buildList {
                    val arr = res.optJSONArray("models")
                    if (arr != null) {
                        for (i in 0 until arr.length()) {
                            arr.optNonNullString(i).takeIf { it.isNotBlank() }?.let(::add)
                        }
                    }
                }
                // K-F11: ZERO fictional model fallback ("gemini-3.8-flash" / "gpt-6.1-sol")!
                _state.update { cur ->
                    cur.copy(
                        magpieGateway = cur.magpieGateway.copy(
                            discoveredModels = models,
                            modelsDevStatusLabel = if (models.isNotEmpty()) {
                                "已同步 ${models.size} 个上游模型 (${nowTimeLabel()})"
                            } else {
                                "未发现可用上游模型（请先配置有效 Provider API Key）"
                            },
                        ),
                        toastBanner = if (models.isNotEmpty()) "已刷新模型目录 (${models.size} 个)" else "未拉取到上游模型",
                    )
                }
            }.onFailure { err ->
                _state.update { cur ->
                    cur.copy(
                        magpieGateway = cur.magpieGateway.copy(
                            modelsDevStatusLabel = "模型目录同步失败：${err.message ?: "请检查网络与 API Key"}",
                        ),
                        toastBanner = "同步模型目录失败：${err.message}",
                    )
                }
            }
        }
    }

    fun scanLocalSubscriptions() {
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                hostRequestResult("magpie.discoverSyncableSubscriptions", JSONObject().put("targetIsRemote", true))
            }
            result.onSuccess { res ->
                val count = res.optInt("total", 0)
                _state.update { cur ->
                    cur.copy(
                        magpieGateway = cur.magpieGateway.copy(
                            localSubscriptionsSummary = "最近扫描 (${nowTimeLabel()})：发现 $count 个可同步本地订阅通道",
                        ),
                        toastBanner = "本地订阅扫描完成：发现 $count 个可同步通道",
                    )
                }
            }.onFailure { err ->
                postBanner("扫描本地订阅失败：${err.message}")
            }
        }
    }

    // ------------------------------------------------------------------------
    // Real Remote Control: Computers, UU-Remote Assist, Sessions & OAuth 2.1 MCP Grants (F11)
    // ------------------------------------------------------------------------

    fun refreshComputers() {
        val capturedGen = generationFence.currentGeneration
        val capturedEpoch = computersEpoch.get()
        viewModelScope.launch {
            val (localResp, serverResp) = withContext(Dispatchers.IO) {
                val local = hostRequestResult("computer.listNodes").getOrNull()
                val srv = if (_state.value.isAuthenticated) {
                    hostRequestResult("telegram.server.listComputers")
                } else null
                local to srv
            }
            if (!generationFence.isCurrent(capturedGen)) return@launch
            if (serverResp != null && isUnauthorizedError(serverResp.exceptionOrNull())) {
                handleUnauthorizedSession(serverResp.exceptionOrNull()?.message ?: "HTTP 401", capturedGen)
                return@launch
            }
            val serverNodes = serverResp?.getOrNull()?.let(::parseServerComputers)
            val mikaAvailable = mikaBridge?.currentSnapshot()?.available ?: _state.value.mikaSnapshot.available
            val merged = mergeComputerNodes(localResp, serverNodes, mikaAvailable)
            _state.update { cur ->
                if (computersEpoch.get() != capturedEpoch) cur
                else cur.copy(
                    cloudComputer = cur.cloudComputer.copy(
                        nodes = merged,
                        selectedNodeId = cur.cloudComputer.selectedNodeId.takeIf { id -> merged.any { it.id == id } }
                            ?: merged.firstOrNull()?.id.orEmpty(),
                    ),
                )
            }
        }
    }

    fun upsertCloudComputer(
        displayName: String,
        host: String,
        port: Int = 22,
        username: String = "root",
        authMethod: String = "password",
        secret: String? = null,
        nodeId: String? = null,
        verifyConnection: Boolean = false,
    ) {
        if (host.isBlank()) {
            postBanner("云服务器主机地址不能为空")
            return
        }
        if (!_state.value.isAuthenticated) {
            postBanner("请先登录服务器账号后再添加云服务器")
            return
        }
        computersEpoch.incrementAndGet()
        val capturedGen = generationFence.currentGeneration
        viewModelScope.launch {
            val payload = JSONObject()
                .put("displayName", displayName.trim().ifBlank { host.trim() })
                .put("host", host.trim())
                .put("port", port)
                .put("username", username.trim())
                .put("authMethod", authMethod)
                .put("verifyConnection", verifyConnection)
            nodeId?.takeIf { it.isNotBlank() }?.let { payload.put("nodeId", it) }
            secret?.takeIf { it.isNotBlank() }?.let { payload.put("secret", it) }
            val result = withContext(Dispatchers.IO) {
                hostRequestResult("telegram.server.upsertCloudComputer", payload)
            }
            if (!generationFence.isCurrent(capturedGen)) return@launch
            result.onSuccess { resp ->
                val nodeObj = resp.optJSONObject("computer") ?: resp
                val parsed = parseServerComputerObject(nodeObj)
                if (parsed != null) {
                    _state.update { cur ->
                        val nextNodes = cur.cloudComputer.nodes.filterNot { it.id == parsed.id } + parsed
                        cur.copy(
                            cloudComputer = cur.cloudComputer.copy(nodes = nextNodes, selectedNodeId = parsed.id),
                            toastBanner = "已保存远程节点「${parsed.name}」",
                        )
                    }
                } else {
                    refreshComputers()
                }
            }.onFailure { err ->
                if (isUnauthorizedError(err)) {
                    handleUnauthorizedSession(err.message ?: "HTTP 401", capturedGen)
                } else {
                    postBanner("保存云服务器失败：${err.message}")
                }
            }
        }
    }

    fun patchComputerPolicy(nodeId: String, runPolicy: String? = null, displayName: String? = null) {
        if (!_state.value.isAuthenticated || nodeId.isBlank()) return
        computersEpoch.incrementAndGet()
        val capturedGen = generationFence.currentGeneration
        viewModelScope.launch {
            val payload = JSONObject().put("nodeId", nodeId)
            runPolicy?.let { payload.put("runPolicy", it) }
            displayName?.let { payload.put("displayName", it) }
            val result = withContext(Dispatchers.IO) {
                hostRequestResult("telegram.server.patchComputerPolicy", payload)
            }
            if (!generationFence.isCurrent(capturedGen)) return@launch
            result.onSuccess { resp ->
                val nodeObj = resp.optJSONObject("computer") ?: resp
                val parsed = parseServerComputerObject(nodeObj)
                _state.update { cur ->
                    val nextNodes = cur.cloudComputer.nodes.map { n ->
                        if (n.id == nodeId) {
                            parsed ?: n.copy(runPolicy = runPolicy ?: n.runPolicy, name = displayName ?: n.name)
                        } else n
                    }
                    cur.copy(cloudComputer = cur.cloudComputer.copy(nodes = nextNodes), toastBanner = "节点执行策略已更新")
                }
            }.onFailure { err ->
                if (isUnauthorizedError(err)) {
                    handleUnauthorizedSession(err.message ?: "HTTP 401", capturedGen)
                } else {
                    postBanner("更新节点策略失败：${err.message}")
                }
            }
        }
    }

    fun deleteComputer(nodeId: String) {
        if (!_state.value.isAuthenticated || nodeId.isBlank()) return
        val capturedGen = generationFence.currentGeneration
        computersEpoch.incrementAndGet()
        var removedNode: ComputerNodeItem? = null
        _state.update { cur ->
            removedNode = cur.cloudComputer.nodes.firstOrNull { it.id == nodeId }
            val filtered = cur.cloudComputer.nodes.filterNot { it.id == nodeId }
            cur.copy(
                cloudComputer = cur.cloudComputer.copy(
                    nodes = filtered,
                    selectedNodeId = if (cur.cloudComputer.selectedNodeId == nodeId) filtered.firstOrNull()?.id.orEmpty() else cur.cloudComputer.selectedNodeId,
                ),
            )
        }
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                hostRequestResult("telegram.server.deleteComputer", JSONObject().put("nodeId", nodeId))
            }
            if (!generationFence.isCurrent(capturedGen)) return@launch
            result.onSuccess {
                postBanner("已删除远程节点")
            }.onFailure { err ->
                if (isUnauthorizedError(err)) {
                    handleUnauthorizedSession(err.message ?: "HTTP 401", capturedGen)
                } else {
                    val restore = removedNode
                    _state.update { cur ->
                        val nextNodes = if (restore != null && cur.cloudComputer.nodes.none { it.id == restore.id }) {
                            cur.cloudComputer.nodes + restore
                        } else cur.cloudComputer.nodes
                        cur.copy(cloudComputer = cur.cloudComputer.copy(nodes = nextNodes), toastBanner = "删除节点失败：${err.message}")
                    }
                }
            }
        }
    }

    fun testComputer(nodeId: String) {
        if (!_state.value.isAuthenticated || nodeId.isBlank()) {
            postBanner("请先登录服务器账号后再测试远程节点")
            return
        }
        computersEpoch.incrementAndGet()
        val capturedGen = generationFence.currentGeneration
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                hostRequestResult("telegram.server.testComputer", JSONObject().put("nodeId", nodeId))
            }
            if (!generationFence.isCurrent(capturedGen)) return@launch
            result.onSuccess { resp ->
                val ok = resp.optBoolean("ok", false)
                val stdout = resp.optNonNullString("stdout", "")
                val stderr = resp.optNonNullString("stderr", "")
                val outputText = buildString {
                    append("[$nodeId] test -> ${if (ok) "OK" else "FAILED"}")
                    if (stdout.isNotBlank()) append("\n$stdout")
                    if (stderr.isNotBlank()) append("\nstderr: $stderr")
                }
                _state.update { cur ->
                    val nextNodes = cur.cloudComputer.nodes.map { n ->
                        if (n.id == nodeId) n.copy(online = ok) else n
                    }
                    cur.copy(
                        cloudComputer = cur.cloudComputer.copy(nodes = nextNodes, lastOpOutput = outputText),
                        toastBanner = if (ok) "节点连通性测试通过" else "节点连通性测试失败：${stderr.ifBlank { "不可达" }}",
                    )
                }
            }.onFailure { err ->
                if (isUnauthorizedError(err)) {
                    handleUnauthorizedSession(err.message ?: "HTTP 401", capturedGen)
                } else {
                    _state.update { cur ->
                        cur.copy(
                            cloudComputer = cur.cloudComputer.copy(lastOpOutput = "[$nodeId] test error: ${err.message}"),
                            toastBanner = "测试连接失败：${err.message}",
                        )
                    }
                }
            }
        }
    }

    fun selectComputerNode(nodeId: String) {
        _state.update { it.copy(cloudComputer = it.cloudComputer.copy(selectedNodeId = nodeId)) }
    }

    fun acquireCuaLease(nodeId: String) {
        if (nodeId.isBlank()) return
        computersEpoch.incrementAndGet()
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                hostRequestResult("computer.leaseAcquire", JSONObject().put("nodeId", nodeId))
            }
            result.onSuccess { resp ->
                val holder = resp.optNonNullString("leaseHolder").ifBlank { resp.optNonNullString("holder", "android-operator") }
                val gen = resp.optLong("generation", resp.optLong("leaseGeneration", 1L))
                _state.update { cur ->
                    val nodes = cur.cloudComputer.nodes.map {
                        if (it.id == nodeId) it.copy(leaseOwner = holder, leaseGeneration = gen, humanTakeoverActive = false) else it
                    }
                    cur.copy(
                        cloudComputer = cur.cloudComputer.copy(
                            nodes = nodes,
                            lastOpOutput = "已获取节点 $nodeId 租约 (holder=$holder, gen=$gen)",
                        ),
                    )
                }
            }.onFailure { err ->
                postBanner("获取 CUA 租约失败：${err.message}")
            }
        }
    }

    fun emergencyHumanTakeover(nodeId: String) {
        computersEpoch.incrementAndGet()
        mikaBridge?.stopActiveTask()
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                hostRequestResult("computer.leaseTakeover", JSONObject().put("nodeId", nodeId))
            }
            result.onSuccess {
                _state.update { cur ->
                    val nodes = cur.cloudComputer.nodes.map {
                        if (it.id == nodeId) it.copy(leaseOwner = null, humanTakeoverActive = true) else it
                    }
                    cur.copy(
                        cloudComputer = cur.cloudComputer.copy(
                            nodes = nodes,
                            lastOpOutput = "🚨 已执行紧急人工接管：节点 $nodeId 租约已吊销",
                        ),
                        toastBanner = "🚨 已执行紧急人工接管",
                    )
                }
            }.onFailure { err ->
                postBanner("人工接管失败：${err.message}")
            }
        }
    }

    /**
     * Contract #17:
     * - `local-android-mika`: executes via real Mika `AndroidSandbox` (fails closed if not ready)
     * - Cloud / server nodes: dispatches real `telegram.server.execComputer` with tagged `RemoteComputerOp`
     * Zero fake "→ 执行完成 (0)" fallback!
     */
    fun executeComputerOp(command: String) {
        val clean = command.trim().ifBlank { return }
        val nodeId = _state.value.cloudComputer.selectedNodeId
        if (nodeId.isBlank()) {
            postBanner("请先选择一个计算节点")
            return
        }
        if (nodeId == "local-android-mika") {
            viewModelScope.launch {
                val bridge = mikaBridge
                if (bridge == null) {
                    val msg = "[$nodeId] LOCAL_SANDBOX_NOT_READY: Mika 本机沙箱未连接"
                    _state.update { cur -> cur.copy(cloudComputer = cur.cloudComputer.copy(lastOpOutput = msg), toastBanner = msg) }
                    return@launch
                }
                val res = withContext(Dispatchers.IO) { bridge.executeLocalSandboxCommand(clean) }
                res.onSuccess { out ->
                    _state.update { cur ->
                        cur.copy(cloudComputer = cur.cloudComputer.copy(lastOpOutput = "[$nodeId] $ $clean\n$out"))
                    }
                }.onFailure { err ->
                    val errText = "[$nodeId] $ $clean\nerror: ${err.message ?: "执行失败"}"
                    _state.update { cur ->
                        cur.copy(cloudComputer = cur.cloudComputer.copy(lastOpOutput = errText), toastBanner = "本机执行失败：${err.message}")
                    }
                }
            }
            return
        }
        execComputer(
            nodeId = nodeId,
            operation = JSONObject()
                .put("op", "shell_exec")
                .put("command", clean)
                .put("timeout_ms", 15_000),
        )
    }

    fun execComputer(nodeId: String, operation: JSONObject) {
        if (!_state.value.isAuthenticated) {
            val msg = "[$nodeId] SERVER_AUTH_REQUIRED: 请先登录服务器账号"
            _state.update { cur -> cur.copy(cloudComputer = cur.cloudComputer.copy(lastOpOutput = msg), toastBanner = msg) }
            return
        }
        val capturedGen = generationFence.currentGeneration
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                hostRequestResult(
                    "telegram.server.execComputer",
                    JSONObject()
                        .put("nodeId", nodeId)
                        .put("operation", operation),
                )
            }
            if (!generationFence.isCurrent(capturedGen)) return@launch
            result.onSuccess { resp ->
                val ok = resp.optBoolean("ok", false)
                val exitCode = if (resp.has("exit_code") && !resp.isNull("exit_code")) resp.optInt("exit_code") else null
                val stdout = resp.optNonNullString("stdout", "")
                val stderr = resp.optNonNullString("stderr", "")
                val rendered = buildString {
                    append("[$nodeId] exit=${exitCode ?: if (ok) 0 else -1}")
                    if (stdout.isNotBlank()) append("\n$stdout")
                    if (stderr.isNotBlank()) append("\nstderr: $stderr")
                }
                _state.update { cur ->
                    cur.copy(
                        cloudComputer = cur.cloudComputer.copy(lastOpOutput = rendered),
                        toastBanner = if (!ok) "远程命令执行失败：${stderr.ifBlank { "exit $exitCode" }}" else cur.toastBanner,
                    )
                }
            }.onFailure { err ->
                if (isUnauthorizedError(err)) {
                    handleUnauthorizedSession(err.message ?: "HTTP 401", capturedGen)
                } else {
                    val errLine = "[$nodeId] error: ${err.message ?: "远程执行失败"}"
                    _state.update { cur ->
                        cur.copy(
                            cloudComputer = cur.cloudComputer.copy(lastOpOutput = errLine),
                            toastBanner = "远程执行失败：${err.message}",
                        )
                    }
                }
            }
        }
    }

    fun advanceGlobalTaskFsm(taskId: String) {
        val task = _state.value.cloudComputer.globalTasks.firstOrNull { it.id == taskId } ?: return
        val states = GlobalTaskFsmState.entries
        val nextState = states[(task.state.ordinal + 1) % states.size]
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                hostRequestResult(
                    "computer.tasks.transition",
                    JSONObject().put("taskId", taskId).put("state", nextState.name.lowercase()),
                )
            }
            result.onSuccess {
                _state.update { cur ->
                    val tasks = cur.cloudComputer.globalTasks.map { t ->
                        if (t.id == taskId) t.copy(state = nextState) else t
                    }
                    cur.copy(cloudComputer = cur.cloudComputer.copy(globalTasks = tasks))
                }
            }.onFailure { err ->
                postBanner("更新任务状态失败：${err.message}")
            }
        }
    }

    fun refreshRemoteAssist() {
        if (!_state.value.isAuthenticated) return
        val capturedGen = generationFence.currentGeneration
        val capturedEpoch = assistEpoch.get()
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                hostRequestResult("telegram.server.assistInfo")
            }
            if (!generationFence.isCurrent(capturedGen)) return@launch
            result.onSuccess { resp ->
                if (assistEpoch.get() == capturedEpoch) {
                    applyAssistConfigResponse(resp)
                }
            }.onFailure { err ->
                if (isUnauthorizedError(err)) handleUnauthorizedSession(err.message ?: "HTTP 401", capturedGen)
            }
        }
    }

    fun toggleRemoteAssist(allow: Boolean) {
        if (!_state.value.isAuthenticated) {
            postBanner("请先登录服务器账号后再配置远程协助")
            return
        }
        assistEpoch.incrementAndGet()
        val capturedGen = generationFence.currentGeneration
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                hostRequestResult("telegram.server.assistAllow", JSONObject().put("allowAssist", allow))
            }
            if (!generationFence.isCurrent(capturedGen)) return@launch
            result.onSuccess { resp ->
                applyAssistConfigResponse(resp)
                postBanner(if (allow) "已开启 9 位远程协助接入" else "已关闭远程协助接入")
            }.onFailure { err ->
                if (isUnauthorizedError(err)) {
                    handleUnauthorizedSession(err.message ?: "HTTP 401", capturedGen)
                } else {
                    postBanner("更新远程协助开关失败：${err.message}")
                }
            }
        }
    }

    fun resetRemoteAssistCode() {
        if (!_state.value.isAuthenticated) {
            postBanner("请先登录服务器账号后再重置验证码")
            return
        }
        assistEpoch.incrementAndGet()
        val capturedGen = generationFence.currentGeneration
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                hostRequestResult("telegram.server.assistResetCode")
            }
            if (!generationFence.isCurrent(capturedGen)) return@launch
            result.onSuccess { resp ->
                applyAssistConfigResponse(resp)
                postBanner("远程协助临时验证码已重置")
            }.onFailure { err ->
                if (isUnauthorizedError(err)) {
                    handleUnauthorizedSession(err.message ?: "HTTP 401", capturedGen)
                } else {
                    postBanner("重置验证码失败：${err.message}")
                }
            }
        }
    }

    fun updateRemoteAssistCode(code: String) {
        val clean = code.trim()
        if (!isValidCustomAssistCode(clean)) {
            postBanner("远程协助验证码需为 8–16 位 ASCII 字母与数字组合（至少包含 1 个字母和 1 个数字）")
            return
        }
        if (!_state.value.isAuthenticated) {
            postBanner("请先登录服务器账号后再设置远程协助验证码")
            return
        }
        assistEpoch.incrementAndGet()
        val capturedGen = generationFence.currentGeneration
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                hostRequestResult("telegram.server.assistSetCode", JSONObject().put("code", clean))
            }
            if (!generationFence.isCurrent(capturedGen)) return@launch
            result.onSuccess { resp ->
                applyAssistConfigResponse(resp)
                postBanner("自定义远程协助验证码已更新")
            }.onFailure { err ->
                if (isUnauthorizedError(err)) {
                    handleUnauthorizedSession(err.message ?: "HTTP 401", capturedGen)
                } else {
                    postBanner("更新验证码失败：${err.message}")
                }
            }
        }
    }

    fun startRemoteAssist(assistId: String, accessCode: String, mode: String = "desktop") {
        val cleanId = assistId.trim()
        val cleanCode = accessCode.trim()
        if (cleanId.isBlank() || cleanCode.isBlank()) {
            postBanner("请输入 9 位协助 ID 与验证码")
            return
        }
        if (!_state.value.isAuthenticated) {
            postBanner("请先登录服务器账号后再发起远程协助")
            return
        }
        remoteSessionsEpoch.incrementAndGet()
        val capturedGen = generationFence.currentGeneration
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                hostRequestResult(
                    "telegram.server.assistStart",
                    JSONObject()
                        .put("assistId", cleanId)
                        .put("accessCode", cleanCode)
                        .put("requesterPlatform", "android")
                        .put("mode", mode),
                )
            }
            if (!generationFence.isCurrent(capturedGen)) return@launch
            result.onSuccess { resp ->
                val sessionId = resp.optNonNullString("session_id")
                val targetNodeId = resp.optNonNullString("target_node_id")
                _state.update { cur ->
                    val item = RemoteSessionItem(
                        sessionId = sessionId,
                        targetNodeId = targetNodeId,
                        controllerKind = "human_assist",
                        controllerName = "Android",
                        relayMode = mode,
                    )
                    cur.copy(
                        cloudComputer = cur.cloudComputer.copy(
                            remoteSessions = listOf(item) + cur.cloudComputer.remoteSessions.filterNot { it.sessionId == sessionId },
                            lastOpOutput = "已建立远程协助会话 $sessionId (target=$targetNodeId)",
                        ),
                        toastBanner = "已连接远程协助会话 $sessionId",
                    )
                }
            }.onFailure { err ->
                if (isUnauthorizedError(err)) {
                    handleUnauthorizedSession(err.message ?: "HTTP 401", capturedGen)
                } else {
                    postBanner("发起远程协助失败：${err.message}")
                }
            }
        }
    }

    private fun applyAssistConfigResponse(resp: JSONObject) {
        _state.update { cur ->
            cur.copy(
                cloudComputer = cur.cloudComputer.copy(
                    assistConfig = RemoteAssistConfigUi(
                        assistId = resp.optNonNullString("assist_id", cur.cloudComputer.assistConfig.assistId),
                        allowAssist = resp.optBoolean("allow_assist", cur.cloudComputer.assistConfig.allowAssist),
                        codeMode = resp.optNonNullString("code_mode", cur.cloudComputer.assistConfig.codeMode),
                        currentCode = resp.optNonNullString("current_code", cur.cloudComputer.assistConfig.currentCode),
                    ),
                ),
            )
        }
    }

    fun refreshRemoteSessions() {
        if (!_state.value.isAuthenticated) return
        val capturedGen = generationFence.currentGeneration
        val capturedEpoch = remoteSessionsEpoch.get()
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                hostRequestResult("telegram.server.listRemoteSessions")
            }
            if (!generationFence.isCurrent(capturedGen)) return@launch
            result.onSuccess { resp ->
                val parsed = parseRemoteSessions(resp)
                _state.update { cur ->
                    if (remoteSessionsEpoch.get() != capturedEpoch) cur
                    else cur.copy(cloudComputer = cur.cloudComputer.copy(remoteSessions = parsed))
                }
            }.onFailure { err ->
                if (isUnauthorizedError(err)) handleUnauthorizedSession(err.message ?: "HTTP 401", capturedGen)
            }
        }
    }

    fun disconnectRemoteSession(sessionId: String) {
        if (!_state.value.isAuthenticated || sessionId.isBlank()) return
        val capturedGen = generationFence.currentGeneration
        remoteSessionsEpoch.incrementAndGet()
        var removedSession: RemoteSessionItem? = null
        _state.update { cur ->
            removedSession = cur.cloudComputer.remoteSessions.firstOrNull { it.sessionId == sessionId }
            cur.copy(cloudComputer = cur.cloudComputer.copy(remoteSessions = cur.cloudComputer.remoteSessions.filterNot { it.sessionId == sessionId }))
        }
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                hostRequestResult("telegram.server.disconnectRemoteSession", JSONObject().put("sessionId", sessionId))
            }
            if (!generationFence.isCurrent(capturedGen)) return@launch
            result.onSuccess {
                postBanner("已断开远程会话 $sessionId")
            }.onFailure { err ->
                if (isUnauthorizedError(err)) {
                    handleUnauthorizedSession(err.message ?: "HTTP 401", capturedGen)
                } else {
                    val restore = removedSession
                    _state.update { cur ->
                        val nextSessions = if (restore != null && cur.cloudComputer.remoteSessions.none { it.sessionId == restore.sessionId }) {
                            cur.cloudComputer.remoteSessions + restore
                        } else cur.cloudComputer.remoteSessions
                        cur.copy(cloudComputer = cur.cloudComputer.copy(remoteSessions = nextSessions), toastBanner = "断开会话失败：${err.message}")
                    }
                }
            }
        }
    }

    fun disconnectAllRemoteSessions() {
        if (!_state.value.isAuthenticated) return
        val capturedGen = generationFence.currentGeneration
        remoteSessionsEpoch.incrementAndGet()
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                hostRequestResult("telegram.server.disconnectAllRemoteSessions")
            }
            if (!generationFence.isCurrent(capturedGen)) return@launch
            result.onSuccess {
                _state.update { cur ->
                    cur.copy(
                        cloudComputer = cur.cloudComputer.copy(remoteSessions = emptyList()),
                        toastBanner = "已急停并断开所有远程会话",
                    )
                }
            }.onFailure { err ->
                if (isUnauthorizedError(err)) {
                    handleUnauthorizedSession(err.message ?: "HTTP 401", capturedGen)
                } else {
                    postBanner("断开全部会话失败：${err.message}")
                }
            }
        }
    }

    fun updateRemoteStreamSettings(settings: RemoteStreamSettingsUi) {
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                hostRequestResult(
                    "remote.updateStreamSettings",
                    JSONObject()
                        .put("codec", settings.codec)
                        .put("target_fps", settings.targetFps)
                        .put("quality", settings.quality)
                        .put("color_mode", settings.colorMode)
                        .put("hdr", settings.hdr)
                        .put("audio_enabled", settings.audioEnabled)
                        .put("mic_passthrough", settings.micPassthrough)
                        .put("privacy_screen", settings.privacyScreen)
                        .put("display_index", settings.displayIndex),
                )
            }
            result.onSuccess {
                _state.update { cur ->
                    cur.copy(
                        cloudComputer = cur.cloudComputer.copy(streamSettings = settings),
                        toastBanner = "远程串流配置已同步 (${settings.codec.uppercase()} · ${settings.targetFps}fps)",
                    )
                }
            }.onFailure { err ->
                postBanner("保存串流配置失败：${err.message}")
            }
        }
    }

    fun refreshMcpGrants() {
        if (!_state.value.isAuthenticated) return
        val capturedGen = generationFence.currentGeneration
        val capturedEpoch = mcpGrantsEpoch.get()
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                hostRequestResult("telegram.server.listMcpGrants")
            }
            if (!generationFence.isCurrent(capturedGen)) return@launch
            result.onSuccess { resp ->
                val parsed = parseMcpGrants(resp)
                _state.update { cur ->
                    if (mcpGrantsEpoch.get() != capturedEpoch) cur
                    else cur.copy(cloudComputer = cur.cloudComputer.copy(mcpGrants = parsed))
                }
            }.onFailure { err ->
                if (isUnauthorizedError(err)) handleUnauthorizedSession(err.message ?: "HTTP 401", capturedGen)
            }
        }
    }

    fun issueRemoteMcpGrant(clientName: String) {
        // R-F5 / K-F9: Never mint fake local fam_<uuid> OAuth grants! Fail closed via Rust host.
        val clean = clientName.trim()
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                hostRequestResult("remote.issueMcpGrant", JSONObject().put("clientName", clean))
            }
            result.onFailure { err ->
                postBanner("OAuth 2.1 MCP 授权需通过标准 OAuth 流程完成：${err.message ?: "OAUTH_FLOW_REQUIRED"}")
            }
        }
    }

    fun revokeRemoteMcpGrant(familyId: String) {
        if (!_state.value.isAuthenticated || familyId.isBlank()) {
            postBanner("请先登录服务器账号后再撤销 MCP 授权")
            return
        }
        val capturedGen = generationFence.currentGeneration
        mcpGrantsEpoch.incrementAndGet()
        var removedGrant: RemoteMcpGrantItem? = null
        _state.update { cur ->
            removedGrant = cur.cloudComputer.mcpGrants.firstOrNull { it.familyId == familyId }
            cur.copy(cloudComputer = cur.cloudComputer.copy(mcpGrants = cur.cloudComputer.mcpGrants.filterNot { it.familyId == familyId }))
        }
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                hostRequestResult("telegram.server.revokeMcpGrant", JSONObject().put("familyId", familyId))
            }
            if (!generationFence.isCurrent(capturedGen)) return@launch
            result.onSuccess {
                postBanner("已撤销 Remote MCP 授权 ($familyId)")
            }.onFailure { err ->
                if (isUnauthorizedError(err)) {
                    handleUnauthorizedSession(err.message ?: "HTTP 401", capturedGen)
                } else {
                    val restore = removedGrant
                    _state.update { cur ->
                        val nextGrants = if (restore != null && cur.cloudComputer.mcpGrants.none { it.familyId == restore.familyId }) {
                            cur.cloudComputer.mcpGrants + restore
                        } else cur.cloudComputer.mcpGrants
                        cur.copy(cloudComputer = cur.cloudComputer.copy(mcpGrants = nextGrants), toastBanner = "撤销 MCP 授权失败：${err.message}")
                    }
                }
            }
        }
    }

    // ------------------------------------------------------------------------
    // User Vault & MiniApp PiP
    // ------------------------------------------------------------------------

    fun initSplitKeyVault(password: String) {
        if (password.length < 6) {
            postBanner("零知识保险库主口令至少需要 6 位")
            return
        }
        val uid = _state.value.activeAccountId.ifBlank { "local" }
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                hostRequestResult(
                    "userVault.initEnvelope",
                    JSONObject().put("userId", uid).put("password", password),
                )
            }
            result.onSuccess {
                _state.update { cur ->
                    cur.copy(
                        userVault = cur.userVault.copy(
                            initialized = true,
                            kdfAlg = "split_key_v2",
                            keyVersion = 2,
                            lastVaultMessage = "Split-Key V2 信封已在 Rust 原生端派生并封装 (Argon2id + ChaCha20-Poly1305)",
                        ),
                        toastBanner = "Split-Key V2 零知识保险库初始化成功",
                    )
                }
            }.onFailure { err ->
                postBanner("初始化保险库失败：${err.message}")
            }
        }
    }

    fun requestDeviceVaultSync() {
        val uid = _state.value.activeAccountId.ifBlank { "local" }
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                hostRequestResult(
                    "userVault.requestDeviceSync",
                    JSONObject().put("userId", uid).put("deviceName", "Fabushi Android"),
                )
            }
            result.onSuccess { res ->
                val reqId = res.optNonNullString("requestId")
                val code = res.optNonNullString("verificationCode")
                _state.update { cur ->
                    cur.copy(
                        userVault = cur.userVault.copy(
                            activeSyncRequestId = reqId,
                            activeSyncVerificationCode = code,
                            activeSyncStatus = "pending · 等待受信任桌面端批准",
                        ),
                        toastBanner = "已生成 X25519 设备信任环核验码：$code",
                    )
                }
            }.onFailure { err ->
                postBanner("发起设备信任环同步失败：${err.message}")
            }
        }
    }

    fun minimizeGlobalDharmaToPip() {
        val summary = "累计供养 ${_state.value.globalDharma.totalGlobalOfferings} 盏 · 功德 ${_state.value.globalDharma.myMeritPoints}"
        viewModelScope.launch(Dispatchers.IO) {
            hostRequestResult(
                "miniapp.webmcp.setControlMode",
                JSONObject()
                    .put("botUsername", "fabushi_bot")
                    .put("shortName", "global_dharma")
                    .put("appTitle", "全球法布施")
                    .put("mode", "user_takeover")
                    .put("statusText", summary)
                    .put("pipMinimized", true),
            )
        }
        _state.update { cur ->
            cur.copy(
                subScreen = TelegramSubScreen.HOME,
                miniAppPip = MiniAppPipSessionUi(
                    active = true,
                    minimized = true,
                    botUsername = "fabushi_bot",
                    shortName = "global_dharma",
                    appTitle = "全球法布施",
                    controlMode = "user_takeover",
                    statusText = summary,
                ),
                toastBanner = "全球法布施已最小化至悬浮 PiP",
            )
        }
    }

    fun restoreMiniAppFromPip() {
        viewModelScope.launch(Dispatchers.IO) {
            hostRequestResult(
                "miniapp.webmcp.setControlMode",
                JSONObject()
                    .put("botUsername", _state.value.miniAppPip.botUsername)
                    .put("shortName", _state.value.miniAppPip.shortName)
                    .put("mode", "user_takeover")
                    .put("pipMinimized", false),
            )
        }
        _state.update { cur ->
            cur.copy(
                subScreen = TelegramSubScreen.GLOBAL_DHARMA,
                miniAppPip = cur.miniAppPip.copy(minimized = false),
            )
        }
    }

    fun dismissMiniAppPip() {
        _state.update { cur -> cur.copy(miniAppPip = MiniAppPipSessionUi()) }
    }

    // ------------------------------------------------------------------------
    // Real gpui_telegram_server Auth, Contacts, Paginated History & WebSocket Engine
    // ------------------------------------------------------------------------

    fun updateDesktopServerEndpoint(url: String) {
        val clean = url.trim().trimEnd('/').ifBlank { DEFAULT_DESKTOP_SERVER_URL }
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                hostRequestResult("telegram.server.configureEndpoint", JSONObject().put("endpoint", clean))
            }
            result.onSuccess {
                prefs.edit().putString("server_endpoint", clean).apply()
                _state.update { it.copy(serverEndpoint = clean, authError = null, toastBanner = "服务器地址已更新：$clean") }
            }.onFailure { err ->
                _state.update { it.copy(authError = err.message ?: "无效服务器地址") }
            }
        }
    }

    fun toggleAuthThemeMode() {
        updateThemeSettings { cur ->
            val nextPreset = if (cur.isDarkEffective) TelegramThemePreset.APPLE_DAY else TelegramThemePreset.APPLE_NIGHT
            cur.copy(preset = nextPreset)
        }
    }

    fun toggleAuthLanguage() {
        val nextLang = if (isEnglishLanguage(_state.value.uiLanguage)) "zh-CN" else "en-US"
        updateLanguageAndTranslate(
            language = nextLang,
            showTranslateButton = _state.value.showTranslateButton,
            autoTranslate = _state.value.autoTranslateChannels,
        )
    }

    private fun formatFriendlyAuthError(raw: String?, fallbackZh: String, fallbackEn: String): String {
        val english = isEnglishLanguage(_state.value.uiLanguage)
        val msg = raw?.trim().orEmpty()
        if (msg.isBlank()) return if (english) fallbackEn else fallbackZh
        return when {
            msg.contains("HTTP 401") || msg.contains("invalid_credentials", ignoreCase = true) ->
                if (english) "Invalid username or password (401)" else "用户名或密码不正确，请检查后重试"
            msg.contains("HTTP 409") || msg.contains("user_already_exists", ignoreCase = true) ->
                if (english) "This username or email is already registered (409)" else "该用户名或邮箱已被注册，请直接登录"
            msg.contains("SERVER_UNREACHABLE") ->
                if (english) "Cannot reach server. Please check network or server URL." else "无法连接服务器，请检查网络或服务器地址"
            else -> msg
        }
    }

    fun loginWithPassword(username: String, password: String) {
        // H1: Advance generationFence BEFORE writing new credentials so stale in-flight 401s cannot wipe the new token
        generationFence.advance()
        alipayAutoPollJob?.cancel()
        val ep = _state.value.serverEndpoint
        _state.update {
            it.copy(
                isAuthenticating = true,
                authError = null,
                pendingTwoFactorChallengeToken = null,
                pendingTwoFactorHint = null,
            )
        }
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                hostRequestResult(
                    "telegram.server.login",
                    JSONObject()
                        .put("endpoint", ep)
                        .put("username", username)
                        .put("password", password)
                        .put("deviceName", "Fabushi Android (${Build.MODEL ?: "Android"})"),
                )
            }
            result.onSuccess { resp ->
                if (resp.optBoolean("requires_2fa", false)) {
                    val chal = resp.optNonNullString("challenge_token")
                    val hint = resp.optNullableString("hint")
                    _state.update {
                        it.copy(
                            isAuthenticating = false,
                            isAuthenticated = false,
                            pendingTwoFactorChallengeToken = chal.ifBlank { null },
                            pendingTwoFactorHint = hint,
                            authError = null,
                        )
                    }
                    return@onSuccess
                }
                generationFence.advance()
                clearInFlightGuards()
                chatPtsMap.clear()
                prefs.edit().putBoolean("offline_local_mode", false).apply()
                _state.update {
                    it.copy(
                        isAuthenticating = false,
                        isAuthenticated = true,
                        offlineLocalMode = false,
                        pendingTwoFactorChallengeToken = null,
                        pendingTwoFactorHint = null,
                        authError = null,
                        toastBanner = "已登录 ${resp.optNonNullString("username", username)}",
                    )
                }
                refreshAllFromHost()
            }.onFailure { err ->
                _state.update {
                    it.copy(
                        isAuthenticating = false,
                        isAuthenticated = false,
                        authError = formatFriendlyAuthError(
                            err.message,
                            "登录失败，请检查服务器地址与账号密码",
                            "Sign in failed. Please check server URL and credentials.",
                        ),
                    )
                }
            }
        }
    }

    fun submitTwoFactorCloudPassword(cloudPassword: String) {
        val challengeToken = _state.value.pendingTwoFactorChallengeToken?.takeIf { it.isNotBlank() } ?: return
        if (cloudPassword.isBlank()) return
        generationFence.advance()
        val ep = _state.value.serverEndpoint
        _state.update { it.copy(isAuthenticating = true, authError = null) }
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                hostRequestResult(
                    "telegram.server.verifyTwoFactor",
                    JSONObject()
                        .put("endpoint", ep)
                        .put("challengeToken", challengeToken)
                        .put("cloudPassword", cloudPassword)
                        .put("deviceName", "Fabushi Android (${Build.MODEL ?: "Android"})"),
                )
            }
            result.onSuccess { resp ->
                generationFence.advance()
                clearInFlightGuards()
                chatPtsMap.clear()
                prefs.edit().putBoolean("offline_local_mode", false).apply()
                _state.update {
                    it.copy(
                        isAuthenticating = false,
                        isAuthenticated = true,
                        offlineLocalMode = false,
                        pendingTwoFactorChallengeToken = null,
                        pendingTwoFactorHint = null,
                        authError = null,
                        toastBanner = "两步验证通过，已登录 ${resp.optNonNullString("username")}",
                    )
                }
                refreshAllFromHost()
            }.onFailure { err ->
                _state.update {
                    it.copy(
                        isAuthenticating = false,
                        isAuthenticated = false,
                        authError = formatFriendlyAuthError(
                            err.message,
                            "两步验证密码不正确，请重试",
                            "Incorrect two-factor cloud password.",
                        ),
                    )
                }
            }
        }
    }

    fun cancelTwoFactorChallenge() {
        _state.update {
            it.copy(
                pendingTwoFactorChallengeToken = null,
                pendingTwoFactorHint = null,
                authError = null,
                isAuthenticating = false,
            )
        }
    }

    fun registerWithPassword(username: String, password: String, firstName: String) {
        generationFence.advance()
        alipayAutoPollJob?.cancel()
        val ep = _state.value.serverEndpoint
        _state.update {
            it.copy(
                isAuthenticating = true,
                authError = null,
                pendingTwoFactorChallengeToken = null,
                pendingTwoFactorHint = null,
            )
        }
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                hostRequestResult(
                    "telegram.server.register",
                    JSONObject()
                        .put("endpoint", ep)
                        .put("username", username)
                        .put("password", password)
                        .put("firstName", firstName.ifBlank { username })
                        .put("deviceName", "Fabushi Android (${Build.MODEL ?: "Android"})"),
                )
            }
            result.onSuccess {
                generationFence.advance()
                clearInFlightGuards()
                chatPtsMap.clear()
                prefs.edit().putBoolean("offline_local_mode", false).apply()
                _state.update {
                    it.copy(
                        isAuthenticating = false,
                        isAuthenticated = true,
                        offlineLocalMode = false,
                        authError = null,
                        toastBanner = "注册并登录成功",
                    )
                }
                refreshAllFromHost()
            }.onFailure { err ->
                _state.update {
                    it.copy(
                        isAuthenticating = false,
                        isAuthenticated = false,
                        authError = formatFriendlyAuthError(
                            err.message,
                            "注册失败，请检查服务器响应",
                            "Registration failed.",
                        ),
                    )
                }
            }
        }
    }

    fun sendEmailVerificationCode(email: String) {
        val ep = _state.value.serverEndpoint
        _state.update { it.copy(isAuthenticating = true, authError = null, emailCodeSentNotice = null) }
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                hostRequestResult(
                    "telegram.server.sendEmailCode",
                    JSONObject().put("endpoint", ep).put("email", email),
                )
            }
            result.onSuccess { resp ->
                val debugCode = resp.optNullableString("debug_code")
                val notice = if (debugCode != null) "验证码已发送（调试码：$debugCode）" else "验证码已发送至 $email"
                _state.update { it.copy(isAuthenticating = false, emailCodeSentNotice = notice) }
            }.onFailure { err ->
                _state.update { it.copy(isAuthenticating = false, authError = err.message ?: "发送验证码失败") }
            }
        }
    }

    fun registerWithEmail(email: String, code: String, username: String, password: String, firstName: String) {
        generationFence.advance()
        alipayAutoPollJob?.cancel()
        val ep = _state.value.serverEndpoint
        _state.update { it.copy(isAuthenticating = true, authError = null) }
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                hostRequestResult(
                    "telegram.server.registerEmail",
                    JSONObject()
                        .put("endpoint", ep)
                        .put("email", email)
                        .put("code", code)
                        .put("username", username)
                        .put("password", password)
                        .put("firstName", firstName.ifBlank { username })
                        .put("deviceName", "Fabushi Android (${Build.MODEL ?: "Android"})"),
                )
            }
            result.onSuccess {
                generationFence.advance()
                clearInFlightGuards()
                chatPtsMap.clear()
                prefs.edit().putBoolean("offline_local_mode", false).apply()
                _state.update {
                    it.copy(
                        isAuthenticating = false,
                        isAuthenticated = true,
                        offlineLocalMode = false,
                        authError = null,
                        toastBanner = "邮箱注册并登录成功",
                    )
                }
                refreshAllFromHost()
            }.onFailure { err ->
                _state.update {
                    it.copy(
                        isAuthenticating = false,
                        authError = formatFriendlyAuthError(err.message, "邮箱注册失败", "Email registration failed."),
                    )
                }
            }
        }
    }

    fun startAlipayLogin() {
        alipayAutoPollJob?.cancel()
        val ep = _state.value.serverEndpoint
        _state.update { it.copy(isAuthenticating = true, authError = null, alipayStatusText = "正在向服务器请求支付宝授权…") }
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                hostRequestResult("telegram.server.alipayStart", JSONObject().put("endpoint", ep))
            }
            result.onSuccess { resp ->
                val attemptId = resp.optNonNullString("attempt_id")
                val loginUrl = resp.optNonNullString("login_url")
                    .ifBlank { resp.optNonNullString("auth_url") }
                    .ifBlank { resp.optNonNullString("qr_payload") }
                _state.update {
                    it.copy(
                        isAuthenticating = false,
                        alipayAttemptId = attemptId,
                        alipayLoginUrl = loginUrl,
                        alipayStatusText = "请使用支付宝扫码或点击链接授权（自动检测状态中…）",
                    )
                }
                if (attemptId.isNotBlank()) {
                    startAlipayAutoPolling(attemptId)
                }
            }.onFailure { err ->
                _state.update {
                    it.copy(
                        isAuthenticating = false,
                        authError = formatFriendlyAuthError(err.message, "无法发起支付宝登录", "Unable to start Alipay login."),
                    )
                }
            }
        }
    }

    private fun startAlipayAutoPolling(attemptId: String) {
        alipayAutoPollJob?.cancel()
        alipayAutoPollJob = viewModelScope.launch {
            while (isActive) {
                delay(2500L)
                val cur = _state.value
                if (cur.isAuthenticated || cur.alipayAttemptId != attemptId) break
                pollAlipayLogin(silent = true)
            }
        }
    }

    fun pollAlipayLogin(silent: Boolean = false) {
        val attemptId = _state.value.alipayAttemptId ?: return
        if (!silent) {
            generationFence.advance()
        }
        val ep = _state.value.serverEndpoint
        if (!silent) {
            _state.update { it.copy(isAuthenticating = true, authError = null) }
        }
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                hostRequestResult(
                    "telegram.server.alipayPoll",
                    JSONObject().put("endpoint", ep).put("attemptId", attemptId),
                )
            }
            result.onSuccess { resp ->
                val status = resp.optNonNullString("status")
                when (status.lowercase()) {
                    "complete", "completed" -> {
                        alipayAutoPollJob?.cancel()
                        generationFence.advance()
                        clearInFlightGuards()
                        chatPtsMap.clear()
                        prefs.edit().putBoolean("offline_local_mode", false).apply()
                        _state.update {
                            it.copy(
                                isAuthenticating = false,
                                isAuthenticated = true,
                                offlineLocalMode = false,
                                alipayAttemptId = null,
                                alipayStatusText = "支付宝登录成功",
                            )
                        }
                        refreshAllFromHost()
                    }
                    "expired", "cancelled", "failed" -> {
                        alipayAutoPollJob?.cancel()
                        _state.update {
                            it.copy(
                                isAuthenticating = false,
                                alipayAttemptId = null,
                                alipayStatusText = "授权已失效 ($status)，请重新发起",
                                authError = "支付宝授权已过期 ($status)",
                            )
                        }
                    }
                    else -> {
                        _state.update {
                            it.copy(
                                isAuthenticating = false,
                                alipayStatusText = "等待支付宝确认中 ($status)…",
                            )
                        }
                    }
                }
            }.onFailure { err ->
                alipayAutoPollJob?.cancel()
                _state.update {
                    it.copy(
                        isAuthenticating = false,
                        alipayAttemptId = null,
                        authError = err.message ?: "轮询支付宝状态失败",
                    )
                }
            }
        }
    }

    fun enterOfflineLocalMode() {
        alipayAutoPollJob?.cancel()
        prefs.edit().putBoolean("offline_local_mode", true).apply()
        _state.update {
            it.copy(
                offlineLocalMode = true,
                authError = null,
                pendingTwoFactorChallengeToken = null,
                pendingTwoFactorHint = null,
                syncStatusLabel = "信息（本地）",
            )
        }
    }

    fun signOutToAuthWizard() {
        generationFence.advance()
        alipayAutoPollJob?.cancel()
        disconnectServerWebSocket()
        clearInFlightGuards()
        chatPtsMap.clear()
        currentServerUserId = null
        prefs.edit().putBoolean("offline_local_mode", false).apply()
        // Snapshot old token for best-effort async server revocation BEFORE synchronous local SQLite clear (<1ms)
        val oldSession = hostRequestResult("telegram.server.session").getOrNull()
        val oldEndpoint = oldSession?.optNonNullString("endpoint")?.ifBlank { null } ?: _state.value.serverEndpoint
        val oldToken = oldSession?.optNonNullString("token")?.ifBlank { null }
        val oldSessionId = oldSession?.optNullableString("sessionId")
        hostRequestResult("telegram.server.logout")
        if (oldToken != null) {
            viewModelScope.launch(Dispatchers.IO) {
                val payload = JSONObject()
                    .put("endpoint", oldEndpoint)
                    .put("token", oldToken)
                oldSessionId?.let { payload.put("sessionId", it) }
                hostRequestResult("telegram.server.remoteLogout", payload)
            }
        }
        _state.update {
            buildCleanInitialState().copy(
                serverEndpoint = it.serverEndpoint,
                themeSettings = it.themeSettings,
                uiLanguage = it.uiLanguage,
                showTranslateButton = it.showTranslateButton,
                autoTranslateChannels = it.autoTranslateChannels,
                powerSavingThreshold = it.powerSavingThreshold,
                inAppSoundsEnabled = it.inAppSoundsEnabled,
                passcodeConfigured = it.passcodeConfigured,
                biometricUnlockEnabled = it.biometricUnlockEnabled,
                storageCacheMb = it.storageCacheMb,
                mikaSnapshot = it.mikaSnapshot,
                toastBanner = "已退出当前账号",
            )
        }
    }

    fun refreshContacts() {
        if (!_state.value.isAuthenticated) return
        val capturedGen = generationFence.currentGeneration
        val capturedEpoch = contactsEpoch.get()
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                hostRequestResult("telegram.server.listContacts")
            }
            if (!generationFence.isCurrent(capturedGen)) return@launch
            result.onSuccess { resp ->
                val parsed = parseServerContacts(resp)
                _state.update { cur ->
                    if (contactsEpoch.get() != capturedEpoch) cur else cur.copy(serverContacts = parsed)
                }
            }.onFailure { err ->
                if (isUnauthorizedError(err)) handleUnauthorizedSession(err.message ?: "HTTP 401", capturedGen)
            }
        }
    }

    fun addContactByUsername(username: String, kind: String = "user") {
        val clean = username.trim().trimStart('@')
        if (clean.isEmpty()) return
        if (!_state.value.isAuthenticated) {
            postBanner("请先登录服务器账号后再添加联系人")
            return
        }
        contactsEpoch.incrementAndGet()
        chatsEpoch.incrementAndGet()
        val capturedGen = generationFence.currentGeneration
        viewModelScope.launch {
            val payload = JSONObject()
                .put("username", clean)
                .put("firstName", clean)
                .put("kind", kind)
            if (kind.equals("bot", ignoreCase = true)) {
                payload.put("isBot", true)
            }
            val result = withContext(Dispatchers.IO) {
                hostRequestResult("telegram.server.addContact", payload)
            }
            if (!generationFence.isCurrent(capturedGen)) return@launch
            result.onSuccess { resp ->
                val chatObj = resp.optJSONObject("chat")
                val chatId = resp.optLong("chat_id").takeIf { it > 0L }?.toString()
                    ?: chatObj?.optLong("id")?.takeIf { it > 0L }?.toString()
                val resolvedTitle = chatObj?.optNonNullString("title")?.ifBlank { null }
                    ?: resp.optNonNullString("first_name").ifBlank { null }
                    ?: clean
                val isBotPeer = kind.equals("bot", ignoreCase = true) ||
                    chatObj?.optBoolean("is_bot", false) == true ||
                    resp.optBoolean("is_bot", false)
                val peerUid = chatObj?.optLong("peer_user_id")?.takeIf { it > 0L }
                    ?: resp.optLong("user_id").takeIf { it > 0L }
                if (chatId != null) {
                    openChat(
                        chatId = chatId,
                        provisionalTitle = resolvedTitle,
                        provisionalUsername = clean,
                        provisionalIsBot = isBotPeer,
                        provisionalPeerUserId = peerUid,
                    )
                } else {
                    postBanner("已添加 @$clean")
                }
                refreshAllFromHost()
            }.onFailure { err ->
                if (isUnauthorizedError(err)) {
                    handleUnauthorizedSession(err.message ?: "HTTP 401", capturedGen)
                } else {
                    // E2E-T2-B6: 404 rolls back completely with zero ghost contact
                    postBanner("添加联系人失败：${err.message ?: "未找到用户 @$clean"}")
                }
            }
        }
    }

    fun deleteContact(userId: String) {
        if (!_state.value.isAuthenticated || userId.isBlank()) return
        val capturedGen = generationFence.currentGeneration
        contactsEpoch.incrementAndGet()
        var removedContact: ServerContactItem? = null
        _state.update { cur ->
            removedContact = cur.serverContacts.firstOrNull { c -> c.userId.toString() == userId }
            cur.copy(serverContacts = cur.serverContacts.filterNot { c -> c.userId.toString() == userId })
        }
        viewModelScope.launch {
            val result = withContext(Dispatchers.IO) {
                hostRequestResult("telegram.server.deleteContact", JSONObject().put("userId", userId))
            }
            if (!generationFence.isCurrent(capturedGen)) return@launch
            result.onFailure { err ->
                if (isUnauthorizedError(err)) {
                    handleUnauthorizedSession(err.message ?: "HTTP 401", capturedGen)
                } else {
                    val restore = removedContact
                    _state.update { cur ->
                        val nextContacts = if (restore != null && cur.serverContacts.none { it.userId == restore.userId }) {
                            cur.serverContacts + restore
                        } else cur.serverContacts
                        cur.copy(serverContacts = nextContacts, toastBanner = "删除联系人失败：${err.message}")
                    }
                }
            }
        }
    }

    fun loadOlderMessages(chatId: String, limit: Int = 50) {
        val numericId = chatId.toLongOrNull() ?: return
        if (!_state.value.isAuthenticated) return
        val chat = _state.value.chats.firstOrNull { it.id == chatId }
        if (chat != null && !chat.hasMoreHistory) return
        val oldestId = _state.value.messagesByChat[chatId]
            ?.mapNotNull { it.id.toLongOrNull() }
            ?.minOrNull()
            ?: return
        fetchServerMessagesForChat(chatId, numericId, beforeId = oldestId, limit = limit)
    }

    private fun fetchServerMessagesForChat(
        chatId: String,
        numericId: Long,
        beforeId: Long? = null,
        limit: Int = 50,
        markReadAfterLoad: Boolean = false,
    ) {
        val capturedGen = generationFence.currentGeneration
        val capturedMsgsEpoch = chatMessagesEpochFor(chatId).get()
        viewModelScope.launch {
            val params = JSONObject().put("chatId", numericId).put("limit", limit)
            if (beforeId != null) params.put("beforeId", beforeId)
            val result = withContext(Dispatchers.IO) {
                hostRequestResult("telegram.server.getMessages", params)
            }
            if (!generationFence.isCurrent(capturedGen)) return@launch
            val resp = result.getOrElse { err ->
                if (isUnauthorizedError(err)) {
                    handleUnauthorizedSession(err.message ?: "HTTP 401", capturedGen)
                }
                return@launch
            }
            val arr = resp.optJSONArray("messages") ?: return@launch
            val myUid = currentServerUserId
            val chatSnapshot = _state.value.chats.firstOrNull { it.id == chatId }
            val readOutboxMaxId = chatSnapshot?.readOutboxMaxId ?: 0L
            val inFlightPins = inFlightMessagePins[chatId]?.second
            val pinnedIds = inFlightPins ?: chatSnapshot?.pinnedMessageIds?.toSet().orEmpty()
            val existingById = _state.value.messagesByChat[chatId].orEmpty().associateBy { it.id }

            val parsed = buildList {
                for (i in 0 until arr.length()) {
                    val m = arr.optJSONObject(i) ?: continue
                    val item = parseServerMessageJson(m, chatId, myUid, readOutboxMaxId) ?: continue
                    val reactionKey = "$chatId:${item.id}"
                    val protectedReactions = if (inFlightReactions.containsKey(reactionKey)) {
                        existingById[item.id]?.reactions ?: item.reactions
                    } else item.reactions
                    val resolvedPinned = if (inFlightPins != null) {
                        item.id in inFlightPins
                    } else {
                        item.pinned || item.id in pinnedIds
                    }
                    add(
                        item.copy(
                            pinned = resolvedPinned,
                            reactions = protectedReactions,
                        ),
                    )
                }
            }

            val pts = resp.optLong("current_pts").takeIf { it > 0L } ?: resp.optLong("pts", 0L)
            if (pts > 0L) chatPtsMap[numericId] = maxOf(chatPtsMap[numericId] ?: 0L, pts)
            val hasMore = resp.optBoolean("has_more", false) && parsed.isNotEmpty()

            _state.update { cur ->
                if (chatMessagesEpochFor(chatId).get() != capturedMsgsEpoch) {
                    return@update cur
                }
                val existing = cur.messagesByChat[chatId].orEmpty()
                val merged = if (beforeId != null) {
                    (parsed + existing).distinctBy { it.id }.sortedWith(
                        compareBy<TelegramMessageItem> { it.timestampMs }.thenBy { it.id.toLongOrNull() ?: Long.MAX_VALUE },
                    )
                } else {
                    val pendingLocal = existing.filter { it.deliveryState == "sending" || it.deliveryState == "failed" }
                    val serverNonces = parsed.mapNotNull { it.clientNonce }.toSet()
                    val keptPending = pendingLocal.filterNot { it.clientNonce != null && it.clientNonce in serverNonces }
                    (parsed + keptPending).distinctBy { it.id }.sortedWith(
                        compareBy<TelegramMessageItem> { it.timestampMs }.thenBy { it.id.toLongOrNull() ?: Long.MAX_VALUE },
                    )
                }
                val updatedChats = cur.chats.map { c ->
                    if (c.id == chatId) c.copy(hasMoreHistory = hasMore) else c
                }
                cur.copy(
                    messagesByChat = cur.messagesByChat + (chatId to merged),
                    chats = updatedChats,
                )
            }

            if (markReadAfterLoad) {
                val maxMsgId = parsed.mapNotNull { it.id.toLongOrNull() }.maxOrNull()
                if (maxMsgId != null && maxMsgId > 0L) {
                    markChatRead(chatId, maxMsgId)
                }
            }
        }
    }

    // ------------------------------------------------------------------------
    // WebSocket Engine (/api/v1/ws): URL-Encoded Token, Exponential Backoff,
    // 401 Stop-Loop, PTS Gap Recovery & All 27 UpdateEvent Variants
    // ------------------------------------------------------------------------

    private fun connectServerWebSocket(endpoint: String, token: String, capturedGen: Long) {
        disconnectServerWebSocket(clearIntent = false)
        wsEndpoint = endpoint
        wsToken = token
        wsShouldReconnect = true
        wsReconnectAttempt = 0
        openWebSocketInternal(endpoint, token, capturedGen, isReconnect = false)
    }

    private fun openWebSocketInternal(endpoint: String, token: String, capturedGen: Long, isReconnect: Boolean) {
        if (!generationFence.isCurrent(capturedGen) || !wsShouldReconnect) return
        val clean = endpoint.trim().trimEnd('/')
        val wsBase = when {
            clean.startsWith("https://") -> "wss://" + clean.removePrefix("https://")
            clean.startsWith("http://") -> "ws://" + clean.removePrefix("http://")
            clean.startsWith("ws://") || clean.startsWith("wss://") -> clean
            else -> "ws://$clean"
        }
        val encodedToken = URLEncoder.encode(token, "UTF-8").replace("+", "%20")
        val req = Request.Builder()
            .url("$wsBase/api/v1/ws?token=$encodedToken")
            .header("Authorization", "Bearer $token")
            .build()

        val listener = object : WebSocketListener() {
            override fun onOpen(webSocket: WebSocket, response: Response) {
                val active = synchronized(wsLifecycleLock) {
                    serverWebSocket === webSocket && generationFence.isCurrent(capturedGen)
                }
                if (!active) {
                    runCatching { webSocket.close(1000, "superseded") }
                    return
                }
                val wasReconnect = isReconnect || wsReconnectAttempt > 0
                wsReconnectAttempt = 0
                _state.update { it.copy(serverWsConnected = true, syncStatusLabel = "信息") }
                if (wasReconnect) {
                    syncPtsGapRecovery(capturedGen)
                }
            }

            override fun onMessage(webSocket: WebSocket, text: String) {
                val active = synchronized(wsLifecycleLock) {
                    serverWebSocket === webSocket && generationFence.isCurrent(capturedGen)
                }
                if (!active) return
                runCatching {
                    val obj = JSONObject(text)
                    handleWsServerFrame(obj, capturedGen)
                }
            }

            override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
                val wasCurrent = synchronized(wsLifecycleLock) {
                    if (serverWebSocket !== webSocket) {
                        false
                    } else {
                        serverWebSocket = null
                        true
                    }
                }
                if (!wasCurrent) return
                if (!generationFence.isCurrent(capturedGen)) return
                _state.update { it.copy(serverWsConnected = false) }
                if (code != 1000 && wsShouldReconnect) {
                    scheduleWsReconnect(capturedGen)
                }
            }

            override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
                val wasCurrent = synchronized(wsLifecycleLock) {
                    if (serverWebSocket !== webSocket) {
                        false
                    } else {
                        serverWebSocket = null
                        true
                    }
                }
                if (!wasCurrent) return
                if (!generationFence.isCurrent(capturedGen)) return
                _state.update { it.copy(serverWsConnected = false) }
                if (response?.code == 401) {
                    wsShouldReconnect = false
                    handleUnauthorizedSession("HTTP 401: WebSocket unauthorized", capturedGen)
                    return
                }
                if (wsShouldReconnect) {
                    scheduleWsReconnect(capturedGen)
                }
            }
        }

        val staleSocket: WebSocket?
        synchronized(wsLifecycleLock) {
            if (!generationFence.isCurrent(capturedGen) || !wsShouldReconnect) return
            staleSocket = serverWebSocket
            wsConnectCounter.incrementAndGet()
            serverWebSocket = wsHttpClient.newWebSocket(req, listener)
        }
        runCatching { staleSocket?.close(1000, "superseded") }
    }

    private fun scheduleWsReconnect(capturedGen: Long) {
        if (!wsShouldReconnect || !generationFence.isCurrent(capturedGen)) return
        wsReconnectJob?.cancel()
        val attempt = wsReconnectAttempt
        wsReconnectAttempt = (attempt + 1).coerceAtMost(10)
        val base = minOf(30_000L, wsBaseBackoffMs * (1L shl minOf(attempt, 5)))
        val jittered = (base * (0.75 + Random.nextDouble() * 0.5)).toLong()
        val minBound = minOf(500L, wsBaseBackoffMs)
        val delayMs = jittered.coerceIn(minBound, 30_000L)
        wsReconnectJob = viewModelScope.launch {
            delay(delayMs)
            if (!wsShouldReconnect || !generationFence.isCurrent(capturedGen)) return@launch
            val ep = wsEndpoint ?: return@launch
            val tok = wsToken ?: return@launch
            openWebSocketInternal(ep, tok, capturedGen, isReconnect = true)
        }
    }

    private fun syncPtsGapRecovery(capturedGen: Long) {
        viewModelScope.launch {
            val chatPtsJson = JSONObject()
            chatPtsMap.forEach { (cid, pts) -> chatPtsJson.put(cid.toString(), pts) }
            val params = JSONObject().put("liveNetwork", true)
            if (chatPtsJson.length() > 0) params.put("chatPts", chatPtsJson)
            val result = withContext(Dispatchers.IO) {
                hostRequestResult("telegram.server.syncDifference", params)
            }
            if (!generationFence.isCurrent(capturedGen)) return@launch
            result.onSuccess { resp ->
                val updates = resp.optJSONArray("updates")
                if (updates != null) {
                    for (i in 0 until updates.length()) {
                        val u = updates.optJSONObject(i) ?: continue
                        val fallbackChat = u.optLong("chat_id").takeIf { it > 0L }
                        val ev = u.optJSONObject("event") ?: u
                        handleUpdateEvent(ev, fallbackChat, capturedGen)
                    }
                }
                val msgs = resp.optJSONArray("messages")
                if (msgs != null) {
                    for (i in 0 until msgs.length()) {
                        val m = msgs.optJSONObject(i) ?: continue
                        val cid = m.optLong("chat_id").takeIf { it > 0L }
                        if (cid != null) applyIncomingNewMessage(m, cid)
                    }
                }
                if (resp.optBoolean("snapshot_required", false)) {
                    refreshAllFromHost()
                }
            }.onFailure { err ->
                if (isUnauthorizedError(err)) handleUnauthorizedSession(err.message ?: "HTTP 401", capturedGen)
            }
        }
    }

    internal fun handleWsServerFrame(frame: JSONObject, capturedGen: Long) {
        if (!generationFence.isCurrent(capturedGen)) return
        val frameType = frame.optNonNullString("type")
        if (frameType.isBlank()) {
            // Legacy flat frame fallback
            val cid = frame.optLong("chat_id").takeIf { it > 0L }
                ?: frame.optJSONObject("message")?.optLong("chat_id")?.takeIf { it > 0L }
            if (cid != null) {
                applyIncomingNewMessage(frame.optJSONObject("message") ?: frame, cid)
            }
            return
        }
        val payload = frame.optJSONObject("payload")
        when (frameType) {
            "Pong" -> Unit
            "Error" -> {
                val code = payload?.optInt("code", 0) ?: 0
                val msg = payload?.optNonNullString("message").orEmpty()
                if (code == 401) {
                    wsShouldReconnect = false
                    handleUnauthorizedSession("WS Error 401: $msg", capturedGen)
                }
            }
            "Ack" -> {
                val msgId = payload?.optLong("message_id")?.takeIf { it > 0L }?.toString()
                val nonce = payload?.optNullableString("client_nonce")
                val ackChatId = payload?.optLong("chat_id")?.takeIf { it > 0L }?.toString()
                if (msgId != null && nonce != null) {
                    ackChatId?.let { chatMessagesEpochFor(it).incrementAndGet() }
                    _state.update { cur ->
                        val nextMap = cur.messagesByChat.mapValues { (cid, list) ->
                            if (ackChatId != null && cid != ackChatId) {
                                list
                            } else {
                                val hasMatch = list.any { it.clientNonce == nonce || it.id == msgId }
                                if (!hasMatch) {
                                    list
                                } else {
                                    chatMessagesEpochFor(cid).incrementAndGet()
                                    val existingServer = list.firstOrNull { it.id == msgId }
                                    val anyRead = existingServer?.deliveryState == "read" ||
                                        list.any { it.clientNonce == nonce && it.deliveryState == "read" }
                                    val mergedState = if (anyRead) "read" else "sent"
                                    if (existingServer != null) {
                                        list.filterNot { it.clientNonce == nonce && it.id != msgId }.map { m ->
                                            if (m.id == msgId) m.copy(deliveryState = mergedState) else m
                                        }
                                    } else {
                                        list.map { m ->
                                            if (m.clientNonce == nonce) {
                                                m.copy(
                                                    id = msgId,
                                                    deliveryState = if (m.deliveryState == "read") "read" else "sent",
                                                )
                                            } else m
                                        }
                                    }
                                }
                            }
                        }
                        cur.copy(messagesByChat = nextMap)
                    }
                }
            }
            "Update" -> {
                if (payload != null) handleUpdateEvent(payload, null, capturedGen)
            }
            "PtsPush" -> {
                if (payload != null) {
                    val cid = payload.optLong("chat_id").takeIf { it > 0L }
                    val pts = payload.optLong("pts", 0L)
                    if (cid != null && pts > 0L) chatPtsMap[cid] = maxOf(chatPtsMap[cid] ?: 0L, pts)
                    val ev = payload.optJSONObject("event")
                    if (ev != null) handleUpdateEvent(ev, cid, capturedGen)
                }
            }
            "SyncDiff" -> {
                if (payload != null) {
                    val cid = payload.optLong("chat_id").takeIf { it > 0L }
                    val pts = payload.optLong("current_pts").takeIf { it > 0L } ?: payload.optLong("pts", 0L)
                    if (cid != null && pts > 0L) chatPtsMap[cid] = maxOf(chatPtsMap[cid] ?: 0L, pts)
                    val newMsgs = payload.optJSONArray("new_messages")
                    if (newMsgs != null) {
                        for (i in 0 until newMsgs.length()) {
                            val m = newMsgs.optJSONObject(i) ?: continue
                            val msgCid = m.optLong("chat_id").takeIf { it > 0L } ?: cid
                            if (msgCid != null) applyIncomingNewMessage(m, msgCid)
                        }
                    }
                    val otherUpdates = payload.optJSONArray("other_updates")
                    if (otherUpdates != null) {
                        for (i in 0 until otherUpdates.length()) {
                            otherUpdates.optJSONObject(i)?.let { handleUpdateEvent(it, cid, capturedGen) }
                        }
                    }
                    val ptsUpdates = payload.optJSONArray("updates")
                    if (ptsUpdates != null) {
                        for (i in 0 until ptsUpdates.length()) {
                            val pu = ptsUpdates.optJSONObject(i) ?: continue
                            val ev = pu.optJSONObject("event") ?: pu
                            handleUpdateEvent(ev, pu.optLong("chat_id").takeIf { it > 0L } ?: cid, capturedGen)
                        }
                    }
                    if (payload.optBoolean("snapshot_required", false)) {
                        refreshAllFromHost()
                    }
                }
            }
            else -> {
                // Direct UpdateEvent frame (if server sends UpdateEvent unwrapped)
                handleUpdateEvent(frame, null, capturedGen)
            }
        }
    }

    private fun handleUpdateEvent(event: JSONObject, fallbackChatId: Long?, capturedGen: Long) {
        if (!generationFence.isCurrent(capturedGen)) return
        val type = event.optNonNullString("type")
        val data = event.optJSONObject("data") ?: JSONObject()
        val chatIdNum = data.optLong("chat_id").takeIf { it > 0L } ?: fallbackChatId
        val chatIdStr = chatIdNum?.toString()

        when (type) {
            "NewMessage" -> {
                val cid = data.optLong("chat_id").takeIf { it > 0L } ?: fallbackChatId ?: return
                applyIncomingNewMessage(data, cid)
            }
            "EditMessage" -> {
                val cid = (data.optLong("chat_id").takeIf { it > 0L } ?: fallbackChatId)?.toString() ?: return
                val msgId = data.optLong("id").takeIf { it > 0L }?.toString()
                    ?: data.optLong("message_id").takeIf { it > 0L }?.toString()
                    ?: return
                chatMessagesEpochFor(cid).incrementAndGet()
                val newText = extractMessageText(data)
                val (plain, ents) = TelegramEntitiesEngine.parseMarkdownAndAutoEntities(newText)
                updateMessageInChat(cid, msgId) { it.copy(text = plain, entities = ents, edited = true) }
            }
            "DeleteMessages" -> {
                val cid = chatIdStr ?: return
                val arr = data.optJSONArray("message_ids") ?: return
                val deletedIds = buildSet {
                    for (i in 0 until arr.length()) {
                        arr.optLong(i).takeIf { it > 0L }?.toString()?.let(::add)
                    }
                }
                if (deletedIds.isEmpty()) return
                chatMessagesEpochFor(cid).incrementAndGet()
                chatPinsEpochFor(cid).incrementAndGet()
                _state.update { cur ->
                    val remaining = cur.messagesByChat[cid].orEmpty().filterNot { it.id in deletedIds }
                    val updatedChats = cur.chats.map { c ->
                        if (c.id == cid) c.copy(pinnedMessageIds = c.pinnedMessageIds.filterNot { it in deletedIds }) else c
                    }
                    cur.copy(
                        messagesByChat = cur.messagesByChat + (cid to remaining),
                        chats = updatedChats,
                    )
                }
            }
            "UserTyping" -> {
                val cid = chatIdStr ?: return
                val action = data.optNonNullString("action", "typing").lowercase()
                typingTimeoutJobs[cid]?.cancel()
                if (action == "cancel") {
                    _state.update { cur ->
                        cur.copy(chats = cur.chats.map { if (it.id == cid) it.copy(typingLabel = null) else it })
                    }
                } else {
                    val label = when (action) {
                        "upload_photo" -> "正在发送图片…"
                        "upload_document" -> "正在发送文件…"
                        "record_voice" -> "正在录制语音…"
                        else -> "正在输入…"
                    }
                    _state.update { cur ->
                        cur.copy(chats = cur.chats.map { if (it.id == cid) it.copy(typingLabel = label) else it })
                    }
                    typingTimeoutJobs[cid] = viewModelScope.launch {
                        delay(5_000L)
                        if (!generationFence.isCurrent(capturedGen)) return@launch
                        _state.update { cur ->
                            cur.copy(chats = cur.chats.map { if (it.id == cid && it.typingLabel == label) it.copy(typingLabel = null) else it })
                        }
                    }
                }
            }
            "ChatAction" -> {
                val cid = chatIdStr ?: return
                val actionName = data.optNonNullString("action")
                val actionPayload = data.optJSONObject("payload")
                when (actionName) {
                    "TitleChanged" -> {
                        val title = actionPayload?.optNonNullString("new_title").orEmpty()
                            .ifBlank { actionPayload?.optNonNullString("title").orEmpty() }
                        if (title.isNotBlank()) {
                            _state.update { cur ->
                                cur.copy(chats = cur.chats.map { if (it.id == cid) it.copy(title = title) else it })
                            }
                        }
                    }
                    "PinnedMessageChanged" -> {
                        val pid = actionPayload?.optLong("message_id")?.takeIf { it > 0L }?.toString()
                        applyPinnedMessageIdsToChat(cid, if (pid != null) listOf(pid) else emptyList())
                    }
                }
            }
            "ReadHistory" -> {
                val cid = chatIdStr ?: return
                val maxId = data.optLong("max_id", 0L)
                val isOutbox = data.optBoolean("is_outbox", false)
                if (isOutbox) {
                    applyReadOutbox(cid, maxId)
                } else {
                    val unread = data.optInt("unread_count", 0)
                    _state.update { cur ->
                        cur.copy(
                            chats = cur.chats.map { c ->
                                if (c.id == cid) c.copy(unreadCount = unread, readInboxMaxId = maxOf(c.readInboxMaxId, maxId)) else c
                            },
                        )
                    }
                }
            }
            "ReadHistoryOut" -> {
                val cid = chatIdStr ?: return
                applyReadOutbox(cid, data.optLong("max_id", 0L))
            }
            "ReadHistoryIn" -> {
                val cid = chatIdStr ?: return
                val maxId = data.optLong("max_id", 0L)
                _state.update { cur ->
                    cur.copy(
                        chats = cur.chats.map { c ->
                            if (c.id == cid) c.copy(unreadCount = 0, readInboxMaxId = maxOf(c.readInboxMaxId, maxId)) else c
                        },
                    )
                }
            }
            "MessageReactions", "ReactionUpdated" -> {
                val cid = chatIdStr ?: return
                val msgId = data.optLong("message_id").takeIf { it > 0L }?.toString() ?: return
                if (inFlightReactions.containsKey("$cid:$msgId")) return
                val arr = data.optJSONArray("reactions") ?: return
                chatMessagesEpochFor(cid).incrementAndGet()
                val reactions = parseReactionsArray(arr)
                updateMessageInChat(cid, msgId) { it.copy(reactions = reactions) }
            }
            "UserStatusChanged" -> {
                val uid = data.optLong("user_id").takeIf { it > 0L } ?: return
                val statusStr = data.optNonNullString("status", "").lowercase()
                val online = statusStr == "online"
                _state.update { cur ->
                    cur.copy(
                        chats = cur.chats.map { c ->
                            if (c.peerUserId == uid) c.copy(online = online) else c
                        },
                    )
                }
            }
            "PinnedMessageChanged" -> {
                val cid = chatIdStr ?: return
                val mid = if (data.has("message_id") && !data.isNull("message_id")) data.optLong("message_id").takeIf { it > 0L }?.toString() else null
                if (!inFlightMessagePins.containsKey(cid)) {
                    chatPinsEpochFor(cid).incrementAndGet()
                    chatsEpoch.incrementAndGet()
                    applyPinnedMessageIdsToChat(cid, if (mid != null) listOf(mid) else emptyList())
                }
            }
            "PinnedMessagesChanged" -> {
                val cid = chatIdStr ?: return
                if (inFlightMessagePins.containsKey(cid)) return
                chatPinsEpochFor(cid).incrementAndGet()
                chatsEpoch.incrementAndGet()
                val arr = data.optJSONArray("pinned_ids") ?: data.optJSONArray("pinned_message_ids") ?: JSONArray()
                val pins = buildList {
                    for (i in 0 until arr.length()) {
                        arr.optLong(i).takeIf { it > 0L }?.toString()?.let(::add)
                    }
                }
                applyPinnedMessageIdsToChat(cid, pins)
            }
            "PollUpdated" -> {
                val cid = chatIdStr ?: return
                val msgId = data.optLong("message_id").takeIf { it > 0L }?.toString()
                val pollObj = data.optJSONObject("poll") ?: return
                val parsedPoll = parsePollObject(pollObj) ?: return
                val pollId = pollObj.optNonNullString("id").ifBlank { null }
                _state.update { cur ->
                    val msgs = cur.messagesByChat[cid].orEmpty().map { m ->
                        if ((msgId != null && m.id == msgId) || (pollId != null && m.pollId == pollId)) {
                            m.copy(poll = parsedPoll, pollId = pollId ?: m.pollId)
                        } else m
                    }
                    cur.copy(messagesByChat = cur.messagesByChat + (cid to msgs))
                }
            }
            "TodoUpdated" -> {
                val cid = chatIdStr ?: return
                val msgId = data.optLong("message_id").takeIf { it > 0L }?.toString() ?: return
                val title = data.optJSONObject("todo")?.optNonNullString("title").orEmpty()
                if (title.isNotBlank()) {
                    updateMessageInChat(cid, msgId) { it.copy(text = title) }
                }
            }
            "DraftUpdated" -> {
                val cid = chatIdStr ?: return
                val draftObj = data.optJSONObject("draft")
                val draftText = draftObj?.optNonNullString("text").orEmpty()
                chatDraftEpochFor(cid).incrementAndGet()
                _state.update { cur ->
                    cur.copy(chats = cur.chats.map { if (it.id == cid) it.copy(draftText = draftText) else it })
                }
            }
            "ForumTopicChanged" -> {
                val cid = chatIdStr ?: return
                val topicObj = data.optJSONObject("topic") ?: return
                val tid = topicObj.optLong("id").takeIf { it > 0L }?.toString()
                    ?: topicObj.optNonNullString("topic_id").ifBlank { return }
                val topic = TelegramForumTopic(
                    id = tid,
                    conversationId = cid,
                    title = topicObj.optNonNullString("title", "话题 #$tid"),
                    iconEmoji = topicObj.optNonNullString("icon_emoji", "💡"),
                    isPinned = topicObj.optBoolean("is_pinned", false),
                    isClosed = topicObj.optBoolean("is_closed", false),
                )
                _state.update { cur ->
                    val list = cur.topicsByChat[cid].orEmpty().filterNot { it.id == tid } + topic
                    cur.copy(topicsByChat = cur.topicsByChat + (cid to list))
                }
            }
            "FolderChanged" -> {
                val folderObj = data.optJSONObject("folder") ?: return
                val parsed = parseServerFolderObject(folderObj) ?: return
                foldersEpoch.incrementAndGet()
                _state.update { cur ->
                    val builtIn = cur.folderTabs.filter { it.isBuiltIn }
                    val custom = cur.folderTabs.filterNot { it.isBuiltIn }.filterNot { it.id == parsed.id } + parsed
                    cur.copy(folderTabs = builtIn + custom)
                }
            }
            "StoryUpdated" -> {
                val storyObj = data.optJSONObject("story") ?: return
                val sid = storyObj.optNonNullString("id").ifBlank { return }
                val item = TelegramStoryItem(
                    id = sid,
                    authorId = storyObj.optNonNullString("author_id"),
                    authorName = storyObj.optNonNullString("author_name"),
                    avatarSeed = storyObj.optNonNullString("author_id", sid),
                    headline = storyObj.optNonNullString("headline"),
                    caption = storyObj.optNonNullString("caption"),
                    viewCount = storyObj.optInt("view_count", 0),
                    reactionsCount = storyObj.optInt("reactions_count", 0),
                )
                _state.update { cur ->
                    cur.copy(stories = listOf(item) + cur.stories.filterNot { it.id == sid })
                }
            }
            "BotActivityChanged" -> {
                val botUserId = data.optLong("bot_user_id", 0L)
                val execState = data.optNonNullString("state", "").trim().lowercase()
                val detail = data.optNullableString("detail")
                botsEpoch.incrementAndGet()
                val subtitle = when (execState) {
                    "", "idle" -> null
                    "tool", "running_tool" -> "正在调用 ${detail ?: "工具"}..."
                    "writing" -> detail ?: "正在组织回复..."
                    else -> detail ?: "AI 正在思考..."
                }
                _state.update { cur ->
                    val updatedBots = cur.fabushiBots.map { b ->
                        if (botUserId > 0L && b.botUserId == botUserId) b.copy(executionState = execState) else b
                    }
                    val updatedChats = cur.chats.map { c ->
                        val matches = (chatIdStr != null && c.id == chatIdStr) || (botUserId > 0L && c.peerUserId == botUserId)
                        if (matches) {
                            c.copy(
                                typingLabel = subtitle,
                                botExecutionState = execState,
                                botActivityDetail = detail,
                            )
                        } else c
                    }
                    cur.copy(fabushiBots = updatedBots, chats = updatedChats)
                }
            }
            "MiniAppControlStatus" -> {
                val mode = data.optNonNullString("mode", "idle")
                val statusText = data.optNonNullString("status_text", "")
                val activeTool = data.optNullableString("active_tool")
                val appTitle = data.optNonNullString("app_title").ifBlank { _state.value.miniAppPip.appTitle }
                val botUsername = data.optNonNullString("bot_username").ifBlank { _state.value.miniAppPip.botUsername }
                val shortName = data.optNonNullString("short_name").ifBlank { _state.value.miniAppPip.shortName }
                _state.update { cur ->
                    cur.copy(
                        miniAppPip = cur.miniAppPip.copy(
                            active = mode != "idle" && mode != "closed",
                            botUsername = botUsername,
                            shortName = shortName,
                            appTitle = appTitle,
                            controlMode = mode,
                            statusText = statusText,
                            activeTool = activeTool,
                        ),
                    )
                }
            }
            "CallUpdated", "AgentFloorChanged", "AgentWorkspaceUpdated", "AgentHandoffEvent",
            "MiniAppWebMcpCommand", "CodingHostCommand" -> Unit
        }
    }

    internal fun applyIncomingNewMessage(msgJson: JSONObject, chatIdNum: Long) {
        val chatIdStr = chatIdNum.toString()
        val myUid = currentServerUserId
        val chatSnapshot = _state.value.chats.firstOrNull { it.id == chatIdStr }
        val parsed = parseServerMessageJson(msgJson, chatIdStr, myUid, chatSnapshot?.readOutboxMaxId ?: 0L) ?: return
        chatMessagesEpochFor(chatIdStr).incrementAndGet()
        chatsEpoch.incrementAndGet()

        _state.update { cur ->
            val existing = cur.messagesByChat[chatIdStr].orEmpty()
            val matchingExisting = existing.firstOrNull {
                it.id == parsed.id || (parsed.clientNonce != null && it.clientNonce == parsed.clientNonce)
            }
            val mergedParsed = if (matchingExisting?.deliveryState == "read") {
                parsed.copy(deliveryState = "read")
            } else parsed
            val deduped = existing.filterNot {
                it.id == mergedParsed.id || (mergedParsed.clientNonce != null && it.clientNonce == mergedParsed.clientNonce)
            } + mergedParsed
            val sortedMsgs = deduped.sortedWith(
                compareBy<TelegramMessageItem> { it.timestampMs }.thenBy { it.id.toLongOrNull() ?: Long.MAX_VALUE },
            )
            val previewText = parsed.text.ifBlank {
                when (parsed.contentType) {
                    "photo" -> "[图片]"
                    "voice" -> "[语音]"
                    "video", "videoNote" -> "[视频]"
                    "document" -> "[文件] ${parsed.mediaFileName.orEmpty()}".trim()
                    else -> ""
                }
            }.lineSequence().firstOrNull().orEmpty().take(72)

            val isActiveChat = cur.activeChatId == chatIdStr && cur.subScreen == TelegramSubScreen.CHAT
            val updatedChats = cur.chats.map { c ->
                if (c.id == chatIdStr) {
                    c.copy(
                        preview = previewText,
                        time = parsed.time,
                        lastMessageAtMs = maxOf(c.lastMessageAtMs, parsed.timestampMs),
                        lastMessageOutgoing = parsed.outgoing,
                        lastDeliveryState = parsed.deliveryState,
                        unreadCount = if (!parsed.outgoing && !isActiveChat) c.unreadCount + 1 else c.unreadCount,
                    )
                } else c
            }
            cur.copy(
                messagesByChat = cur.messagesByChat + (chatIdStr to sortedMsgs),
                chats = sortTelegramChats(updatedChats),
            )
        }
    }

    private fun applyReadOutbox(chatId: String, maxId: Long) {
        if (maxId <= 0L) return
        _state.update { cur ->
            val msgs = cur.messagesByChat[chatId]?.map { m ->
                val mid = m.id.toLongOrNull()
                if (m.outgoing && mid != null && mid <= maxId && m.deliveryState != "failed") {
                    m.copy(deliveryState = "read")
                } else m
            }
            val updatedChats = cur.chats.map { c ->
                if (c.id == chatId) {
                    val nextMax = maxOf(c.readOutboxMaxId, maxId)
                    val lastMsg = msgs?.lastOrNull()
                    c.copy(
                        readOutboxMaxId = nextMax,
                        lastDeliveryState = if (lastMsg?.outgoing == true && lastMsg.deliveryState == "read") "read" else c.lastDeliveryState,
                    )
                } else c
            }
            cur.copy(
                chats = updatedChats,
                messagesByChat = if (msgs != null) cur.messagesByChat + (chatId to msgs) else cur.messagesByChat,
            )
        }
    }

    private fun disconnectServerWebSocket(clearIntent: Boolean = true) {
        val active: WebSocket?
        synchronized(wsLifecycleLock) {
            if (clearIntent) {
                wsShouldReconnect = false
                wsEndpoint = null
                wsToken = null
            }
            wsReconnectJob?.cancel()
            wsReconnectJob = null
            active = serverWebSocket
            serverWebSocket = null
        }
        runCatching { active?.close(1000, "client_disconnect") }
        if (clearIntent) {
            _state.update { it.copy(serverWsConnected = false) }
        }
    }

    // ------------------------------------------------------------------------
    // JSON Parsers & Merge Helpers (Zero Fake Data, Media-Only Preserved)
    // ------------------------------------------------------------------------

    private fun parseServerChats(resp: JSONObject?): List<TelegramChatItem> {
        val arr = resp?.optJSONArray("chats") ?: return emptyList()
        return buildList {
            for (i in 0 until arr.length()) {
                val c = arr.optJSONObject(i) ?: continue
                parseServerChatObject(c)?.let(::add)
            }
        }
    }

    internal fun parseServerChatObject(c: JSONObject): TelegramChatItem? {
        val id = c.optLong("chat_id").takeIf { it > 0L }?.toString()
            ?: c.optLong("id").takeIf { it > 0L }?.toString()
            ?: c.optNonNullString("chat_id").ifBlank { c.optNonNullString("id") }.ifBlank { return null }

        val title = c.optNonNullString("title")
            .ifBlank { c.optNonNullString("peer_display_name") }
            .ifBlank { c.optNonNullString("peer_name") }
            .ifBlank { c.optNonNullString("username") }
            .ifBlank { "会话 #$id" }

        val kind = resolveConversationKind(c)
        val isBot = c.optBoolean("is_bot", false) || c.optNonNullString("kind").lowercase().contains("bot")
        val lastMsgObj = c.optJSONObject("last_message")
        val preview = c.optNonNullString("last_message_text")
            .ifBlank { lastMsgObj?.let(::extractMessageText).orEmpty() }
            .ifBlank { c.optNonNullString("preview") }

        val tsMs = c.optLong("last_message_date_ms").takeIf { it > 0L }
            ?: c.optLong("last_message_at_ms").takeIf { it > 0L }
            ?: lastMsgObj?.optLong("date_ms")?.takeIf { it > 0L }
            ?: c.optLong("updated_at_ms").takeIf { it > 0L }
            ?: 0L

        val pinnedMsgId = c.optLong("pinned_message_id").takeIf { it > 0L }?.toString()
        val draftText = c.optJSONObject("draft")?.optNonNullString("text").orEmpty()
        val peerUsername = c.optNonNullString("peer_username").ifBlank { c.optNonNullString("username") }.ifBlank { null }
        val pts = c.optLong("pts", 0L)
        id.toLongOrNull()?.let { cid -> if (pts > 0L) chatPtsMap[cid] = maxOf(chatPtsMap[cid] ?: 0L, pts) }

        return TelegramChatItem(
            id = id,
            title = title,
            subtitle = c.optNonNullString("about").ifBlank {
                peerUsername?.let { "@${it.trimStart('@')}" } ?: kind.label
            },
            preview = preview,
            time = if (tsMs > 0L) nowTimeLabel(tsMs) else "",
            kind = kind,
            unreadCount = c.optInt("unread_count", 0),
            isPinned = c.optBoolean("is_pinned", false),
            isMuted = c.optBoolean("is_muted", false),
            isArchived = c.optBoolean("is_archived", false),
            isBot = isBot,
            isForum = c.optBoolean("is_forum", false),
            isSecretChat = kind == ConversationKind.SECRET,
            online = c.optBoolean("online", false),
            memberCount = c.optInt("member_count", 0),
            subscriberCount = if (kind == ConversationKind.CHANNEL) c.optInt("member_count", 0) else 0,
            ttlSeconds = c.optInt("message_ttl_seconds", 0),
            lastDeliveryState = "",
            lastMessageAtMs = tsMs,
            pts = pts,
            readInboxMaxId = c.optLong("read_inbox_max_id", 0L),
            readOutboxMaxId = c.optLong("read_outbox_max_id", 0L),
            muteUntilMs = if (c.has("mute_until_ms") && !c.isNull("mute_until_ms")) c.optLong("mute_until_ms") else null,
            peerUserId = c.optLong("peer_user_id").takeIf { it > 0L },
            peerBio = c.optNonNullString("peer_bio").ifBlank { null },
            draftText = draftText,
            pinnedMessageIds = if (pinnedMsgId != null) listOf(pinnedMsgId) else emptyList(),
            username = peerUsername?.let { "@${it.trimStart('@')}" },
        )
    }

    private fun resolveConversationKind(c: JSONObject): ConversationKind {
        if (c.optBoolean("is_channel", false)) return ConversationKind.CHANNEL
        if (c.optBoolean("is_group", false)) return ConversationKind.GROUP
        val kindStr = c.optNonNullString("kind").lowercase()
        if (kindStr.isNotBlank()) {
            return when {
                kindStr.contains("channel") -> ConversationKind.CHANNEL
                kindStr.contains("group") -> ConversationKind.GROUP
                kindStr.contains("secret") -> ConversationKind.SECRET
                kindStr.contains("saved") -> ConversationKind.SAVED_MESSAGES
                else -> ConversationKind.DIRECT
            }
        }
        val ctInt = c.optInt("chat_type", -1)
        return when (ctInt) {
            1, 3 -> ConversationKind.GROUP
            2 -> ConversationKind.CHANNEL
            4 -> ConversationKind.SECRET
            5 -> ConversationKind.SAVED_MESSAGES
            else -> {
                val ctStr = c.optNonNullString("chat_type").lowercase()
                when {
                    ctStr.contains("channel") -> ConversationKind.CHANNEL
                    ctStr.contains("group") -> ConversationKind.GROUP
                    ctStr.contains("secret") -> ConversationKind.SECRET
                    ctStr.contains("saved") -> ConversationKind.SAVED_MESSAGES
                    else -> ConversationKind.DIRECT
                }
            }
        }
    }

    /**
     * Anti-Flicker Reconciliation Rule 3 & 5:
     * NEVER bitwise-OR `remote.isPinned || local.isPinned`.
     * Protect in-flight toggles (`inFlightPinChats`, `inFlightMuteChats`, `inFlightArchiveChats`, `inFlightMessagePins`)
     * and preserve live `typingLabel` / `botExecutionState` / `botActivityDetail`.
     */
    private fun mergeServerChats(existing: List<TelegramChatItem>, serverChats: List<TelegramChatItem>): List<TelegramChatItem> {
        val existingMap = existing.associateBy { it.id }
        val byId = LinkedHashMap<String, TelegramChatItem>()
        serverChats.forEach { remote ->
            val prev = existingMap[remote.id]
            val resolvedPinned = inFlightPinChats[remote.id]?.second ?: remote.isPinned
            val resolvedMuted = inFlightMuteChats[remote.id]?.second ?: remote.isMuted
            val resolvedArchived = inFlightArchiveChats[remote.id]?.second ?: remote.isArchived
            val inFlightPins = inFlightMessagePins[remote.id]?.second
            val resolvedPins = when {
                inFlightPins != null -> inFlightPins.toList()
                remote.pinnedMessageIds.isEmpty() -> emptyList()
                remote.pinnedMessageIds.size == 1 && prev != null && remote.pinnedMessageIds.first() in prev.pinnedMessageIds -> prev.pinnedMessageIds
                else -> remote.pinnedMessageIds
            }
            byId[remote.id] = remote.copy(
                isPinned = resolvedPinned,
                isMuted = resolvedMuted,
                muteUntilMs = if (resolvedMuted) remote.muteUntilMs else null,
                isArchived = resolvedArchived,
                pinnedMessageIds = resolvedPins,
                typingLabel = prev?.typingLabel,
                botExecutionState = prev?.botExecutionState ?: remote.botExecutionState,
                botActivityDetail = prev?.botActivityDetail ?: remote.botActivityDetail,
                avatarShape = prev?.avatarShape ?: remote.avatarShape,
                avatarColor = prev?.avatarColor ?: remote.avatarColor,
                hasMoreHistory = prev?.hasMoreHistory ?: remote.hasMoreHistory,
                draftText = remote.draftText.ifEmpty { prev?.draftText.orEmpty() },
            )
        }
        // Keep non-numeric local-only chats (e.g., E2EE secret chats)
        existing.forEach { local ->
            if (!byId.containsKey(local.id) && local.id.toLongOrNull() == null) {
                byId[local.id] = local
            }
        }
        return byId.values.toList()
    }

    internal fun parseServerMessageJson(
        m: JSONObject,
        chatId: String,
        myUid: Long?,
        readOutboxMaxId: Long,
    ): TelegramMessageItem? {
        val idNum = m.optLong("id").takeIf { it > 0L } ?: m.optLong("message_id").takeIf { it > 0L }
        val idStr = idNum?.toString() ?: m.optNonNullString("id").ifBlank { m.optNonNullString("message_id") }.ifBlank { return null }

        val contentObj = m.optJSONObject("content")
        val contentData = contentObj?.optJSONObject("data")
        val contentType = inferContentType(m, contentObj)
        // K-F22: NEVER drop media-only messages where rawText is blank!
        val rawText = extractMessageText(m)
        val (plain, ents) = if (rawText.isNotEmpty()) {
            TelegramEntitiesEngine.parseMarkdownAndAutoEntities(rawText)
        } else "" to emptyList()

        val senderIdNum = m.optLong("sender_id", -1L)
        val isOutgoing = (myUid != null && senderIdNum == myUid) || m.optBoolean("is_outgoing", false)
        val senderName = m.optNonNullString("sender_name").ifBlank {
            if (isOutgoing) _state.value.activeAccount?.displayName ?: "我" else "对方"
        }

        val tsMs = parseTimestampMs(m)
        val deliveryState = when {
            !isOutgoing -> "sent"
            idNum != null && readOutboxMaxId > 0L && idNum <= readOutboxMaxId -> "read"
            else -> m.optNonNullString("status").ifBlank { "sent" }
        }

        val replyToObj = m.optJSONObject("reply_to")
        val replyToInfoObj = m.optJSONObject("reply_to_info")
        val replyId = replyToObj?.optLong("reply_to_msg_id")?.takeIf { it > 0L }?.toString()
            ?: replyToObj?.optLong("message_id")?.takeIf { it > 0L }?.toString()
            ?: m.optLong("reply_to").takeIf { it > 0L }?.toString()
            ?: m.optLong("reply_to_msg_id").takeIf { it > 0L }?.toString()
            ?: m.optLong("reply_to_id").takeIf { it > 0L }?.toString()
            ?: replyToInfoObj?.optLong("message_id")?.takeIf { it > 0L }?.toString()
        val replyHeader = replyId?.let { rid ->
            val info = replyToInfoObj ?: replyToObj
            TelegramReplyHeader(
                replyToMessageId = rid,
                senderName = info?.optNonNullString("sender_name").orEmpty(),
                quoteText = info?.optNonNullString("quote_text").orEmpty().ifBlank { info?.optNonNullString("text").orEmpty() },
            )
        }

        val fwdObj = m.optJSONObject("forward_info")
        val fwdHeader = fwdObj?.let {
            TelegramForwardHeader(
                originName = it.optNonNullString("from_name").ifBlank { it.optNonNullString("origin_name", "转发消息") },
            )
        }

        val reactions = m.optJSONArray("reactions")?.let(::parseReactionsArray).orEmpty()
        val inlineKeyboard = parseInlineKeyboard(m.optJSONArray("inline_keyboard")) ?: emptyList()

        val pollObj = m.optJSONObject("poll") ?: contentData?.takeIf { contentType == "poll" }
        val poll = parsePollObject(pollObj)
        val pollId = pollObj?.optNullableString("id")
            ?: pollObj?.optLong("id")?.takeIf { it > 0L }?.toString()

        val mediaBlobId = m.optNullableString("media_id")
            ?: m.optNullableString("file_id")
            ?: contentData?.optNullableString("file_id")
            ?: contentObj?.optNullableString("file_id")
        val mediaFileName = m.optNullableString("filename")
            ?: m.optNullableString("file_name")
            ?: contentData?.optNullableString("file_name")
            ?: contentObj?.optNullableString("file_name")
        val mediaMimeType = m.optNullableString("mime_type")
            ?: contentData?.optNullableString("mime_type")
            ?: contentObj?.optNullableString("mime_type")
        val mediaSizeBytes = m.optInt("size_bytes", contentData?.optInt("size_bytes", 0) ?: 0)
        val durationSec = m.optInt("duration_seconds", contentData?.optInt("duration", 0) ?: 0)
        val thumbUrl = m.optNullableString("thumbnail_data_url")
            ?: contentData?.optNullableString("thumbnail_data_url")
        val clientNonce = m.optNullableString("client_nonce")
        val isEdited = m.optBoolean("is_edited", false) ||
            m.optLong("edit_date", 0L) > 0L ||
            (!m.isNull("edit_date") && m.optNonNullString("edit_date").let { it.isNotBlank() && it != "0" }) ||
            m.optLong("edited_at_ms", 0L) > 0L
        val ttlSec = m.optInt("ttl_seconds", 0).takeIf { it > 0 }

        val latitude = if (contentType == "location" && contentData != null && contentData.has("latitude") && !contentData.isNull("latitude")) {
            contentData.optDouble("latitude").takeIf { !it.isNaN() }
        } else null
        val longitude = if (contentType == "location" && contentData != null && contentData.has("longitude") && !contentData.isNull("longitude")) {
            contentData.optDouble("longitude").takeIf { !it.isNaN() }
        } else null
        val venueTitle = if (contentType == "location") contentData?.optNullableString("title") else null
        val contactName = if (contentType == "contact") {
            contentData?.optNullableString("display_name") ?: contentData?.optNullableString("name")
        } else null
        val contactPhone = if (contentType == "contact") contentData?.optNullableString("phone") else null

        return TelegramMessageItem(
            id = idStr,
            conversationId = chatId,
            senderId = if (senderIdNum > 0L) senderIdNum.toString() else senderName,
            senderName = senderName,
            text = plain,
            entities = ents,
            contentType = contentType,
            outgoing = isOutgoing,
            time = nowTimeLabel(tsMs),
            timestampMs = tsMs,
            deliveryState = deliveryState,
            edited = isEdited,
            pinned = m.optBoolean("pinned", false) || m.optBoolean("is_pinned", false),
            ttlSeconds = ttlSec,
            replyHeader = replyHeader,
            forwardHeader = fwdHeader,
            reactions = reactions,
            inlineKeyboard = inlineKeyboard,
            poll = poll,
            pollId = pollId,
            durationSeconds = durationSec,
            mediaFileName = mediaFileName,
            mediaBlobId = mediaBlobId,
            mediaMimeType = mediaMimeType,
            mediaSizeBytes = mediaSizeBytes,
            mediaThumbnailDataUrl = thumbUrl,
            latitude = latitude,
            longitude = longitude,
            venueTitle = venueTitle,
            contactName = contactName,
            contactPhone = contactPhone,
            clientNonce = clientNonce,
        )
    }

    private fun extractMessageText(m: JSONObject): String {
        val direct = m.optNonNullString("text", "")
        if (direct.isNotEmpty()) return direct
        val contentObj = m.optJSONObject("content")
        if (contentObj != null) {
            val data = contentObj.optJSONObject("data")
            val formatted = data?.optJSONObject("text")?.optNonNullString("text").orEmpty()
            if (formatted.isNotEmpty()) return formatted
            val dataCaption = data?.optNonNullString("caption").orEmpty()
            if (dataCaption.isNotEmpty()) return dataCaption
            val caption = contentObj.optNonNullString("caption").orEmpty()
            if (caption.isNotEmpty()) return caption
            val contentText = contentObj.optNonNullString("text").orEmpty()
            if (contentText.isNotEmpty()) return contentText
        }
        val contentStr = m.opt("content") as? String
        if (!contentStr.isNullOrEmpty()) return contentStr
        return ""
    }

    private fun inferContentType(m: JSONObject, contentObj: JSONObject?): String {
        val explicit = m.optNonNullString("content_type", "").lowercase()
        if (explicit.isNotBlank()) return if (explicit == "video_note") "videoNote" else explicit
        val fromContent = contentObj?.optNonNullString("type", "")?.lowercase().orEmpty()
        return when (fromContent) {
            "photo", "image" -> "photo"
            "voice", "voice_note" -> "voice"
            "video" -> "video"
            "video_note", "videonote" -> "videoNote"
            "audio" -> "audio"
            "document", "file" -> "document"
            "sticker" -> "sticker"
            "poll" -> "poll"
            "location" -> "location"
            "contact" -> "contact"
            "service" -> "service"
            else -> {
                if (m.has("poll") && !m.isNull("poll")) "poll"
                else if (m.optNonNullString("media_id").isNotBlank() || m.optNonNullString("file_id").isNotBlank()) {
                    val mime = m.optNonNullString("mime_type", "")
                    when {
                        mime.startsWith("image/") -> "photo"
                        mime.startsWith("video/") -> "video"
                        mime.startsWith("audio/") -> "voice"
                        else -> "document"
                    }
                } else "text"
            }
        }
    }

    private fun parseTimestampMs(m: JSONObject): Long {
        m.optLong("date_ms").takeIf { it > 0L }?.let { return it }
        m.optLong("created_at_ms").takeIf { it > 0L }?.let { return it }
        m.optLong("timestamp_ms").takeIf { it > 0L }?.let { return it }
        m.optLong("timestamp").takeIf { it > 0L }?.let { return if (it < 10_000_000_000L) it * 1000L else it }
        val dateStr = m.optNonNullString("date").trim()
        if (dateStr.isNotEmpty()) {
            dateStr.toLongOrNull()?.let { return if (it < 10_000_000_000L) it * 1000L else it }
            runCatching { Instant.parse(dateStr).toEpochMilli() }.getOrNull()?.let { return it }
        }
        return System.currentTimeMillis()
    }

    private fun parseReactionsArray(arr: JSONArray): List<ChatReaction> = buildList {
        for (i in 0 until arr.length()) {
            val r = arr.optJSONObject(i) ?: continue
            val emoji = r.optNonNullString("emoji").ifBlank { r.optNonNullString("reaction") }
            if (emoji.isBlank()) continue
            val count = r.optInt("count", 1)
            val chosen = r.optBoolean("is_chosen_by_me", r.optBoolean("chosen_by_me", r.optBoolean("chosenByMe", false)))
            if (count > 0) add(ChatReaction(emoji, count, chosen))
        }
    }

    internal fun parseInlineKeyboard(arr: JSONArray?): List<InlineKeyboardRow>? {
        if (arr == null) return null
        return buildList {
            for (r in 0 until arr.length()) {
                val rowArr = arr.optJSONArray(r) ?: continue
                val buttons = buildList {
                    for (c in 0 until rowArr.length()) {
                        val b = rowArr.optJSONObject(c) ?: continue
                        val text = b.optNonNullString("text")
                        if (text.isBlank()) continue
                        val url = b.optNullableString("url")
                        val cb = b.optNullableString("callback_data")
                        val webApp = b.optNullableString("web_app_url")
                            ?: b.optJSONObject("web_app")?.optNullableString("url")
                        val (kind, payload) = when {
                            webApp != null -> InlineKeyboardButtonKind.MINI_APP to webApp
                            url != null -> InlineKeyboardButtonKind.URL to url
                            else -> InlineKeyboardButtonKind.CALLBACK to (cb ?: "")
                        }
                        add(
                            InlineKeyboardButtonModel(
                                id = "kb-$r-$c",
                                text = text,
                                kind = kind,
                                payload = payload,
                            ),
                        )
                    }
                }
                if (buttons.isNotEmpty()) add(InlineKeyboardRow(buttons))
            }
        }
    }

    private fun parsePollObject(pollObj: JSONObject?): TelegramPollModel? {
        if (pollObj == null) return null
        val question = pollObj.optNonNullString("question")
        val optArr = pollObj.optJSONArray("options") ?: return null
        if (question.isBlank() || optArr.length() == 0) return null
        val correctIdx = if (pollObj.has("correct_option_id") && !pollObj.isNull("correct_option_id")) pollObj.optInt("correct_option_id", -1) else -1
        val options = buildList {
            for (i in 0 until optArr.length()) {
                val o = optArr.optJSONObject(i)
                if (o != null) {
                    val oid = o.optNonNullString("id").ifBlank { "opt-${i + 1}" }
                    add(
                        TelegramPollOption(
                            id = oid,
                            text = o.optNonNullString("text"),
                            voterCount = o.optInt("voter_count", o.optInt("votes", 0)),
                            chosen = o.optBoolean("chosen", o.optBoolean("is_chosen", false)),
                            isCorrect = i == correctIdx,
                        ),
                    )
                } else {
                    val label = optArr.optNonNullString(i)
                    if (label.isNotBlank()) add(TelegramPollOption("opt-${i + 1}", label, 0, false, i == correctIdx))
                }
            }
        }
        return TelegramPollModel(
            question = question,
            options = options,
            isAnonymous = pollObj.optBoolean("is_anonymous", true),
            multipleAnswers = pollObj.optBoolean("multiple_choice", pollObj.optBoolean("multiple_answers", false)),
            isQuiz = pollObj.optBoolean("is_quiz", false),
            correctOptionId = if (correctIdx in options.indices) options[correctIdx].id else null,
            explanation = pollObj.optNonNullString("solution").ifBlank { pollObj.optNonNullString("explanation").ifBlank { null } },
            closed = pollObj.optBoolean("closed", pollObj.optBoolean("is_closed", false)),
        )
    }

    private fun parsePinnedMessageIds(resp: JSONObject): List<String> {
        val arr = resp.optJSONArray("pinned_message_ids") ?: resp.optJSONArray("pinned_ids")
        if (arr != null) {
            return buildList {
                for (i in 0 until arr.length()) {
                    arr.optLong(i).takeIf { it > 0L }?.toString()?.let(::add)
                }
            }
        }
        val pinsArr = resp.optJSONArray("pins") ?: return emptyList()
        return buildList {
            for (i in 0 until pinsArr.length()) {
                val p = pinsArr.optJSONObject(i) ?: continue
                p.optLong("message_id").takeIf { it > 0L }?.toString()?.let(::add)
            }
        }
    }

    internal fun parseServerFolders(resp: JSONObject): List<TelegramFolderTab> {
        val arr = resp.optJSONArray("folders") ?: return emptyList()
        return buildList {
            for (i in 0 until arr.length()) {
                arr.optJSONObject(i)?.let(::parseServerFolderObject)?.let(::add)
            }
        }
    }

    internal fun parseServerFolderObject(f: JSONObject): TelegramFolderTab? {
        val id = f.optLong("id").takeIf { it > 0L }?.toString()
            ?: f.optNullableString("id")
            ?: return null
        if (id.lowercase() in RESERVED_FOLDER_TAB_IDS) return null
        val title = f.optNullableString("title") ?: return null
        val included = buildList {
            val arr = f.optJSONArray("included_chat_ids") ?: f.optJSONArray("includedChatIds")
            if (arr != null) for (i in 0 until arr.length()) add(arr.optLong(i).toString())
        }
        return TelegramFolderTab(
            id = id,
            title = title,
            icon = f.optNullableString("icon"),
            isBuiltIn = false,
            conversationIds = included,
            includeContacts = f.optBoolean("include_contacts", f.optBoolean("includeContacts", false)),
            includeBots = f.optBoolean("include_bots", f.optBoolean("includeBots", false)),
            includeGroups = f.optBoolean("include_groups", f.optBoolean("includeGroups", false)),
            includeChannels = f.optBoolean("include_channels", f.optBoolean("includeChannels", false)),
            excludeMuted = f.optBoolean("exclude_muted", f.optBoolean("excludeMuted", false)),
            excludeRead = f.optBoolean("exclude_read", f.optBoolean("excludeRead", false)),
            excludeArchived = f.optBoolean("exclude_archived", f.optBoolean("excludeArchived", true)),
        )
    }

    private fun parseServerContacts(resp: JSONObject): List<ServerContactItem> {
        val arr = resp.optJSONArray("contacts") ?: return emptyList()
        return buildList {
            for (i in 0 until arr.length()) {
                val c = arr.optJSONObject(i) ?: continue
                val uid = c.optLong("user_id").takeIf { it > 0L } ?: c.optLong("id", 0L)
                val username = c.optNonNullString("username")
                val displayName = c.optNonNullString("display_name")
                    .ifBlank { c.optNonNullString("first_name") }
                    .ifBlank { username }
                add(
                    ServerContactItem(
                        userId = uid,
                        username = username,
                        displayName = displayName,
                        phone = c.optNonNullString("phone", ""),
                        bio = c.optNonNullString("bio", ""),
                        chatId = c.optLong("chat_id").takeIf { it > 0L },
                    ),
                )
            }
        }
    }

    private fun parseServerSessions(resp: JSONObject): List<ServerSessionItem> {
        val arr = resp.optJSONArray("sessions") ?: return emptyList()
        return buildList {
            for (i in 0 until arr.length()) {
                val s = arr.optJSONObject(i) ?: continue
                val sid = s.optNonNullString("session_id").ifBlank { s.optNonNullString("id") }
                if (sid.isBlank()) continue
                add(
                    ServerSessionItem(
                        sessionId = sid,
                        deviceName = s.optNonNullString("device_name", "Unknown Device"),
                        platform = s.optNonNullString("platform", ""),
                        ipAddress = s.optNonNullString("ip", s.optNonNullString("ip_address", "")),
                        isCurrent = s.optBoolean("is_current", false),
                        lastActiveMs = s.optLong("last_active_ms", s.optLong("updated_at_ms", 0L)),
                    ),
                )
            }
        }
    }

    private fun mergeServerBots(myBotsResp: JSONObject?, fabushiBotsResp: JSONObject?): List<FabushiBotItem> {
        val map = LinkedHashMap<Long, FabushiBotItem>()
        myBotsResp?.optJSONArray("bots")?.let { arr ->
            for (i in 0 until arr.length()) {
                arr.optJSONObject(i)?.let { parseFabushiBotDto(it, isFabushi = false) }?.let { map[it.botUserId] = it }
            }
        }
        fabushiBotsResp?.optJSONArray("bots")?.let { arr ->
            for (i in 0 until arr.length()) {
                arr.optJSONObject(i)?.let { parseFabushiBotDto(it, isFabushi = true) }?.let { map[it.botUserId] = it }
            }
        }
        return map.values.toList()
    }

    internal fun parseFabushiBotDto(b: JSONObject, isFabushi: Boolean): FabushiBotItem? {
        val botUserId = b.optLong("bot_user_id").takeIf { it > 0L }
            ?: b.optLong("user_id").takeIf { it > 0L }
            ?: b.optLong("id").takeIf { it > 0L }
            ?: return null
        val username = b.optNonNullString("username", "").trimStart('@')
        val displayName = b.optNonNullString("display_name")
            .ifBlank { b.optNonNullString("title") }
            .ifBlank { username.ifBlank { "Bot #$botUserId" } }
        val codingObj = b.optJSONObject("coding_agent")
        val codingItem = codingObj?.let { ca ->
            val provWire = ca.optNonNullString("provider_kind", "claude_code")
            val permWire = ca.optNonNullString("permission_mode", "ask")
            CodingAgentBotItem(
                botId = botUserId.toString(),
                displayName = displayName,
                username = username,
                provider = CodingProviderKindUi.entries.firstOrNull { it.wire == provWire } ?: CodingProviderKindUi.CLAUDE_CODE,
                permissionMode = CodingPermissionModeUi.entries.firstOrNull { it.wire == permWire } ?: CodingPermissionModeUi.ASK,
                workspacePath = ca.optNonNullString("workspace_path", ""),
                useWorktree = ca.optBoolean("use_worktree", true),
                boundNodeId = b.optNonNullString("bound_node_id").ifBlank { ca.optNonNullString("bound_node_id").ifBlank { null } },
                activeBranch = ca.optNonNullString("active_branch", "main"),
                totalTurns = ca.optInt("total_turns", 0),
            )
        }
        return FabushiBotItem(
            botUserId = botUserId,
            agentId = b.optNonNullString("agent_id", ""),
            username = username,
            displayName = displayName,
            description = b.optNonNullString("description", ""),
            title = b.optNonNullString("title", displayName),
            executionState = b.optNonNullString("execution_state", ""),
            avatarShape = b.optNonNullString("avatar_shape", "circle"),
            avatarColor = b.optNonNullString("avatar_color", "#007AFF"),
            preferredModel = b.optNonNullString("preferred_model", ""),
            thinkingLevel = b.optNonNullString("thinking_level", ""),
            approvalMode = b.optNonNullString("approval_mode", "ask"),
            toolsetPreset = b.optNonNullString("toolset_preset", ""),
            companionChatId = b.optLong("companion_chat_id").takeIf { it > 0L },
            boundNodeId = b.optNonNullString("bound_node_id").ifBlank { codingItem?.boundNodeId },
            memoryEntriesCount = b.optInt("memory_entries_count", 0),
            skillsCount = b.optInt("skills_count", 0),
            isFabushiBot = isFabushi,
            codingAgent = codingItem,
        )
    }

    private fun parseServerComputers(resp: JSONObject): List<ComputerNodeItem> {
        val arr = resp.optJSONArray("computers") ?: return emptyList()
        return buildList {
            for (i in 0 until arr.length()) {
                arr.optJSONObject(i)?.let(::parseServerComputerObject)?.let(::add)
            }
        }
    }

    internal fun parseServerComputerObject(c: JSONObject): ComputerNodeItem? {
        val nodeId = c.optNonNullString("node_id").ifBlank { c.optNonNullString("id") }.ifBlank { return null }
        val kindRaw = c.optNonNullString("kind", "").lowercase()
        val kind = when {
            kindRaw.contains("sandbox") -> ComputerNodeKind.CLOUD_SANDBOX
            kindRaw.contains("local") -> ComputerNodeKind.LOCAL_DEVICE
            else -> ComputerNodeKind.CLOUD_SERVER
        }
        val boundBots = buildList {
            val arr = c.optJSONArray("bound_bot_ids")
            if (arr != null) for (i in 0 until arr.length()) arr.optLong(i).takeIf { it > 0L }?.let(::add)
        }
        val resolution = c.optNonNullString("resolution", "")
        return ComputerNodeItem(
            id = nodeId,
            name = c.optNonNullString("display_name").ifBlank { nodeId },
            kind = kind,
            osInfo = c.optNonNullString("os_name", ""),
            online = c.optBoolean("online", false),
            cpuLoadPercent = 0,
            memorySummary = resolution,
            leaseOwner = null,
            humanTakeoverActive = false,
            host = c.optNonNullString("host").ifBlank { c.optNonNullString("ip_address").ifBlank { null } },
            sshPort = c.optInt("ssh_port", 0).takeIf { it > 0 },
            sshUsername = c.optNonNullString("ssh_username").ifBlank { null },
            authMethod = c.optNonNullString("auth_method").ifBlank { null },
            hasSavedCredential = c.optBoolean("has_saved_credential", false),
            resolution = resolution,
            runPolicy = c.optNonNullString("run_policy", "ask_every_time"),
            isCurrentDevice = c.optBoolean("is_current_device", false),
            deviceId = c.optNonNullString("device_id").ifBlank { null },
            tierBadge = c.optNonNullString("tier_badge", ""),
            boundBotIds = boundBots,
            latestFrameDataUrl = c.optNonNullString("latest_frame_data_url").ifBlank { null },
        )
    }

    private fun mergeComputerNodes(
        localNodesResp: JSONObject?,
        serverNodes: List<ComputerNodeItem>?,
        localMikaOnline: Boolean,
    ): List<ComputerNodeItem> {
        val map = LinkedHashMap<String, ComputerNodeItem>()
        // Contract #16: read `platform` & nested `viewport.{width,height}` from computer.listNodes
        localNodesResp?.optJSONArray("nodes")?.let { arr ->
            for (i in 0 until arr.length()) {
                val n = arr.optJSONObject(i) ?: continue
                val id = n.optNonNullString("nodeId").ifBlank { continue }
                val vp = n.optJSONObject("viewport")
                val res = if (vp != null) "${vp.optInt("width", 0)}x${vp.optInt("height", 0)}" else ""
                val kind = when (n.optNonNullString("kind")) {
                    "cloud_server" -> ComputerNodeKind.CLOUD_SERVER
                    "cloud_sandbox" -> ComputerNodeKind.CLOUD_SANDBOX
                    else -> ComputerNodeKind.LOCAL_DEVICE
                }
                val isOnline = if (id == "local-android-mika") localMikaOnline else n.optNonNullString("status") == "online"
                map[id] = ComputerNodeItem(
                    id = id,
                    name = n.optNonNullString("name", id),
                    kind = kind,
                    osInfo = n.optNonNullString("platform", "android"),
                    online = isOnline,
                    cpuLoadPercent = 0,
                    memorySummary = res,
                    leaseOwner = n.optNonNullString("leaseHolder").ifBlank { null },
                    leaseGeneration = n.optLong("leaseGeneration", 0L),
                    resolution = res,
                    isCurrentDevice = id == "local-android-mika",
                )
            }
        }
        serverNodes?.forEach { srv ->
            map[srv.id] = srv
        }
        return map.values.toList()
    }

    private fun parseRemoteSessions(resp: JSONObject): List<RemoteSessionItem> {
        val arr = resp.optJSONArray("sessions") ?: return emptyList()
        return buildList {
            for (i in 0 until arr.length()) {
                val s = arr.optJSONObject(i) ?: continue
                val sid = s.optNonNullString("session_id").ifBlank { s.optNonNullString("id") }
                if (sid.isBlank()) continue
                add(
                    RemoteSessionItem(
                        sessionId = sid,
                        targetNodeId = s.optNonNullString("target_node_id", ""),
                        controllerKind = s.optNonNullString("controller_kind", ""),
                        controllerName = s.optNonNullString("controller_name", ""),
                        relayMode = s.optNonNullString("relay_mode", s.optNonNullString("mode", "")),
                        rttMs = if (s.has("rtt_ms") && !s.isNull("rtt_ms")) s.optInt("rtt_ms") else null,
                        fps = if (s.has("fps") && !s.isNull("fps")) s.optInt("fps") else null,
                        activeTool = s.optNonNullString("active_tool").ifBlank { null },
                    ),
                )
            }
        }
    }

    private fun parseMcpGrants(resp: JSONObject): List<RemoteMcpGrantItem> {
        val arr = resp.optJSONArray("grants") ?: return emptyList()
        return buildList {
            for (i in 0 until arr.length()) {
                val g = arr.optJSONObject(i) ?: continue
                val scopes = buildList {
                    val sArr = g.optJSONArray("scopes")
                    if (sArr != null) for (j in 0 until sArr.length()) add(sArr.optNonNullString(j))
                }
                val nodes = buildList {
                    val nArr = g.optJSONArray("allowed_node_ids")
                    if (nArr != null) for (j in 0 until nArr.length()) add(nArr.optNonNullString(j))
                }
                add(
                    RemoteMcpGrantItem(
                        familyId = g.optNonNullString("family_id"),
                        clientName = g.optNonNullString("client_name"),
                        scopes = scopes,
                        allowedNodeIds = nodes,
                    ),
                )
            }
        }
    }

    private fun applyMagpieProvidersResponse(current: MagpieGatewayUiState, resp: JSONObject): MagpieGatewayUiState {
        val liveReady = resp.optBoolean("liveReady", false)
        val arr = resp.optJSONArray("providers")
        val byPreset = HashMap<String, JSONObject>()
        var totalReqs = 0L
        if (arr != null) {
            for (i in 0 until arr.length()) {
                val p = arr.optJSONObject(i) ?: continue
                val pid = p.optNonNullString("presetId").ifBlank { p.optNonNullString("id") }
                if (pid.isNotBlank()) byPreset[pid] = p
                totalReqs += p.optLong("totalRequests", 0L)
            }
        }
        val updatedPresets = current.presets.map { preset ->
            val p = byPreset[preset.id]
            if (p != null) {
                val configured = p.optBoolean("configuredForLive", false) || p.optBoolean("hasApiKey", false)
                val failures = p.optInt("consecutiveFailures", 0)
                val enabled = p.optBoolean("enabled", true)
                preset.copy(
                    configured = configured,
                    healthy = configured && enabled && failures == 0,
                )
            } else {
                preset.copy(configured = false, healthy = false)
            }
        }
        val stratWire = resp.optNonNullString("routingStrategy")
        val strat = MagpieRoutingStrategy.entries.firstOrNull { it.wire.equals(stratWire, ignoreCase = true) } ?: current.strategy
        return current.copy(
            strategy = strat,
            liveReady = liveReady,
            presets = updatedPresets,
            totalRequests = totalReqs,
        )
    }

    private fun parseDharmaCountries(resp: JSONObject?): List<CountryOfferingNode> {
        val arr = resp?.optJSONArray("countries") ?: return emptyList()
        return buildList {
            for (i in 0 until arr.length()) {
                val c = arr.optJSONObject(i) ?: continue
                val code = c.optNonNullString("code").ifBlank { continue }
                val nameZh = c.optNonNullString("name_zh").ifBlank { c.optNonNullString("name_en", code) }
                val nameEn = c.optNonNullString("name_en", code)
                val lat = c.optDouble("lat", 0.0)
                val lon = if (c.has("lon")) c.optDouble("lon", 0.0) else c.optDouble("lng", 0.0)
                add(
                    CountryOfferingNode(
                        code = code,
                        nameZh = nameZh,
                        nameEn = nameEn,
                        region = c.optNonNullString("region", c.optNonNullString("continent", "")),
                        latitude = lat,
                        longitude = lon,
                        offeringsCount = c.optLong("offerings_count", 0L),
                        lampLitByMe = c.optBoolean("lamp_lit_by_me", false),
                    ),
                )
            }
        }
    }

    private fun parseWalletCollectibles(resp: JSONObject?): List<TonCollectibleItem> {
        val arr = resp?.optJSONArray("collectibles") ?: return emptyList()
        return buildList {
            for (i in 0 until arr.length()) {
                val item = arr.optJSONObject(i) ?: continue
                val meta = item.optJSONObject("metadata")
                val id = item.optNonNullString("itemId").ifBlank { item.optNonNullString("id") }
                val name = item.optNonNullString("title").ifBlank { item.optNonNullString("name") }
                val collection = item.optNonNullString("collectionName").ifBlank { item.optNonNullString("collection") }
                val kind = item.optNonNullString("kind").lowercase()
                val rarity = item.optNonNullString("rarity")
                val starsValue = item.optLong("starsValue", 0L)
                val floorTon = when {
                    item.has("floorTon") && !item.isNull("floorTon") -> item.optDouble("floorTon", 0.0)
                    meta != null && meta.has("floorTon") -> meta.optDouble("floorTon", 0.0)
                    starsValue > 0L -> starsValue.toDouble() / 100.0
                    else -> 0.0
                }
                val isGift = item.optBoolean("isGift", kind == "gift" || kind.contains("gift"))
                val badgeEmoji = item.optNonNullString("badgeEmoji")
                    .ifBlank { meta?.optNonNullString("badgeEmoji").orEmpty() }
                    .ifBlank {
                        when {
                            isGift -> "🎁"
                            rarity.equals("legendary", ignoreCase = true) -> "👑"
                            else -> "💎"
                        }
                    }
                add(
                    TonCollectibleItem(
                        id = id,
                        name = name,
                        collection = collection,
                        floorTon = floorTon,
                        badgeEmoji = badgeEmoji,
                        isGift = isGift,
                    ),
                )
            }
        }
    }

    internal fun parseHermesStudioState(
        current: HermesStudioUiState,
        sessionsResp: JSONObject?,
        memoryResp: JSONObject?,
        skillsResp: JSONObject?,
        cronResp: JSONObject?,
        kanbanResp: JSONObject?,
        approvalsResp: JSONObject?,
    ): HermesStudioUiState {
        val sessions = sessionsResp?.optJSONArray("sessions")?.let { arr ->
            buildList {
                for (i in 0 until arr.length()) {
                    val s = arr.optJSONObject(i) ?: continue
                    add(
                        HermesSessionItem(
                            id = s.optNonNullString("sessionId").ifBlank { s.optNonNullString("id") },
                            title = s.optNonNullString("title"),
                            provider = s.optNonNullString("providerId").ifBlank { s.optNonNullString("provider") },
                            model = s.optNonNullString("model"),
                            parentBranchId = s.optNonNullString("parentSessionId")
                                .ifBlank { s.optNonNullString("parentBranchId") }
                                .ifBlank { null },
                            turnCount = if (s.has("messageCount")) s.optInt("messageCount", 0) else s.optInt("turnCount", 0),
                            lastOutput = s.optNonNullString("lastOutput", ""),
                        ),
                    )
                }
            }
        } ?: current.sessions

        var soul = current.soulMarkdown
        val memBlocks = mutableListOf<Pair<String, String>>()
        memoryResp?.optJSONArray("entries")?.let { arr ->
            for (i in 0 until arr.length()) {
                val e = arr.optJSONObject(i) ?: continue
                val target = e.optNonNullString("target")
                val content = e.optNonNullString("content")
                if (target == "SOUL.md") soul = content
                else if (target.isNotBlank() && content.isNotBlank()) memBlocks += target to content
            }
        }

        val skills = skillsResp?.optJSONArray("skills")?.let { arr ->
            buildList {
                for (i in 0 until arr.length()) {
                    val s = arr.optJSONObject(i) ?: continue
                    val name = s.optNonNullString("name")
                    val id = s.optNonNullString("id").ifBlank { name }
                    add(
                        HermesSkillItem(
                            id = id,
                            name = name.ifBlank { id },
                            category = s.optNonNullString("category").ifBlank { "skill" },
                            description = s.optNonNullString("description"),
                            enabled = s.optBoolean("enabled", true),
                        ),
                    )
                }
            }
        } ?: current.skills

        val crons = cronResp?.optJSONArray("jobs")?.let { arr ->
            buildList {
                for (i in 0 until arr.length()) {
                    val j = arr.optJSONObject(i) ?: continue
                    val prompt = j.optNonNullString("prompt")
                    val lastRunMs = if (j.has("lastRunMs") && !j.isNull("lastRunMs")) j.optLong("lastRunMs", 0L) else 0L
                    val lastStatus = j.optNonNullString("lastRunStatus").ifBlank {
                        if (lastRunMs > 0L) nowTimeLabel(lastRunMs) else ""
                    }
                    add(
                        HermesCronItem(
                            id = j.optNonNullString("jobId").ifBlank { j.optNonNullString("id") },
                            title = j.optNonNullString("title").ifBlank { prompt.take(40) },
                            scheduleExpr = j.optNonNullString("schedule").ifBlank { j.optNonNullString("scheduleExpr") },
                            prompt = prompt,
                            enabled = j.optBoolean("enabled", true),
                            lastRunStatus = lastStatus,
                        ),
                    )
                }
            }
        } ?: current.cronJobs

        val kanban = kanbanResp?.optJSONArray("cards")?.let { arr ->
            buildList {
                for (i in 0 until arr.length()) {
                    val c = arr.optJSONObject(i) ?: continue
                    val colRaw = c.optNonNullString("columnId").ifBlank { c.optNonNullString("column") }
                    val col = parseKanbanColumn(colRaw)
                    add(
                        HermesKanbanCard(
                            id = c.optNonNullString("cardId").ifBlank { c.optNonNullString("id") },
                            title = c.optNonNullString("title"),
                            assigneeAgent = c.optNonNullString("assignee").ifBlank { c.optNonNullString("assigneeAgent") },
                            column = col,
                        ),
                    )
                }
            }
        } ?: current.kanbanCards

        val approvals = approvalsResp?.optJSONArray("approvals")?.let { arr ->
            buildList {
                for (i in 0 until arr.length()) {
                    val a = arr.optJSONObject(i) ?: continue
                    val riskRaw = a.optNonNullString("riskLevel").ifBlank { a.optNonNullString("riskTier") }
                    val tier = parseActionRiskTier(riskRaw)
                    add(
                        HermesApprovalRequest(
                            id = a.optNonNullString("approvalId").ifBlank { a.optNonNullString("id") },
                            toolName = a.optNonNullString("toolName"),
                            summary = a.optNonNullString("commandSummary").ifBlank { a.optNonNullString("summary") },
                            riskTier = tier,
                            status = a.optNonNullString("status", "pending"),
                        ),
                    )
                }
            }
        } ?: current.approvals

        return current.copy(
            sessions = sessions,
            activeSessionId = current.activeSessionId.takeIf { id -> id.isNotBlank() && sessions.any { it.id == id } }
                ?: sessions.firstOrNull()?.id.orEmpty(),
            soulMarkdown = soul,
            memoryBlocks = if (memoryResp != null) memBlocks else current.memoryBlocks,
            skills = skills,
            cronJobs = crons,
            kanbanCards = kanban,
            approvals = approvals,
        )
    }

    private fun KanbanColumn.toRustColumnId(): String = when (this) {
        KanbanColumn.BACKLOG -> "todo"
        KanbanColumn.IN_PROGRESS -> "in_progress"
        KanbanColumn.REVIEW -> "review"
        KanbanColumn.DONE -> "done"
    }

    private fun parseKanbanColumn(raw: String): KanbanColumn = when (raw.trim().lowercase()) {
        "todo", "backlog" -> KanbanColumn.BACKLOG
        "in_progress", "inprogress" -> KanbanColumn.IN_PROGRESS
        "review" -> KanbanColumn.REVIEW
        "done", "completed" -> KanbanColumn.DONE
        else -> KanbanColumn.entries.firstOrNull { it.name.equals(raw, ignoreCase = true) } ?: KanbanColumn.BACKLOG
    }

    private fun parseActionRiskTier(raw: String): ActionRiskTier = when (raw.trim().lowercase()) {
        "r0", "read", "readonly", "none" -> ActionRiskTier.R0_READ
        "r1", "low" -> ActionRiskTier.R1_LOW
        "r2", "medium", "moderate" -> ActionRiskTier.R2_MEDIUM
        "r3", "high" -> ActionRiskTier.R3_HIGH
        "r4", "critical", "critical_irreversible", "destructive" -> ActionRiskTier.R4_CRITICAL
        else -> ActionRiskTier.entries.firstOrNull {
            it.code.equals(raw, ignoreCase = true) || it.name.equals(raw, ignoreCase = true)
        } ?: ActionRiskTier.R3_HIGH
    }

    internal fun parseMikaConfirmPayload(raw: String, fallbackThreadId: String): Pair<String, String?> {
        val sep = raw.indexOf(':')
        if (sep < 0) {
            val decodedThread = decodeConfirmSegment(raw).ifBlank { fallbackThreadId }
            return decodedThread to null
        }
        val encThread = raw.substring(0, sep)
        val encAsk = raw.substring(sep + 1)
        val decodedThread = decodeConfirmSegment(encThread).ifBlank { fallbackThreadId }
        val decodedAsk = decodeConfirmSegment(encAsk).ifBlank { null }
        return decodedThread to decodedAsk
    }

    private fun decodeConfirmSegment(encoded: String): String =
        runCatching { URLDecoder.decode(encoded, "UTF-8") }.getOrDefault(encoded)

    private fun appendMessageToChat(conversationId: String, message: TelegramMessageItem, clearDraft: Boolean) {
        _state.update { current ->
            val existing = current.messagesByChat[conversationId].orEmpty()
            val match = existing.firstOrNull {
                it.id == message.id || (message.clientNonce != null && it.clientNonce == message.clientNonce)
            }
            val resolvedMessage = if (match?.deliveryState == "read") {
                message.copy(deliveryState = "read")
            } else message
            val list = existing.filterNot {
                it.id == resolvedMessage.id || (resolvedMessage.clientNonce != null && it.clientNonce == resolvedMessage.clientNonce)
            } + resolvedMessage
            val updatedChats = current.chats.map { chat ->
                if (chat.id == conversationId) {
                    chat.copy(
                        preview = resolvedMessage.text.lineSequence().firstOrNull().orEmpty().take(72),
                        time = resolvedMessage.time,
                        lastMessageAtMs = maxOf(chat.lastMessageAtMs, resolvedMessage.timestampMs),
                        lastMessageOutgoing = resolvedMessage.outgoing,
                        lastDeliveryState = if (chat.lastDeliveryState == "read" && match?.id == resolvedMessage.id) "read" else resolvedMessage.deliveryState,
                        draftText = if (clearDraft) "" else chat.draftText,
                    )
                } else chat
            }
            current.copy(
                messagesByChat = current.messagesByChat + (conversationId to list),
                chats = sortTelegramChats(updatedChats),
                replyTarget = if (clearDraft) null else current.replyTarget,
            )
        }
    }

    private fun updateMessageInChat(
        conversationId: String,
        messageId: String,
        transform: (TelegramMessageItem) -> TelegramMessageItem,
    ) {
        _state.update { current ->
            val updated = current.messagesByChat[conversationId].orEmpty().map {
                if (it.id == messageId) transform(it) else it
            }
            current.copy(messagesByChat = current.messagesByChat + (conversationId to updated))
        }
    }

    private fun sendProtocolV2Command(command: JSONObject): Result<JSONObject> {
        val reqId = "tg-cmd-${UUID.randomUUID()}"
        val envelope = JSONObject()
            .put("protocolVersion", 2)
            .put(
                "context",
                JSONObject()
                    .put("requestId", reqId)
                    .put("deviceId", "android:native")
                    .put("actorId", actorId)
                    .put("sessionId", "account-session:android-native")
                    .put("sentAtMs", System.currentTimeMillis()),
            )
            .put("command", command)
        return hostRequestResult("feature.messaging.execute", JSONObject().put("requestId", reqId).put("envelope", envelope))
    }

    private fun serializeEntities(entities: List<MessageEntity>): JSONArray {
        val arr = JSONArray()
        entities.forEach { e ->
            arr.put(
                JSONObject()
                    .put("type", e.type.wire)
                    .put("offset", e.offset)
                    .put("length", e.length)
                    .apply {
                        if (e.url != null) put("url", e.url)
                        if (e.language != null) put("language", e.language)
                    },
            )
        }
        return arr
    }

    private fun sortTelegramChats(chats: List<TelegramChatItem>): List<TelegramChatItem> =
        chats.sortedWith(
            compareBy<TelegramChatItem> { it.isArchived }
                .thenByDescending { it.isPinned }
                .thenByDescending { it.lastMessageAtMs }
                .thenBy { it.id },
        )

    private fun nowTimeLabel(epochMs: Long = System.currentTimeMillis()): String =
        TelegramLocalTimeFormatter.formatLocalTime(epochMs)

    override fun onCleared() {
        generationFence.advance()
        disconnectServerWebSocket()
        typingTimeoutJobs.values.forEach { it.cancel() }
        typingTimeoutJobs.clear()
        mikaEventsJob?.cancel()
        recordingTimerJob?.cancel()
        audioProgressJob?.cancel()
        callTimerJob?.cancel()
        if (voiceRecorder.isRecording) voiceRecorder.cancel()
        stopAudioPlayback()
        runCatching { wsHttpClient.dispatcher.executorService.shutdown() }
        runCatching { wsHttpClient.connectionPool.evictAll() }
        runCatching { mahayanaHost?.close() }
        super.onCleared()
    }

    /**
     * Zero-fake-data initial state: contains strictly static composer picker palettes
     * (slash hints, emoji packs) and empty collections until hydrated from real
     * `gpui_telegram_server` or local SQLite persistence.
     */
    private fun buildCleanInitialState(): TelegramUiState {
        val stickerPacks = listOf(
            StickerPackModel("pack-dharma", "莲华禅心贴纸", true, listOf("🪷", "🙏", "☸️", "🪔", "✨", "🕊️", "🌕", "🍵", "🧘", "📿", "🏔️", "🌈")),
            StickerPackModel("pack-cyber", "极客 AI 助手", true, listOf("🤖", "🦀", "⚡", "🚀", "💎", "🛡️", "🧠", "🛰️", "🔥", "🎯", "🎉", "👀")),
            StickerPackModel("pack-animals", "表情包", true, listOf("🐶", "🐱", "🐼", "🦊", "🦄", "🐧", "🦉", "🐬", "🦋", "🐢", "🦁", "🐨")),
        )
        val slashCommands = listOf(
            SlashCommandSuggestion("/mika", "调用 Mika 本机手机智能体自动执行跨 App 任务", "@MikaPhoneBot"),
            SlashCommandSuggestion("/newbot", "打开 BotFather 创建机器人或 Coding Agent Bot", "@BotFather"),
            SlashCommandSuggestion("/moa", "启动 Hermes Mixture-of-Agents 多模型协同推理", "@HermesStudioBot"),
            SlashCommandSuggestion("/dharma", "打开全球法布施小程序", "@GlobalDharmaBot"),
            SlashCommandSuggestion("/wallet", "打开自托管 TON 钱包"),
            SlashCommandSuggestion("/hermes", "打开 Hermes 智能体工作室"),
            SlashCommandSuggestion("/magpie", "打开 Magpie AI 网关控制台"),
            SlashCommandSuggestion("/computer", "打开多设备云电脑与 UU-Remote 控制台"),
        )
        return TelegramUiState(
            stickerPacks = stickerPacks,
            slashCommands = slashCommands,
        )
    }
}
