package com.ombhrum.fabushi.telegram

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.ModalDrawerSheet
import androidx.compose.material3.OutlinedTextField
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
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.ombhrum.fabushi.ConversationKind

@Composable
fun TelegramDrawerSheetContent(
    state: TelegramUiState,
    onToggleNightMode: () -> Unit,
    onSwitchAccount: (String) -> Unit,
    onAddAccount: (displayName: String, phone: String, username: String) -> Unit,
    onOpenSavedMessages: () -> Unit,
    onRequestCreateGroup: () -> Unit,
    onRequestCreateSecretChat: () -> Unit,
    onRequestCreateChannel: () -> Unit,
    onNavigate: (TelegramSubScreen, TelegramSettingsPage) -> Unit,
    onSwitchToGrokShell: () -> Unit,
    onSwitchToClassicFabushi: () -> Unit,
    onCloseDrawer: () -> Unit,
) {
    val palette = LocalTelegramPalette.current
    val activeAccount = state.activeAccount
    var accountsExpanded by remember { mutableStateOf(false) }
    var showAddAccountDialog by remember { mutableStateOf(false) }
    val chevronRotation by animateFloatAsState(targetValue = if (accountsExpanded) 180f else 0f, label = "drawer-chevron")

    ModalDrawerSheet(
        modifier = Modifier
            .width(308.dp)
            .fillMaxHeight()
            .testTag("telegram-drawer-sheet"),
        drawerContainerColor = palette.surfaceColor,
        drawerContentColor = palette.primaryText,
    ) {
        Column(
            modifier = Modifier
                .fillMaxHeight()
                .verticalScroll(rememberScrollState())
                .navigationBarsPadding(),
        ) {
            // 1. Authentic Telegram Gradient Drawer Header
            Box(
                modifier = Modifier
                    .fillMaxWidth()
                    .background(Brush.linearGradient(listOf(palette.drawerHeaderStart, palette.drawerHeaderEnd)))
                    .statusBarsPadding()
                    .padding(horizontal = 18.dp, vertical = 16.dp),
            ) {
                Column(modifier = Modifier.fillMaxWidth()) {
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.SpaceBetween,
                        verticalAlignment = Alignment.Top,
                    ) {
                        TelegramAvatar(
                            idSeed = activeAccount?.avatarSeed ?: "acc-primary",
                            title = activeAccount?.displayName ?: "Fabushi",
                            size = 62.dp,
                            online = true,
                        )
                        Box(
                            modifier = Modifier
                                .size(40.dp)
                                .clip(CircleShape)
                                .background(Color.White.copy(alpha = 0.14f))
                                .clickable(onClick = onToggleNightMode)
                                .testTag("telegram-night-mode-toggle"),
                            contentAlignment = Alignment.Center,
                        ) {
                            TelegramIcon(
                                glyph = if (palette.isDark) TelegramGlyph.SUN else TelegramGlyph.MOON,
                                tint = Color.White,
                                size = 21.dp,
                            )
                        }
                    }

                    Spacer(Modifier.height(14.dp))

                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .clickable { accountsExpanded = !accountsExpanded }
                            .testTag("telegram-account-expander"),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Column(modifier = Modifier.weight(1f)) {
                            Row(verticalAlignment = Alignment.CenterVertically) {
                                Text(
                                    text = activeAccount?.displayName ?: "全球法布施",
                                    color = Color.White,
                                    fontSize = 16.sp,
                                    fontWeight = FontWeight.SemiBold,
                                    maxLines = 1,
                                    overflow = TextOverflow.Ellipsis,
                                )
                                if (activeAccount?.isPremium == true) {
                                    Text(" ✦", color = Color(0xFFFFD54F), fontSize = 14.sp, fontWeight = FontWeight.Bold)
                                }
                            }
                            Text(
                                text = "${activeAccount?.phone ?: "+86"} · ${activeAccount?.username ?: "@fabushi"}",
                                color = Color.White.copy(alpha = 0.80f),
                                fontSize = 13.sp,
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                            )
                        }
                        Text(
                            text = "▾",
                            color = Color.White,
                            fontSize = 18.sp,
                            modifier = Modifier
                                .padding(start = 8.dp)
                                .rotate(chevronRotation),
                        )
                    }
                }
            }

            // 2. Expandable Multi-Account Switcher (up to 4 accounts)
            AnimatedVisibility(visible = accountsExpanded) {
                Column(
                    modifier = Modifier
                        .fillMaxWidth()
                        .background(palette.elevatedSurface)
                        .padding(vertical = 6.dp),
                ) {
                    state.accounts.forEach { account ->
                        Row(
                            modifier = Modifier
                                .fillMaxWidth()
                                .clickable {
                                    onSwitchAccount(account.id)
                                    onCloseDrawer()
                                }
                                .padding(horizontal = 18.dp, vertical = 9.dp),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            Box {
                                TelegramAvatar(idSeed = account.avatarSeed, title = account.displayName, size = 36.dp)
                                if (account.isActive) {
                                    Box(
                                        modifier = Modifier
                                            .align(Alignment.BottomEnd)
                                            .size(14.dp)
                                            .clip(CircleShape)
                                            .background(Color(0xFF4ECC5E)),
                                        contentAlignment = Alignment.Center,
                                    ) {
                                        TelegramIcon(TelegramGlyph.CHECK_SINGLE, tint = Color.White, size = 9.dp)
                                    }
                                }
                            }
                            Spacer(Modifier.width(14.dp))
                            Column(modifier = Modifier.weight(1f)) {
                                Text(account.displayName, color = palette.primaryText, fontSize = 14.sp, fontWeight = FontWeight.Medium, maxLines = 1)
                                Text(account.username, color = palette.secondaryText, fontSize = 12.sp, maxLines = 1)
                            }
                            if (account.unreadCount > 0 && !account.isActive) {
                                Box(
                                    modifier = Modifier
                                        .clip(RoundedCornerShape(10.dp))
                                        .background(palette.unreadBadgeActive)
                                        .padding(horizontal = 7.dp, vertical = 2.dp),
                                ) {
                                    Text(account.unreadCount.toString(), color = Color.White, fontSize = 11.sp, fontWeight = FontWeight.Bold)
                                }
                            }
                        }
                    }
                    if (state.accounts.size < 4) {
                        Row(
                            modifier = Modifier
                                .fillMaxWidth()
                                .clickable { showAddAccountDialog = true }
                                .padding(horizontal = 18.dp, vertical = 10.dp),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            Box(modifier = Modifier.size(36.dp), contentAlignment = Alignment.Center) {
                                TelegramIcon(TelegramGlyph.PLUS, tint = palette.primaryAccent, size = 20.dp)
                            }
                            Spacer(Modifier.width(14.dp))
                            Text("添加账号 (${state.accounts.size}/4)", color = palette.primaryAccent, fontSize = 14.sp, fontWeight = FontWeight.Medium)
                        }
                    }
                    TelegramHairlineDivider()
                }
            }

            Spacer(Modifier.height(6.dp))

            // 3. Core Telegram Navigation Items
            DrawerMenuItem(
                glyph = TelegramGlyph.CONTACTS,
                title = "我的个人资料",
                onClick = { onCloseDrawer(); onNavigate(TelegramSubScreen.SETTINGS, TelegramSettingsPage.PROFILE_BIO) },
            )
            DrawerMenuItem(
                glyph = TelegramGlyph.BOOKMARK_SAVED,
                title = "收藏夹 (Saved Messages)",
                onClick = { onCloseDrawer(); onOpenSavedMessages() },
            )
            DrawerMenuItem(
                glyph = TelegramGlyph.CONTACTS,
                title = "新建群组",
                onClick = { onCloseDrawer(); onRequestCreateGroup() },
            )
            DrawerMenuItem(
                glyph = TelegramGlyph.LOCK_CLOSED,
                title = "新建加密聊天",
                iconTint = palette.secretGreen,
                onClick = { onCloseDrawer(); onRequestCreateSecretChat() },
            )
            DrawerMenuItem(
                glyph = TelegramGlyph.SEND_PLANE,
                title = "新建频道",
                onClick = { onCloseDrawer(); onRequestCreateChannel() },
            )
            DrawerMenuItem(
                glyph = TelegramGlyph.CONTACTS,
                title = "联系人",
                onClick = { onCloseDrawer(); onNavigate(TelegramSubScreen.CONTACTS, TelegramSettingsPage.OVERVIEW) },
            )
            DrawerMenuItem(
                glyph = TelegramGlyph.PHONE_CALL,
                title = "通话记录",
                badgeText = state.callHistory.size.takeIf { it > 0 }?.toString(),
                onClick = { onCloseDrawer(); onNavigate(TelegramSubScreen.CALLS_HISTORY, TelegramSettingsPage.OVERVIEW) },
            )

            TelegramHairlineDivider(Modifier.padding(vertical = 6.dp))

            // 4. GPUI-Telegram & Fabushi & Mika Special Workspaces
            DrawerMenuItem(
                glyph = TelegramGlyph.DHARMA_WHEEL,
                title = "小程序 · 全球法布施",
                iconTint = palette.goldMerit,
                badgeText = "240国",
                badgeColor = palette.goldMerit,
                onClick = { onCloseDrawer(); onNavigate(TelegramSubScreen.GLOBAL_DHARMA, TelegramSettingsPage.OVERVIEW) },
            )
            DrawerMenuItem(
                glyph = TelegramGlyph.MIKA_PHONE,
                title = "Mika 手机操控中心",
                iconTint = Color(0xFF00BFA5),
                badgeText = when {
                    state.mikaSnapshot.activeTask != null -> "执行中"
                    state.mikaSnapshot.accessibilityEnabled || state.mikaSnapshot.helperBinderAlive -> "已就绪"
                    else -> "操控"
                },
                badgeColor = Color(0xFF00BFA5),
                testTag = "telegram-drawer-mika-entry",
                onClick = { onCloseDrawer(); onNavigate(TelegramSubScreen.MIKA_WORKSPACE, TelegramSettingsPage.OVERVIEW) },
            )
            DrawerMenuItem(
                glyph = TelegramGlyph.TON_DIAMOND,
                title = "TON 钱包与 Stars",
                iconTint = Color(0xFF2AABEE),
                badgeText = "${"%.1f".format(state.tonWallet.tonBalance)} TON",
                onClick = { onCloseDrawer(); onNavigate(TelegramSubScreen.TON_WALLET, TelegramSettingsPage.OVERVIEW) },
            )
            DrawerMenuItem(
                glyph = TelegramGlyph.BOT_ROBOT,
                title = "@BotFather 与 AI 机器人",
                onClick = { onCloseDrawer(); onNavigate(TelegramSubScreen.BOTFATHER, TelegramSettingsPage.OVERVIEW) },
            )
            DrawerMenuItem(
                glyph = TelegramGlyph.HERMES_SPARK,
                title = "Hermes AI 工作室",
                badgeText = "6-Tab",
                onClick = { onCloseDrawer(); onNavigate(TelegramSubScreen.HERMES_STUDIO, TelegramSettingsPage.OVERVIEW) },
            )
            DrawerMenuItem(
                glyph = TelegramGlyph.MAGPIE_GATEWAY,
                title = "Magpie 免订阅 AI 网关",
                badgeText = "48+",
                onClick = { onCloseDrawer(); onNavigate(TelegramSubScreen.MAGPIE_GATEWAY, TelegramSettingsPage.OVERVIEW) },
            )
            DrawerMenuItem(
                glyph = TelegramGlyph.CLOUD_COMPUTER,
                title = "云电脑与 Action Bus",
                onClick = { onCloseDrawer(); onNavigate(TelegramSubScreen.CLOUD_COMPUTER, TelegramSettingsPage.OVERVIEW) },
            )

            TelegramHairlineDivider(Modifier.padding(vertical = 6.dp))

            DrawerMenuItem(
                glyph = TelegramGlyph.SETTINGS_GEAR,
                title = "设置 (11 项全功能配置)",
                onClick = { onCloseDrawer(); onNavigate(TelegramSubScreen.SETTINGS, TelegramSettingsPage.OVERVIEW) },
            )
            DrawerMenuItem(
                glyph = TelegramGlyph.HERMES_SPARK,
                title = "切换至 Grok 极简伴侣模式",
                onClick = { onCloseDrawer(); onSwitchToGrokShell() },
            )
            DrawerMenuItem(
                glyph = TelegramGlyph.MINI_APP_GRID,
                title = "经典 Fabushi 市场与宿主面板",
                testTag = "telegram-drawer-classic-shell",
                onClick = { onCloseDrawer(); onSwitchToClassicFabushi() },
            )

            Spacer(Modifier.height(14.dp))
            Text(
                text = "GPUI-Telegram × Mika Android v2.0.0\nSQLite WAL · Signal E2EE · 16KB Aligned",
                color = palette.hintText,
                fontSize = 11.sp,
                lineHeight = 16.sp,
                modifier = Modifier.padding(horizontal = 20.dp, vertical = 8.dp),
            )
        }
    }

    if (showAddAccountDialog) {
        var name by remember { mutableStateOf("") }
        var phone by remember { mutableStateOf("+86 ") }
        var username by remember { mutableStateOf("@") }
        AlertDialog(
            onDismissRequest = { showAddAccountDialog = false },
            title = { Text("添加 Telegram / Fabushi 账号") },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    OutlinedTextField(value = name, onValueChange = { name = it }, label = { Text("显示昵称") }, singleLine = true)
                    OutlinedTextField(value = phone, onValueChange = { phone = it }, label = { Text("手机号") }, singleLine = true)
                    OutlinedTextField(value = username, onValueChange = { username = it }, label = { Text("@用户名") }, singleLine = true)
                }
            },
            confirmButton = {
                Button(onClick = {
                    onAddAccount(name, phone, username)
                    showAddAccountDialog = false
                }) { Text("登录并切换") }
            },
            dismissButton = {
                TextButton(onClick = { showAddAccountDialog = false }) { Text("取消") }
            },
        )
    }
}

@Composable
private fun DrawerMenuItem(
    glyph: TelegramGlyph,
    title: String,
    iconTint: Color? = null,
    badgeText: String? = null,
    badgeColor: Color? = null,
    testTag: String? = null,
    onClick: () -> Unit,
) {
    val palette = LocalTelegramPalette.current
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .let { if (testTag != null) it.testTag(testTag) else it }
            .clickable(onClick = onClick)
            .padding(horizontal = 20.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        TelegramIcon(
            glyph = glyph,
            tint = iconTint ?: palette.secondaryText,
            size = 22.dp,
        )
        Spacer(Modifier.width(22.dp))
        Text(
            text = title,
            color = palette.primaryText,
            fontSize = 15.sp,
            fontWeight = FontWeight.Medium,
            modifier = Modifier.weight(1f),
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        if (!badgeText.isNullOrBlank()) {
            Box(
                modifier = Modifier
                    .clip(RoundedCornerShape(10.dp))
                    .background((badgeColor ?: palette.primaryAccent).copy(alpha = 0.16f))
                    .padding(horizontal = 8.dp, vertical = 2.dp),
            ) {
                Text(
                    text = badgeText,
                    color = badgeColor ?: palette.primaryAccent,
                    fontSize = 11.sp,
                    fontWeight = FontWeight.SemiBold,
                )
            }
        }
    }
}

@Composable
fun CreateConversationOrSecretDialog(
    kind: ConversationKind?,
    onDismiss: () -> Unit,
    onCreateGroupOrChannel: (ConversationKind, String, String, Boolean) -> Unit,
    onCreateSecretChat: (String) -> Unit,
) {
    if (kind == null) return
    var title by remember(kind) { mutableStateOf("") }
    var desc by remember(kind) { mutableStateOf("") }
    var forumTopicsEnabled by remember(kind) { mutableStateOf(kind == ConversationKind.GROUP) }

    AlertDialog(
        onDismissRequest = onDismiss,
        title = {
            Text(
                when (kind) {
                    ConversationKind.SECRET -> "🔒 新建端到端加密聊天"
                    ConversationKind.CHANNEL -> "📢 新建广播频道"
                    else -> "👥 新建群组"
                },
            )
        },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                OutlinedTextField(
                    value = title,
                    onValueChange = { title = it },
                    label = { Text(if (kind == ConversationKind.SECRET) "对方昵称或 @用户名" else "名称") },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth(),
                )
                if (kind != ConversationKind.SECRET) {
                    OutlinedTextField(
                        value = desc,
                        onValueChange = { desc = it },
                        label = { Text("简介说明（可选）") },
                        modifier = Modifier.fillMaxWidth(),
                    )
                } else {
                    Text(
                        "采用 X3DH (X25519 + Ed25519) 密钥协商与 Double Ratchet 双棘轮加密，支持 4-Emoji 安全指纹核验与阅后即焚。",
                        fontSize = 13.sp,
                    )
                }
                if (kind == ConversationKind.GROUP) {
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .clickable { forumTopicsEnabled = !forumTopicsEnabled }
                            .padding(vertical = 4.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Text(if (forumTopicsEnabled) "☑️" else "⬜", fontSize = 18.sp)
                        Spacer(Modifier.width(8.dp))
                        Text("启用超级群组多话题模式 (Forum Topics)", fontSize = 14.sp)
                    }
                }
            }
        },
        confirmButton = {
            Button(
                onClick = {
                    if (kind == ConversationKind.SECRET) {
                        onCreateSecretChat(title.ifBlank { "加密伙伴" })
                    } else {
                        onCreateGroupOrChannel(kind, title.ifBlank { "新建${kind.label}" }, desc, forumTopicsEnabled)
                    }
                    onDismiss()
                },
            ) { Text("创建") }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) { Text("取消") }
        },
    )
}
