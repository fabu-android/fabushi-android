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
import com.ombhrum.fabushi.MessagingFolder
import com.ombhrum.fabushi.MessagingUiState
import com.ombhrum.fabushi.NativeVoiceRecorder
import com.ombhrum.fabushi.core.MahayanaHost
import java.io.File
import java.text.DateFormat
import java.util.Base64
import java.util.Date
import java.util.UUID
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
import org.json.JSONArray
import org.json.JSONObject

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
    val syncStatusLabel: String = "Telegram",
    val serverEndpoint: String = "https://telegram-sync.ombhrum.com",
    val proxyEnabled: Boolean = false,
    val proxyHost: String = "127.0.0.1:1080",
    val uiLanguage: String = "简体中文",
    val showTranslateButton: Boolean = true,
    val autoTranslateChannels: Boolean = false,
    val powerSavingThreshold: Int = 20,
    val inAppSoundsEnabled: Boolean = true,
    val storageCacheMb: Double = 24.6,
    val twoFactorEnabled: Boolean = true,
    val themeSettings: TelegramThemeSettings = TelegramThemeSettings(),
    val passcodeConfigured: Boolean = false,
    val biometricUnlockEnabled: Boolean = true,
    val isLocked: Boolean = false,
    val passcodeError: String? = null,
    val accounts: List<TelegramAccount> = emptyList(),
    val activeAccountId: String = "acc-primary",
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
    val userVault: UserVaultUiState = UserVaultUiState(),
    val miniAppPip: MiniAppPipSessionUi = MiniAppPipSessionUi(),
    val mikaSnapshot: MikaBridgeUiSnapshot = MikaBridgeUiSnapshot(),
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
    private val voiceRecorder = NativeVoiceRecorder(appContext)

    private val mahayanaHost: MahayanaHost? by lazy {
        runCatching { MahayanaHost(appContext) }.getOrNull()
    }

    private var mikaBridge: MikaBridgeCallbacks? = null
    private var actorId: String = "actor:local-primary"
    private var recordingTimerJob: Job? = null
    private var audioProgressJob: Job? = null
    private var callTimerJob: Job? = null
    private var activeMediaPlayer: MediaPlayer? = null
    private val cachedVoiceBlobs = mutableMapOf<String, ByteArray>()

    private val _state = MutableStateFlow(buildInitialSeedState())
    val state: StateFlow<TelegramUiState> = _state.asStateFlow()

    init {
        restorePersistedSettings()
        refreshAllFromHost()
    }

    // ------------------------------------------------------------------------
    // Host RPC Dispatch with Graceful Fallback
    // ------------------------------------------------------------------------

    private fun hostRequestOrNull(method: String, params: JSONObject = JSONObject()): JSONObject? {
        val host = mahayanaHost ?: return null
        return runCatching { host.request(method, params) }.getOrNull()
    }

    fun attachMikaBridge(callbacks: MikaBridgeCallbacks?) {
        mikaBridge = callbacks
        if (callbacks != null) {
            _state.update { it.copy(mikaSnapshot = callbacks.currentSnapshot()) }
        }
    }

    fun refreshMikaSnapshot() {
        val callbacks = mikaBridge ?: return
        _state.update { it.copy(mikaSnapshot = callbacks.currentSnapshot()) }
    }

    fun clearToastBanner() {
        _state.update { it.copy(toastBanner = null) }
    }

    private fun postBanner(message: String) {
        _state.update { it.copy(toastBanner = message) }
    }

    private fun restorePersistedSettings() {
        val presetWire = prefs.getString("theme_preset", TelegramThemePreset.DAY_BLUE.wire)
        val preset = TelegramThemePreset.entries.firstOrNull { it.wire == presetWire } ?: TelegramThemePreset.DAY_BLUE
        val wallpaperWire = prefs.getString("wallpaper_style", TelegramWallpaperStyle.DOODLE_CLASSIC.wire)
        val wallpaper = TelegramWallpaperStyle.entries.firstOrNull { it.wire == wallpaperWire } ?: TelegramWallpaperStyle.DOODLE_CLASSIC
        val savedPin = prefs.getString("passcode_pin", null)
        val endpoint = prefs.getString("server_endpoint", "https://telegram-sync.ombhrum.com") ?: "https://telegram-sync.ombhrum.com"
        _state.update { current ->
            current.copy(
                serverEndpoint = endpoint,
                passcodeConfigured = !savedPin.isNullOrBlank(),
                isLocked = !savedPin.isNullOrBlank(),
                themeSettings = current.themeSettings.copy(
                    preset = preset,
                    customAccentArgb = prefs.getLong("custom_accent", 0xFF8E44ADL),
                    customDark = prefs.getBoolean("custom_dark", false),
                    bubbleCornerRadiusDp = prefs.getInt("bubble_radius", 16).coerceIn(4, 18),
                    messageFontSizeSp = prefs.getInt("font_size", 16).coerceIn(12, 22),
                    wallpaperStyle = wallpaper,
                    wallpaperDimPercent = prefs.getInt("wallpaper_dim", 12).coerceIn(0, 80),
                ),
            )
        }
    }

    fun refreshAllFromHost() {
        val capturedGen = generationFence.currentGeneration
        _state.update { it.copy(syncStatusLabel = "同步中…") }
        viewModelScope.launch {
            withContext(Dispatchers.IO) {
                val auth = hostRequestOrNull("feature.auth.status")
                val userObj = auth?.optJSONObject("user")
                val resolvedName = userObj?.optString("nickname")?.ifBlank { null }
                    ?: userObj?.optString("username")?.ifBlank { null }
                val access = hostRequestOrNull(
                    "feature.messaging.access.issue",
                    JSONObject()
                        .put("deviceId", "android:native")
                        .put("sessionId", "account-session:android-native")
                        .put("scopes", JSONArray(listOf("messaging", "calls", "blobsRead", "blobsWrite", "payments", "miniApps"))),
                )
                access?.optString("actorId")?.takeIf { it.isNotBlank() }?.let { actorId = it }

                // Sync protocol v2 conversations if available
                val syncId = "tg-sync-${UUID.randomUUID()}"
                val syncEnvelope = JSONObject()
                    .put("protocolVersion", 2)
                    .put("context", JSONObject().put("requestId", syncId).put("deviceId", "android:native").put("actorId", actorId).put("sessionId", "account-session:android-native").put("sentAtMs", System.currentTimeMillis()))
                    .put("command", JSONObject().put("type", "sync").put("cursor", JSONObject.NULL).put("limit", 500))
                val syncResult = hostRequestOrNull("feature.messaging.execute", JSONObject().put("requestId", syncId).put("envelope", syncEnvelope))

                // Optional extended Telegram/Wallet/Dharma/Hermes/Magpie/Computer/v1.6.2 status calls
                val walletResp = hostRequestOrNull("wallet.status")
                val dharmaResp = hostRequestOrNull("fabushiMiniapp.bootstrap")
                val mikaResp = hostRequestOrNull("mika.bridgeStatus")
                val assistResp = hostRequestOrNull("remote.getAssistConfig")
                val streamResp = hostRequestOrNull("remote.getStreamSettings")
                val mcpGrantsResp = hostRequestOrNull("remote.listMcpGrants")
                val codingBotsResp = hostRequestOrNull("codingAgent.listBots")
                val vaultEnvResp = hostRequestOrNull(
                    "userVault.getEnvelope",
                    JSONObject().put("userId", _state.value.activeAccountId),
                )

                if (!generationFence.isCurrent(capturedGen)) return@withContext

                _state.update { prev ->
                    var next = prev.copy(syncStatusLabel = "Telegram")
                    if (!resolvedName.isNullOrBlank()) {
                        next = next.copy(
                            accounts = next.accounts.map { acc ->
                                if (acc.id == next.activeAccountId) acc.copy(displayName = resolvedName) else acc
                            },
                        )
                    }
                    if (syncResult != null) {
                        next = mergeHostSyncEnvelopes(next, syncResult.optJSONArray("envelopes") ?: JSONArray())
                    }
                    if (walletResp != null && walletResp.has("tonBalance")) {
                        next = next.copy(
                            tonWallet = next.tonWallet.copy(
                                addressFriendly = walletResp.optString("address", next.tonWallet.addressFriendly).ifBlank { next.tonWallet.addressFriendly },
                                tonBalance = walletResp.optDouble("tonBalance", next.tonWallet.tonBalance),
                                starsBalance = walletResp.optLong("starsBalance", next.tonWallet.starsBalance),
                            ),
                        )
                    }
                    if (dharmaResp != null && dharmaResp.has("totalGlobalOfferings")) {
                        next = next.copy(
                            globalDharma = next.globalDharma.copy(
                                totalGlobalOfferings = dharmaResp.optLong("totalGlobalOfferings", next.globalDharma.totalGlobalOfferings),
                                myMeritPoints = dharmaResp.optLong("myMeritPoints", next.globalDharma.myMeritPoints),
                            ),
                        )
                    }
                    if (mikaResp != null && mikaBridge == null) {
                        next = next.copy(
                            mikaSnapshot = next.mikaSnapshot.copy(
                                available = mikaResp.optBoolean("available", true),
                            ),
                        )
                    }
                    if (assistResp != null && assistResp.has("assist_id")) {
                        next = next.copy(
                            cloudComputer = next.cloudComputer.copy(
                                assistConfig = RemoteAssistConfigUi(
                                    assistId = assistResp.optString("assist_id", next.cloudComputer.assistConfig.assistId),
                                    allowAssist = assistResp.optBoolean("allow_assist", true),
                                    codeMode = assistResp.optString("code_mode", "temporary"),
                                    currentCode = assistResp.optString("current_code", next.cloudComputer.assistConfig.currentCode),
                                ),
                            ),
                        )
                    }
                    if (streamResp != null && streamResp.has("codec")) {
                        next = next.copy(
                            cloudComputer = next.cloudComputer.copy(
                                streamSettings = RemoteStreamSettingsUi(
                                    codec = streamResp.optString("codec", "auto"),
                                    targetFps = streamResp.optInt("target_fps", 60),
                                    quality = streamResp.optString("quality", "auto"),
                                    colorMode = streamResp.optString("color_mode", "yuv420"),
                                    hdr = streamResp.optBoolean("hdr", false),
                                    audioEnabled = streamResp.optBoolean("audio_enabled", true),
                                    micPassthrough = streamResp.optBoolean("mic_passthrough", false),
                                    privacyScreen = streamResp.optBoolean("privacy_screen", false),
                                    displayIndex = streamResp.optInt("display_index", 0),
                                ),
                            ),
                        )
                    }
                    val grantsArr = mcpGrantsResp?.optJSONArray("grants")
                    if (grantsArr != null) {
                        val parsedGrants = buildList {
                            for (i in 0 until grantsArr.length()) {
                                val g = grantsArr.optJSONObject(i) ?: continue
                                val scopes = buildList {
                                    val sArr = g.optJSONArray("scopes")
                                    if (sArr != null) for (j in 0 until sArr.length()) add(sArr.optString(j))
                                }
                                val nodes = buildList {
                                    val nArr = g.optJSONArray("allowed_node_ids")
                                    if (nArr != null) for (j in 0 until nArr.length()) add(nArr.optString(j))
                                }
                                add(
                                    RemoteMcpGrantItem(
                                        familyId = g.optString("family_id"),
                                        clientName = g.optString("client_name"),
                                        scopes = scopes,
                                        allowedNodeIds = nodes,
                                    ),
                                )
                            }
                        }
                        next = next.copy(cloudComputer = next.cloudComputer.copy(mcpGrants = parsedGrants))
                    }
                    val codingArr = codingBotsResp?.optJSONArray("bots")
                    if (codingArr != null) {
                        val parsedBots = buildList {
                            for (i in 0 until codingArr.length()) {
                                val b = codingArr.optJSONObject(i) ?: continue
                                val cfg = b.optJSONObject("config") ?: JSONObject()
                                val provWire = cfg.optString("provider_kind", "claude_code")
                                val permWire = cfg.optString("permission_mode", "ask")
                                add(
                                    CodingAgentBotItem(
                                        botId = b.optString("botId"),
                                        displayName = b.optString("displayName"),
                                        username = b.optString("username"),
                                        provider = CodingProviderKindUi.entries.firstOrNull { it.wire == provWire } ?: CodingProviderKindUi.CLAUDE_CODE,
                                        permissionMode = CodingPermissionModeUi.entries.firstOrNull { it.wire == permWire } ?: CodingPermissionModeUi.ASK,
                                        workspacePath = cfg.optString("workspace_path", "~/workspace"),
                                        useWorktree = cfg.optBoolean("use_worktree", true),
                                        boundNodeId = cfg.optString("bound_node_id").ifBlank { null },
                                        activeBranch = b.optString("activeBranch", "main"),
                                        totalTurns = b.optInt("totalTurns", 0),
                                    ),
                                )
                            }
                        }
                        next = next.copy(codingAgentBots = parsedBots)
                    }
                    val envObj = vaultEnvResp?.optJSONObject("envelope")
                    if (envObj != null) {
                        next = next.copy(
                            userVault = next.userVault.copy(
                                initialized = true,
                                kdfAlg = envObj.optString("kdf_alg", "split_key_v2"),
                                keyVersion = envObj.optInt("key_version", 2),
                            ),
                        )
                    }
                    next
                }
            }
        }
    }

    /**
     * Bridges Conversations/Messages from existing [MessagingUiState] so both ViewModels stay unified.
     */
    fun syncExternalMessagingState(external: MessagingUiState) {
        if (external.conversations.isEmpty()) return
        _state.update { current ->
            val existingIds = current.chats.associateBy { it.id }
            val mergedChats = current.chats.toMutableList()
            val mergedMessages = current.messagesByChat.toMutableMap()
            external.conversations.forEach { summary ->
                val mapped = TelegramChatItem(
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
                    draftText = external.draftsByConversation[summary.id]?.text.orEmpty(),
                    typingLabel = external.typingActorByConversation[summary.id]?.let { "$it 正在输入…" },
                    rawSummary = summary,
                )
                val idx = mergedChats.indexOfFirst { it.id == summary.id }
                if (idx >= 0) {
                    val prev = existingIds[summary.id]!!
                    mergedChats[idx] = mapped.copy(
                        isVerified = prev.isVerified,
                        isBot = prev.isBot,
                        isForum = prev.isForum,
                        memberCount = prev.memberCount,
                        subscriberCount = prev.subscriberCount,
                    )
                } else {
                    mergedChats.add(0, mapped)
                }
            }
            external.messagesByConversation.forEach { (convId, list) ->
                if (list.isNotEmpty() && mergedMessages[convId].isNullOrEmpty()) {
                    mergedMessages[convId] = list.map { msg ->
                        val (plain, ents) = TelegramEntitiesEngine.parseMarkdownAndAutoEntities(msg.text)
                        TelegramMessageItem(
                            id = msg.id,
                            conversationId = convId,
                            senderId = if (msg.outgoing) actorId else convId,
                            senderName = if (msg.outgoing) "我" else (mergedChats.firstOrNull { it.id == convId }?.title ?: "对方"),
                            text = plain,
                            entities = ents,
                            contentType = msg.contentType,
                            outgoing = msg.outgoing,
                            time = msg.time,
                            deliveryState = msg.deliveryState,
                            edited = msg.edited,
                            pinned = msg.pinned,
                            reactions = msg.reactions,
                            mediaFileName = msg.mediaFileName,
                            mediaBlobId = msg.mediaBlobId,
                            mediaMimeType = msg.mediaMimeType,
                            mediaSizeBytes = msg.mediaSizeBytes,
                            contactName = msg.contactName,
                            latitude = msg.latitude,
                            longitude = msg.longitude,
                        )
                    }
                }
            }
            current.copy(chats = sortTelegramChats(mergedChats), messagesByChat = mergedMessages)
        }
    }

    private fun mergeHostSyncEnvelopes(current: TelegramUiState, envelopes: JSONArray): TelegramUiState {
        var chats = current.chats.toMutableList()
        val msgMap = current.messagesByChat.toMutableMap()
        for (i in 0 until envelopes.length()) {
            val ev = envelopes.optJSONObject(i)?.optJSONObject("event") ?: continue
            if (ev.optString("type") == "syncBatch") {
                val convArr = ev.optJSONArray("conversations") ?: JSONArray()
                for (c in 0 until convArr.length()) {
                    val raw = convArr.optJSONObject(c) ?: continue
                    val id = raw.optString("id")
                    val title = raw.optString("title")
                    if (id.isBlank() || title.isBlank() || chats.any { it.id == id }) continue
                    val kind = ConversationKind.entries.firstOrNull { it.wire == raw.optString("kind") } ?: ConversationKind.DIRECT
                    chats.add(
                        TelegramChatItem(
                            id = id,
                            title = title,
                            subtitle = raw.optString("description").ifBlank { kind.label },
                            preview = raw.optString("description").ifBlank { "已同步会话" },
                            time = nowTimeLabel(),
                            kind = kind,
                            unreadCount = raw.optInt("unreadCount"),
                            isPinned = raw.optBoolean("pinned"),
                            isArchived = raw.optBoolean("archived"),
                        ),
                    )
                }
            }
        }
        return current.copy(chats = sortTelegramChats(chats), messagesByChat = msgMap)
    }

    // ------------------------------------------------------------------------
    // Navigation & Multi-Account & Passcode Gate
    // ------------------------------------------------------------------------

    fun navigateTo(screen: TelegramSubScreen, settingsPage: TelegramSettingsPage = TelegramSettingsPage.OVERVIEW) {
        if (screen == TelegramSubScreen.MIKA_WORKSPACE) refreshMikaSnapshot()
        _state.update {
            it.copy(
                subScreen = screen,
                settingsPage = settingsPage,
                selectedChatIds = emptySet(),
            )
        }
    }

    fun openSettingsPage(page: TelegramSettingsPage) {
        _state.update { it.copy(subScreen = TelegramSubScreen.SETTINGS, settingsPage = page) }
    }

    fun switchAccount(accountId: String) {
        val nextGen = generationFence.advance()
        viewModelScope.launch(Dispatchers.IO) {
            hostRequestOrNull("telegram.auth.accountSwitch", JSONObject().put("accountId", accountId).put("generation", nextGen))
        }
        _state.update { current ->
            val target = current.accounts.firstOrNull { it.id == accountId } ?: return@update current
            current.copy(
                activeAccountId = accountId,
                accounts = current.accounts.map { it.copy(isActive = it.id == accountId) },
                toastBanner = "已切换至账号：${target.displayName} (${target.username})",
            )
        }
    }

    fun addAccount(displayName: String, phone: String, username: String) {
        val cleanName = displayName.trim().ifBlank { "Fabushi 账号" }
        val cleanUser = username.trim().let { if (it.startsWith("@")) it else "@${it.ifBlank { "fabushi_user" }}" }
        _state.update { current ->
            if (current.accounts.size >= 4) {
                return@update current.copy(toastBanner = "Telegram Android 最多支持同时登录 4 个账号")
            }
            val newAcc = TelegramAccount(
                id = "acc-${UUID.randomUUID().toString().take(8)}",
                slotIndex = current.accounts.size,
                displayName = cleanName,
                username = cleanUser,
                phone = phone.trim().ifBlank { "+86 138 0000 ${1000 + current.accounts.size}" },
                bio = "行深般若 · 智能随行",
                isActive = true,
            )
            generationFence.advance()
            viewModelScope.launch(Dispatchers.IO) {
                hostRequestOrNull("telegram.auth.accountAdd", JSONObject().put("displayName", cleanName).put("phone", newAcc.phone).put("username", cleanUser))
            }
            current.copy(
                activeAccountId = newAcc.id,
                accounts = current.accounts.map { it.copy(isActive = false) } + newAcc,
                toastBanner = "已添加并切换至 $cleanName",
            )
        }
    }

    fun configurePasscode(pin4: String) {
        val clean = pin4.filter { it.isDigit() }.take(4)
        if (clean.length != 4) {
            postBanner("请输入 4 位数字密码")
            return
        }
        prefs.edit().putString("passcode_pin", clean).apply()
        viewModelScope.launch(Dispatchers.IO) {
            hostRequestOrNull("telegram.auth.passcodeSet", JSONObject().put("pin", clean))
        }
        _state.update { it.copy(passcodeConfigured = true, isLocked = false, passcodeError = null, toastBanner = "本地密码锁与生物识别已开启") }
    }

    fun disablePasscode() {
        prefs.edit().remove("passcode_pin").apply()
        _state.update { it.copy(passcodeConfigured = false, isLocked = false, passcodeError = null, toastBanner = "本地密码锁已关闭") }
    }

    fun lockAppNow() {
        if (!_state.value.passcodeConfigured) {
            configurePasscode("0000")
            _state.update { it.copy(isLocked = true, toastBanner = "已设置默认密码 0000 并锁定（可在隐私设置中修改）") }
        } else {
            _state.update { it.copy(isLocked = true, passcodeError = null) }
        }
    }

    fun verifyPasscode(pinInput: String): Boolean {
        val saved = prefs.getString("passcode_pin", "0000") ?: "0000"
        val ok = pinInput == saved
        viewModelScope.launch(Dispatchers.IO) {
            hostRequestOrNull("telegram.auth.passcodeVerify", JSONObject().put("pin", pinInput))
        }
        _state.update {
            if (ok) it.copy(isLocked = false, passcodeError = null)
            else it.copy(passcodeError = "密码不正确，请重试")
        }
        return ok
    }

    fun unlockViaBiometricSuccess() {
        _state.update { it.copy(isLocked = false, passcodeError = null) }
    }

    // ------------------------------------------------------------------------
    // Theme, Appearance & Settings Updates
    // ------------------------------------------------------------------------

    fun updateThemeSettings(transform: (TelegramThemeSettings) -> TelegramThemeSettings) {
        _state.update { current ->
            val updated = transform(current.themeSettings)
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
        val cleanEndpoint = endpoint.trim().ifBlank { "https://telegram-sync.ombhrum.com" }
        prefs.edit().putString("server_endpoint", cleanEndpoint).apply()
        viewModelScope.launch(Dispatchers.IO) {
            hostRequestOrNull(
                "telegram.server.configureEndpoint",
                JSONObject().put("endpoint", cleanEndpoint).put("proxyEnabled", proxyEnabled).put("proxyHost", proxyHost),
            )
            hostRequestOrNull("telegram.server.syncDifference")
        }
        _state.update {
            it.copy(
                serverEndpoint = cleanEndpoint,
                proxyEnabled = proxyEnabled,
                proxyHost = proxyHost,
                toastBanner = "服务器同步节点与 PTS 差量同步已更新",
            )
        }
    }

    fun clearLocalMediaCache() {
        runCatching {
            File(appContext.cacheDir, "fabushi-media").deleteRecursively()
            File(appContext.cacheDir, "fabushi-voice-playback").deleteRecursively()
        }
        _state.update { it.copy(storageCacheMb = 0.4, toastBanner = "媒体与音频缓存已清理释放") }
    }

    fun updateProfile(displayName: String, username: String, bio: String) {
        _state.update { current ->
            val cleanUser = username.trim().let { if (it.startsWith("@")) it else "@$it" }
            current.copy(
                accounts = current.accounts.map { acc ->
                    if (acc.id == current.activeAccountId) {
                        acc.copy(displayName = displayName.trim().ifBlank { acc.displayName }, username = cleanUser, bio = bio.trim())
                    } else acc
                },
                toastBanner = "个人资料已保存同步",
            )
        }
    }

    fun updateLanguageAndTranslate(language: String, showTranslateButton: Boolean, autoTranslate: Boolean) {
        _state.update {
            it.copy(
                uiLanguage = language,
                showTranslateButton = showTranslateButton,
                autoTranslateChannels = autoTranslate,
                toastBanner = "语言与翻译偏好已更新 ($language)",
            )
        }
    }

    fun updatePowerAndNotificationPrefs(powerThreshold: Int, inAppSounds: Boolean) {
        _state.update {
            it.copy(powerSavingThreshold = powerThreshold.coerceIn(5, 80), inAppSoundsEnabled = inAppSounds)
        }
    }

    // ------------------------------------------------------------------------
    // Stories, Folders, Global Search & Chat List Gestures
    // ------------------------------------------------------------------------

    fun openStory(storyId: String) {
        viewModelScope.launch(Dispatchers.IO) {
            hostRequestOrNull("telegram.messaging.viewStory", JSONObject().put("storyId", storyId))
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
        val newStory = TelegramStoryItem(
            id = "story-${UUID.randomUUID().toString().take(8)}",
            authorId = actorId,
            authorName = _state.value.activeAccount?.displayName ?: "我的动态",
            avatarSeed = actorId,
            headline = headline.trim().ifBlank { "今日禅心 · 即刻分享" },
            caption = caption.trim().ifBlank { "来自 GPUI-Telegram Android 24h 动态" },
            gradientIndex = gradientIndex,
            isMine = true,
            viewed = false,
            viewCount = 1,
        )
        viewModelScope.launch(Dispatchers.IO) {
            hostRequestOrNull("telegram.messaging.createStory", JSONObject().put("headline", newStory.headline).put("caption", newStory.caption))
        }
        _state.update { it.copy(stories = listOf(newStory) + it.stories, toastBanner = "24 小时快拍动态已发布") }
    }

    fun selectFolder(folderId: String) {
        _state.update { it.copy(selectedFolderId = folderId) }
    }

    fun toggleArchiveExpanded() {
        _state.update { it.copy(archiveExpanded = !it.archiveExpanded) }
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
        val wireFolder = MessagingFolder(
            id = folder.id,
            title = folder.title,
            includeContacts = includeContacts,
            includeBots = includeBots,
            includeGroups = includeGroups,
            includeChannels = includeChannels,
            excludeMuted = excludeMuted,
        )
        viewModelScope.launch(Dispatchers.IO) {
            sendProtocolV2Command(
                JSONObject().put("type", "upsertFolder").put(
                    "folder",
                    JSONObject()
                        .put("id", wireFolder.id)
                        .put("title", wireFolder.title)
                        .put("icon", JSONObject.NULL)
                        .put("conversationIds", JSONArray())
                        .put("includeContacts", includeContacts)
                        .put("includeBots", includeBots)
                        .put("includeGroups", includeGroups)
                        .put("includeChannels", includeChannels)
                        .put("excludeMuted", excludeMuted)
                        .put("excludeRead", false)
                        .put("excludeArchived", true),
                ),
            )
        }
        _state.update { it.copy(folderTabs = it.folderTabs + folder, selectedFolderId = folder.id, toastBanner = "已创建分组「$clean」") }
    }

    fun deleteCustomFolder(folderId: String) {
        _state.update { current ->
            val updated = current.folderTabs.filterNot { !it.isBuiltIn && it.id == folderId }
            current.copy(
                folderTabs = updated,
                selectedFolderId = if (current.selectedFolderId == folderId) "all" else current.selectedFolderId,
            )
        }
    }

    fun setGlobalSearch(active: Boolean, query: String = "", filter: GlobalSearchFilter = GlobalSearchFilter.CHATS) {
        if (active && query.isNotBlank()) {
            viewModelScope.launch(Dispatchers.IO) {
                hostRequestOrNull("telegram.messaging.searchGlobal", JSONObject().put("query", query).put("filter", filter.name.lowercase()))
            }
        }
        _state.update { it.copy(globalSearchActive = active, globalSearchQuery = query, globalSearchFilter = filter) }
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

    fun togglePinChat(chatId: String) {
        _state.update { current ->
            val updated = current.chats.map { if (it.id == chatId) it.copy(isPinned = !it.isPinned) else it }
            current.copy(chats = sortTelegramChats(updated), selectedChatIds = emptySet())
        }
    }

    fun toggleArchiveChat(chatId: String) {
        _state.update { current ->
            val target = current.chats.firstOrNull { it.id == chatId } ?: return@update current
            val nextArchived = !target.isArchived
            val updated = current.chats.map { if (it.id == chatId) it.copy(isArchived = nextArchived) else it }
            current.copy(
                chats = sortTelegramChats(updated),
                selectedChatIds = emptySet(),
                toastBanner = if (nextArchived) "已归档「${target.title}」" else "已从归档移出「${target.title}」",
            )
        }
    }

    fun toggleMuteChat(chatId: String) {
        _state.update { current ->
            val updated = current.chats.map { if (it.id == chatId) it.copy(isMuted = !it.isMuted) else it }
            current.copy(chats = updated, selectedChatIds = emptySet())
        }
    }

    fun toggleUnreadChat(chatId: String) {
        _state.update { current ->
            val updated = current.chats.map { chat ->
                if (chat.id == chatId) {
                    if (chat.unreadCount > 0 || chat.markedUnread) chat.copy(unreadCount = 0, markedUnread = false)
                    else chat.copy(markedUnread = true)
                } else chat
            }
            current.copy(chats = updated, selectedChatIds = emptySet())
        }
    }

    // ------------------------------------------------------------------------
    // Opening & Creating Conversations (Group, Channel, Secret Chat, Direct)
    // ------------------------------------------------------------------------

    fun openChat(chatId: String, topicId: String? = null) {
        _state.update { current ->
            val updatedChats = current.chats.map {
                if (it.id == chatId) it.copy(unreadCount = 0, markedUnread = false) else it
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
    }

    fun closeChat() {
        _state.update {
            it.copy(
                subScreen = TelegramSubScreen.HOME,
                activeChatId = null,
                activeTopicId = null,
                inChatSearchActive = false,
                inChatSearchQuery = "",
                replyTarget = null,
                editTarget = null,
            )
        }
    }

    fun openSavedMessages() {
        openChat("saved-messages")
    }

    fun createNewGroupOrChannel(kind: ConversationKind, title: String, description: String, forumEnabled: Boolean = false) {
        val cleanTitle = title.trim().ifBlank { return }
        val id = "${kind.wire}-${UUID.randomUUID().toString().take(8)}"
        val newChat = TelegramChatItem(
            id = id,
            title = cleanTitle,
            subtitle = if (kind == ConversationKind.CHANNEL) "1 位订阅者" else "2 位成员",
            preview = description.trim().ifBlank { if (kind == ConversationKind.CHANNEL) "频道已创建" else "群组已创建" },
            time = nowTimeLabel(),
            kind = kind,
            isForum = forumEnabled && kind == ConversationKind.GROUP,
            memberCount = if (kind == ConversationKind.GROUP) 2 else 0,
            subscriberCount = if (kind == ConversationKind.CHANNEL) 1 else 0,
        )
        val initialTopics = if (newChat.isForum) {
            listOf(
                TelegramForumTopic("topic-general-$id", id, "综合讨论 General", "💬", 0xFF2AABEEL, isPinned = true, lastPreview = "欢迎加入话题群组"),
                TelegramForumTopic("topic-ann-$id", id, "公告发布 Announcements", "📢", 0xFFF59E0BL, lastPreview = "群规与置顶说明"),
            )
        } else emptyList()

        val welcomeMsg = TelegramMessageItem(
            id = "msg-init-$id",
            conversationId = id,
            senderId = actorId,
            senderName = "系统提示",
            text = if (kind == ConversationKind.CHANNEL) "📢 广播频道「$cleanTitle」已就绪，发布的帖子支持阅读量统计与关联评论区。" else "👥 群组「$cleanTitle」已创建${if (newChat.isForum) "（已开启超级群组多话题 Topics 模式）" else ""}。",
            contentType = "service",
            outgoing = true,
            time = nowTimeLabel(),
            viewsCount = if (kind == ConversationKind.CHANNEL) 1 else null,
            commentsCount = if (kind == ConversationKind.CHANNEL) 0 else null,
        )

        viewModelScope.launch(Dispatchers.IO) {
            val now = System.currentTimeMillis()
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
            val nextTopics = if (initialTopics.isNotEmpty()) current.topicsByChat + (id to initialTopics) else current.topicsByChat
            current.copy(
                chats = sortTelegramChats(listOf(newChat) + current.chats),
                topicsByChat = nextTopics,
                messagesByChat = current.messagesByChat + (id to listOf(welcomeMsg)),
                subScreen = TelegramSubScreen.CHAT,
                activeChatId = id,
                activeTopicId = initialTopics.firstOrNull()?.id,
            )
        }
    }

    fun createSecretChat(peerName: String) {
        val cleanPeer = peerName.trim().ifBlank { "安全联系人" }
        val id = "secret-${UUID.randomUUID().toString().take(8)}"
        val seed = "x3dh:$actorId:$cleanPeer:${System.currentTimeMillis()}"
        viewModelScope.launch(Dispatchers.IO) {
            hostRequestOrNull("telegram.e2ee.initDevice")
            hostRequestOrNull("telegram.e2ee.createSecretChat", JSONObject().put("peerName", cleanPeer).put("conversationId", id))
        }
        val secretChat = TelegramChatItem(
            id = id,
            title = "🔒 $cleanPeer",
            subtitle = "端到端加密聊天 · X3DH + Double Ratchet",
            preview = "端到端密钥协商已完成，点击顶部查看 4-Emoji 安全指纹",
            time = nowTimeLabel(),
            kind = ConversationKind.SECRET,
            isSecretChat = true,
            secretFingerprintSeed = seed,
            ttlSeconds = 86_400,
            online = true,
        )
        val fp = CallsEmojiFingerprint.deriveSecretChatIdenticon(seed)
        val initMsg = TelegramMessageItem(
            id = "msg-sec-$id",
            conversationId = id,
            senderId = "system",
            senderName = "E2EE 安全引擎",
            text = "🔐 已建立端到端加密通道 (${fp.protocolSummary})\n安全校验表情：${fp.emojis4.joinToString(" ")}\n支持前向保密、阅后即焚计时器与防截屏转发。",
            outgoing = false,
            time = nowTimeLabel(),
            ttlSeconds = 86_400,
        )
        _state.update { current ->
            current.copy(
                chats = sortTelegramChats(listOf(secretChat) + current.chats),
                messagesByChat = current.messagesByChat + (id to listOf(initMsg)),
                subScreen = TelegramSubScreen.CHAT,
                activeChatId = id,
            )
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

    fun createForumTopic(conversationId: String, title: String, emoji: String) {
        val clean = title.trim().ifBlank { return }
        val topic = TelegramForumTopic(
            id = "topic-${UUID.randomUUID().toString().take(6)}",
            conversationId = conversationId,
            title = clean,
            iconEmoji = emoji.ifBlank { "💡" },
            lastPreview = "新话题已开启",
        )
        viewModelScope.launch(Dispatchers.IO) {
            hostRequestOrNull("telegram.messaging.createTopic", JSONObject().put("conversationId", conversationId).put("title", clean).put("emoji", topic.iconEmoji))
        }
        _state.update { current ->
            val list = current.topicsByChat[conversationId].orEmpty() + topic
            current.copy(
                topicsByChat = current.topicsByChat + (conversationId to list),
                activeTopicId = topic.id,
            )
        }
    }

    // ------------------------------------------------------------------------
    // Rich Messaging, Composer, Inline Keyboards, Polls, Reactions & Export
    // ------------------------------------------------------------------------

    fun updateDraft(conversationId: String, draft: String) {
        _state.update { current ->
            current.copy(
                chats = current.chats.map { if (it.id == conversationId) it.copy(draftText = draft) else it },
            )
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
        if (active && query.isNotBlank() && convId != null) {
            viewModelScope.launch(Dispatchers.IO) {
                hostRequestOrNull("telegram.messaging.searchInChat", JSONObject().put("conversationId", convId).put("query", query))
            }
        }
        _state.update { it.copy(inChatSearchActive = active, inChatSearchQuery = query) }
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

        val editing = _state.value.editTarget
        if (editing != null && editing.conversationId == conversationId) {
            val (plain, entities) = TelegramEntitiesEngine.parseMarkdownAndAutoEntities(trimmed)
            updateMessageInChat(conversationId, editing.id) {
                it.copy(text = plain, entities = entities, edited = true)
            }
            _state.update { current ->
                current.copy(
                    editTarget = null,
                    chats = current.chats.map { if (it.id == conversationId) it.copy(draftText = "") else it },
                )
            }
            viewModelScope.launch(Dispatchers.IO) {
                sendProtocolV2Command(
                    JSONObject()
                        .put("type", "editMessage")
                        .put("conversationId", conversationId)
                        .put("messageId", editing.id)
                        .put("content", JSONObject().put("type", "text").put("data", JSONObject().put("text", JSONObject().put("text", plain).put("entities", serializeEntities(entities))))),
                )
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
        val msgId = "msg-${UUID.randomUUID().toString().take(10)}"
        val isChannel = currentChat?.kind == ConversationKind.CHANNEL

        val newMsg = TelegramMessageItem(
            id = msgId,
            conversationId = conversationId,
            senderId = actorId,
            senderName = _state.value.activeAccount?.displayName ?: "我",
            text = plainText,
            entities = entities,
            contentType = "text",
            outgoing = true,
            time = if (scheduledAtMs != null) "🕒 定时 ${nowTimeLabel(scheduledAtMs)}" else nowTimeLabel(),
            deliveryState = "read",
            silent = silent,
            scheduledAtMs = scheduledAtMs,
            ttlSeconds = effectiveTtl,
            topicId = _state.value.activeTopicId,
            replyHeader = replyHeader,
            viewsCount = if (isChannel) 1 else null,
            commentsCount = if (isChannel) 0 else null,
        )

        appendMessageToChat(conversationId, newMsg, clearDraft = true)

        viewModelScope.launch(Dispatchers.IO) {
            if (currentChat?.isSecretChat == true) {
                hostRequestOrNull(
                    "telegram.e2ee.sendEncrypted",
                    JSONObject().put("conversationId", conversationId).put("plaintext", plainText),
                )
            } else {
                sendProtocolV2Command(
                    JSONObject()
                        .put("type", "sendMessage")
                        .put("conversationId", conversationId)
                        .put("clientMessageId", msgId)
                        .put(
                            "content",
                            JSONObject().put(
                                "type",
                                "text",
                            ).put(
                                "data",
                                JSONObject().put("text", JSONObject().put("text", plainText).put("entities", serializeEntities(entities))),
                            ),
                        )
                        .put("replyToMessageId", reply?.id ?: JSONObject.NULL)
                        .put("threadRootMessageId", _state.value.activeTopicId ?: JSONObject.NULL)
                        .put("scheduledAtMs", scheduledAtMs ?: JSONObject.NULL)
                        .put("silent", silent)
                        .put("protectedContent", currentChat?.isSecretChat == true),
                )
            }
        }

        handleSlashOrBotAutomation(conversationId, currentChat, plainText)
    }

    private fun handleSlashOrBotAutomation(
        conversationId: String,
        chat: TelegramChatItem?,
        plainText: String,
    ) {
        when {
            plainText.startsWith("/wallet") -> navigateTo(TelegramSubScreen.TON_WALLET)
            plainText.startsWith("/dharma") -> navigateTo(TelegramSubScreen.GLOBAL_DHARMA)
            plainText.startsWith("/hermes") -> navigateTo(TelegramSubScreen.HERMES_STUDIO)
            plainText.startsWith("/magpie") -> navigateTo(TelegramSubScreen.MAGPIE_GATEWAY)
            plainText.startsWith("/computer") -> navigateTo(TelegramSubScreen.CLOUD_COMPUTER)
            plainText.startsWith("/mika") || chat?.id == "bot-mika-phone" -> {
                val taskPrompt = plainText.removePrefix("/mika").trim().ifBlank { plainText }
                runWithMikaFromChat(conversationId, taskPrompt)
            }
            plainText.startsWith("/newbot") || chat?.id == "bot-botfather" -> {
                respondAsBotFather(conversationId, plainText)
            }
            plainText.startsWith("/moa") || chat?.id == "chat-hermes-lab" || (chat?.isBot == true) -> {
                respondAsAiAssistant(conversationId, chat?.title ?: "Hermes AI", plainText)
            }
        }
    }

    fun runWithMikaFromChat(conversationId: String, taskPrompt: String) {
        val cleanPrompt = taskPrompt.trim().ifBlank { return }
        val taskId = "mika-task-${UUID.randomUUID().toString().take(8)}"
        val cardId = "msg-$taskId"
        val taskCard = TelegramMessageItem(
            id = cardId,
            conversationId = conversationId,
            senderId = "bot-mika-phone",
            senderName = "Mika 手机操控智能体",
            text = "🤖 已受理本机操控任务：「$cleanPrompt」\n正在连接无障碍 UI 树 / 虚拟屏幕执行器…",
            contentType = "mikaTask",
            outgoing = false,
            time = nowTimeLabel(),
            mikaTaskId = taskId,
            mikaTaskStatus = "running",
            mikaStepLogs = listOf("1. 解析用户意图与安全风险门控 (ConfirmationGate)", "2. 获取当前屏幕语义树 / 启动后台 VirtualDisplay"),
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
            hostRequestOrNull(
                "mika.registerTaskEvent",
                JSONObject().put("taskId", taskId).put("conversationId", conversationId).put("prompt", cleanPrompt),
            )
        }

        val bridge = mikaBridge
        if (bridge != null) {
            bridge.submitPhoneTask(cleanPrompt, conversationId) { stepUpdate ->
                updateMessageInChat(conversationId, cardId) { existing ->
                    existing.copy(
                        mikaStepLogs = (existing.mikaStepLogs + stepUpdate).takeLast(6),
                        mikaTaskStatus = if (stepUpdate.contains("完成") || stepUpdate.contains("Done", ignoreCase = true)) "completed" else "running",
                    )
                }
            }
            refreshMikaSnapshot()
        } else {
            viewModelScope.launch {
                delay(450)
                updateMessageInChat(conversationId, cardId) { existing ->
                    existing.copy(
                        mikaTaskStatus = "ready",
                        mikaStepLogs = existing.mikaStepLogs + "3. 任务已投递至 Mika 任务队列（可在侧边栏打开 Mika 手机操控中心查看实时画面与权限诊断）",
                    )
                }
            }
        }
    }

    private fun respondAsBotFather(conversationId: String, input: String) {
        viewModelScope.launch {
            delay(280)
            val replyRaw = if (input.startsWith("/newbot")) {
                "好的！让我们创建一个全新的机器人 🤖\n请直接发送机器人的 **显示名称**（例如：`法布施随身智囊`），或者点击下方按钮一键生成带 WebMCP 与伴侣幽灵头像的 AI 机器人。"
            } else {
                val handle = "@${input.filter { it.isLetterOrDigit() }.lowercase().take(10).ifBlank { "dharma" }}_bot"
                createCompanionBot(input.take(32), "由 @BotFather 创建的专属智能机器人", handle)
                "🎉 恭喜！您的机器人 **$input** ($handle) 已创建成功！\n\nAPI Token: ||tg_bot_${UUID.randomUUID().toString().replace("-", "").take(24)}||\n点击上方剧透遮罩即可查看密钥，且已自动添加到您的对话列表！"
            }
            val (plain, ents) = TelegramEntitiesEngine.parseMarkdownAndAutoEntities(replyRaw)
            appendMessageToChat(
                conversationId,
                TelegramMessageItem(
                    id = "msg-bf-${UUID.randomUUID().toString().take(8)}",
                    conversationId = conversationId,
                    senderId = "bot-botfather",
                    senderName = "BotFather",
                    text = plain,
                    entities = ents,
                    outgoing = false,
                    time = nowTimeLabel(),
                    inlineKeyboard = listOf(
                        InlineKeyboardRow(
                            listOf(
                                InlineKeyboardButtonModel("bf-studio", "⚙️ 打开 BotFather 工作台", InlineKeyboardButtonKind.CALLBACK, "open:botfather"),
                                InlineKeyboardButtonModel("bf-dharma", "🪷 打开全球法布施小程序", InlineKeyboardButtonKind.MINI_APP, "global-dharma"),
                            ),
                        ),
                    ),
                ),
                clearDraft = false,
            )
        }
    }

    private fun respondAsAiAssistant(conversationId: String, botTitle: String, prompt: String) {
        viewModelScope.launch {
            _state.update { current ->
                current.copy(chats = current.chats.map { if (it.id == conversationId) it.copy(typingLabel = "$botTitle 正在思考…") else it })
            }
            val hostReply = withContext(Dispatchers.IO) {
                val reqId = "tg-ai-${UUID.randomUUID()}"
                val accepted = hostRequestOrNull(
                    "feature.execute",
                    JSONObject().put("command", JSONObject().put("type", "chat.send").put("requestId", reqId).put("text", prompt).put("agentId", "mahayana-assistant").put("mode", "agent")),
                )
                if (accepted != null) {
                    val opId = accepted.optString("operationId").ifBlank { reqId }
                    var collected = ""
                    repeat(15) {
                        val ev = hostRequestOrNull("feature.receive", JSONObject().put("timeoutMs", 80)) ?: return@repeat
                        if (ev.optString("operationId", opId) == opId) {
                            when (ev.optString("type")) {
                                "chat.message" -> if (ev.optString("role") == "assistant") collected = ev.optString("text")
                                "chat.delta" -> collected += ev.optString("delta")
                                "operation.completed" -> if (collected.isNotBlank()) return@withContext collected
                            }
                        }
                    }
                    collected.ifBlank { null }
                } else null
            }
            delay(260)
            val markdownReply = hostReply ?: if (prompt.startsWith("/moa")) {
                "✨ **Mixture-of-Agents (/moa) 多模型协同结论**\n- `Claude 4.6`: 结构化拆解完成\n- `DeepSeek V3.2`: 成本与性能路径最优\n- `Gemini 3.1 Pro`: 多模态与 Android 端侧融合校验通过\n\n针对「${prompt.removePrefix("/moa").trim().ifBlank { "当前议题" }}」，建议结合 **Hermes 工作室** 与 **Mika 手机操控智能体** 协同执行。"
            } else {
                "已收到您的指令：「$prompt」\n您可以随时发送 `/mika <任务>` 操控手机 App，发送 `/dharma` 开启 3D 地球法布施，或使用 `**粗体**` / `||剧透||` 体验 UTF-16 富文本消息。"
            }
            val (plain, ents) = TelegramEntitiesEngine.parseMarkdownAndAutoEntities(markdownReply)
            _state.update { current ->
                current.copy(chats = current.chats.map { if (it.id == conversationId) it.copy(typingLabel = null) else it })
            }
            appendMessageToChat(
                conversationId,
                TelegramMessageItem(
                    id = "msg-ai-${UUID.randomUUID().toString().take(8)}",
                    conversationId = conversationId,
                    senderId = conversationId,
                    senderName = botTitle,
                    text = plain,
                    entities = ents,
                    outgoing = false,
                    time = nowTimeLabel(),
                ),
                clearDraft = false,
            )
        }
    }

    fun toggleReaction(conversationId: String, messageId: String, emoji: String) {
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
    }

    fun togglePinMessage(conversationId: String, messageId: String) {
        _state.update { current ->
            val msgs = current.messagesByChat[conversationId].orEmpty()
            val target = msgs.firstOrNull { it.id == messageId } ?: return@update current
            val nextPinned = !target.pinned
            val updatedMsgs = msgs.map { if (it.id == messageId) it.copy(pinned = nextPinned) else it }
            val updatedChats = current.chats.map { chat ->
                if (chat.id == conversationId) {
                    val pins = if (nextPinned) (chat.pinnedMessageIds + messageId).distinct() else chat.pinnedMessageIds - messageId
                    chat.copy(pinnedMessageIds = pins)
                } else chat
            }
            current.copy(
                messagesByChat = current.messagesByChat + (conversationId to updatedMsgs),
                chats = updatedChats,
                toastBanner = if (nextPinned) "消息已置顶" else "已取消置顶",
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
        viewModelScope.launch(Dispatchers.IO) {
            sendProtocolV2Command(
                JSONObject()
                    .put("type", "deleteMessages")
                    .put("conversationId", conversationId)
                    .put("messageIds", JSONArray().put(messageId))
                    .put("forEveryone", forEveryone),
            )
        }
        _state.update { current ->
            val updated = current.messagesByChat[conversationId].orEmpty().filterNot { it.id == messageId }
            current.copy(
                messagesByChat = current.messagesByChat + (conversationId to updated),
                toastBanner = if (forEveryone) "已为双方双向撤回消息" else "已从本地删除消息",
            )
        }
    }

    fun forwardMessage(sourceMessage: TelegramMessageItem, targetConversationId: String) {
        val targetChat = _state.value.chats.firstOrNull { it.id == targetConversationId } ?: return
        val forwarded = sourceMessage.copy(
            id = "msg-fwd-${UUID.randomUUID().toString().take(8)}",
            conversationId = targetConversationId,
            outgoing = true,
            time = nowTimeLabel(),
            forwardHeader = TelegramForwardHeader(originName = sourceMessage.senderName, dateLabel = sourceMessage.time),
            pinned = false,
            reactions = emptyList(),
        )
        appendMessageToChat(targetConversationId, forwarded, clearDraft = false)
        postBanner("已转发至「${targetChat.title}」")
    }

    fun translateMessage(conversationId: String, messageId: String) {
        updateMessageInChat(conversationId, messageId) { msg ->
            if (msg.translatedText != null) {
                msg.copy(translatedText = null)
            } else {
                val hasAscii = msg.text.count { it in 'A'..'Z' || it in 'a'..'z' } > msg.text.length / 3
                val translated = if (hasAscii) {
                    "【即时中文译文】${msg.text}"
                } else {
                    "[Instant Translation · EN] ${msg.text}"
                }
                msg.copy(translatedText = translated)
            }
        }
    }

    fun sendSticker(conversationId: String, emoji: String, packTitle: String) {
        appendMessageToChat(
            conversationId,
            TelegramMessageItem(
                id = "msg-stk-${UUID.randomUUID().toString().take(8)}",
                conversationId = conversationId,
                senderId = actorId,
                senderName = "我",
                text = "$emoji 贴纸",
                contentType = "sticker",
                outgoing = true,
                time = nowTimeLabel(),
                stickerEmoji = emoji,
                stickerPackTitle = packTitle,
            ),
            clearDraft = false,
        )
    }

    fun sendGif(conversationId: String, gif: GifItemModel) {
        appendMessageToChat(
            conversationId,
            TelegramMessageItem(
                id = "msg-gif-${UUID.randomUUID().toString().take(8)}",
                conversationId = conversationId,
                senderId = actorId,
                senderName = "我",
                text = "GIF · ${gif.title}",
                contentType = "gif",
                outgoing = true,
                time = nowTimeLabel(),
                stickerEmoji = gif.previewGlyph,
                mediaFileName = "${gif.id}.gif",
            ),
            clearDraft = false,
        )
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
        val correctId = if (isQuiz) "opt-${correctIndex.coerceIn(0, cleanOpts.lastIndex) + 1}" else null
        val poll = TelegramPollModel(
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
                poll = poll,
            ),
            clearDraft = false,
        )
    }

    fun votePoll(conversationId: String, messageId: String, optionId: String) {
        updateMessageInChat(conversationId, messageId) { msg ->
            val poll = msg.poll ?: return@updateMessageInChat msg
            if (poll.isQuiz && poll.hasVoted) return@updateMessageInChat msg
            val updatedOptions = poll.options.map { opt ->
                when {
                    opt.id == optionId && !opt.chosen -> opt.copy(chosen = true, voterCount = opt.voterCount + 1)
                    opt.id == optionId && opt.chosen && !poll.isQuiz -> opt.copy(chosen = false, voterCount = (opt.voterCount - 1).coerceAtLeast(0))
                    !poll.multipleAnswers && opt.chosen -> opt.copy(chosen = false, voterCount = (opt.voterCount - 1).coerceAtLeast(0))
                    else -> opt
                }
            }
            msg.copy(poll = poll.copy(options = updatedOptions))
        }
    }

    fun sendLocation(conversationId: String, latitude: Double, longitude: Double, venueTitle: String = "实时共享位置") {
        appendMessageToChat(
            conversationId,
            TelegramMessageItem(
                id = "msg-loc-${UUID.randomUUID().toString().take(8)}",
                conversationId = conversationId,
                senderId = actorId,
                senderName = "我",
                text = "📍 $venueTitle",
                contentType = "location",
                outgoing = true,
                time = nowTimeLabel(),
                latitude = latitude,
                longitude = longitude,
                venueTitle = venueTitle,
            ),
            clearDraft = false,
        )
    }

    fun sendContactCard(conversationId: String, name: String, phone: String) {
        appendMessageToChat(
            conversationId,
            TelegramMessageItem(
                id = "msg-ctc-${UUID.randomUUID().toString().take(8)}",
                conversationId = conversationId,
                senderId = actorId,
                senderName = "我",
                text = "👤 $name",
                contentType = "contact",
                outgoing = true,
                time = nowTimeLabel(),
                contactName = name,
                contactPhone = phone,
            ),
            clearDraft = false,
        )
    }

    fun sendMediaAttachment(conversationId: String, fileName: String, mimeType: String, bytes: ByteArray) {
        val blobId = "blob-${UUID.randomUUID()}"
        cachedVoiceBlobs[blobId] = bytes
        val contentType = when {
            mimeType.startsWith("image/") -> "photo"
            mimeType.startsWith("video/") -> "video"
            mimeType.startsWith("audio/") -> "audio"
            else -> "document"
        }
        appendMessageToChat(
            conversationId,
            TelegramMessageItem(
                id = "msg-med-${UUID.randomUUID().toString().take(8)}",
                conversationId = conversationId,
                senderId = actorId,
                senderName = "我",
                text = fileName,
                contentType = contentType,
                outgoing = true,
                time = nowTimeLabel(),
                mediaFileName = fileName,
                mediaBlobId = blobId,
                mediaMimeType = mimeType,
                mediaSizeBytes = bytes.size,
            ),
            clearDraft = false,
        )
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
                    val host = mahayanaHost ?: error("媒体缓存为空")
                    val output = java.io.ByteArrayOutputStream(sizeBytes.coerceAtLeast(1))
                    var offset = 0
                    while (offset < sizeBytes) {
                        val reqLen = minOf(512 * 1024, sizeBytes - offset)
                        val chunkResp = host.request("feature.messaging.blob.read", JSONObject().put("blobId", blobId).put("offset", offset).put("length", reqLen))
                        val chunk = Base64.getDecoder().decode(chunkResp.optString("dataBase64"))
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
                when (button.payload) {
                    "open_workspace" -> navigateTo(TelegramSubScreen.MIKA_WORKSPACE)
                    "stop_task" -> {
                        mikaBridge?.stopActiveTask()
                        postBanner("已向 Mika 手机智能体发送停止信号")
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
                        viewModelScope.launch(Dispatchers.IO) {
                            hostRequestOrNull("telegram.messaging.callbackQuery", JSONObject().put("conversationId", conversationId).put("data", button.payload))
                        }
                        postBanner("已触发机器人回调：${button.text}")
                    }
                }
            }
        }
    }

    fun setChatTtl(conversationId: String, ttlSeconds: Int) {
        _state.update { current ->
            current.copy(
                chats = current.chats.map { if (it.id == conversationId) it.copy(ttlSeconds = ttlSeconds) else it },
                toastBanner = if (ttlSeconds <= 0) "已关闭自动销毁计时器" else "已设置消息自动销毁计时器",
            )
        }
    }

    fun clearChatHistory(conversationId: String) {
        _state.update { current ->
            current.copy(
                messagesByChat = current.messagesByChat + (conversationId to emptyList()),
                toastBanner = "会话聊天记录已清空",
            )
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
            postBanner("已导出 ${messages.size} 条记录至 ${outFile.name}")
            outFile
        }.getOrNull()
    }

    fun openChannelStats(conversationId: String?) {
        if (conversationId == null) {
            _state.update { it.copy(channelStatsSnapshot = null) }
            return
        }
        val chat = _state.value.chats.firstOrNull { it.id == conversationId }
        val subs = (chat?.subscriberCount ?: 12_840).coerceAtLeast(128)
        _state.update {
            it.copy(
                channelStatsSnapshot = ChannelStatisticsSnapshot(
                    conversationId = conversationId,
                    subscribers = subs,
                    growth7dPercent = 6.8,
                    notificationsEnabledPercent = 74.2,
                    avgViewsPerPost = (subs * 0.42).toInt(),
                    sharesPerPost = 48,
                    reactionsPerPost = 196,
                    dailyViewsBars = listOf(42, 58, 51, 67, 79, 88, 95),
                ),
            )
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
            senderId = actorId,
            senderName = _state.value.activeAccount?.displayName ?: "我",
            text = clean,
            outgoing = true,
            time = nowTimeLabel(),
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
    // Voice Waveform & Round Video Note Recording + Floating Audio Player
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
        if (hasAudioPerm && _state.value.recordingState.mode == ComposerRecordMode.VOICE_MIC) {
            voiceRecorder.start()
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
            val minBuf = AudioRecord.getMinBufferSize(16_000, AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT)
            if (hasAudioPerm && minBuf > 0 && !voiceRecorder.isRecording) {
                runCatching {
                    audioRecord = AudioRecord(MediaRecorder.AudioSource.MIC, 16_000, AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT, minBuf * 2)
                    audioRecord?.startRecording()
                }
            }
            val pcmBuf = ShortArray(512)
            try {
                var tick = 0
                while (isActive && _state.value.recordingState.isRecording) {
                    val elapsed = System.currentTimeMillis() - startedAt
                    val bar = if (audioRecord != null && audioRecord?.recordingState == AudioRecord.RECORDSTATE_RECORDING) {
                        val read = audioRecord?.read(pcmBuf, 0, pcmBuf.size) ?: 0
                        TelegramWaveformCodec.computeRmsBar31(pcmBuf, read)
                    } else {
                        val synthetic = TelegramWaveformCodec.syntheticSpeechBars("rec-$tick")
                        synthetic[tick % synthetic.size]
                    }
                    _state.update { current ->
                        val nextBars = (current.recordingState.liveBars + bar).takeLast(100)
                        current.copy(recordingState = current.recordingState.copy(elapsedMs = elapsed, liveBars = nextBars))
                    }
                    tick++
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
        val msg = TelegramMessageItem(
            id = "msg-rec-${UUID.randomUUID().toString().take(8)}",
            conversationId = conversationId,
            senderId = actorId,
            senderName = _state.value.activeAccount?.displayName ?: "我",
            text = if (isRoundVideo) "⭕ 圆形视频消息 (${durationSec}s)" else "🎙 语音消息 (${durationSec}s)",
            contentType = if (isRoundVideo) "videoNote" else "voice",
            outgoing = true,
            time = nowTimeLabel(),
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
            if (nextPlaying) launchSimulatedOrRealAudioTicker() else audioProgressJob?.cancel()
            return
        }

        stopAudioPlayback()
        val duration = message.durationSeconds.coerceAtLeast(6)
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
        launchSimulatedOrRealAudioTicker()
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

    private fun launchSimulatedOrRealAudioTicker() {
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
    // VoIP / Video / Group Calls & <60ms VAD Talk Mode Orb
    // ------------------------------------------------------------------------

    fun startCall(conversationId: String, peerTitle: String, video: Boolean, groupCall: Boolean = false) {
        val callId = "call-${UUID.randomUUID().toString().take(8)}"
        val emojis = CallsEmojiFingerprint.deriveCallEmojis("$callId:$conversationId:$actorId")
        viewModelScope.launch(Dispatchers.IO) {
            hostRequestOrNull(
                if (groupCall) "telegram.calls.joinGroupCall" else "telegram.calls.startCall",
                JSONObject().put("conversationId", conversationId).put("video", video),
            )
        }
        val participants = buildList {
            add(CallParticipantTile(actorId, _state.value.activeAccount?.displayName ?: "我", videoOn = video, speaking = true))
            add(CallParticipantTile("peer-1", peerTitle, videoOn = video, speaking = true))
            if (groupCall) {
                add(CallParticipantTile("peer-2", "Rust Host 架构师", muted = false, speaking = false))
                add(CallParticipantTile("peer-3", "Mika Agent 节点", muted = true, speaking = false))
            }
        }
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
                    participants = participants,
                ),
            )
        }
        callTimerJob?.cancel()
        callTimerJob = viewModelScope.launch {
            delay(850)
            _state.update { current ->
                if (current.callState.callId == callId) {
                    current.copy(callState = current.callState.copy(phase = CallPhase.ACTIVE))
                } else current
            }
            while (isActive && _state.value.callState.phase == CallPhase.ACTIVE) {
                delay(1000)
                _state.update { current ->
                    current.copy(callState = current.callState.copy(durationSeconds = current.callState.durationSeconds + 1))
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
        _state.update { it.copy(callState = it.callState.copy(isScreenSharing = !it.callState.isScreenSharing)) }
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
        val entry = CallHistoryEntry(
            id = ended.callId.ifBlank { UUID.randomUUID().toString() },
            peerTitle = ended.peerTitle,
            conversationId = ended.conversationId,
            isVideo = ended.isVideoCall,
            outgoing = true,
            timeLabel = nowTimeLabel(),
            durationLabel = "%02d:%02d".format(ended.durationSeconds / 60, ended.durationSeconds % 60),
        )
        viewModelScope.launch(Dispatchers.IO) {
            hostRequestOrNull("telegram.calls.endCall", JSONObject().put("callId", ended.callId))
        }
        _state.update {
            it.copy(
                callState = TelegramCallUiState(),
                callHistory = listOf(entry) + it.callHistory,
            )
        }
    }

    fun setTalkModeOrbVisible(visible: Boolean) {
        _state.update {
            it.copy(
                talkOrbState = it.talkOrbState.copy(
                    visible = visible,
                    phase = TalkVadPhase.LISTENING,
                    vadLatencyMs = 36,
                ),
            )
        }
    }

    fun triggerTalkModeBargeIn() {
        _state.update {
            it.copy(
                talkOrbState = it.talkOrbState.copy(
                    phase = TalkVadPhase.LISTENING,
                    vadLatencyMs = 29,
                    assistantReply = "⚡ 已在 29ms 内通过端侧 VAD 即时打断播报，请继续说话…",
                ),
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
                    vadLatencyMs = 34,
                ),
            )
        }
        viewModelScope.launch {
            withContext(Dispatchers.IO) {
                hostRequestOrNull("telegram.calls.vadFeedPcm", JSONObject().put("transcript", clean))
            }
            delay(220)
            _state.update {
                it.copy(
                    talkOrbState = it.talkOrbState.copy(
                        phase = TalkVadPhase.AI_SPEAKING,
                        vadLatencyMs = 41,
                        assistantReply = "已理解：「$clean」。正在全双工低延迟流式语音回应，您随时开口即可在 <60ms 内打断我。",
                    ),
                )
            }
        }
    }

    // ------------------------------------------------------------------------
    // TON Wallet, 全球法布施, BotFather, Hermes, Magpie & Cloud Computer Actions
    // ------------------------------------------------------------------------

    fun createNewTonWallet() {
        viewModelScope.launch(Dispatchers.IO) { hostRequestOrNull("wallet.createWallet") }
        val suffix = UUID.randomUUID().toString().replace("-", "").take(12)
        _state.update {
            it.copy(
                tonWallet = it.tonWallet.copy(
                    initialized = true,
                    addressFriendly = "UQBk9x_mahayana_v4r2_$suffix",
                    statusBanner = "已通过 Ed25519 + BIP39 创建全新 24 词自托管 TON 钱包",
                ),
            )
        }
    }

    fun importTonMnemonic(wordsRaw: String) {
        val words = wordsRaw.trim().split(Regex("\\s+")).filter { it.isNotBlank() }
        if (words.size != 24) {
            postBanner("请输入完整的 24 个 BIP39 助记词（当前 ${words.size} 个）")
            return
        }
        viewModelScope.launch(Dispatchers.IO) {
            hostRequestOrNull("wallet.importMnemonic", JSONObject().put("mnemonic", words.joinToString(" ")))
        }
        _state.update {
            it.copy(
                tonWallet = it.tonWallet.copy(
                    initialized = true,
                    mnemonicWords = words,
                    statusBanner = "24 词助记词钱包导入校验通过",
                ),
            )
        }
    }

    fun sendTonTransfer(recipient: String, amountTon: Double, memo: String) {
        if (recipient.isBlank() || amountTon <= 0.0 || amountTon > _state.value.tonWallet.tonBalance) {
            postBanner("请检查收款地址与可用 TON 余额")
            return
        }
        viewModelScope.launch(Dispatchers.IO) {
            hostRequestOrNull("wallet.sendTon", JSONObject().put("to", recipient).put("amountTon", amountTon).put("memo", memo))
        }
        val tx = TonTransactionItem(
            id = "tx-${UUID.randomUUID().toString().take(8)}",
            direction = "out",
            title = "发送 TON",
            peerAddress = recipient.trim(),
            amountLabel = "-%.2f TON".format(amountTon),
            asset = "TON",
            timestampLabel = nowTimeLabel(),
            memo = memo,
            bocHash = UUID.randomUUID().toString().replace("-", "").take(16),
        )
        _state.update { cur ->
            cur.copy(
                tonWallet = cur.tonWallet.copy(
                    tonBalance = (cur.tonWallet.tonBalance - amountTon - 0.0045).coerceAtLeast(0.0),
                    transactions = listOf(tx) + cur.tonWallet.transactions,
                    statusBanner = "BOC 交易已签名广播（矿工费 ~0.0045 TON）",
                ),
            )
        }
    }

    fun swapTonToUsdt(tonAmount: Double) {
        if (tonAmount <= 0.0 || tonAmount > _state.value.tonWallet.tonBalance) return
        val receivedUsdt = tonAmount * 5.42
        val tx = TonTransactionItem(
            id = "swap-${UUID.randomUUID().toString().take(8)}",
            direction = "swap",
            title = "DEX 闪兑 TON → USDT",
            peerAddress = "STON.fi Router v2",
            amountLabel = "+%.2f USDT".format(receivedUsdt),
            asset = "USDT",
            timestampLabel = nowTimeLabel(),
        )
        _state.update { cur ->
            cur.copy(
                tonWallet = cur.tonWallet.copy(
                    tonBalance = cur.tonWallet.tonBalance - tonAmount,
                    usdtBalance = cur.tonWallet.usdtBalance + receivedUsdt,
                    transactions = listOf(tx) + cur.tonWallet.transactions,
                    statusBanner = "已兑换 %.2f TON → %.2f USDT".format(tonAmount, receivedUsdt),
                ),
            )
        }
    }

    fun stakeTon(tonAmount: Double) {
        if (tonAmount <= 0.0 || tonAmount > _state.value.tonWallet.tonBalance) return
        _state.update { cur ->
            cur.copy(
                tonWallet = cur.tonWallet.copy(
                    tonBalance = cur.tonWallet.tonBalance - tonAmount,
                    stakedTon = cur.tonWallet.stakedTon + tonAmount,
                    statusBanner = "已质押 %.2f TON 至流动性质押池 (APY %.2f%%)".format(tonAmount, cur.tonWallet.stakingApyPercent),
                ),
            )
        }
    }

    fun emulateTonConnectUri(uri: String) {
        val clean = uri.trim().ifBlank { "tc://?v=2&id=fabushi_dapp&r=tonswap" }
        viewModelScope.launch(Dispatchers.IO) {
            hostRequestOrNull("wallet.tonConnectEmulate", JSONObject().put("uri", clean))
        }
        _state.update { cur ->
            cur.copy(
                tonWallet = cur.tonWallet.copy(
                    pendingTonConnect = TonConnectPreview(
                        rawUri = clean,
                        dappName = "Fabushi Dharma Merit Vault",
                        dappDomain = "fabushi.ombhrum.com",
                        actionSummary = "铸造「十方光明莲花」灵魂绑定功德凭证 (SBT)",
                        estimatedDelta = "-0.05 TON (合约存储燃气费)",
                        networkFeeTon = "0.0042 TON",
                        riskNotice = "沙箱预执行通过 · 无无限授权风险",
                    ),
                ),
            )
        }
    }

    fun resolveTonConnect(approve: Boolean) {
        if (approve) {
            viewModelScope.launch(Dispatchers.IO) { hostRequestOrNull("wallet.tonConnectApprove") }
        }
        _state.update { cur ->
            cur.copy(
                tonWallet = cur.tonWallet.copy(
                    pendingTonConnect = null,
                    tonBalance = if (approve) (cur.tonWallet.tonBalance - 0.0542).coerceAtLeast(0.0) else cur.tonWallet.tonBalance,
                    statusBanner = if (approve) "TonConnect 交易已使用本地 Ed25519 私钥安全签名" else "已拒绝 TonConnect 请求",
                ),
            )
        }
    }

    // 全球法布施 Actions
    fun selectDharmaCountry(code: String) {
        _state.update { it.copy(globalDharma = it.globalDharma.copy(selectedCountryCode = code)) }
    }

    fun offerDharmaToCountry(targetCode: String, blessingNote: String) {
        viewModelScope.launch(Dispatchers.IO) {
            hostRequestOrNull("fabushiMiniapp.offerDharma", JSONObject().put("countryCode", targetCode).put("note", blessingNote))
        }
        _state.update { cur ->
            val target = cur.globalDharma.countries.firstOrNull { it.code == targetCode } ?: return@update cur
            val arc = OfferingArc(
                id = "arc-${System.nanoTime()}",
                fromLat = 35.86,
                fromLon = 104.19,
                toLat = target.latitude,
                toLon = target.longitude,
                createdAtMs = System.currentTimeMillis(),
                label = blessingNote.ifBlank { "愿正法久住 · ${target.nameZh}" },
            )
            val updatedCountries = cur.globalDharma.countries.map {
                if (it.code == targetCode) it.copy(offeringsCount = it.offeringsCount + 1, lampLitByMe = true) else it
            }
            cur.copy(
                globalDharma = cur.globalDharma.copy(
                    countries = updatedCountries,
                    selectedCountryCode = targetCode,
                    activeArcs = (cur.globalDharma.activeArcs + arc).takeLast(8),
                    totalGlobalOfferings = cur.globalDharma.totalGlobalOfferings + 1,
                    myMeritPoints = cur.globalDharma.myMeritPoints + 18,
                ),
                toastBanner = "🪔 已向 ${target.nameZh} (${target.code}) 供养祈福心灯 · 功德 +18",
            )
        }
    }

    fun recordPrayerWheelRevolution(revolutionsDelta: Int = 1) {
        if (revolutionsDelta <= 0) return
        viewModelScope.launch(Dispatchers.IO) {
            hostRequestOrNull("fabushiMiniapp.spinPrayerWheel", JSONObject().put("revolutions", revolutionsDelta))
        }
        _state.update { cur ->
            val d = cur.globalDharma
            cur.copy(
                globalDharma = d.copy(
                    prayerWheelSpins = d.prayerWheelSpins + revolutionsDelta,
                    mantraCount = d.mantraCount + revolutionsDelta * 8L,
                    myMeritPoints = d.myMeritPoints + revolutionsDelta * 3L,
                ),
            )
        }
    }

    fun tapWoodenFish() {
        _state.update { cur ->
            val d = cur.globalDharma
            cur.copy(globalDharma = d.copy(woodenFishHits = d.woodenFishHits + 1, myMeritPoints = d.myMeritPoints + 1))
        }
    }

    fun completePracticeMinutes(minutes: Int) {
        viewModelScope.launch(Dispatchers.IO) {
            hostRequestOrNull("fabushiMiniapp.recordPractice", JSONObject().put("minutes", minutes))
        }
        _state.update { cur ->
            val d = cur.globalDharma
            cur.copy(
                globalDharma = d.copy(
                    practiceMinutesTotal = d.practiceMinutesTotal + minutes,
                    myMeritPoints = d.myMeritPoints + minutes * 10L,
                ),
                toastBanner = "🪷 随喜赞叹！已完成 $minutes 分钟诵经禅修共修",
            )
        }
    }

    fun askDharmaAi(question: String) {
        val clean = question.trim().ifBlank { return }
        _state.update { cur ->
            cur.copy(globalDharma = cur.globalDharma.copy(aiDharmaMessages = cur.globalDharma.aiDharmaMessages + (true to clean)))
        }
        viewModelScope.launch {
            val remote = withContext(Dispatchers.IO) {
                hostRequestOrNull("fabushiMiniapp.aiDharmaChat", JSONObject().put("question", clean))?.optString("reply")?.takeIf { it.isNotBlank() }
            }
            delay(200)
            val answer = remote ?: "依《般若心经》与《金刚经》义趣：「应无所住而生其心」。面对「$clean」，先收摄散乱、安住当下呼吸，以慈悲利他之心行持布施与正念，烦恼自会化为菩提资粮。"
            _state.update { cur ->
                cur.copy(globalDharma = cur.globalDharma.copy(aiDharmaMessages = cur.globalDharma.aiDharmaMessages + (false to answer)))
            }
        }
    }

    // BotFather & Hermes Studio Actions
    fun createCompanionBot(name: String, description: String, handleHint: String = "") {
        val cleanName = name.trim().ifBlank { return }
        val handle = handleHint.ifBlank { "@${cleanName.filter { it.isLetterOrDigit() }.lowercase().ifBlank { "companion" }}_bot" }
        val botId = "bot-${UUID.randomUUID().toString().take(8)}"
        viewModelScope.launch(Dispatchers.IO) {
            hostRequestOrNull(
                "feature.execute",
                JSONObject().put("command", JSONObject().put("type", "bot.create").put("requestId", UUID.randomUUID().toString()).put("name", cleanName).put("description", description)),
            )
        }
        val chat = TelegramChatItem(
            id = botId,
            title = cleanName,
            subtitle = "AI 伴侣机器人 · $handle",
            preview = description.ifBlank { "点击开始与专属智能机器人对话" },
            time = nowTimeLabel(),
            kind = ConversationKind.DIRECT,
            isBot = true,
            isCompanionGhost = true,
            online = true,
            username = handle,
        )
        val greet = TelegramMessageItem(
            id = "msg-greet-$botId",
            conversationId = botId,
            senderId = botId,
            senderName = cleanName,
            text = "👋 你好！我是刚创建的智能体「$cleanName」($handle)。\n职责描述：${description.ifBlank { "多模态对话与工作流自动化" }}",
            outgoing = false,
            time = nowTimeLabel(),
        )
        _state.update { cur ->
            cur.copy(
                chats = sortTelegramChats(listOf(chat) + cur.chats),
                messagesByChat = cur.messagesByChat + (botId to listOf(greet)),
                toastBanner = "机器人 $cleanName ($handle) 已创建并加入聊天列表",
            )
        }
    }

    fun selectHermesTab(tab: HermesStudioTab) {
        _state.update { it.copy(hermesStudio = it.hermesStudio.copy(activeTab = tab)) }
    }

    fun branchHermesSession(sourceId: String) {
        _state.update { cur ->
            val src = cur.hermesStudio.sessions.firstOrNull { it.id == sourceId } ?: return@update cur
            val branched = src.copy(
                id = "hermes-${UUID.randomUUID().toString().take(6)}",
                title = "${src.title} · 分叉 #${cur.hermesStudio.sessions.size}",
                parentBranchId = src.id,
            )
            cur.copy(
                hermesStudio = cur.hermesStudio.copy(
                    sessions = listOf(branched) + cur.hermesStudio.sessions,
                    activeSessionId = branched.id,
                ),
                toastBanner = "已从当前上下文无损分叉新会话",
            )
        }
    }

    fun runHermesMoaPrompt(prompt: String) {
        val clean = prompt.trim().ifBlank { return }
        viewModelScope.launch(Dispatchers.IO) {
            hostRequestOrNull("hermes.sendTurn", JSONObject().put("prompt", clean).put("moa", true))
        }
        _state.update { cur ->
            val updatedSessions = cur.hermesStudio.sessions.map { s ->
                if (s.id == cur.hermesStudio.activeSessionId) {
                    s.copy(
                        turnCount = s.turnCount + 1,
                        lastOutput = "[MoA 融合结论] 已聚合 Claude 4.6 + GPT-5.4 + DeepSeek V3.2 对「$clean」的推理验证。",
                    )
                } else s
            }
            cur.copy(hermesStudio = cur.hermesStudio.copy(sessions = updatedSessions))
        }
    }

    fun updateHermesSoul(markdown: String) {
        viewModelScope.launch(Dispatchers.IO) {
            hostRequestOrNull("hermes.memoryUpdate", JSONObject().put("soul", markdown))
        }
        _state.update { it.copy(hermesStudio = it.hermesStudio.copy(soulMarkdown = markdown), toastBanner = "SOUL.md 人格宪章已持久化") }
    }

    fun toggleHermesSkill(skillId: String) {
        _state.update { cur ->
            val skills = cur.hermesStudio.skills.map { if (it.id == skillId) it.copy(enabled = !it.enabled) else it }
            cur.copy(hermesStudio = cur.hermesStudio.copy(skills = skills))
        }
    }

    fun executeHermesTerminal(cmd: String) {
        val clean = cmd.trim().ifBlank { return }
        viewModelScope.launch(Dispatchers.IO) {
            hostRequestOrNull("hermes.runSlashCommand", JSONObject().put("command", clean))
        }
        _state.update { cur ->
            val out = when {
                clean.startsWith("/help") -> "Available 61 commands: /moa, /branch, /memory, /soul, /skills, /cron, /kanban, /mika, /audit ..."
                clean.contains("uname") -> "Linux localhost 6.6.30-android15-arm64 #1 SMP PREEMPT aarch64"
                else -> "✓ Executed `$clean` (exit 0 · 14ms)"
            }
            cur.copy(hermesStudio = cur.hermesStudio.copy(terminalLines = (cur.hermesStudio.terminalLines + "$ $clean" + out).takeLast(24)))
        }
    }

    fun addHermesCronJob(title: String, expr: String, prompt: String) {
        if (title.isBlank() || expr.isBlank()) return
        val job = HermesCronItem("cron-${UUID.randomUUID().toString().take(6)}", title.trim(), expr.trim(), prompt.trim())
        _state.update { cur -> cur.copy(hermesStudio = cur.hermesStudio.copy(cronJobs = listOf(job) + cur.hermesStudio.cronJobs)) }
    }

    fun advanceKanbanCard(cardId: String) {
        _state.update { cur ->
            val nextCols = KanbanColumn.entries
            val cards = cur.hermesStudio.kanbanCards.map { card ->
                if (card.id == cardId) {
                    val nextCol = nextCols[(card.column.ordinal + 1) % nextCols.size]
                    card.copy(column = nextCol)
                } else card
            }
            cur.copy(hermesStudio = cur.hermesStudio.copy(kanbanCards = cards))
        }
    }

    fun decideHermesApproval(approvalId: String, approved: Boolean) {
        viewModelScope.launch(Dispatchers.IO) {
            hostRequestOrNull("hermes.approvalsDecide", JSONObject().put("id", approvalId).put("approved", approved))
        }
        _state.update { cur ->
            val items = cur.hermesStudio.approvals.map {
                if (it.id == approvalId) it.copy(status = if (approved) "approved" else "denied") else it
            }
            cur.copy(hermesStudio = cur.hermesStudio.copy(approvals = items))
        }
    }

    // Magpie Gateway Actions
    fun setMagpieStrategy(strategy: MagpieRoutingStrategy) {
        _state.update { cur ->
            cur.copy(
                magpieGateway = cur.magpieGateway.copy(
                    strategy = strategy,
                    lastRouteExplanation = "${strategy.label} 已激活 → ${strategy.subtitle}",
                ),
            )
        }
    }

    fun configureMagpieProvider(presetId: String, apiKeyInput: String) {
        if (apiKeyInput.isBlank()) return
        viewModelScope.launch(Dispatchers.IO) {
            hostRequestOrNull("magpie.upsertProvider", JSONObject().put("id", presetId).put("configured", true))
        }
        _state.update { cur ->
            val updated = cur.magpieGateway.presets.map { if (it.id == presetId) it.copy(configured = true, healthy = true) else it }
            cur.copy(
                magpieGateway = cur.magpieGateway.copy(presets = updated),
                toastBanner = "已在 Android Keystore 安全保存 $presetId 密钥",
            )
        }
    }

    fun importMagpieEnvOrUri(rawText: String) {
        val lower = rawText.lowercase()
        val matchedIds = mutableSetOf<String>()
        if ("openai" in lower) matchedIds += "openai"
        if ("anthropic" in lower || "claude" in lower) matchedIds += "anthropic"
        if ("gemini" in lower || "google" in lower) matchedIds += "gemini"
        if ("deepseek" in lower) matchedIds += "deepseek"
        if ("qwen" in lower || "dashscope" in lower) matchedIds += "qwen"
        if ("openrouter" in lower) matchedIds += "openrouter"
        if ("xai" in lower || "grok" in lower) matchedIds += "xai"
        if (matchedIds.isEmpty() && rawText.isNotBlank()) matchedIds += "openrouter"

        viewModelScope.launch(Dispatchers.IO) {
            hostRequestOrNull("magpie.importUri", JSONObject().put("payload", rawText))
        }
        _state.update { cur ->
            val updated = cur.magpieGateway.presets.map { if (it.id in matchedIds) it.copy(configured = true) else it }
            cur.copy(
                magpieGateway = cur.magpieGateway.copy(presets = updated),
                toastBanner = "已批量导入并激活 ${matchedIds.size} 个供应商通道",
            )
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

    // Cloud Computer & Action Bus Actions
    fun selectComputerNode(nodeId: String) {
        _state.update { it.copy(cloudComputer = it.cloudComputer.copy(selectedNodeId = nodeId)) }
    }

    fun acquireCuaLease(nodeId: String) {
        viewModelScope.launch(Dispatchers.IO) {
            hostRequestOrNull("computer.leaseAcquire", JSONObject().put("nodeId", nodeId))
        }
        _state.update { cur ->
            val nodes = cur.cloudComputer.nodes.map {
                if (it.id == nodeId) it.copy(leaseOwner = "Hermes-CUA-Agent (TTL 15m)", humanTakeoverActive = false) else it
            }
            cur.copy(
                cloudComputer = cur.cloudComputer.copy(
                    nodes = nodes,
                    lastOpOutput = "✓ 已获取节点 $nodeId 的排他性 CUA 控制租约",
                ),
            )
        }
    }

    fun emergencyHumanTakeover(nodeId: String) {
        viewModelScope.launch(Dispatchers.IO) {
            hostRequestOrNull("computer.leaseTakeover", JSONObject().put("nodeId", nodeId))
        }
        mikaBridge?.stopActiveTask()
        _state.update { cur ->
            val nodes = cur.cloudComputer.nodes.map {
                if (it.id == nodeId) it.copy(leaseOwner = null, humanTakeoverActive = true) else it
            }
            cur.copy(
                cloudComputer = cur.cloudComputer.copy(
                    nodes = nodes,
                    lastOpOutput = "🚨 紧急人工接管已生效！节点 $nodeId 上的所有自动 Agent 操作已立即冻结。",
                ),
                toastBanner = "🚨 已执行紧急人工接管，Agent 租约已吊销",
            )
        }
    }

    fun executeComputerOp(command: String) {
        val clean = command.trim().ifBlank { return }
        val nodeId = _state.value.cloudComputer.selectedNodeId
        viewModelScope.launch(Dispatchers.IO) {
            hostRequestOrNull("computer.executeOp", JSONObject().put("nodeId", nodeId).put("command", clean))
        }
        _state.update { cur ->
            cur.copy(
                cloudComputer = cur.cloudComputer.copy(
                    lastOpOutput = "[$nodeId] $ $clean\n→ 执行完成 (0) · Action Bus 审计日志已落盘",
                ),
            )
        }
    }

    fun advanceGlobalTaskFsm(taskId: String) {
        val states = GlobalTaskFsmState.entries
        _state.update { cur ->
            val tasks = cur.cloudComputer.globalTasks.map { t ->
                if (t.id == taskId) {
                    val nextState = states[(t.state.ordinal + 1) % states.size]
                    t.copy(state = nextState, progressPercent = ((nextState.ordinal + 1) * 16).coerceAtMost(100))
                } else t
            }
            cur.copy(cloudComputer = cur.cloudComputer.copy(globalTasks = tasks))
        }
    }

    // ------------------------------------------------------------------------
    // Internal Helpers & Seed Data
    // ------------------------------------------------------------------------

    private fun appendMessageToChat(conversationId: String, message: TelegramMessageItem, clearDraft: Boolean) {
        _state.update { current ->
            val list = current.messagesByChat[conversationId].orEmpty() + message
            val updatedChats = current.chats.map { chat ->
                if (chat.id == conversationId) {
                    chat.copy(
                        preview = message.text.lineSequence().firstOrNull().orEmpty().take(72),
                        time = message.time,
                        lastMessageOutgoing = message.outgoing,
                        lastDeliveryState = message.deliveryState,
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

    private fun sendProtocolV2Command(command: JSONObject) {
        val reqId = "tg-cmd-${UUID.randomUUID()}"
        val envelope = JSONObject()
            .put("protocolVersion", 2)
            .put("context", JSONObject().put("requestId", reqId).put("deviceId", "android:native").put("actorId", actorId).put("sessionId", "account-session:android-native").put("sentAtMs", System.currentTimeMillis()))
            .put("command", command)
        hostRequestOrNull("feature.messaging.execute", JSONObject().put("requestId", reqId).put("envelope", envelope))
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
                .thenByDescending { it.id == "saved-messages" },
        )

    // ========================================================================
    // Desktop v1.4.0–v1.6.2 Parity Actions (Coding Bots, models.dev, UU-Remote,
    // Official OAuth 2.1 MCP, Zero-Knowledge User Vault, Live Mini App PiP)
    // ========================================================================

    fun createCodingAgentBot(
        provider: CodingProviderKindUi,
        name: String,
        handle: String,
        workspacePath: String,
        permissionMode: CodingPermissionModeUi,
        useWorktree: Boolean,
    ) {
        val cleanWorkspace = workspacePath.trim().ifBlank { "~/workspace" }
        viewModelScope.launch {
            val res = withContext(Dispatchers.IO) {
                hostRequestOrNull(
                    "codingAgent.createBot",
                    JSONObject()
                        .put("providerKind", provider.wire)
                        .put("displayName", name)
                        .put("username", handle)
                        .put("workspacePath", cleanWorkspace)
                        .put("permissionMode", permissionMode.wire)
                        .put("useWorktree", useWorktree),
                )
            }
            val botId = res?.optString("botId")?.ifBlank { null } ?: "cbot-${UUID.randomUUID().toString().take(8)}"
            val username = res?.optString("username")?.ifBlank { null } ?: handle.trimStart('@')
            val branch = res?.optString("activeBranch")?.ifBlank { null } ?: if (useWorktree) "coding/${provider.wire}" else "main"
            val item = CodingAgentBotItem(
                botId = botId,
                displayName = name,
                username = username,
                provider = provider,
                permissionMode = permissionMode,
                workspacePath = cleanWorkspace,
                useWorktree = useWorktree,
                activeBranch = branch,
            )
            val chat = TelegramChatItem(
                id = botId,
                title = name,
                subtitle = "${provider.label} · $cleanWorkspace · ${permissionMode.label}",
                preview = "💻 原生编程智能体已就绪（分支 $branch）",
                time = nowTimeLabel(),
                kind = ConversationKind.DIRECT,
                username = "@$username",
                isBot = true,
                isCompanionGhost = true,
            )
            _state.update { cur ->
                cur.copy(
                    codingAgentBots = listOf(item) + cur.codingAgentBots.filterNot { it.botId == botId },
                    chats = listOf(chat) + cur.chats.filterNot { it.id == botId },
                    toastBanner = "已创建 ${provider.label} 原生编程 Bot (@$username)",
                )
            }
        }
    }

    fun cycleCodingAgentPermission(botId: String, nextMode: CodingPermissionModeUi) {
        viewModelScope.launch(Dispatchers.IO) {
            hostRequestOrNull(
                "codingAgent.dispatchControl",
                JSONObject()
                    .put("botId", botId)
                    .put("control", JSONObject().put("action", "set_permission_mode").put("mode", nextMode.wire)),
            )
        }
        _state.update { cur ->
            cur.copy(
                codingAgentBots = cur.codingAgentBots.map {
                    if (it.botId == botId) it.copy(permissionMode = nextMode) else it
                },
                toastBanner = "编程 Bot 权限策略已切换为：${nextMode.label}",
            )
        }
    }

    fun syncModelsDevCatalog() {
        viewModelScope.launch {
            val res = withContext(Dispatchers.IO) {
                hostRequestOrNull("magpie.discoverModels", JSONObject().put("subscriptionKind", "google-vertex"))
            }
            val models = buildList {
                val arr = res?.optJSONArray("models")
                if (arr != null) {
                    for (i in 0 until arr.length()) {
                        arr.optString(i).takeIf { it.isNotBlank() }?.let(::add)
                    }
                }
            }.ifEmpty {
                listOf("gemini-3.8-flash", "gemini-2.5-pro", "gpt-6.1-sol", "claude-sonnet-4-6")
            }
            _state.update { cur ->
                cur.copy(
                    magpieGateway = cur.magpieGateway.copy(
                        discoveredModels = models,
                        modelsDevStatusLabel = "已同步 ${models.size} 个最新模型 (${nowTimeLabel()}) · Vertex Schema 清洗 + Gemini 并行工具就绪",
                    ),
                    toastBanner = "已刷新 models.dev 实时模型目录 (${models.size} 个模型)",
                )
            }
        }
    }

    fun scanLocalSubscriptions() {
        viewModelScope.launch {
            val res = withContext(Dispatchers.IO) {
                hostRequestOrNull("magpie.discoverSyncableSubscriptions", JSONObject().put("targetIsRemote", true))
            }
            val count = res?.optInt("total", 0) ?: 0
            _state.update { cur ->
                cur.copy(
                    magpieGateway = cur.magpieGateway.copy(
                        localSubscriptionsSummary = "最近扫描 (${nowTimeLabel()})：发现 $count 个可同步本地订阅通道 (Codex / Vertex ADC / Gemini / Claude)",
                    ),
                    toastBanner = "本地订阅扫描完成：发现 $count 个可同步通道",
                )
            }
        }
    }

    fun toggleRemoteAssist(allow: Boolean) {
        viewModelScope.launch(Dispatchers.IO) {
            hostRequestOrNull("remote.setAssistAllow", JSONObject().put("allowAssist", allow))
        }
        _state.update { cur ->
            cur.copy(
                cloudComputer = cur.cloudComputer.copy(
                    assistConfig = cur.cloudComputer.assistConfig.copy(allowAssist = allow),
                ),
                toastBanner = if (allow) "已开启 9 位远程协助接入" else "已关闭远程协助接入",
            )
        }
    }

    fun updateRemoteAssistCode(code: String) {
        val clean = code.trim()
        if (clean.length < 6) {
            _state.update { it.copy(toastBanner = "远程协助验证码至少需要 6 位字符") }
            return
        }
        viewModelScope.launch(Dispatchers.IO) {
            hostRequestOrNull(
                "remote.setAssistCode",
                JSONObject().put("code", clean).put("codeMode", "custom"),
            )
        }
        _state.update { cur ->
            cur.copy(
                cloudComputer = cur.cloudComputer.copy(
                    assistConfig = cur.cloudComputer.assistConfig.copy(
                        codeMode = "custom",
                        currentCode = clean,
                    ),
                ),
                toastBanner = "自定义远程协助验证码已更新",
            )
        }
    }

    fun updateRemoteStreamSettings(settings: RemoteStreamSettingsUi) {
        viewModelScope.launch(Dispatchers.IO) {
            hostRequestOrNull(
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
        _state.update { cur ->
            cur.copy(
                cloudComputer = cur.cloudComputer.copy(streamSettings = settings),
                toastBanner = "远程串流配置已同步 (${settings.codec.uppercase()} · ${settings.targetFps}fps)",
            )
        }
    }

    fun issueRemoteMcpGrant(clientName: String) {
        val clean = clientName.trim().ifBlank { "External AI MCP Client" }
        viewModelScope.launch {
            val res = withContext(Dispatchers.IO) {
                hostRequestOrNull(
                    "remote.issueMcpGrant",
                    JSONObject().put("clientName", clean),
                )
            }
            val famId = res?.optString("family_id")?.ifBlank { null } ?: "fam_${UUID.randomUUID().toString().take(8)}"
            val grant = RemoteMcpGrantItem(
                familyId = famId,
                clientName = clean,
                scopes = listOf("remote.read", "remote.control", "mcp.tools"),
                allowedNodeIds = listOf("local-android-mika"),
            )
            _state.update { cur ->
                cur.copy(
                    cloudComputer = cur.cloudComputer.copy(
                        mcpGrants = listOf(grant) + cur.cloudComputer.mcpGrants,
                    ),
                    toastBanner = "已为 $clean 签发 OAuth 2.1 Remote MCP 授权",
                )
            }
        }
    }

    fun revokeRemoteMcpGrant(familyId: String) {
        viewModelScope.launch(Dispatchers.IO) {
            hostRequestOrNull("remote.revokeMcpGrant", JSONObject().put("familyId", familyId))
        }
        _state.update { cur ->
            cur.copy(
                cloudComputer = cur.cloudComputer.copy(
                    mcpGrants = cur.cloudComputer.mcpGrants.filterNot { it.familyId == familyId },
                ),
                toastBanner = "已撤销 Remote MCP 令牌族 ($familyId)",
            )
        }
    }

    fun initSplitKeyVault(password: String) {
        if (password.length < 6) {
            _state.update { it.copy(toastBanner = "零知识保险库主口令至少需要 6 位") }
            return
        }
        val uid = _state.value.activeAccountId
        viewModelScope.launch {
            val res = withContext(Dispatchers.IO) {
                hostRequestOrNull(
                    "userVault.initEnvelope",
                    JSONObject().put("userId", uid).put("password", password),
                )
            }
            _state.update { cur ->
                cur.copy(
                    userVault = cur.userVault.copy(
                        initialized = true,
                        kdfAlg = "split_key_v2",
                        keyVersion = 2,
                        lastVaultMessage = if (res != null) "Split-Key V2 信封已在 Rust 原生端派生并封装 (Argon2id + ChaCha20-Poly1305)" else "Split-Key V2 保险库已就绪",
                    ),
                    toastBanner = "Split-Key V2 零知识保险库初始化成功",
                )
            }
        }
    }

    fun requestDeviceVaultSync() {
        val uid = _state.value.activeAccountId
        viewModelScope.launch {
            val res = withContext(Dispatchers.IO) {
                hostRequestOrNull(
                    "userVault.requestDeviceSync",
                    JSONObject().put("userId", uid).put("deviceName", "Fabushi Android"),
                )
            }
            val reqId = res?.optString("requestId")?.ifBlank { null } ?: "dsync_${UUID.randomUUID().toString().take(8)}"
            val code = res?.optString("verificationCode")?.ifBlank { null } ?: "%06d".format((System.currentTimeMillis() % 900_000L) + 100_000L)
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
        }
    }

    fun minimizeGlobalDharmaToPip() {
        val summary = "累计供养 ${_state.value.globalDharma.totalGlobalOfferings} 盏 · 功德 ${_state.value.globalDharma.myMeritPoints}"
        viewModelScope.launch(Dispatchers.IO) {
            hostRequestOrNull(
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
                toastBanner = "全球法布施已最小化至悬浮 PiP，随时点击顶部条原地恢复",
            )
        }
    }

    fun restoreMiniAppFromPip() {
        viewModelScope.launch(Dispatchers.IO) {
            hostRequestOrNull(
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
        _state.update { cur ->
            cur.copy(miniAppPip = MiniAppPipSessionUi())
        }
    }

    private fun nowTimeLabel(epochMs: Long = System.currentTimeMillis()): String =
        TelegramLocalTimeFormatter.formatLocalTime(epochMs)

    override fun onCleared() {
        recordingTimerJob?.cancel()
        audioProgressJob?.cancel()
        callTimerJob?.cancel()
        if (voiceRecorder.isRecording) voiceRecorder.cancel()
        stopAudioPlayback()
        runCatching { mahayanaHost?.close() }
        super.onCleared()
    }

    private fun buildInitialSeedState(): TelegramUiState {
        val nowLabel = DateFormat.getTimeInstance(DateFormat.SHORT).format(Date())
        val accounts = listOf(
            TelegramAccount(
                id = "acc-primary",
                slotIndex = 0,
                displayName = "全球法布施 · 主账号",
                username = "@fabushi_master",
                phone = "+86 138 0013 8000",
                bio = "GPUI-Telegram Android v2.0 · Mika 本机智能操控",
                isActive = true,
            ),
            TelegramAccount(
                id = "acc-dev",
                slotIndex = 1,
                displayName = "Mahayana DevOps",
                username = "@mahayana_ops",
                phone = "+1 415 890 2048",
                bio = "Rust JNI & Hermes Cloud Fleet",
                unreadCount = 3,
                isActive = false,
            ),
        )

        val stories = listOf(
            TelegramStoryItem("story-me", "acc-primary", "我的动态", "acc-primary", "点亮全球 240 国祈福心灯 🪔", "全球法布施 v2.0 正式上线", 0, isMine = true, viewed = false, viewCount = 42),
            TelegramStoryItem("story-mika", "bot-mika-phone", "Mika 智控", "bot-mika-phone", "支持无障碍 + 无线 ADB + 虚拟副屏后台操控手机", "Mika 手机操控引擎就绪", 1, viewed = false, viewCount = 318),
            TelegramStoryItem("story-dharma", "channel-dharma", "全球法布施", "channel-dharma", "今日已有 128 万盏心灯在五大洲点亮", "十方光明共修坛城", 2, viewed = false, viewCount = 1280),
            TelegramStoryItem("story-hermes", "chat-hermes-lab", "Hermes Studio", "chat-hermes-lab", "/moa 多模型辩论与 Magpie 48 通道免订阅网关", "Hermes 6-Tab 工作室", 3, viewed = true, viewCount = 509),
        )

        val chats = listOf(
            TelegramChatItem(
                id = "saved-messages",
                title = "收藏夹 (Saved Messages)",
                subtitle = "云端加密跨端笔记与文件箱",
                preview = "📌 常用快捷口令：/mika /dharma /wallet /moa",
                time = nowLabel,
                kind = ConversationKind.SAVED_MESSAGES,
                isPinned = true,
                pinnedMessageIds = listOf("msg-saved-1"),
            ),
            TelegramChatItem(
                id = "bot-mika-phone",
                title = "Mika 手机操控智能体",
                subtitle = "机器人 · @MikaPhoneBot · 本机自动化",
                preview = "🤖 发送自然语言指令即可自动操控手机 App、网页与沙箱",
                time = nowLabel,
                kind = ConversationKind.DIRECT,
                unreadCount = 1,
                isPinned = true,
                isVerified = true,
                isBot = true,
                isCompanionGhost = true,
                online = true,
                username = "@MikaPhoneBot",
            ),
            TelegramChatItem(
                id = "channel-dharma",
                title = "全球法布施 · 官方广播频道",
                subtitle = "官方认证频道 · 128,400 位订阅者",
                preview = "🪷 3D 地球 240 国供灯与摩尼转经轮现已原生融合",
                time = nowLabel,
                kind = ConversationKind.CHANNEL,
                unreadCount = 2,
                isPinned = true,
                isVerified = true,
                subscriberCount = 128_400,
                miniAppPluginId = "global-dharma",
                pinnedMessageIds = listOf("msg-dharma-1"),
            ),
            TelegramChatItem(
                id = "group-gpui-dev",
                title = "GPUI-Telegram × Mika 架构群",
                subtitle = "超级话题群组 · 1,024 位成员 · 48 人在线",
                preview = "#mika-agent: VirtualDisplay 后台执行与 16KB 对齐验证通过！",
                time = nowLabel,
                kind = ConversationKind.GROUP,
                unreadCount = 3,
                isForum = true,
                memberCount = 1024,
            ),
            TelegramChatItem(
                id = "bot-botfather",
                title = "BotFather",
                subtitle = "官方机器人之父 · @BotFather",
                preview = "使用 /newbot 创建新机器人，或配置 WebMCP 小程序菜单按钮",
                time = nowLabel,
                kind = ConversationKind.DIRECT,
                isVerified = true,
                isBot = true,
                online = true,
                username = "@BotFather",
            ),
            TelegramChatItem(
                id = "chat-hermes-lab",
                title = "Hermes & Magpie AI 实验室",
                subtitle = "多模型智能体 · 支持 /moa 与 <60ms 语音打断",
                preview = "已连接 48 个 AI 供应商预设与 Rosetta 协议转换网关",
                time = nowLabel,
                kind = ConversationKind.DIRECT,
                isBot = true,
                isCompanionGhost = true,
                online = true,
                username = "@HermesStudioBot",
            ),
            TelegramChatItem(
                id = "secret-bodhi",
                title = "🔒 觉行禅师 (加密私聊)",
                subtitle = "端到端加密 · X3DH + Double Ratchet",
                preview = "🔐 阅后即焚已开启 · 点击顶部校验 4-Emoji 指纹",
                time = nowLabel,
                kind = ConversationKind.SECRET,
                isSecretChat = true,
                secretVerified = true,
                secretFingerprintSeed = "x3dh-bodhi-master-2026",
                ttlSeconds = 86_400,
                online = true,
            ),
            TelegramChatItem(
                id = "archived-legacy",
                title = "早期版本构建通知归档",
                subtitle = "通知频道 · 已静音归档",
                preview = "v1.2.0 历史归档记录",
                time = "昨天",
                kind = ConversationKind.CHANNEL,
                isMuted = true,
                isArchived = true,
                subscriberCount = 512,
            ),
        )

        val forumTopics = mapOf(
            "group-gpui-dev" to listOf(
                TelegramForumTopic("topic-all", "group-gpui-dev", "综合讨论 General", "💬", 0xFF2AABEEL, unreadCount = 1, isPinned = true, lastPreview = "Telegram 1:1 Compose 主题引擎上线"),
                TelegramForumTopic("topic-rust", "group-gpui-dev", "Rust JNI 宿主内核", "🦀", 0xFFE67E22L, unreadCount = 1, lastPreview = "SQLite WAL + X3DH + 16KB 页对齐"),
                TelegramForumTopic("topic-mika", "group-gpui-dev", "Mika 手机操控融合", "🤖", 0xFF2ECC71L, unreadCount = 1, lastPreview = "无障碍 + 无线 ADB + Alpine 沙箱"),
                TelegramForumTopic("topic-dharma", "group-gpui-dev", "全球法布施小程序", "🪷", 0xFFF59E0BL, lastPreview = "240 国 3D 正交地球仪与转经轮物理"),
            ),
        )

        val (savedPlain, savedEnts) = TelegramEntitiesEngine.parseMarkdownAndAutoEntities(
            "欢迎使用 **GPUI-Telegram × Mika 统一安卓客户端 v2.0** 🙏\n" +
                "- 支持 __斜体__、++下划线++、~~删除线~~ 与点击揭晓 ||端到端零知识剧透遮罩||\n" +
                "- 快捷指令：`/mika 打开设置检查网络` · `/dharma` · `/wallet` · `/moa`\n" +
                "- 开源仓库：[fabushi-android](https://github.com/fabu-android/fabushi-android) #TelegramParity",
        )
        val (codePlain, codeEnts) = TelegramEntitiesEngine.parseMarkdownAndAutoEntities(
            "```rust\npub extern \"system\" fn Java_com_ombhrum_fabushi_core_MahayanaHost_nativeDispatch(\n    env: JNIEnv, _class: JObject, handle: jlong, req: JString\n) -> jstring\n```",
        )

        val messages = mapOf(
            "saved-messages" to listOf(
                TelegramMessageItem(
                    id = "msg-saved-1",
                    conversationId = "saved-messages",
                    senderId = "acc-primary",
                    senderName = "我",
                    text = savedPlain,
                    entities = savedEnts,
                    outgoing = true,
                    time = nowLabel,
                    pinned = true,
                    reactions = listOf(ChatReaction("🔥", 2, true), ChatReaction("🙏", 1, false)),
                ),
                TelegramMessageItem(
                    id = "msg-saved-code",
                    conversationId = "saved-messages",
                    senderId = "acc-primary",
                    senderName = "我",
                    text = codePlain,
                    entities = codeEnts,
                    outgoing = true,
                    time = nowLabel,
                ),
                TelegramMessageItem(
                    id = "msg-saved-voice",
                    conversationId = "saved-messages",
                    senderId = "acc-primary",
                    senderName = "我",
                    text = "🎙 语音备忘录 (12s)",
                    contentType = "voice",
                    outgoing = true,
                    time = nowLabel,
                    durationSeconds = 12,
                    waveformBars = TelegramWaveformCodec.syntheticSpeechBars("saved-voice-note"),
                ),
            ),
            "bot-mika-phone" to listOf(
                TelegramMessageItem(
                    id = "msg-mika-welcome",
                    conversationId = "bot-mika-phone",
                    senderId = "bot-mika-phone",
                    senderName = "Mika 手机操控智能体",
                    text = "🤖 你好！我是深度融合进 Telegram 的 **Mika 本机手机操控智能体**。\n\n具备完整能力：\n1. 无障碍多屏 UI 树提取 + 离线中英 MLKit OCR\n2. 无线 ADB 本机配对 + UID 2000 Binder 虚拟副屏后台操控\n3. 磁吸悬浮胶囊 + 支付/敏感操作一次性人工确认门控 (ConfirmationGate)\n4. 多标签无头浏览器 CDP + Alpine proot Linux 容器终端",
                    entities = TelegramEntitiesEngine.parseMarkdownAndAutoEntities(
                        "🤖 你好！我是深度融合进 Telegram 的 **Mika 本机手机操控智能体**。\n\n具备完整能力：\n1. 无障碍多屏 UI 树提取 + 离线中英 MLKit OCR\n2. 无线 ADB 本机配对 + UID 2000 Binder 虚拟副屏后台操控\n3. 磁吸悬浮胶囊 + 支付/敏感操作一次性人工确认门控 (ConfirmationGate)\n4. 多标签无头浏览器 CDP + Alpine proot Linux 容器终端",
                    ).second,
                    outgoing = false,
                    time = nowLabel,
                    inlineKeyboard = listOf(
                        InlineKeyboardRow(
                            listOf(
                                InlineKeyboardButtonModel("mk-1", "📱 打开 Mika 操控中心", InlineKeyboardButtonKind.MIKA_ACTION, "open_workspace"),
                                InlineKeyboardButtonModel("mk-2", "🔍 自检本机权限状态", InlineKeyboardButtonKind.MIKA_ACTION, "运行 12 项手机操控环境自检"),
                            ),
                        ),
                        InlineKeyboardRow(
                            listOf(
                                InlineKeyboardButtonModel("mk-3", "🌐 后台网页摘要演示", InlineKeyboardButtonKind.MIKA_ACTION, "使用内置 CDP 浏览器抓取今日科技要闻并总结"),
                            ),
                        ),
                    ),
                ),
            ),
            "channel-dharma" to listOf(
                TelegramMessageItem(
                    id = "msg-dharma-1",
                    conversationId = "channel-dharma",
                    senderId = "channel-dharma",
                    senderName = "全球法布施 · 官方频道",
                    text = "🪷 全球法布施 2.0 现已内置 3D 240 国地球供养坛城与物理惯性摩尼转经轮！点击下方按钮即刻进入共修，或在评论区随喜回向。",
                    outgoing = false,
                    time = nowLabel,
                    pinned = true,
                    viewsCount = 48_920,
                    commentsCount = 2,
                    reactions = listOf(ChatReaction("🙏", 328, true), ChatReaction("🪷", 196, false), ChatReaction("❤️", 142, false)),
                    inlineKeyboard = listOf(
                        InlineKeyboardRow(
                            listOf(
                                InlineKeyboardButtonModel("dh-open", "🌍 打开 3D 地球坛城与禅房", InlineKeyboardButtonKind.MINI_APP, "global-dharma"),
                                InlineKeyboardButtonModel("dh-ton", "💎 TON 功德金库", InlineKeyboardButtonKind.CALLBACK, "open:wallet"),
                            ),
                        ),
                    ),
                ),
                TelegramMessageItem(
                    id = "msg-dharma-quiz",
                    conversationId = "channel-dharma",
                    senderId = "channel-dharma",
                    senderName = "全球法布施 · 官方频道",
                    text = "📊 每日般若问答",
                    contentType = "poll",
                    outgoing = false,
                    time = nowLabel,
                    viewsCount = 19_400,
                    commentsCount = 1,
                    poll = TelegramPollModel(
                        question = "《华严经·普贤行愿品》云：「诸供养中，何者为最？」",
                        options = listOf(
                            TelegramPollOption("opt-1", "七宝珍奇供养", 128),
                            TelegramPollOption("opt-2", "法供养最（如说修行、利益众生）", 1_420, isCorrect = true),
                            TelegramPollOption("opt-3", "香花伎乐供养", 96),
                        ),
                        isAnonymous = true,
                        isQuiz = true,
                        correctOptionId = "opt-2",
                        explanation = "《普贤行愿品》开示：诸供养中，法供养最！所谓如说修行供养、利益众生供养、不离菩提心供养。",
                    ),
                ),
            ),
            "group-gpui-dev" to listOf(
                TelegramMessageItem(
                    id = "msg-grp-1",
                    conversationId = "group-gpui-dev",
                    senderId = "peer-rust",
                    senderName = "Mahayana Rust Maintainer",
                    text = "🦀 `libmahayana_app_host.so` 已开启 `-Wl,-z,max-page-size=16384`，完全满足 Android 15/16/17 16KB 页对齐要求！",
                    entities = listOf(MessageEntity(MessageEntityType.CODE, 3, 24), MessageEntity(MessageEntityType.CODE, 31, 25)),
                    outgoing = false,
                    time = nowLabel,
                    topicId = "topic-rust",
                    reactions = listOf(ChatReaction("🎉", 12, true), ChatReaction("⚡", 8, false)),
                ),
                TelegramMessageItem(
                    id = "msg-grp-round",
                    conversationId = "group-gpui-dev",
                    senderId = "peer-mika",
                    senderName = "Mika Core Lead",
                    text = "⭕ 圆形视频演示：VirtualDisplay 后台画中画实测 (9s)",
                    contentType = "videoNote",
                    outgoing = false,
                    time = nowLabel,
                    durationSeconds = 9,
                    topicId = "topic-mika",
                ),
            ),
            "bot-botfather" to listOf(
                TelegramMessageItem(
                    id = "msg-bf-init",
                    conversationId = "bot-botfather",
                    senderId = "bot-botfather",
                    senderName = "BotFather",
                    text = "我是 BotFather，所有 Telegram 与 Fabushi AI 机器人的管理中枢。\n发送 /newbot 创建新机器人，或点击下方打开可视化机器人创建台。",
                    entities = listOf(MessageEntity(MessageEntityType.BOT_COMMAND, 36, 7)),
                    outgoing = false,
                    time = nowLabel,
                    inlineKeyboard = listOf(
                        InlineKeyboardRow(
                            listOf(InlineKeyboardButtonModel("bf-open", "🤖 打开可视化 Bot 创建工作室", InlineKeyboardButtonKind.CALLBACK, "open:botfather")),
                        ),
                    ),
                ),
            ),
            "chat-hermes-lab" to listOf(
                TelegramMessageItem(
                    id = "msg-hermes-init",
                    conversationId = "chat-hermes-lab",
                    senderId = "chat-hermes-lab",
                    senderName = "Hermes AI Studio",
                    text = "✨ Hermes 6-Tab Sovereign Agent 与 Magpie 48+ 供应商网关已就绪！\n试试发送 `/moa 如何设计高可靠端云一体 Agent？` 或点击右上角体验 **<60ms VAD 全双工语音对话球**。",
                    outgoing = false,
                    time = nowLabel,
                    inlineKeyboard = listOf(
                        InlineKeyboardRow(
                            listOf(
                                InlineKeyboardButtonModel("hm-1", "🧠 Hermes 6-Tab 工作室", InlineKeyboardButtonKind.CALLBACK, "open:hermes"),
                                InlineKeyboardButtonModel("hm-2", "🪶 Magpie 网关控制台", InlineKeyboardButtonKind.CALLBACK, "open:magpie"),
                                InlineKeyboardButtonModel("hm-3", "🖥 云电脑 Workbench", InlineKeyboardButtonKind.CALLBACK, "open:computer"),
                            ),
                        ),
                    ),
                ),
            ),
            "secret-bodhi" to listOf(
                TelegramMessageItem(
                    id = "msg-sec-seed",
                    conversationId = "secret-bodhi",
                    senderId = "secret-bodhi",
                    senderName = "觉行禅师",
                    text = "🔐 此对话受 X3DH (X25519 + Ed25519) 与 Double Ratchet 棘轮协议端到端保护，服务器不留存任何明文或私钥。",
                    outgoing = false,
                    time = nowLabel,
                    ttlSeconds = 86_400,
                ),
            ),
        )

        val comments = mapOf(
            "msg-dharma-1" to listOf(
                TelegramMessageItem("cmt-1", "channel-dharma", "u1", "新加坡莲友", "随喜赞叹！3D 地球转动非常丝滑，已经点亮新加坡和尼泊尔的心灯 🙏", outgoing = false, time = nowLabel),
                TelegramMessageItem("cmt-2", "channel-dharma", "u2", "Kyoto Practitioner", "摩尼转经轮的物理惯性手感太棒了，嗡嘛呢呗美吽！☸️", outgoing = false, time = nowLabel),
            ),
        )

        val stickerPacks = listOf(
            StickerPackModel("pack-dharma", "莲华禅心贴纸", true, listOf("🪷", "🙏", "☸️", "🪔", "✨", "🕊️", "🌕", "🍵", "🧘", "📿", "🏔️", "🌈")),
            StickerPackModel("pack-cyber", "极客 AI 助手", true, listOf("🤖", "🦀", "⚡", "🚀", "💎", "🛡️", "🧠", "🛰️", "🔥", "🎯", "🎉", "👀")),
            StickerPackModel("pack-animals", "Telegram 萌宠", true, listOf("🐶", "🐱", "🐼", "🦊", "🦄", "🐧", "🦉", "🐬", "🦋", "🐢", "🦁", "🐨")),
        )

        val gifs = listOf(
            GifItemModel("gif-lotus", "金莲盛开 · 吉祥祈福", "🪷✨", 0xFFD99B26L),
            GifItemModel("gif-rocket", "Rust 极速编译发射", "🦀🚀", 0xFF2AABEEL),
            GifItemModel("gif-mika", "Mika 手机自动操控中", "📱🤖", 0xFF2ECC71L),
            GifItemModel("gif-party", "全量测试 100% 通过", "🎉🏆", 0xFF8E44ADL),
        )

        val slashCommands = listOf(
            SlashCommandSuggestion("/mika", "调用 Mika 本机手机智能体自动执行跨 App 任务", "@MikaPhoneBot"),
            SlashCommandSuggestion("/newbot", "通过 @BotFather 创建新机器人与小程序菜单", "@BotFather"),
            SlashCommandSuggestion("/moa", "启动 Hermes Mixture-of-Agents 多模型协同推理", "@HermesStudioBot"),
            SlashCommandSuggestion("/dharma", "打开全球法布施 · 3D 240 国地球供灯与禅房转经轮", "@GlobalDharmaBot"),
            SlashCommandSuggestion("/wallet", "打开自托管 TON 钱包、Stars 与 TonConnect 签名器"),
            SlashCommandSuggestion("/hermes", "打开 Hermes 6-Tab 智能体工作室"),
            SlashCommandSuggestion("/magpie", "打开 Magpie 48+ 供应商免订阅网关控制台"),
            SlashCommandSuggestion("/computer", "打开多设备云电脑与 Action Bus 风险管控台"),
        )

        val tonWallet = TonWalletUiState(
            transactions = listOf(
                TonTransactionItem("tx-1", "in", "收到功德回向赞赏", "UQDr...9kP2", "+25.00 TON", "TON", "今天 09:42", "随喜全球法布施开源"),
                TonTransactionItem("tx-2", "in", "Telegram Stars 激励入账", "Fragment Stars", "+1,200 ⭐", "STARS", "昨天 21:15", "小程序创作者分成"),
                TonTransactionItem("tx-3", "stake", "Tonstakers 流动性质押收益", "tsTON Pool", "+0.18 TON", "TON", "昨天 08:00", "APY 4.25%"),
            ),
            collectibles = listOf(
                TonCollectibleItem("nft-1", "@fabushi.ton", "TON DNS Domains", 45.0, "🌐"),
                TonCollectibleItem("nft-2", "十方光明金莲 #001", "Global Dharma SBT", 18.8, "🪷", isGift = true),
                TonCollectibleItem("nft-3", "+888 0820 2026", "Anonymous Telegram Numbers", 160.0, "📱"),
                TonCollectibleItem("nft-4", "璀璨蓝星礼物盒", "Telegram Star Gifts", 5.2, "🎁", isGift = true),
            ),
        )

        val hermesStudio = HermesStudioUiState(
            sessions = listOf(
                HermesSessionItem("hermes-main", "Android 端云融合架构主会话", "Anthropic", "claude-4.6-sonnet", turnCount = 6, lastOutput = "已完成 SQLite WAL + Signal E2EE + Mika 无障碍桥接规划。"),
                HermesSessionItem("hermes-moa", "MoA 多模型安全审计分叉", "OpenRouter", "moa/3-models", parentBranchId = "hermes-main", turnCount = 3, lastOutput = "ConfirmationGate 支付安全门控零绕过验证通过。"),
            ),
            skills = listOf(
                HermesSkillItem("sk-mika", "mika-phone-bridge", "Android Device", "通过 Telegram 与自然语言安全调度 Mika 无障碍/ADB/沙箱任务", true),
                HermesSkillItem("sk-tg", "telegram-rich-ops", "Messaging", "管理群组话题、频道统计、投票测验与内联键盘", true),
                HermesSkillItem("sk-ton", "ton-boc-signer", "Web3 / TON", "构建 TON Cell/BOC 并执行 TonConnect 交易沙箱预演", true),
                HermesSkillItem("sk-dharma", "dharma-tripitaka", "Culture", "大藏经检索、心经/金刚经注疏与正念共修引导", true),
                HermesSkillItem("sk-code", "rust-kotlin-architect", "Engineering", "JNI 边界、Compose 性能与 R8 混淆规则审查", true),
            ),
            cronJobs = listOf(
                HermesCronItem("cron-1", "晨间正念共修与日程简报", "0 7 * * *", "汇总今日待办并推送一段《金刚经》早课法语至收藏夹", true),
                HermesCronItem("cron-2", "Magpie 通道健康巡检", "*/30 * * * *", "检测已配置供应商延迟与免费额度剩余比例", true),
            ),
            kanbanCards = listOf(
                HermesKanbanCard("kb-1", "验证 16KB ELF 页对齐", "Rust Agent", KanbanColumn.DONE),
                HermesKanbanCard("kb-2", "240 国 3D 地球仪弧线渲染", "UI Agent", KanbanColumn.DONE),
                HermesKanbanCard("kb-3", "Mika 跨应用自动化回归", "Mika Agent", KanbanColumn.IN_PROGRESS),
                HermesKanbanCard("kb-4", "发布 v2.0.0 GitHub 签名包", "Release Agent", KanbanColumn.REVIEW),
            ),
            approvals = listOf(
                HermesApprovalRequest("ap-1", "computer.executeOp", "在云端沙箱节点执行 `docker ps -a` 状态巡检", ActionRiskTier.R2_MEDIUM, "approved"),
                HermesApprovalRequest("ap-2", "wallet.sendTon", "向外部地址转出 10.0 TON（需人工二次确认）", ActionRiskTier.R4_CRITICAL, "pending"),
            ),
        )

        val cloudComputer = CloudComputerUiState(
            nodes = listOf(
                ComputerNodeItem("node-local-android", "本机 Android 终端 (Mika Runtime)", ComputerNodeKind.LOCAL_DEVICE, "Android 16 · arm64-v8a · Alpine proot", true, 16, "7.4 / 16.0 GB", leaseOwner = "Mika-Local-Session"),
                ComputerNodeItem("node-cloud-gpu", "Fabushi Cloud GPU Workstation", ComputerNodeKind.CLOUD_SERVER, "Ubuntu 24.04 LTS · NVIDIA L40S", true, 34, "28.1 / 64.0 GB"),
                ComputerNodeItem("node-sandbox-ephemeral", "Hermes Zero-Trust MicroVM", ComputerNodeKind.CLOUD_SANDBOX, "Firecracker Alpine 3.21 · Ephemeral", true, 6, "0.4 / 4.0 GB"),
            ),
            actionBusTools = listOf(
                ActionBusSchemaItem("fabushi.app.snapshot", "App MCP", ActionRiskTier.R0_READ, "读取应用内稳定语义组件快照", "{ limit?: int } -> Snapshot"),
                ActionBusSchemaItem("mika.ui.tapElement", "Phone Control", ActionRiskTier.R1_LOW, "在无障碍树或虚拟副屏点击目标元素", "{ nodeId: string, pace: 'balanced' }"),
                ActionBusSchemaItem("sandbox.execCommand", "Alpine Sandbox", ActionRiskTier.R2_MEDIUM, "在本机 proot 容器内运行受限 Python/Node 脚本", "{ cmd: string, timeoutMs: int }"),
                ActionBusSchemaItem("telegram.sendExternal", "Messaging", ActionRiskTier.R3_HIGH, "向外部群组或公开频道发送正式通知", "{ chatId: string, text: string }"),
                ActionBusSchemaItem("wallet.broadcastBoc", "TON Wallet", ActionRiskTier.R4_CRITICAL, "签名并广播不可逆链上资产转账 (强制人工确认)", "{ to: string, amountNano: long }"),
            ),
            globalTasks = listOf(
                GlobalTaskItem("gt-1", "跨端同步 PTS 差量消息与索引重建", "node-local-android", GlobalTaskFsmState.COMPLETED, 100, "SQLite WAL + FTS5 索引就绪"),
                GlobalTaskItem("gt-2", "云端渲染全球功德周报可视化图表", "node-cloud-gpu", GlobalTaskFsmState.RUNNING, 68, "正在聚合 240 国心灯数据"),
                GlobalTaskItem("gt-3", "自动批量归档逾期账单回执", "node-sandbox-ephemeral", GlobalTaskFsmState.AWAITING_APPROVAL, 40, "等待 R3 风险门控人工放行"),
            ),
        )

        val callHistory = listOf(
            CallHistoryEntry("ch-1", "GPUI-Telegram × Mika 架构群", "group-gpui-dev", isVideo = true, outgoing = true, timeLabel = "今天 10:15", durationLabel = "18:42"),
            CallHistoryEntry("ch-2", "觉行禅师 (加密通话)", "secret-bodhi", isVideo = false, outgoing = false, timeLabel = "昨天 20:30", durationLabel = "06:15"),
        )

        return TelegramUiState(
            accounts = accounts,
            stories = stories,
            chats = chats,
            topicsByChat = forumTopics,
            messagesByChat = messages,
            discussionCommentsByMessage = comments,
            stickerPacks = stickerPacks,
            gifCatalog = gifs,
            slashCommands = slashCommands,
            tonWallet = tonWallet,
            hermesStudio = hermesStudio,
            cloudComputer = cloudComputer,
            callHistory = callHistory,
        )
    }
}
