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
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
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
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.ombhrum.fabushi.AndroidUpdatePhase
import com.ombhrum.fabushi.AndroidUpdateUiState
import kotlin.math.roundToInt

/**
 * Apple iOS Grouped Inset Settings Screen (`F15`, Master Spec §8.8).
 * Features truthful authenticated profile, real server active sessions (`GET/DELETE /api/v1/auth/sessions*`),
 * working Sign Out, live `zh-CN` <-> `en-US` language switching, Apple Day/Night/Tinted + Desktop v1.6.6
 * 8 accent swatches, and real `cacheDir` measurement/clearing.
 */
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
    onOpenGlobalDharma: () -> Unit = {},
    onCheckForUpdates: () -> Unit,
    onDownloadAndInstallUpdate: () -> Unit,
    onInitSplitKeyVault: (String) -> Unit = {},
    onRequestDeviceVaultSync: () -> Unit = {},
    onRefreshSessions: () -> Unit = {},
    onRevokeSession: (String) -> Unit = {},
    onRevokeOtherSessions: () -> Unit = {},
    onSignOut: () -> Unit = {},
    onBackToHome: () -> Unit,
) {
    val palette = LocalTelegramPalette.current
    val strings = LocalAppleUiStrings.current
    val page = state.settingsPage

    BackHandler {
        if (page == TelegramSettingsPage.OVERVIEW) onBackToHome()
        else onSelectPage(TelegramSettingsPage.OVERVIEW)
    }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(palette.groupedBackground)
            .testTag("telegram-settings-screen"),
    ) {
        if (page == TelegramSettingsPage.OVERVIEW) {
            SettingsOverviewContent(
                state = state,
                updateState = updateState,
                onSelectPage = onSelectPage,
                onOpenTonWallet = onOpenTonWallet,
                onOpenGlobalDharma = onOpenGlobalDharma,
                onCheckForUpdates = onCheckForUpdates,
                onDownloadAndInstallUpdate = onDownloadAndInstallUpdate,
                onSignOut = onSignOut,
            )
        } else {
            AppleSubPageHeader(
                title = localizedPageTitle(page, strings),
                backLabel = strings.settingsTitle,
                onBack = { onSelectPage(TelegramSettingsPage.OVERVIEW) },
            )
            SettingsSubPageContent(
                page = page,
                state = state,
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
                onInitSplitKeyVault = onInitSplitKeyVault,
                onRequestDeviceVaultSync = onRequestDeviceVaultSync,
                onRefreshSessions = onRefreshSessions,
                onRevokeSession = onRevokeSession,
                onRevokeOtherSessions = onRevokeOtherSessions,
            )
        }
    }
}

private fun localizedPageTitle(page: TelegramSettingsPage, strings: AppleUiStrings): String = when (page) {
    TelegramSettingsPage.OVERVIEW -> strings.settingsTitle
    TelegramSettingsPage.PROFILE_BIO -> strings.settingsProfile
    TelegramSettingsPage.CHAT_THEMES -> strings.settingsAppearance
    TelegramSettingsPage.PRIVACY_SECURITY -> strings.settingsPrivacySecurity
    TelegramSettingsPage.NOTIFICATIONS_SOUNDS -> strings.settingsNotifications
    TelegramSettingsPage.DATA_STORAGE -> strings.settingsDataStorage
    TelegramSettingsPage.CHAT_FOLDERS -> strings.settingsFolders
    TelegramSettingsPage.DEVICES_SESSIONS -> strings.settingsActiveSessions
    TelegramSettingsPage.POWER_SAVING -> strings.settingsPowerSaving
    TelegramSettingsPage.LANGUAGE_TRANSLATE -> strings.settingsLanguage
    TelegramSettingsPage.STARS_WALLET -> strings.settingsTonWallet
    TelegramSettingsPage.PROXY_SERVER -> strings.settingsServerProxy
}

@Composable
private fun AppleSubPageHeader(
    title: String,
    backLabel: String,
    onBack: () -> Unit,
) {
    val palette = LocalTelegramPalette.current
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .background(palette.topBarBackground)
            .statusBarsPadding()
            .padding(horizontal = 10.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Row(
            modifier = Modifier
                .clip(RoundedCornerShape(10.dp))
                .clickable(onClick = onBack)
                .padding(horizontal = 6.dp, vertical = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            TelegramIcon(TelegramGlyph.CHEVRON_LEFT, tint = palette.topBarActionColor, size = 18.dp)
            Spacer(Modifier.width(2.dp))
            Text(backLabel, color = palette.topBarActionColor, fontSize = 15.sp, fontWeight = FontWeight.Medium)
        }
        Spacer(Modifier.width(8.dp))
        Text(
            text = title,
            color = palette.topBarContent,
            fontSize = 16.sp,
            fontWeight = FontWeight.SemiBold,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
    TelegramHairlineDivider()
}

@Composable
private fun SettingsOverviewContent(
    state: TelegramUiState,
    updateState: AndroidUpdateUiState?,
    onSelectPage: (TelegramSettingsPage) -> Unit,
    onOpenTonWallet: () -> Unit,
    onOpenGlobalDharma: () -> Unit,
    onCheckForUpdates: () -> Unit,
    onDownloadAndInstallUpdate: () -> Unit,
    onSignOut: () -> Unit,
) {
    val palette = LocalTelegramPalette.current
    val strings = LocalAppleUiStrings.current
    val account = state.activeAccount

    LazyColumn(
        modifier = Modifier.fillMaxSize(),
        contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 8.dp, bottom = 28.dp),
        verticalArrangement = Arrangement.spacedBy(18.dp),
    ) {
        // iOS Large Title Header
        item {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .statusBarsPadding()
                    .padding(top = 8.dp, bottom = 2.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.SpaceBetween,
            ) {
                Text(
                    text = strings.settingsTitle,
                    color = palette.primaryText,
                    fontSize = 32.sp,
                    fontWeight = FontWeight.Bold,
                )
                if (state.isAuthenticated) {
                    Surface(
                        shape = RoundedCornerShape(12.dp),
                        color = if (state.serverWsConnected) palette.secretGreen.copy(alpha = 0.14f) else palette.inputBg,
                    ) {
                        Text(
                            text = if (state.serverWsConnected) {
                                if (strings.isEnglish) "Connected" else "已连接"
                            } else {
                                if (strings.isEnglish) "Reconnecting" else "同步中"
                            },
                            color = if (state.serverWsConnected) palette.statusGreenText else palette.secondaryText,
                            fontSize = 11.sp,
                            fontWeight = FontWeight.SemiBold,
                            modifier = Modifier.padding(horizontal = 10.dp, vertical = 4.dp),
                        )
                    }
                }
            }
        }

        // Top Profile Card (truthful account details, zero fake "@fabushi" or "+86" fallbacks)
        item {
            SettingsInsetGroup {
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .clickable { onSelectPage(TelegramSettingsPage.PROFILE_BIO) }
                        .padding(horizontal = 14.dp, vertical = 14.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    val displayName = account?.displayName?.takeIf { it.isNotBlank() }
                        ?: if (state.offlineLocalMode) strings.offlineMode else strings.signedOutTitle
                    TelegramAvatar(
                        idSeed = account?.id ?: "account-none",
                        title = displayName,
                        size = 58.dp,
                        online = state.isAuthenticated && state.serverWsConnected,
                    )
                    Spacer(Modifier.width(14.dp))
                    Column(modifier = Modifier.weight(1f)) {
                        Text(
                            text = displayName,
                            color = palette.primaryText,
                            fontSize = 18.sp,
                            fontWeight = FontWeight.Bold,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                        val detailParts = listOfNotNull(
                            account?.username?.takeIf { it.isNotBlank() }?.let { if (it.startsWith("@")) it else "@$it" },
                            account?.phone?.takeIf { it.isNotBlank() },
                        )
                        val subtitle = if (detailParts.isNotEmpty()) {
                            detailParts.joinToString(" · ")
                        } else if (state.isAuthenticated) {
                            account?.bio?.takeIf { it.isNotBlank() } ?: state.serverEndpoint
                        } else {
                            state.serverEndpoint
                        }
                        Text(
                            text = subtitle,
                            color = palette.secondaryText,
                            fontSize = 13.sp,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                    TelegramIcon(TelegramGlyph.CHEVRON_RIGHT, tint = palette.hintText, size = 15.dp)
                }
            }
        }

        // Group 1: Account & Security
        item {
            SettingsInsetGroup {
                SettingsInsetRow(
                    glyph = TelegramGlyph.CONTACTS,
                    badgeColor = Color(0xFF007AFF),
                    title = strings.settingsProfile,
                    trailingValue = account?.username?.takeIf { it.isNotBlank() }.orEmpty(),
                    onClick = { onSelectPage(TelegramSettingsPage.PROFILE_BIO) },
                )
                SettingsRowDivider()
                SettingsInsetRow(
                    glyph = TelegramGlyph.CLOUD_COMPUTER,
                    badgeColor = Color(0xFF5856D6),
                    title = strings.settingsActiveSessions,
                    trailingValue = if (state.isAuthenticated) "${state.serverSessions.size}" else "—",
                    onClick = { onSelectPage(TelegramSettingsPage.DEVICES_SESSIONS) },
                )
                SettingsRowDivider()
                SettingsInsetRow(
                    glyph = TelegramGlyph.LOCK_CLOSED,
                    badgeColor = Color(0xFF34C759),
                    title = strings.settingsPrivacySecurity,
                    trailingValue = if (state.passcodeConfigured) {
                        if (strings.isEnglish) "Locked" else "已开启"
                    } else {
                        if (strings.isEnglish) "Off" else "未设置"
                    },
                    onClick = { onSelectPage(TelegramSettingsPage.PRIVACY_SECURITY) },
                )
            }
        }

        // Group 2: Appearance, Language, Notifications & Folders
        item {
            SettingsInsetGroup {
                SettingsInsetRow(
                    glyph = TelegramGlyph.SUN,
                    badgeColor = Color(0xFFAF52DE),
                    title = strings.settingsAppearance,
                    trailingValue = state.themeSettings.preset.label,
                    onClick = { onSelectPage(TelegramSettingsPage.CHAT_THEMES) },
                )
                SettingsRowDivider()
                SettingsInsetRow(
                    glyph = TelegramGlyph.TRANSLATE,
                    badgeColor = Color(0xFF5AC8FA),
                    title = strings.settingsLanguage,
                    trailingValue = if (isEnglishLanguage(state.uiLanguage)) "English" else "简体中文",
                    onClick = { onSelectPage(TelegramSettingsPage.LANGUAGE_TRANSLATE) },
                )
                SettingsRowDivider()
                SettingsInsetRow(
                    glyph = TelegramGlyph.BELL,
                    badgeColor = Color(0xFFFF3B30),
                    title = strings.settingsNotifications,
                    trailingValue = if (state.inAppSoundsEnabled) {
                        if (strings.isEnglish) "On" else "开启"
                    } else {
                        if (strings.isEnglish) "Muted" else "静音"
                    },
                    onClick = { onSelectPage(TelegramSettingsPage.NOTIFICATIONS_SOUNDS) },
                )
                SettingsRowDivider()
                SettingsInsetRow(
                    glyph = TelegramGlyph.FOLDER,
                    badgeColor = Color(0xFFFF9500),
                    title = strings.settingsFolders,
                    trailingValue = "${state.folderTabs.size}",
                    onClick = { onSelectPage(TelegramSettingsPage.CHAT_FOLDERS) },
                )
            }
        }

        // Group 3: Storage, Server, Wallet, Dharma & Power
        item {
            SettingsInsetGroup {
                val cacheLabel = if (state.storageCacheMb <= 0.0) "0 MB" else "%.2f MB".format(state.storageCacheMb)
                SettingsInsetRow(
                    glyph = TelegramGlyph.DOCUMENT_FILE,
                    badgeColor = Color(0xFF30B0C7),
                    title = strings.settingsDataStorage,
                    trailingValue = cacheLabel,
                    onClick = { onSelectPage(TelegramSettingsPage.DATA_STORAGE) },
                )
                SettingsRowDivider()
                SettingsInsetRow(
                    glyph = TelegramGlyph.LINK_CHAIN,
                    badgeColor = Color(0xFF8E8E93),
                    title = strings.settingsServerProxy,
                    trailingValue = state.serverEndpoint.removePrefix("http://").removePrefix("https://").take(22),
                    onClick = { onSelectPage(TelegramSettingsPage.PROXY_SERVER) },
                )
                SettingsRowDivider()
                SettingsInsetRow(
                    glyph = TelegramGlyph.TON_DIAMOND,
                    badgeColor = Color(0xFF0098EA),
                    title = strings.settingsTonWallet,
                    trailingValue = if (state.tonWallet.initialized) {
                        "%.2f TON".format(state.tonWallet.tonBalance)
                    } else {
                        if (strings.isEnglish) "Setup" else "未创建"
                    },
                    onClick = onOpenTonWallet,
                )
                SettingsRowDivider()
                SettingsInsetRow(
                    glyph = TelegramGlyph.DHARMA_WHEEL,
                    badgeColor = Color(0xFFD99B26),
                    title = strings.settingsGlobalDharma,
                    trailingValue = if (state.globalDharma.myMeritPoints > 0) "${state.globalDharma.myMeritPoints}" else "",
                    onClick = onOpenGlobalDharma,
                )
                SettingsRowDivider()
                SettingsInsetRow(
                    glyph = TelegramGlyph.MOON,
                    badgeColor = Color(0xFF34C759),
                    title = strings.settingsPowerSaving,
                    trailingValue = "${state.powerSavingThreshold}%",
                    onClick = { onSelectPage(TelegramSettingsPage.POWER_SAVING) },
                )
            }
        }

        // App Self-Update Card
        if (updateState != null) {
            item {
                UpdateStatusCard(
                    updateState = updateState,
                    onCheck = onCheckForUpdates,
                    onInstall = onDownloadAndInstallUpdate,
                )
            }
        }

        // Group 4: Sign Out (Destructive Red) or Sign In
        item {
            SettingsInsetGroup {
                Box(
                    modifier = Modifier
                        .fillMaxWidth()
                        .clickable(onClick = onSignOut)
                        .padding(vertical = 14.dp)
                        .testTag("settings-sign-out-button"),
                    contentAlignment = Alignment.Center,
                ) {
                    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        TelegramIcon(
                            glyph = TelegramGlyph.SIGN_OUT,
                            tint = if (state.isAuthenticated) palette.destructiveRed else palette.primaryAccent,
                            size = 18.dp,
                        )
                        Text(
                            text = if (state.isAuthenticated) strings.signOut else strings.signIn,
                            color = if (state.isAuthenticated) palette.destructiveRed else palette.primaryAccent,
                            fontSize = 15.sp,
                            fontWeight = FontWeight.SemiBold,
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun SettingsInsetGroup(content: @Composable () -> Unit) {
    val palette = LocalTelegramPalette.current
    Surface(
        shape = RoundedCornerShape(12.dp),
        color = palette.modalBg,
        modifier = Modifier
            .fillMaxWidth()
            .border(0.5.dp, palette.borderColor, RoundedCornerShape(12.dp)),
    ) {
        Column(modifier = Modifier.fillMaxWidth()) {
            content()
        }
    }
}

@Composable
private fun SettingsInsetRow(
    glyph: TelegramGlyph,
    badgeColor: Color,
    title: String,
    trailingValue: String = "",
    onClick: () -> Unit,
) {
    val palette = LocalTelegramPalette.current
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(onClick = onClick)
            .padding(horizontal = 14.dp, vertical = 11.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(
            modifier = Modifier
                .size(29.dp)
                .clip(RoundedCornerShape(7.dp))
                .background(badgeColor),
            contentAlignment = Alignment.Center,
        ) {
            TelegramIcon(glyph = glyph, tint = Color.White, size = 16.dp)
        }
        Spacer(Modifier.width(12.dp))
        Text(
            text = title,
            color = palette.primaryText,
            fontSize = 15.sp,
            modifier = Modifier.weight(1f),
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        if (trailingValue.isNotBlank()) {
            Spacer(Modifier.width(8.dp))
            Text(
                text = trailingValue,
                color = palette.secondaryText,
                fontSize = 14.sp,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        Spacer(Modifier.width(6.dp))
        TelegramIcon(TelegramGlyph.CHEVRON_RIGHT, tint = palette.hintText, size = 14.dp)
    }
}

@Composable
private fun SettingsRowDivider() {
    TelegramHairlineDivider(modifier = Modifier.padding(start = 55.dp))
}

@Composable
private fun SettingsSubPageContent(
    page: TelegramSettingsPage,
    state: TelegramUiState,
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
    onInitSplitKeyVault: (String) -> Unit,
    onRequestDeviceVaultSync: () -> Unit,
    onRefreshSessions: () -> Unit,
    onRevokeSession: (String) -> Unit,
    onRevokeOtherSessions: () -> Unit,
) {
    val palette = LocalTelegramPalette.current
    val strings = LocalAppleUiStrings.current
    val inputColors = OutlinedTextFieldDefaults.colors(
        focusedTextColor = palette.primaryText,
        unfocusedTextColor = palette.primaryText,
        cursorColor = palette.searchCursorColor,
        focusedBorderColor = palette.primaryAccent,
        unfocusedBorderColor = palette.borderColor,
    )

    LazyColumn(
        modifier = Modifier.fillMaxSize(),
        contentPadding = PaddingValues(16.dp),
        verticalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        when (page) {
            TelegramSettingsPage.PROFILE_BIO -> {
                item {
                    val acc = state.activeAccount
                    var displayName by remember(acc?.id, acc?.displayName) { mutableStateOf(acc?.displayName.orEmpty()) }
                    var username by remember(acc?.id, acc?.username) { mutableStateOf(acc?.username.orEmpty()) }
                    var bio by remember(acc?.id, acc?.bio) { mutableStateOf(acc?.bio.orEmpty()) }
                    SettingsInsetGroup {
                        Column(
                            modifier = Modifier.padding(14.dp),
                            verticalArrangement = Arrangement.spacedBy(10.dp),
                        ) {
                            OutlinedTextField(
                                value = displayName,
                                onValueChange = { displayName = it },
                                label = { Text(if (strings.isEnglish) "Display Name" else "显示名称") },
                                singleLine = true,
                                colors = inputColors,
                                modifier = Modifier.fillMaxWidth(),
                            )
                            OutlinedTextField(
                                value = username,
                                onValueChange = { username = it },
                                label = { Text(if (strings.isEnglish) "Username (@handle)" else "用户名 (@handle)") },
                                singleLine = true,
                                colors = inputColors,
                                modifier = Modifier.fillMaxWidth(),
                            )
                            OutlinedTextField(
                                value = bio,
                                onValueChange = { bio = it },
                                label = { Text(if (strings.isEnglish) "Bio" else "个人简介") },
                                minLines = 2,
                                colors = inputColors,
                                modifier = Modifier.fillMaxWidth(),
                            )
                            Button(
                                enabled = state.isAuthenticated && displayName.isNotBlank(),
                                onClick = { onUpdateProfile(displayName.trim(), username.trim(), bio.trim()) },
                                colors = ButtonDefaults.buttonColors(containerColor = palette.primaryAccent, contentColor = Color.White),
                                shape = RoundedCornerShape(10.dp),
                                modifier = Modifier.fillMaxWidth(),
                            ) {
                                Text(strings.save)
                            }
                        }
                    }
                }
            }

            TelegramSettingsPage.CHAT_THEMES -> {
                item {
                    val cur = state.themeSettings
                    SettingsInsetGroup {
                        Column(modifier = Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                            Text(
                                text = if (strings.isEnglish) "Apple Messages Themes" else "Apple Messages 主题",
                                color = palette.secondaryText,
                                fontSize = 12.sp,
                                fontWeight = FontWeight.SemiBold,
                            )
                            Row(
                                modifier = Modifier.fillMaxWidth(),
                                horizontalArrangement = Arrangement.spacedBy(8.dp),
                            ) {
                                listOf(
                                    TelegramThemePreset.APPLE_DAY to (if (strings.isEnglish) "Light" else "明亮"),
                                    TelegramThemePreset.APPLE_NIGHT to (if (strings.isEnglish) "Dark" else "深色"),
                                    TelegramThemePreset.APPLE_TINTED to (if (strings.isEnglish) "Tinted" else "靛蓝"),
                                ).forEach { (preset, label) ->
                                    val selected = cur.preset == preset
                                    val previewBg = when (preset) {
                                        TelegramThemePreset.APPLE_DAY -> Color(0xFFFFFFFF)
                                        TelegramThemePreset.APPLE_NIGHT -> Color(0xFF1E1E1E)
                                        else -> Color(0xFF181C22)
                                    }
                                    val previewFg = if (preset == TelegramThemePreset.APPLE_DAY) Color(0xFF1D1D1F) else Color(0xFFF5F5F7)
                                    Surface(
                                        shape = RoundedCornerShape(12.dp),
                                        color = previewBg,
                                        modifier = Modifier
                                            .weight(1f)
                                            .border(
                                                width = if (selected) 2.dp else 1.dp,
                                                color = if (selected) palette.primaryAccent else palette.borderColor,
                                                shape = RoundedCornerShape(12.dp),
                                            )
                                            .clip(RoundedCornerShape(12.dp))
                                            .clickable {
                                                onUpdateTheme {
                                                    it.copy(
                                                        preset = preset,
                                                        wallpaperStyle = TelegramWallpaperStyle.SOLID_MINIMAL,
                                                        wallpaperDimPercent = 0,
                                                    )
                                                }
                                            },
                                    ) {
                                        Column(
                                            modifier = Modifier.padding(10.dp),
                                            horizontalAlignment = Alignment.CenterHorizontally,
                                            verticalArrangement = Arrangement.spacedBy(6.dp),
                                        ) {
                                            Box(
                                                modifier = Modifier
                                                    .fillMaxWidth()
                                                    .height(16.dp)
                                                    .clip(RoundedCornerShape(8.dp))
                                                    .background(Color(0xFF007AFF)),
                                            )
                                            Text(label, color = previewFg, fontSize = 12.sp, fontWeight = FontWeight.SemiBold)
                                        }
                                    }
                                }
                            }

                            Text(
                                text = if (strings.isEnglish) "Accent Swatches" else "主题强调色 (Desktop v1.6.6)",
                                color = palette.secondaryText,
                                fontSize = 12.sp,
                                fontWeight = FontWeight.SemiBold,
                            )
                            Row(
                                modifier = Modifier.fillMaxWidth(),
                                horizontalArrangement = Arrangement.SpaceBetween,
                            ) {
                                DesktopV166AccentSwatches.forEach { argb ->
                                    val sel = cur.customAccentArgb == argb && cur.preset == TelegramThemePreset.CUSTOM_ACCENT
                                    Box(
                                        modifier = Modifier
                                            .size(30.dp)
                                            .clip(CircleShape)
                                            .background(Color(argb.toInt()))
                                            .border(
                                                width = if (sel) 2.5.dp else 1.dp,
                                                color = if (sel) palette.primaryText else palette.borderColor,
                                                shape = CircleShape,
                                            )
                                            .clickable {
                                                onUpdateTheme {
                                                    it.copy(
                                                        preset = TelegramThemePreset.CUSTOM_ACCENT,
                                                        customAccentArgb = argb,
                                                        customDark = cur.isDarkEffective,
                                                    )
                                                }
                                            },
                                    )
                                }
                            }

                            Text(
                                text = if (strings.isEnglish) "Bubble Corner Radius: ${cur.bubbleCornerRadiusDp}dp" else "气泡圆角：${cur.bubbleCornerRadiusDp}dp",
                                color = palette.primaryText,
                                fontSize = 13.sp,
                            )
                            Slider(
                                value = cur.bubbleCornerRadiusDp.toFloat(),
                                onValueChange = { v -> onUpdateTheme { it.copy(bubbleCornerRadiusDp = v.roundToInt().coerceIn(4, 18)) } },
                                valueRange = 4f..18f,
                            )

                            Text(
                                text = if (strings.isEnglish) "Message Font Size: ${cur.messageFontSizeSp}sp" else "消息字号：${cur.messageFontSizeSp}sp",
                                color = palette.primaryText,
                                fontSize = 13.sp,
                            )
                            Slider(
                                value = cur.messageFontSizeSp.toFloat(),
                                onValueChange = { v -> onUpdateTheme { it.copy(messageFontSizeSp = v.roundToInt().coerceIn(12, 22)) } },
                                valueRange = 12f..22f,
                            )

                            Text(
                                text = if (strings.isEnglish) "Wallpaper Dim: ${cur.wallpaperDimPercent}%" else "壁纸遮罩亮度：${cur.wallpaperDimPercent}%",
                                color = palette.primaryText,
                                fontSize = 13.sp,
                            )
                            Slider(
                                value = cur.wallpaperDimPercent.toFloat(),
                                onValueChange = { v -> onUpdateTheme { it.copy(wallpaperDimPercent = v.roundToInt().coerceIn(0, 60)) } },
                                valueRange = 0f..60f,
                            )
                        }
                    }
                }
            }

            TelegramSettingsPage.PRIVACY_SECURITY -> {
                item {
                    var pinDraft by remember { mutableStateOf("") }
                    SettingsInsetGroup {
                        Column(modifier = Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                            Text(
                                text = if (strings.isEnglish) "Passcode Lock (Rust Argon2id)" else "应用锁密码 (Rust Argon2id)",
                                color = palette.primaryText,
                                fontWeight = FontWeight.SemiBold,
                                fontSize = 15.sp,
                            )
                            OutlinedTextField(
                                value = pinDraft,
                                onValueChange = { if (it.length <= 4 && it.all(Char::isDigit)) pinDraft = it },
                                label = { Text(if (strings.isEnglish) "4-digit PIN" else "设置 4 位数字密码") },
                                singleLine = true,
                                visualTransformation = PasswordVisualTransformation(),
                                colors = inputColors,
                                modifier = Modifier.fillMaxWidth(),
                            )
                            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                Button(
                                    enabled = pinDraft.length == 4,
                                    onClick = {
                                        onConfigurePasscode(pinDraft)
                                        pinDraft = ""
                                    },
                                    colors = ButtonDefaults.buttonColors(containerColor = palette.primaryAccent, contentColor = Color.White),
                                ) {
                                    Text(if (strings.isEnglish) "Enable" else "启用密码锁")
                                }
                                if (state.passcodeConfigured) {
                                    OutlinedButton(onClick = onLockAppNow) {
                                        Text(if (strings.isEnglish) "Lock Now" else "立即锁定", color = palette.primaryAccent)
                                    }
                                    TextButton(onClick = onDisablePasscode) {
                                        Text(if (strings.isEnglish) "Disable" else "关闭", color = palette.destructiveRed)
                                    }
                                }
                            }
                        }
                    }
                }
                item {
                    var vaultPassword by remember { mutableStateOf("") }
                    Surface(
                        shape = RoundedCornerShape(12.dp),
                        color = palette.modalBg,
                        modifier = Modifier
                            .fillMaxWidth()
                            .border(0.5.dp, palette.borderColor, RoundedCornerShape(12.dp))
                            .testTag("settings-user-vault-card"),
                    ) {
                        Column(modifier = Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                            Text(
                                text = if (strings.isEnglish) "Split-Key User Vault" else "分片密钥保险库 (Split-Key Vault)",
                                color = palette.primaryText,
                                fontWeight = FontWeight.SemiBold,
                                fontSize = 15.sp,
                            )
                            Text(
                                text = if (state.userVault.initialized) {
                                    "状态：已初始化 · ${state.userVault.kdfAlg} v${state.userVault.keyVersion} · 已加密 ${state.userVault.encryptedItemCount} 项"
                                } else {
                                    state.userVault.lastVaultMessage
                                },
                                color = palette.secondaryText,
                                fontSize = 12.sp,
                            )
                            OutlinedTextField(
                                value = vaultPassword,
                                onValueChange = { vaultPassword = it },
                                label = { Text(if (strings.isEnglish) "Master Password" else "主密码") },
                                singleLine = true,
                                visualTransformation = PasswordVisualTransformation(),
                                colors = inputColors,
                                modifier = Modifier.fillMaxWidth(),
                            )
                            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                Button(
                                    enabled = vaultPassword.length >= 6,
                                    onClick = {
                                        onInitSplitKeyVault(vaultPassword)
                                        vaultPassword = ""
                                    },
                                    colors = ButtonDefaults.buttonColors(containerColor = palette.primaryAccent, contentColor = Color.White),
                                ) {
                                    Text(if (strings.isEnglish) "Initialize Vault" else "初始化保险库")
                                }
                                OutlinedButton(onClick = onRequestDeviceVaultSync) {
                                    Text(if (strings.isEnglish) "Sync Devices" else "请求跨设备同步", color = palette.primaryAccent)
                                }
                            }
                        }
                    }
                }
            }

            TelegramSettingsPage.DEVICES_SESSIONS -> {
                item {
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.SpaceBetween,
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Text(
                            text = if (strings.isEnglish) "Active Sessions (${state.serverSessions.size})" else "活跃登录会话 (${state.serverSessions.size})",
                            color = palette.secondaryText,
                            fontSize = 12.sp,
                            fontWeight = FontWeight.SemiBold,
                        )
                        TextButton(onClick = onRefreshSessions) {
                            Text(if (strings.isEnglish) "Refresh" else "刷新", color = palette.topBarActionColor)
                        }
                    }
                    SettingsInsetGroup {
                        if (state.serverSessions.isEmpty()) {
                            Box(modifier = Modifier.fillMaxWidth().padding(20.dp), contentAlignment = Alignment.Center) {
                                Text(
                                    text = if (state.isAuthenticated) {
                                        if (strings.isEnglish) "No active server sessions returned." else "服务端暂无返回活跃会话记录。"
                                    } else {
                                        if (strings.isEnglish) "Sign in to view active device sessions." else "登录服务器账号后可查看并管理活跃设备会话。"
                                    },
                                    color = palette.secondaryText,
                                    fontSize = 13.sp,
                                )
                            }
                        } else {
                            state.serverSessions.forEachIndexed { idx, session ->
                                Row(
                                    modifier = Modifier
                                        .fillMaxWidth()
                                        .padding(horizontal = 14.dp, vertical = 12.dp),
                                    verticalAlignment = Alignment.CenterVertically,
                                ) {
                                    Column(modifier = Modifier.weight(1f)) {
                                        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                                            Text(
                                                text = session.deviceName.ifBlank { session.platform.ifBlank { "Device" } },
                                                color = palette.primaryText,
                                                fontWeight = FontWeight.SemiBold,
                                                fontSize = 14.sp,
                                            )
                                            if (session.isCurrent) {
                                                Surface(shape = RoundedCornerShape(6.dp), color = palette.secretGreen.copy(alpha = 0.16f)) {
                                                    Text(
                                                        text = if (strings.isEnglish) "Current" else "当前设备",
                                                        color = palette.statusGreenText,
                                                        fontSize = 10.sp,
                                                        fontWeight = FontWeight.SemiBold,
                                                        modifier = Modifier.padding(horizontal = 6.dp, vertical = 2.dp),
                                                    )
                                                }
                                            }
                                        }
                                        val meta = listOfNotNull(
                                            session.platform.takeIf { it.isNotBlank() },
                                            session.ipAddress.takeIf { it.isNotBlank() },
                                        ).joinToString(" · ")
                                        if (meta.isNotBlank()) {
                                            Text(meta, color = palette.secondaryText, fontSize = 12.sp)
                                        }
                                    }
                                    if (!session.isCurrent) {
                                        TextButton(onClick = { onRevokeSession(session.sessionId) }) {
                                            Text(if (strings.isEnglish) "Revoke" else "注销", color = palette.destructiveRed)
                                        }
                                    }
                                }
                                if (idx < state.serverSessions.lastIndex) {
                                    TelegramHairlineDivider(Modifier.padding(start = 14.dp))
                                }
                            }
                        }
                    }
                    if (state.serverSessions.count { !it.isCurrent } > 0) {
                        Spacer(Modifier.height(10.dp))
                        Button(
                            onClick = onRevokeOtherSessions,
                            colors = ButtonDefaults.buttonColors(containerColor = palette.destructiveRed, contentColor = Color.White),
                            shape = RoundedCornerShape(12.dp),
                            modifier = Modifier.fillMaxWidth(),
                        ) {
                            Text(if (strings.isEnglish) "Terminate All Other Sessions" else "注销其他所有设备会话")
                        }
                    }
                }
            }

            TelegramSettingsPage.LANGUAGE_TRANSLATE -> {
                item {
                    val languages = listOf(
                        "zh-CN" to "简体中文",
                        "en-US" to "English",
                    )
                    val currentCanonical = if (isEnglishLanguage(state.uiLanguage)) "en-US" else "zh-CN"
                    SettingsInsetGroup {
                        languages.forEachIndexed { idx, (code, label) ->
                            val selected = currentCanonical == code
                            Row(
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .clickable {
                                        onUpdateLanguageAndTranslate(code, state.showTranslateButton, state.autoTranslateChannels)
                                    }
                                    .padding(horizontal = 14.dp, vertical = 13.dp)
                                    .testTag("settings-lang-$code"),
                                verticalAlignment = Alignment.CenterVertically,
                            ) {
                                Text(label, color = palette.primaryText, fontSize = 15.sp, modifier = Modifier.weight(1f))
                                if (selected) {
                                    TelegramIcon(TelegramGlyph.CHECK_SINGLE, tint = palette.primaryAccent, size = 18.dp)
                                }
                            }
                            if (idx < languages.lastIndex) TelegramHairlineDivider(Modifier.padding(start = 14.dp))
                        }
                    }
                    Spacer(Modifier.height(12.dp))
                    SettingsInsetGroup {
                        Row(
                            modifier = Modifier
                                .fillMaxWidth()
                                .padding(horizontal = 14.dp, vertical = 10.dp),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            Text(
                                text = if (strings.isEnglish) "Show Translate Button" else "在消息菜单中显示翻译入口",
                                color = palette.primaryText,
                                fontSize = 14.sp,
                                modifier = Modifier.weight(1f),
                            )
                            Switch(
                                checked = state.showTranslateButton,
                                onCheckedChange = { onUpdateLanguageAndTranslate(currentCanonical, it, state.autoTranslateChannels) },
                            )
                        }
                    }
                }
            }

            TelegramSettingsPage.DATA_STORAGE -> {
                item {
                    val cacheLabel = if (state.storageCacheMb <= 0.0) "0.00 MB" else "%.2f MB".format(state.storageCacheMb)
                    SettingsInsetGroup {
                        Column(modifier = Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                            Text(
                                text = if (strings.isEnglish) "Local Cache Directory" else "本机缓存占用 (cacheDir)",
                                color = palette.secondaryText,
                                fontSize = 12.sp,
                            )
                            Text(cacheLabel, color = palette.primaryText, fontSize = 24.sp, fontWeight = FontWeight.Bold)
                            Button(
                                onClick = onClearLocalMediaCache,
                                colors = ButtonDefaults.buttonColors(containerColor = palette.primaryAccent, contentColor = Color.White),
                                shape = RoundedCornerShape(10.dp),
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .testTag("settings-clear-cache-button"),
                            ) {
                                Text(if (strings.isEnglish) "Clear Local Cache" else "清理本机媒体与临时缓存")
                            }
                        }
                    }
                }
            }

            TelegramSettingsPage.PROXY_SERVER -> {
                item {
                    var endpointDraft by remember(state.serverEndpoint) { mutableStateOf(state.serverEndpoint) }
                    var proxyEnabled by remember(state.proxyEnabled) { mutableStateOf(state.proxyEnabled) }
                    var proxyHost by remember(state.proxyHost) { mutableStateOf(state.proxyHost) }
                    SettingsInsetGroup {
                        Column(modifier = Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                            OutlinedTextField(
                                value = endpointDraft,
                                onValueChange = { endpointDraft = it },
                                label = { Text(if (strings.isEnglish) "Server URL" else "同步服务器地址") },
                                singleLine = true,
                                colors = inputColors,
                                modifier = Modifier.fillMaxWidth(),
                            )
                            Row(
                                modifier = Modifier.fillMaxWidth(),
                                verticalAlignment = Alignment.CenterVertically,
                            ) {
                                Text(
                                    text = if (strings.isEnglish) "Enable Proxy" else "启用代理",
                                    color = palette.primaryText,
                                    fontSize = 14.sp,
                                    modifier = Modifier.weight(1f),
                                )
                                Switch(checked = proxyEnabled, onCheckedChange = { proxyEnabled = it })
                            }
                            if (proxyEnabled) {
                                OutlinedTextField(
                                    value = proxyHost,
                                    onValueChange = { proxyHost = it },
                                    label = { Text("SOCKS5 / HTTP Host:Port") },
                                    singleLine = true,
                                    colors = inputColors,
                                    modifier = Modifier.fillMaxWidth(),
                                )
                            }
                            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                OutlinedButton(
                                    onClick = {
                                        endpointDraft = DEFAULT_DESKTOP_SERVER_URL
                                        onConfigureServerEndpoint(DEFAULT_DESKTOP_SERVER_URL, proxyEnabled, proxyHost)
                                    },
                                    modifier = Modifier.weight(1f),
                                ) {
                                    Text(strings.resetDefaultServer, color = palette.primaryAccent)
                                }
                                Button(
                                    onClick = {
                                        onConfigureServerEndpoint(
                                            endpointDraft.trim().ifBlank { DEFAULT_DESKTOP_SERVER_URL },
                                            proxyEnabled,
                                            proxyHost.trim(),
                                        )
                                    },
                                    colors = ButtonDefaults.buttonColors(containerColor = palette.primaryAccent, contentColor = Color.White),
                                    modifier = Modifier.weight(1f),
                                ) {
                                    Text(strings.save)
                                }
                            }
                        }
                    }
                }
            }

            TelegramSettingsPage.NOTIFICATIONS_SOUNDS -> {
                item {
                    SettingsInsetGroup {
                        Row(
                            modifier = Modifier
                                .fillMaxWidth()
                                .padding(horizontal = 14.dp, vertical = 12.dp),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            Text(
                                text = if (strings.isEnglish) "In-App Notification Sounds" else "应用内消息提示音",
                                color = palette.primaryText,
                                fontSize = 15.sp,
                                modifier = Modifier.weight(1f),
                            )
                            Switch(
                                checked = state.inAppSoundsEnabled,
                                onCheckedChange = { onUpdatePowerAndNotifications(state.powerSavingThreshold, it) },
                            )
                        }
                    }
                }
            }

            TelegramSettingsPage.POWER_SAVING -> {
                item {
                    SettingsInsetGroup {
                        Column(modifier = Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                            Text(
                                text = if (strings.isEnglish) "Low-Battery Threshold: ${state.powerSavingThreshold}%" else "自动省电电量阈值：${state.powerSavingThreshold}%",
                                color = palette.primaryText,
                                fontSize = 14.sp,
                                fontWeight = FontWeight.Medium,
                            )
                            Slider(
                                value = state.powerSavingThreshold.toFloat(),
                                onValueChange = { onUpdatePowerAndNotifications(it.roundToInt(), state.inAppSoundsEnabled) },
                                valueRange = 0f..50f,
                            )
                        }
                    }
                }
            }

            TelegramSettingsPage.CHAT_FOLDERS -> {
                item {
                    var folderTitle by remember { mutableStateOf("") }
                    var includeContacts by remember { mutableStateOf(true) }
                    var includeBots by remember { mutableStateOf(false) }
                    var includeGroups by remember { mutableStateOf(true) }
                    var includeChannels by remember { mutableStateOf(false) }
                    var excludeMuted by remember { mutableStateOf(false) }
                    SettingsInsetGroup {
                        Column(modifier = Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                            OutlinedTextField(
                                value = folderTitle,
                                onValueChange = { folderTitle = it },
                                label = { Text(if (strings.isEnglish) "Folder Name" else "新建文件夹名称") },
                                singleLine = true,
                                colors = inputColors,
                                modifier = Modifier.fillMaxWidth(),
                            )
                            Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                                FilterChipToggle("私聊", includeContacts) { includeContacts = !includeContacts }
                                FilterChipToggle("群组", includeGroups) { includeGroups = !includeGroups }
                                FilterChipToggle("机器人", includeBots) { includeBots = !includeBots }
                                FilterChipToggle("频道", includeChannels) { includeChannels = !includeChannels }
                            }
                            Button(
                                enabled = folderTitle.isNotBlank(),
                                onClick = {
                                    onUpsertCustomFolder(
                                        folderTitle.trim(),
                                        includeContacts,
                                        includeBots,
                                        includeGroups,
                                        includeChannels,
                                        excludeMuted,
                                    )
                                    folderTitle = ""
                                },
                                colors = ButtonDefaults.buttonColors(containerColor = palette.primaryAccent, contentColor = Color.White),
                                modifier = Modifier.fillMaxWidth(),
                            ) {
                                Text(if (strings.isEnglish) "Save Folder" else "保存对话文件夹")
                            }
                        }
                    }
                    Spacer(Modifier.height(12.dp))
                    SettingsInsetGroup {
                        state.folderTabs.forEachIndexed { idx, tab ->
                            Row(
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .padding(horizontal = 14.dp, vertical = 11.dp),
                                verticalAlignment = Alignment.CenterVertically,
                            ) {
                                Text(tab.title, color = palette.primaryText, fontSize = 14.sp, modifier = Modifier.weight(1f))
                                if (!tab.isBuiltIn) {
                                    TextButton(onClick = { onDeleteCustomFolder(tab.id) }) {
                                        Text(strings.delete, color = palette.destructiveRed)
                                    }
                                }
                            }
                            if (idx < state.folderTabs.lastIndex) TelegramHairlineDivider(Modifier.padding(start = 14.dp))
                        }
                    }
                }
            }

            TelegramSettingsPage.STARS_WALLET -> {
                item {
                    SettingsInsetGroup {
                        Column(modifier = Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                            Text(
                                text = "TON: ${"%.4f".format(state.tonWallet.tonBalance)} · Stars: ${state.tonWallet.starsBalance}",
                                color = palette.primaryText,
                                fontSize = 15.sp,
                                fontWeight = FontWeight.SemiBold,
                            )
                            Button(
                                onClick = onOpenTonWallet,
                                colors = ButtonDefaults.buttonColors(containerColor = palette.primaryAccent, contentColor = Color.White),
                                modifier = Modifier.fillMaxWidth(),
                            ) {
                                Text(if (strings.isEnglish) "Open TON Wallet" else "打开 TON 自托管钱包")
                            }
                        }
                    }
                }
            }

            TelegramSettingsPage.OVERVIEW -> Unit
        }
    }
}

@Composable
private fun FilterChipToggle(label: String, active: Boolean, onClick: () -> Unit) {
    val palette = LocalTelegramPalette.current
    Surface(
        shape = RoundedCornerShape(10.dp),
        color = if (active) palette.selectedPillBackground else palette.inputBg,
        modifier = Modifier
            .clip(RoundedCornerShape(10.dp))
            .clickable(onClick = onClick),
    ) {
        Text(
            text = label,
            color = if (active) palette.selectedPillContent else palette.primaryText,
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
    SettingsInsetGroup {
        Column(modifier = modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            Row(
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically,
                modifier = Modifier.fillMaxWidth(),
            ) {
                Text(
                    text = "Android 应用更新 · v${updateState.currentVersion}",
                    fontWeight = FontWeight.SemiBold,
                    fontSize = 14.sp,
                    color = palette.primaryText,
                )
                Text(
                    text = updateState.phase.name,
                    fontFamily = FontFamily.Monospace,
                    fontSize = 11.sp,
                    color = palette.primaryAccent,
                )
            }
            if (!updateState.message.isNullOrBlank()) {
                Text(updateState.message, fontSize = 12.sp, color = palette.secondaryText)
            }
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                if (updateState.phase == AndroidUpdatePhase.AVAILABLE || updateState.phase == AndroidUpdatePhase.WAITING_FOR_PERMISSION) {
                    Button(
                        onClick = onInstall,
                        colors = ButtonDefaults.buttonColors(
                            containerColor = palette.approveActionBg,
                            contentColor = palette.approveActionContent,
                        ),
                    ) {
                        Text("下载并安装 v${updateState.availableVersion ?: ""}")
                    }
                }
                TextButton(onClick = onCheck) {
                    Text("检查更新", color = palette.topBarActionColor)
                }
            }
        }
    }
}
