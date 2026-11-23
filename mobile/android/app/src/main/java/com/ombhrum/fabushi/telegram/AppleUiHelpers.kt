package com.ombhrum.fabushi.telegram

import androidx.compose.runtime.Immutable
import androidx.compose.runtime.staticCompositionLocalOf
import java.time.Instant
import java.time.LocalDate
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.util.Locale

internal enum class AppleShellTab {
    MESSAGES,
    CONTACTS,
    AGENTS,
    REMOTE,
    SETTINGS,
}

@Immutable
internal data class PinnedChatsGridLayout(
    val centeredSingleRow: Boolean,
    val rows: List<List<TelegramChatItem>>,
)

internal fun partitionPinnedChatsGrid(chats: List<TelegramChatItem>): PinnedChatsGridLayout {
    val pinned = chats.filter { it.isPinned && !it.isArchived }
    return when {
        pinned.isEmpty() -> PinnedChatsGridLayout(centeredSingleRow = false, rows = emptyList())
        pinned.size <= 2 -> PinnedChatsGridLayout(centeredSingleRow = true, rows = listOf(pinned))
        else -> PinnedChatsGridLayout(centeredSingleRow = false, rows = pinned.chunked(3))
    }
}

internal sealed interface ChatTimelineItem {
    data class DateDivider(val dayKey: String, val label: String) : ChatTimelineItem
    data class MessageEntry(
        val message: TelegramMessageItem,
        val isGroupedWithPrev: Boolean,
        val isGroupedWithNext: Boolean,
    ) : ChatTimelineItem
}

private const val MESSAGE_GROUP_WINDOW_MS = 5L * 60L * 1000L

internal fun buildTimelineItems(
    messages: List<TelegramMessageItem>,
    uiLanguage: String = "zh-CN",
    zoneId: ZoneId = ZoneId.systemDefault(),
): List<ChatTimelineItem> {
    if (messages.isEmpty()) return emptyList()
    val english = isEnglishLanguage(uiLanguage)
    val today = LocalDate.now(zoneId)
    val yesterday = today.minusDays(1)
    val sameYearFmt = DateTimeFormatter.ofPattern(if (english) "MMM d" else "M月d日", if (english) Locale.ENGLISH else Locale.SIMPLIFIED_CHINESE)
    val fullDateFmt = DateTimeFormatter.ofPattern(if (english) "MMM d, yyyy" else "yyyy年M月d日", if (english) Locale.ENGLISH else Locale.SIMPLIFIED_CHINESE)

    fun dayFor(ms: Long): LocalDate =
        Instant.ofEpochMilli(ms.coerceAtLeast(0L)).atZone(zoneId).toLocalDate()

    fun labelFor(date: LocalDate): String = when (date) {
        today -> if (english) "Today" else "今天"
        yesterday -> if (english) "Yesterday" else "昨天"
        else -> if (date.year == today.year) date.format(sameYearFmt) else date.format(fullDateFmt)
    }

    val dayKeys = messages.map { dayFor(it.timestampMs) }
    val result = ArrayList<ChatTimelineItem>(messages.size + 4)

    for (i in messages.indices) {
        val msg = messages[i]
        val day = dayKeys[i]
        val prevDay = if (i > 0) dayKeys[i - 1] else null
        if (prevDay != day) {
            result += ChatTimelineItem.DateDivider(
                dayKey = day.toString(),
                label = labelFor(day),
            )
        }
        val prev = if (i > 0) messages[i - 1] else null
        val next = if (i + 1 < messages.size) messages[i + 1] else null
        val groupedWithPrev = prev != null &&
            dayKeys[i - 1] == day &&
            msg.contentType != "service" &&
            prev.contentType != "service" &&
            prev.outgoing == msg.outgoing &&
            prev.senderId == msg.senderId &&
            (msg.timestampMs - prev.timestampMs) in 0L..MESSAGE_GROUP_WINDOW_MS
        val groupedWithNext = next != null &&
            dayKeys[i + 1] == day &&
            msg.contentType != "service" &&
            next.contentType != "service" &&
            next.outgoing == msg.outgoing &&
            next.senderId == msg.senderId &&
            (next.timestampMs - msg.timestampMs) in 0L..MESSAGE_GROUP_WINDOW_MS

        result += ChatTimelineItem.MessageEntry(
            message = msg,
            isGroupedWithPrev = groupedWithPrev,
            isGroupedWithNext = groupedWithNext,
        )
    }
    return result
}

internal fun timelineItemKey(index: Int, entry: ChatTimelineItem): String = when (entry) {
    is ChatTimelineItem.DateDivider -> "date-${entry.dayKey}-$index"
    is ChatTimelineItem.MessageEntry -> "msg-${entry.message.id}-$index"
}

internal sealed interface MessageContentSegment {
    data class TextSegment(val text: String) : MessageContentSegment
    data class CodeBlockSegment(val language: String, val code: String) : MessageContentSegment
}

private val FENCED_CODE_REGEX = Regex("```([^\\n`]*)\\r?\\n([\\s\\S]*?)```")

internal fun parseMessageCodeSegments(text: String): List<MessageContentSegment> {
    if (text.isEmpty()) return emptyList()
    val matches = FENCED_CODE_REGEX.findAll(text).toList()
    if (matches.isEmpty()) {
        return listOf(MessageContentSegment.TextSegment(text))
    }
    val segments = mutableListOf<MessageContentSegment>()
    var cursor = 0
    for (match in matches) {
        if (match.range.first > cursor) {
            val before = text.substring(cursor, match.range.first).trim('\n', '\r')
            if (before.isNotEmpty()) {
                segments += MessageContentSegment.TextSegment(before)
            }
        }
        val lang = match.groupValues[1].trim()
        val code = match.groupValues[2].replace("\r\n", "\n").replace('\r', '\n').trimEnd('\n')
        segments += MessageContentSegment.CodeBlockSegment(language = lang, code = code)
        cursor = match.range.last + 1
    }
    if (cursor < text.length) {
        val trailing = text.substring(cursor).trim('\n', '\r')
        if (trailing.isNotEmpty()) {
            segments += MessageContentSegment.TextSegment(trailing)
        }
    }
    return segments
}

internal fun canSubmitComposer(text: String, attachmentCount: Int = 0): Boolean =
    text.isNotBlank() || attachmentCount > 0

internal fun filterMessageInlineKeyboard(message: TelegramMessageItem): List<InlineKeyboardRow> {
    if (message.inlineKeyboard.isEmpty()) return emptyList()
    if (!message.mikaRequiresConfirmation) return message.inlineKeyboard
    return message.inlineKeyboard.mapNotNull { row ->
        val remaining = row.buttons.filterNot { btn ->
            btn.payload.startsWith("mika_confirm:approve:") ||
                btn.payload.startsWith("mika_confirm:decline:")
        }
        if (remaining.isEmpty()) null else InlineKeyboardRow(remaining)
    }
}

internal fun matchesBotUsername(chatUsername: String?, botUsername: String?): Boolean {
    val left = chatUsername?.trim()?.trimStart('@').orEmpty()
    val right = botUsername?.trim()?.trimStart('@').orEmpty()
    return left.isNotEmpty() && right.isNotEmpty() && left.equals(right, ignoreCase = true)
}

internal fun isValidCustomAssistCode(code: String): Boolean {
    val trimmed = code.trim()
    return trimmed.length in 8..16 &&
        trimmed.all { it in 'a'..'z' || it in 'A'..'Z' || it in '0'..'9' } &&
        trimmed.any { it.isLetter() } &&
        trimmed.any { it.isDigit() }
}

private val HEX_COLOR_REGEX = Regex("^#[0-9A-Fa-f]{6}$")

internal fun isValidHexColor(input: String): Boolean =
    HEX_COLOR_REGEX.matches(input.trim())

internal fun isEnglishLanguage(uiLanguage: String): Boolean {
    val normalized = uiLanguage.trim().lowercase(Locale.ROOT)
    return normalized.startsWith("en") || normalized == "english"
}

internal fun formatBotActivitySubtitle(
    executionState: String,
    detail: String?,
    uiLanguage: String = "zh-CN",
): String? {
    val english = isEnglishLanguage(uiLanguage)
    val cleanDetail = detail?.trim()?.takeIf { it.isNotEmpty() }
    return when (executionState.trim().lowercase(Locale.ROOT)) {
        "", "idle" -> null
        "tool", "running_tool" -> if (cleanDetail != null) {
            if (english) "Running $cleanDetail..." else "正在调用 $cleanDetail..."
        } else {
            if (english) "Running tool..." else "正在调用工具..."
        }
        "writing" -> cleanDetail ?: if (english) "Writing reply..." else "正在组织回复..."
        "thinking" -> cleanDetail ?: if (english) "Thinking..." else "AI 正在思考..."
        else -> cleanDetail ?: if (english) "Working..." else "处理中..."
    }
}

@Immutable
internal data class AppleUiStrings(
    val isEnglish: Boolean,
    val tabMessages: String,
    val tabContacts: String,
    val tabAgents: String,
    val tabRemote: String,
    val tabSettings: String,
    val edit: String,
    val done: String,
    val cancel: String,
    val save: String,
    val delete: String,
    val copy: String,
    val copied: String,
    val share: String,
    val searchPlaceholder: String,
    val searchInChatPlaceholder: String,
    val composerPlaceholder: String,
    val noMessagesYet: String,
    val noChatsTitle: String,
    val noChatsSubtitle: String,
    val composeTitle: String,
    val addContactAction: String,
    val newDirectChatAction: String,
    val newGroupAction: String,
    val newChannelAction: String,
    val newSecretChatAction: String,
    val newBotAction: String,
    val pinAction: String,
    val unpinAction: String,
    val muteAction: String,
    val unmuteAction: String,
    val archiveAction: String,
    val unarchiveAction: String,
    val markReadAction: String,
    val markUnreadAction: String,
    val loadOlderMessages: String,
    val sending: String,
    val sent: String,
    val delivered: String,
    val read: String,
    val failedTapRetry: String,
    val retry: String,
    val approve: String,
    val decline: String,
    val pinnedMessage: String,
    val copyCode: String,
    val inspectorProfile: String,
    val inspectorMembers: String,
    val inspectorAgentComputers: String,
    val inspectorSharedMedia: String,
    val inspectorSettings: String,
    val settingsTitle: String,
    val settingsProfile: String,
    val settingsActiveSessions: String,
    val settingsPrivacySecurity: String,
    val settingsAppearance: String,
    val settingsLanguage: String,
    val settingsNotifications: String,
    val settingsFolders: String,
    val settingsDataStorage: String,
    val settingsServerProxy: String,
    val settingsTonWallet: String,
    val settingsGlobalDharma: String,
    val settingsPowerSaving: String,
    val signOut: String,
    val signIn: String,
    val signedOutTitle: String,
    val offlineMode: String,
    val remoteMyDevices: String,
    val remoteAssist: String,
    val remoteCloudServers: String,
    val remoteAiMcp: String,
    val agentsBotsTab: String,
    val agentsMagpieTab: String,
    val agentsHermesTab: String,
    val authSubtitle: String,
    val openInAlipay: String,
    val resetDefaultServer: String,
)

internal fun resolveUiStrings(uiLanguage: String): AppleUiStrings {
    return if (isEnglishLanguage(uiLanguage)) {
        AppleUiStrings(
            isEnglish = true,
            tabMessages = "Messages",
            tabContacts = "Contacts",
            tabAgents = "Agents",
            tabRemote = "Remote",
            tabSettings = "Settings",
            edit = "Edit",
            done = "Done",
            cancel = "Cancel",
            save = "Save",
            delete = "Delete",
            copy = "Copy",
            copied = "Copied",
            share = "Share",
            searchPlaceholder = "Search messages or chats",
            searchInChatPlaceholder = "Search in conversation",
            composerPlaceholder = "Message",
            noMessagesYet = "No messages yet",
            noChatsTitle = "No Conversations",
            noChatsSubtitle = "Tap Compose in the top right to start a chat or add a contact.",
            composeTitle = "New Message",
            addContactAction = "Add Contact / Start DM",
            newDirectChatAction = "New Direct Chat",
            newGroupAction = "New Group",
            newChannelAction = "New Channel",
            newSecretChatAction = "New Secret Chat",
            newBotAction = "Create AI Bot",
            pinAction = "Pin",
            unpinAction = "Unpin",
            muteAction = "Mute",
            unmuteAction = "Unmute",
            archiveAction = "Archive",
            unarchiveAction = "Unarchive",
            markReadAction = "Read",
            markUnreadAction = "Unread",
            loadOlderMessages = "Load earlier messages",
            sending = "Sending",
            sent = "Sent",
            delivered = "Delivered",
            read = "Read",
            failedTapRetry = "Failed · Tap to Retry",
            retry = "Retry",
            approve = "Approve",
            decline = "Decline",
            pinnedMessage = "Pinned Message",
            copyCode = "Copy Code",
            inspectorProfile = "Profile",
            inspectorMembers = "Members",
            inspectorAgentComputers = "Agent",
            inspectorSharedMedia = "Media",
            inspectorSettings = "Settings",
            settingsTitle = "Settings",
            settingsProfile = "Profile & Bio",
            settingsActiveSessions = "Devices & Sessions",
            settingsPrivacySecurity = "Privacy & Security",
            settingsAppearance = "Appearance",
            settingsLanguage = "Language",
            settingsNotifications = "Notifications & Sounds",
            settingsFolders = "Chat Folders",
            settingsDataStorage = "Data & Storage",
            settingsServerProxy = "Server & Proxy",
            settingsTonWallet = "TON Wallet",
            settingsGlobalDharma = "Global Dharma",
            settingsPowerSaving = "Power Saving",
            signOut = "Sign Out",
            signIn = "Sign In",
            signedOutTitle = "Not Signed In",
            offlineMode = "Offline Mode",
            remoteMyDevices = "My Devices",
            remoteAssist = "Assist",
            remoteCloudServers = "Cloud",
            remoteAiMcp = "AI MCP",
            agentsBotsTab = "Bots",
            agentsMagpieTab = "Magpie AI",
            agentsHermesTab = "Hermes",
            authSubtitle = "Sign in to sync messages, agents, and remote devices",
            openInAlipay = "Open in Alipay / Browser",
            resetDefaultServer = "Reset Default",
        )
    } else {
        AppleUiStrings(
            isEnglish = false,
            tabMessages = "信息",
            tabContacts = "联系人",
            tabAgents = "智能体",
            tabRemote = "远程",
            tabSettings = "设置",
            edit = "编辑",
            done = "完成",
            cancel = "取消",
            save = "保存",
            delete = "删除",
            copy = "复制",
            copied = "已复制",
            share = "分享",
            searchPlaceholder = "搜索对话或消息",
            searchInChatPlaceholder = "搜索当前对话",
            composerPlaceholder = "信息",
            noMessagesYet = "暂无消息",
            noChatsTitle = "暂无对话",
            noChatsSubtitle = "轻触右上角新建按钮，添加联系人或发起新对话。",
            composeTitle = "新建对话",
            addContactAction = "添加联系人 / 发起私聊",
            newDirectChatAction = "发起私聊",
            newGroupAction = "新建群组",
            newChannelAction = "新建频道",
            newSecretChatAction = "发起加密私聊",
            newBotAction = "创建 AI 智能体",
            pinAction = "置顶",
            unpinAction = "取消置顶",
            muteAction = "静音",
            unmuteAction = "取消静音",
            archiveAction = "归档",
            unarchiveAction = "取消归档",
            markReadAction = "标为已读",
            markUnreadAction = "标为未读",
            loadOlderMessages = "加载更早消息",
            sending = "发送中",
            sent = "已发送",
            delivered = "已送达",
            read = "已读",
            failedTapRetry = "发送失败 · 点击重试",
            retry = "重试",
            approve = "批准",
            decline = "拒绝",
            pinnedMessage = "置顶消息",
            copyCode = "复制代码",
            inspectorProfile = "资料",
            inspectorMembers = "成员",
            inspectorAgentComputers = "智能体",
            inspectorSharedMedia = "共享媒体",
            inspectorSettings = "设置",
            settingsTitle = "设置",
            settingsProfile = "个人资料与简介",
            settingsActiveSessions = "登录设备与会话",
            settingsPrivacySecurity = "隐私与安全",
            settingsAppearance = "外观与主题",
            settingsLanguage = "语言与翻译",
            settingsNotifications = "通知与声音",
            settingsFolders = "对话文件夹",
            settingsDataStorage = "数据与存储",
            settingsServerProxy = "服务器与代理",
            settingsTonWallet = "TON 钱包",
            settingsGlobalDharma = "全球法布施",
            settingsPowerSaving = "省电与性能",
            signOut = "退出登录",
            signIn = "登录服务器账号",
            signedOutTitle = "未登录",
            offlineMode = "离线本地模式",
            remoteMyDevices = "我的设备",
            remoteAssist = "远程协助",
            remoteCloudServers = "云服务器",
            remoteAiMcp = "AI MCP",
            agentsBotsTab = "智能体",
            agentsMagpieTab = "Magpie AI",
            agentsHermesTab = "Hermes",
            authSubtitle = "登录 Fabushi 账号以同步消息、智能体与远程设备",
            openInAlipay = "在支付宝 / 浏览器中打开",
            resetDefaultServer = "恢复默认",
        )
    }
}

internal val LocalAppleUiStrings = staticCompositionLocalOf { resolveUiStrings("zh-CN") }
