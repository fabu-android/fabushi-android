package com.ombhrum.fabushi.telegram

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Slider
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.ombhrum.fabushi.AndroidUpdatePhase
import com.ombhrum.fabushi.AndroidUpdateUiState
import kotlin.math.roundToInt

@Composable
fun TelegramSettingsScreen(
    state: TelegramUiState,
    updateState: AndroidUpdateUiState?,
    onSelectPage: (TelegramSettingsPage) -> Unit,
    onUpdateProfile: (displayName: String, username: String, bio: String) -> Unit,
    onUpdateTheme: ((TelegramThemeSettings) -> TelegramThemeSettings) -> Unit,
    onConfigurePasscode: (String) -> Unit,
    onDisablePasscode: () -> Unit,
    onLockAppNow: () -> Unit,
    onClearLocalMediaCache: () -> Unit,
    onUpsertCustomFolder: (title: String, contacts: Boolean, bots: Boolean, groups: Boolean, channels: Boolean, excludeMuted: Boolean) -> Unit,
    onDeleteCustomFolder: (String) -> Unit,
    onUpdatePowerAndNotifications: (Int, Boolean) -> Unit,
    onUpdateLanguageAndTranslate: (String, Boolean, Boolean) -> Unit,
    onConfigureServerEndpoint: (String, Boolean, String) -> Unit,
    onOpenTonWallet: () -> Unit,
    onCheckForUpdates: () -> Unit,
    onDownloadAndInstallUpdate: () -> Unit,
    onInitSplitKeyVault: (String) -> Unit = {},
    onRequestDeviceVaultSync: () -> Unit = {},
    onBackToHome: () -> Unit,
) {
    val palette = LocalTelegramPalette.current
    val page = state.settingsPage

    BackHandler {
        if (page == TelegramSettingsPage.OVERVIEW) onBackToHome()
        else onSelectPage(TelegramSettingsPage.OVERVIEW)
    }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(palette.appBackground)
            .testTag("telegram-settings-screen"),
    ) {
        if (page == TelegramSettingsPage.OVERVIEW) {
            SettingsOverviewContent(
                state = state,
                updateState = updateState,
                onSelectPage = onSelectPage,
                onCheckForUpdates = onCheckForUpdates,
                onDownloadAndInstallUpdate = onDownloadAndInstallUpdate,
                onBack = onBackToHome,
            )
        } else {
            WorkspaceTopBar(
                title = page.title,
                subtitle = page.subtitle,
                glyph = TelegramGlyph.SETTINGS_GEAR,
                onBack = { onSelectPage(TelegramSettingsPage.OVERVIEW) },
            )
            SettingsSubPageContent(
                page = page,
                state = state,
                updateState = updateState,
                onUpdateProfile = onUpdateProfile,
                onUpdateTheme = onUpdateTheme,
                onConfigurePasscode = onConfigurePasscode,
                onDisablePasscode = onDisablePasscode,
                onLockAppNow = onLockAppNow,
                onClearLocalMediaCache = onClearLocalMediaCache,
                onUpsertCustomFolder = onUpsertCustomFolder,
                onDeleteCustomFolder = onDeleteCustomFolder,
                onUpdatePowerAndNotifications = onUpdatePowerAndNotifications,
                onUpdateLanguageAndTranslate = onUpdateLanguageAndTranslate,
                onConfigureServerEndpoint = onConfigureServerEndpoint,
                onOpenTonWallet = onOpenTonWallet,
                onCheckForUpdates = onCheckForUpdates,
                onDownloadAndInstallUpdate = onDownloadAndInstallUpdate,
                onInitSplitKeyVault = onInitSplitKeyVault,
                onRequestDeviceVaultSync = onRequestDeviceVaultSync,
            )
        }
    }
}

@Composable
private fun SettingsOverviewContent(
    state: TelegramUiState,
    updateState: AndroidUpdateUiState?,
    onSelectPage: (TelegramSettingsPage) -> Unit,
    onCheckForUpdates: () -> Unit,
    onDownloadAndInstallUpdate: () -> Unit,
    onBack: () -> Unit,
) {
    val palette = LocalTelegramPalette.current
    val account = state.activeAccount

    LazyColumn(
        modifier = Modifier.fillMaxSize().navigationBarsPadding(),
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        // Authentic Telegram Expanded Profile Header
        item {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .background(Brush.linearGradient(listOf(palette.drawerHeaderStart, palette.drawerHeaderEnd)))
                    .statusBarsPadding()
                    .padding(16.dp),
            ) {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Box(
                        modifier = Modifier.size(38.dp).clip(CircleShape).clickable(onClick = onBack),
                        contentAlignment = Alignment.Center,
                    ) {
                        TelegramIcon(TelegramGlyph.BACK, tint = Color.White, size = 22.dp)
                    }
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        Box(
                            modifier = Modifier.size(38.dp).clip(CircleShape).clickable { onSelectPage(TelegramSettingsPage.DEVICES_SESSIONS) },
                            contentAlignment = Alignment.Center,
                        ) {
                            TelegramIcon(TelegramGlyph.QR_CODE, tint = Color.White, size = 20.dp)
                        }
                        Box(
                            modifier = Modifier.size(38.dp).clip(CircleShape).clickable { onSelectPage(TelegramSettingsPage.PROFILE_BIO) },
                            contentAlignment = Alignment.Center,
                        ) {
                            TelegramIcon(TelegramGlyph.PENCIL, tint = Color.White, size = 20.dp)
                        }
                    }
                }
                Spacer(Modifier.height(10.dp))
                Row(verticalAlignment = Alignment.CenterVertically) {
                    TelegramAvatar(
                        idSeed = account?.id ?: "acc",
                        title = account?.displayName ?: "Fabushi",
                        size = 64.dp,
                        online = true,
                    )
                    Spacer(Modifier.width(14.dp))
                    Column {
                        Text(account?.displayName ?: "全球法布施", color = Color.White, fontWeight = FontWeight.Bold, fontSize = 19.sp)
                        Text("${account?.phone ?: "+86"} · ${account?.username ?: "@fabushi"}", color = Color.White.copy(alpha = 0.82f), fontSize = 13.sp)
                        if (!account?.bio.isNullOrBlank()) {
                            Text(account!!.bio, color = Color.White.copy(alpha = 0.72f), fontSize = 12.sp)
                        }
                    }
                }
            }
        }

        // GitHub Release Self-Update Card (if provided)
        if (updateState != null) {
            item {
                UpdateStatusCard(
                    updateState = updateState,
                    onCheck = onCheckForUpdates,
                    onInstall = onDownloadAndInstallUpdate,
                    modifier = Modifier.padding(horizontal = 12.dp),
                )
            }
        }

        // All 11 Telegram Settings Pages
        val pages = listOf(
            Triple("👤", TelegramSettingsPage.PROFILE_BIO, account?.username ?: "@fabushi"),
            Triple("🎨", TelegramSettingsPage.CHAT_THEMES, state.themeSettings.preset.label),
            Triple("🔐", TelegramSettingsPage.PRIVACY_SECURITY, if (state.passcodeConfigured) "密码锁已开启" else "X3DH 加密就绪"),
            Triple("🔔", TelegramSettingsPage.NOTIFICATIONS_SOUNDS, if (state.inAppSoundsEnabled) "声音开启" else "静音"),
            Triple("💾", TelegramSettingsPage.DATA_STORAGE, "${"%.1f".format(state.storageCacheMb)} MB 缓存"),
            Triple("📁", TelegramSettingsPage.CHAT_FOLDERS, "${state.folderTabs.size} 个分组标签"),
            Triple("💻", TelegramSettingsPage.DEVICES_SESSIONS, "本机 + 桌面端同步"),
            Triple("🔋", TelegramSettingsPage.POWER_SAVING, "< ${state.powerSavingThreshold}% 自动省电"),
            Triple("🌐", TelegramSettingsPage.LANGUAGE_TRANSLATE, state.uiLanguage),
            Triple("⭐", TelegramSettingsPage.STARS_WALLET, "${state.tonWallet.starsBalance} Stars · ${"%.2f".format(state.tonWallet.tonBalance)} TON"),
            Triple("🛡", TelegramSettingsPage.PROXY_SERVER, state.serverEndpoint),
        )

        item {
            Surface(
                shape = RoundedCornerShape(16.dp),
                color = palette.surfaceColor,
                modifier = Modifier.padding(horizontal = 12.dp).fillMaxWidth(),
            ) {
                Column {
                    pages.forEachIndexed { idx, (emoji, pg, badge) ->
                        Row(
                            modifier = Modifier
                                .fillMaxWidth()
                                .clickable { onSelectPage(pg) }
                                .padding(horizontal = 14.dp, vertical = 13.dp),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            Text(emoji, fontSize = 20.sp)
                            Spacer(Modifier.width(14.dp))
                            Column(modifier = Modifier.weight(1f)) {
                                Text(pg.title, fontWeight = FontWeight.SemiBold, fontSize = 15.sp, color = palette.primaryText)
                                Text(pg.subtitle, fontSize = 12.sp, color = palette.secondaryText)
                            }
                            Text(badge, fontSize = 11.sp, color = palette.primaryAccent, fontWeight = FontWeight.Medium)
                        }
                        if (idx < pages.lastIndex) {
                            TelegramHairlineDivider(modifier = Modifier.padding(start = 48.dp))
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun SettingsSubPageContent(
    page: TelegramSettingsPage,
    state: TelegramUiState,
    updateState: AndroidUpdateUiState?,
    onUpdateProfile: (String, String, String) -> Unit,
    onUpdateTheme: ((TelegramThemeSettings) -> TelegramThemeSettings) -> Unit,
    onConfigurePasscode: (String) -> Unit,
    onDisablePasscode: () -> Unit,
    onLockAppNow: () -> Unit,
    onClearLocalMediaCache: () -> Unit,
    onUpsertCustomFolder: (String, Boolean, Boolean, Boolean, Boolean, Boolean) -> Unit,
    onDeleteCustomFolder: (String) -> Unit,
    onUpdatePowerAndNotifications: (Int, Boolean) -> Unit,
    onUpdateLanguageAndTranslate: (String, Boolean, Boolean) -> Unit,
    onConfigureServerEndpoint: (String, Boolean, String) -> Unit,
    onOpenTonWallet: () -> Unit,
    onCheckForUpdates: () -> Unit,
    onDownloadAndInstallUpdate: () -> Unit,
    onInitSplitKeyVault: (String) -> Unit = {},
    onRequestDeviceVaultSync: () -> Unit = {},
) {
    val palette = LocalTelegramPalette.current

    LazyColumn(
        modifier = Modifier.fillMaxSize().navigationBarsPadding(),
        contentPadding = PaddingValues(14.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        when (page) {
            TelegramSettingsPage.PROFILE_BIO -> {
                item {
                    val acc = state.activeAccount
                    var name by remember(acc?.id) { mutableStateOf(acc?.displayName.orEmpty()) }
                    var user by remember(acc?.id) { mutableStateOf(acc?.username.orEmpty()) }
                    var bio by remember(acc?.id) { mutableStateOf(acc?.bio.orEmpty()) }
                    Surface(shape = RoundedCornerShape(16.dp), color = palette.surfaceColor, modifier = Modifier.fillMaxWidth()) {
                        Column(modifier = Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                            OutlinedTextField(value = name, onValueChange = { name = it }, label = { Text("显示昵称") }, singleLine = true, modifier = Modifier.fillMaxWidth())
                            OutlinedTextField(value = user, onValueChange = { user = it }, label = { Text("@用户名") }, singleLine = true, modifier = Modifier.fillMaxWidth())
                            OutlinedTextField(value = bio, onValueChange = { bio = it }, label = { Text("个人签名 (Bio)") }, modifier = Modifier.fillMaxWidth())
                            Button(onClick = { onUpdateProfile(name, user, bio) }, modifier = Modifier.fillMaxWidth()) {
                                Text("保存个人资料")
                            }
                        }
                    }
                }
            }

            TelegramSettingsPage.CHAT_THEMES -> {
                item {
                    val ts = state.themeSettings
                    // Live Telegram Chat Wallpaper + Bubble Preview
                    TelegramChatWallpaperBackground(
                        modifier = Modifier
                            .fillMaxWidth()
                            .height(148.dp)
                            .clip(RoundedCornerShape(16.dp)),
                    ) {
                        Column(
                            modifier = Modifier.fillMaxSize().padding(12.dp),
                            verticalArrangement = Arrangement.SpaceBetween,
                        ) {
                            Surface(shape = RoundedCornerShape(ts.boundedCornerDp), color = palette.incomingBubble) {
                                Text("👋 实时预览气泡圆角与字号", color = palette.incomingText, fontSize = ts.boundedFontSp, modifier = Modifier.padding(horizontal = 12.dp, vertical = 7.dp))
                            }
                            Surface(
                                shape = RoundedCornerShape(ts.boundedCornerDp),
                                color = if (palette.isAppleStyle) Color.Transparent else palette.outgoingBubble,
                                modifier = Modifier
                                    .align(Alignment.End)
                                    .then(
                                        if (palette.isAppleStyle) {
                                            Modifier.background(palette.outgoingBubbleBrush, RoundedCornerShape(ts.boundedCornerDp))
                                        } else {
                                            Modifier
                                        },
                                    ),
                            ) {
                                Text(
                                    text = if (palette.isAppleStyle) "✨ Apple Messages v1.6 主题 ✓✓" else "✨ 1:1 Telegram 主题引擎 ✓✓",
                                    color = palette.outgoingText,
                                    fontSize = ts.boundedFontSp,
                                    modifier = Modifier.padding(horizontal = 12.dp, vertical = 7.dp),
                                )
                            }
                        }
                    }
                }

                item {
                    val ts = state.themeSettings
                    Surface(shape = RoundedCornerShape(16.dp), color = palette.surfaceColor, modifier = Modifier.fillMaxWidth()) {
                        Column(modifier = Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                            Text("🎨 预设主题色彩", fontWeight = FontWeight.Bold, color = palette.primaryText)
                            TelegramThemePreset.entries.forEach { preset ->
                                Row(
                                    modifier = Modifier
                                        .fillMaxWidth()
                                        .clip(RoundedCornerShape(10.dp))
                                        .clickable { onUpdateTheme { it.copy(preset = preset) } }
                                        .padding(vertical = 6.dp),
                                    verticalAlignment = Alignment.CenterVertically,
                                ) {
                                    Text(if (ts.preset == preset) "🔘" else "⚪", fontSize = 16.sp)
                                    Spacer(Modifier.width(10.dp))
                                    Text(preset.label, color = palette.primaryText, fontSize = 14.sp)
                                }
                            }

                            if (ts.preset == TelegramThemePreset.CUSTOM_ACCENT) {
                                val swatches = listOf(0xFF8E44ADL, 0xFFE91E63L, 0xFF00897BL, 0xFFFF6F00L, 0xFF3949ABL, 0xFF2E7D32L)
                                LazyRow(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                                    items(swatches) { hex ->
                                        Box(
                                            modifier = Modifier
                                                .size(36.dp)
                                                .clip(CircleShape)
                                                .background(Color(hex))
                                                .border(
                                                    width = if (ts.customAccentArgb == hex) 3.dp else 0.dp,
                                                    color = palette.primaryText,
                                                    shape = CircleShape,
                                                )
                                                .clickable { onUpdateTheme { it.copy(customAccentArgb = hex) } },
                                        )
                                    }
                                }
                                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.SpaceBetween, modifier = Modifier.fillMaxWidth()) {
                                    Text("自定义深色底色", color = palette.primaryText)
                                    Switch(checked = ts.customDark, onCheckedChange = { d -> onUpdateTheme { it.copy(customDark = d) } })
                                }
                            }

                            Text("气泡圆角半径：${ts.bubbleCornerRadiusDp} dp", fontSize = 13.sp, color = palette.secondaryText)
                            Slider(
                                value = ts.bubbleCornerRadiusDp.toFloat(),
                                onValueChange = { v -> onUpdateTheme { it.copy(bubbleCornerRadiusDp = v.roundToInt()) } },
                                valueRange = 4f..18f,
                            )

                            Text("聊天消息字号：${ts.messageFontSizeSp} sp", fontSize = 13.sp, color = palette.secondaryText)
                            Slider(
                                value = ts.messageFontSizeSp.toFloat(),
                                onValueChange = { v -> onUpdateTheme { it.copy(messageFontSizeSp = v.roundToInt()) } },
                                valueRange = 12f..22f,
                            )

                            Text("🖼 聊天壁纸纹理", fontWeight = FontWeight.Bold, color = palette.primaryText)
                            TelegramWallpaperStyle.entries.forEach { wp ->
                                Row(
                                    modifier = Modifier
                                        .fillMaxWidth()
                                        .clickable { onUpdateTheme { it.copy(wallpaperStyle = wp) } }
                                        .padding(vertical = 5.dp),
                                    verticalAlignment = Alignment.CenterVertically,
                                ) {
                                    Text(if (ts.wallpaperStyle == wp) "🔘" else "⚪")
                                    Spacer(Modifier.width(10.dp))
                                    Text(wp.label, color = palette.primaryText, fontSize = 14.sp)
                                }
                            }

                            Text("壁纸暗角遮罩：${ts.wallpaperDimPercent}%", fontSize = 13.sp, color = palette.secondaryText)
                            Slider(
                                value = ts.wallpaperDimPercent.toFloat(),
                                onValueChange = { v -> onUpdateTheme { it.copy(wallpaperDimPercent = v.roundToInt()) } },
                                valueRange = 0f..80f,
                            )
                        }
                    }
                }
            }

            TelegramSettingsPage.PRIVACY_SECURITY -> {
                item {
                    var pinInput by remember { mutableStateOf("") }
                    Surface(shape = RoundedCornerShape(16.dp), color = palette.surfaceColor, modifier = Modifier.fillMaxWidth()) {
                        Column(modifier = Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                            Text("🔒 本地 4 位数字密码与系统生物识别", fontWeight = FontWeight.Bold, color = palette.primaryText)
                            OutlinedTextField(
                                value = pinInput,
                                onValueChange = { pinInput = it.filter(Char::isDigit).take(4) },
                                label = { Text("输入新的 4 位 PIN 密码") },
                                singleLine = true,
                                modifier = Modifier.fillMaxWidth(),
                            )
                            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                Button(onClick = { onConfigurePasscode(pinInput) }) { Text("启用/修改密码锁") }
                                if (state.passcodeConfigured) {
                                    TextButton(onClick = onDisablePasscode) { Text("关闭密码锁", color = palette.destructiveRed) }
                                }
                            }
                            Button(onClick = onLockAppNow, modifier = Modifier.fillMaxWidth()) {
                                Text("🔐 立即锁定应用界面")
                            }
                            TelegramHairlineDivider()
                            Text("🛡 端到端加密协议栈：X3DH (X25519 + Ed25519) + Double Ratchet (ChaCha20-Poly1305)", fontSize = 12.sp, color = palette.secondaryText)
                        }
                    }
                }
                item {
                    var vaultMasterPass by remember { mutableStateOf("") }
                    Surface(shape = RoundedCornerShape(16.dp), color = palette.surfaceColor, modifier = Modifier.fillMaxWidth().testTag("settings-user-vault-card")) {
                        Column(modifier = Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                            Text("🔐 零知识用户保险库 (Split-Key V2 · X25519 设备信任环)", fontWeight = FontWeight.Bold, color = palette.primaryText)
                            Text(
                                text = "算法：${state.userVault.kdfAlg} (v${state.userVault.keyVersion}) · 已加密条目：${state.userVault.encryptedItemCount}\n${state.userVault.lastVaultMessage}",
                                fontSize = 12.sp,
                                color = palette.secondaryText,
                            )
                            OutlinedTextField(
                                value = vaultMasterPass,
                                onValueChange = { vaultMasterPass = it },
                                label = { Text("主口令（仅端侧 Argon2id 派生，绝不上传原文）") },
                                singleLine = true,
                                modifier = Modifier.fillMaxWidth(),
                            )
                            Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.fillMaxWidth()) {
                                Button(
                                    onClick = { onInitSplitKeyVault(vaultMasterPass) },
                                    modifier = Modifier.weight(1f),
                                ) {
                                    Text(if (state.userVault.initialized) "重置保险库" else "初始化 Split-Key V2")
                                }
                                Button(
                                    onClick = onRequestDeviceVaultSync,
                                    modifier = Modifier.weight(1f),
                                ) {
                                    Text("X25519 设备同步")
                                }
                            }
                            state.userVault.activeSyncVerificationCode?.let { code ->
                                Surface(shape = RoundedCornerShape(10.dp), color = palette.elevatedSurface, modifier = Modifier.fillMaxWidth()) {
                                    Column(modifier = Modifier.padding(10.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                                        Text("6 位信任环核验码：$code · 状态：${state.userVault.activeSyncStatus}", fontFamily = FontFamily.Monospace, fontWeight = FontWeight.Bold, color = palette.primaryAccent, fontSize = 13.sp)
                                        Text("受信任桌面端批准后，密文将在首次读取时在 SQLite 中原子擦除（一次性消费）。", fontSize = 11.sp, color = palette.secondaryText)
                                    }
                                }
                            }
                        }
                    }
                }
            }

            TelegramSettingsPage.NOTIFICATIONS_SOUNDS -> {
                item {
                    Surface(shape = RoundedCornerShape(16.dp), color = palette.surfaceColor, modifier = Modifier.fillMaxWidth()) {
                        Row(modifier = Modifier.padding(14.dp), horizontalArrangement = Arrangement.SpaceBetween, verticalAlignment = Alignment.CenterVertically) {
                            Column(modifier = Modifier.weight(1f)) {
                                Text("应用内消息提示音与触觉震动", fontWeight = FontWeight.Bold, color = palette.primaryText)
                                Text("发送、语音录制及转经轮物理反馈", fontSize = 12.sp, color = palette.secondaryText)
                            }
                            Switch(
                                checked = state.inAppSoundsEnabled,
                                onCheckedChange = { onUpdatePowerAndNotifications(state.powerSavingThreshold, it) },
                            )
                        }
                    }
                }
            }

            TelegramSettingsPage.DATA_STORAGE -> {
                item {
                    Surface(shape = RoundedCornerShape(16.dp), color = palette.surfaceColor, modifier = Modifier.fillMaxWidth()) {
                        Column(modifier = Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                            Text("本地媒体与音频分片缓存：${"%.1f".format(state.storageCacheMb)} MB", fontWeight = FontWeight.Bold, fontSize = 16.sp, color = palette.primaryText)
                            Text("所有大文件均通过 512KB 流式分块读写，绝不阻塞 UI 线程。", fontSize = 12.sp, color = palette.secondaryText)
                            Button(onClick = onClearLocalMediaCache, modifier = Modifier.fillMaxWidth()) {
                                Text("🧹 立即清理本地缓存释放空间")
                            }
                        }
                    }
                }
            }

            TelegramSettingsPage.CHAT_FOLDERS -> {
                item {
                    var title by remember { mutableStateOf("") }
                    var incContacts by remember { mutableStateOf(true) }
                    var incBots by remember { mutableStateOf(false) }
                    var incGroups by remember { mutableStateOf(true) }
                    var incChannels by remember { mutableStateOf(false) }
                    Surface(shape = RoundedCornerShape(16.dp), color = palette.surfaceColor, modifier = Modifier.fillMaxWidth()) {
                        Column(modifier = Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                            Text("📁 新建自定义顶部滑动分组", fontWeight = FontWeight.Bold, color = palette.primaryText)
                            OutlinedTextField(value = title, onValueChange = { title = it }, label = { Text("分组名称 (如：工作 / 共修)") }, singleLine = true, modifier = Modifier.fillMaxWidth())
                            Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                                FilterChipToggle("私聊", incContacts) { incContacts = !incContacts }
                                FilterChipToggle("机器人", incBots) { incBots = !incBots }
                                FilterChipToggle("群组", incGroups) { incGroups = !incGroups }
                                FilterChipToggle("频道", incChannels) { incChannels = !incChannels }
                            }
                            Button(onClick = {
                                onUpsertCustomFolder(title, incContacts, incBots, incGroups, incChannels, false)
                                title = ""
                            }) { Text("+ 添加到首页标签栏") }
                        }
                    }
                }
                items(state.folderTabs, key = { it.id }) { tab ->
                    Surface(shape = RoundedCornerShape(14.dp), color = palette.surfaceColor, modifier = Modifier.fillMaxWidth()) {
                        Row(modifier = Modifier.padding(12.dp), horizontalArrangement = Arrangement.SpaceBetween, verticalAlignment = Alignment.CenterVertically) {
                            Text("${tab.title} (${if (tab.isBuiltIn) "系统内置" else "自定义"})", fontWeight = FontWeight.SemiBold, color = palette.primaryText)
                            if (!tab.isBuiltIn) {
                                TextButton(onClick = { onDeleteCustomFolder(tab.id) }) {
                                    Text("移除", color = palette.destructiveRed)
                                }
                            }
                        }
                    }
                }
            }

            TelegramSettingsPage.DEVICES_SESSIONS -> {
                item {
                    Surface(shape = RoundedCornerShape(16.dp), color = palette.surfaceColor, modifier = Modifier.fillMaxWidth()) {
                        Column(modifier = Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                            Text("💻 当前活跃登录设备", fontWeight = FontWeight.Bold, color = palette.primaryText)
                            Text("1. Android Native · Fabushi GPUI-Telegram v2.0 (当前本机 · 在线)", fontSize = 13.sp, color = Color(0xFF2FA84F), fontWeight = FontWeight.SemiBold)
                            Text("2. GPUI-Telegram Desktop · Rust Host Sync (上次同步：刚刚)", fontSize = 13.sp, color = palette.secondaryText)
                        }
                    }
                }
            }

            TelegramSettingsPage.POWER_SAVING -> {
                item {
                    Surface(shape = RoundedCornerShape(16.dp), color = palette.surfaceColor, modifier = Modifier.fillMaxWidth()) {
                        Column(modifier = Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                            Text("电量低于 ${state.powerSavingThreshold}% 时自动开启省电模式", fontWeight = FontWeight.Bold, color = palette.primaryText)
                            Slider(
                                value = state.powerSavingThreshold.toFloat(),
                                onValueChange = { onUpdatePowerAndNotifications(it.roundToInt(), state.inAppSoundsEnabled) },
                                valueRange = 5f..80f,
                            )
                        }
                    }
                }
            }

            TelegramSettingsPage.LANGUAGE_TRANSLATE -> {
                item {
                    Surface(shape = RoundedCornerShape(16.dp), color = palette.surfaceColor, modifier = Modifier.fillMaxWidth()) {
                        Column(modifier = Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                            Row(horizontalArrangement = Arrangement.SpaceBetween, verticalAlignment = Alignment.CenterVertically, modifier = Modifier.fillMaxWidth()) {
                                Text("在消息长按菜单中显示「即时翻译」", color = palette.primaryText)
                                Switch(
                                    checked = state.showTranslateButton,
                                    onCheckedChange = { onUpdateLanguageAndTranslate(state.uiLanguage, it, state.autoTranslateChannels) },
                                )
                            }
                            listOf("简体中文", "繁體中文", "English", "日本語").forEach { lang ->
                                Row(
                                    modifier = Modifier
                                        .fillMaxWidth()
                                        .clickable { onUpdateLanguageAndTranslate(lang, state.showTranslateButton, state.autoTranslateChannels) }
                                        .padding(vertical = 6.dp),
                                ) {
                                    Text(if (state.uiLanguage == lang) "🔘 $lang" else "⚪ $lang", color = palette.primaryText)
                                }
                            }
                        }
                    }
                }
            }

            TelegramSettingsPage.STARS_WALLET -> {
                item {
                    Surface(shape = RoundedCornerShape(16.dp), color = palette.surfaceColor, modifier = Modifier.fillMaxWidth()) {
                        Column(modifier = Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                            Text("⭐ Telegram Stars 余额：${state.tonWallet.starsBalance}", fontWeight = FontWeight.Bold, fontSize = 18.sp, color = palette.goldMerit)
                            Text("💎 TON 余额：${"%.2f".format(state.tonWallet.tonBalance)} TON", fontSize = 15.sp, color = palette.primaryAccent)
                            Button(onClick = onOpenTonWallet, modifier = Modifier.fillMaxWidth()) {
                                Text("打开完整 TON Space 自托管钱包 →")
                            }
                        }
                    }
                }
            }

            TelegramSettingsPage.PROXY_SERVER, TelegramSettingsPage.OVERVIEW -> {
                item {
                    var endpoint by remember(state.serverEndpoint) { mutableStateOf(state.serverEndpoint) }
                    var proxyOn by remember(state.proxyEnabled) { mutableStateOf(state.proxyEnabled) }
                    var proxyHost by remember(state.proxyHost) { mutableStateOf(state.proxyHost) }
                    Surface(shape = RoundedCornerShape(16.dp), color = palette.surfaceColor, modifier = Modifier.fillMaxWidth()) {
                        Column(modifier = Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                            Text("🌐 gpui_telegram_server 同步节点与代理", fontWeight = FontWeight.Bold, color = palette.primaryText)
                            OutlinedTextField(value = endpoint, onValueChange = { endpoint = it }, label = { Text("HTTPS / WebSocket 同步服务器端点") }, singleLine = true, modifier = Modifier.fillMaxWidth())
                            Row(horizontalArrangement = Arrangement.SpaceBetween, verticalAlignment = Alignment.CenterVertically, modifier = Modifier.fillMaxWidth()) {
                                Text("启用 SOCKS5 / MTProto 代理", color = palette.primaryText)
                                Switch(checked = proxyOn, onCheckedChange = { proxyOn = it })
                            }
                            if (proxyOn) {
                                OutlinedTextField(value = proxyHost, onValueChange = { proxyHost = it }, label = { Text("代理地址 (host:port)") }, singleLine = true, modifier = Modifier.fillMaxWidth())
                            }
                            Button(onClick = { onConfigureServerEndpoint(endpoint, proxyOn, proxyHost) }, modifier = Modifier.fillMaxWidth()) {
                                Text("保存并触发 PTS 差量同步")
                            }
                        }
                    }
                }
                if (updateState != null) {
                    item {
                        UpdateStatusCard(updateState = updateState, onCheck = onCheckForUpdates, onInstall = onDownloadAndInstallUpdate)
                    }
                }
            }
        }
    }
}

@Composable
private fun FilterChipToggle(label: String, active: Boolean, onClick: () -> Unit) {
    val palette = LocalTelegramPalette.current
    Surface(
        shape = RoundedCornerShape(12.dp),
        color = if (active) palette.primaryAccent else palette.elevatedSurface,
        modifier = Modifier.clickable(onClick = onClick),
    ) {
        Text(
            text = label,
            color = if (active) Color.White else palette.primaryText,
            fontSize = 12.sp,
            fontWeight = FontWeight.SemiBold,
            modifier = Modifier.padding(horizontal = 10.dp, vertical = 6.dp),
        )
    }
}

@Composable
private fun UpdateStatusCard(
    updateState: AndroidUpdateUiState,
    onCheck: () -> Unit,
    onInstall: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val palette = LocalTelegramPalette.current
    Surface(shape = RoundedCornerShape(16.dp), color = palette.surfaceColor, modifier = modifier.fillMaxWidth()) {
        Column(modifier = Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            Row(horizontalArrangement = Arrangement.SpaceBetween, verticalAlignment = Alignment.CenterVertically, modifier = Modifier.fillMaxWidth()) {
                Text("📦 Android 签名版本自更新 · v${updateState.currentVersion}", fontWeight = FontWeight.Bold, fontSize = 14.sp, color = palette.primaryText)
                Text(updateState.phase.name, fontFamily = FontFamily.Monospace, fontSize = 11.sp, color = palette.primaryAccent)
            }
            if (!updateState.message.isNullOrBlank()) {
                Text(updateState.message, fontSize = 12.sp, color = palette.secondaryText)
            }
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                if (updateState.phase == AndroidUpdatePhase.AVAILABLE || updateState.phase == AndroidUpdatePhase.WAITING_FOR_PERMISSION) {
                    Button(
                        onClick = onInstall,
                        colors = ButtonDefaults.buttonColors(containerColor = Color(0xFF2FA84F), contentColor = Color.White),
                    ) {
                        Text("下载并校验签名安装 v${updateState.availableVersion ?: ""}")
                    }
                }
                TextButton(onClick = onCheck) {
                    Text("检查 GitHub Releases 更新")
                }
            }
        }
    }
}
