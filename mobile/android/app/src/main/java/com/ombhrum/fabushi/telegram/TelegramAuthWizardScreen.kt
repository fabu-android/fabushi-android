package com.ombhrum.fabushi.telegram

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.net.Uri
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.delay

/**
 * Desktop v1.6.9 aligned onboarding & login wizard (`R4 / AC-4`):
 * - Top bar Day/Night + 中文/EN quick switches
 * - Server endpoint pill with status indicator + inline Edit & Reset Default
 * - 3 Desktop tabs: Password (with inline Sign In <-> Sign Up toggle), Alipay QR (auto-start & auto-poll), Email Register (60s resend cooldown)
 * - Dedicated Two-Step Verification (2FA cloud password) step when `requires_2fa` is returned
 */
@Composable
fun TelegramAuthWizardScreen(
    serverEndpoint: String,
    isAuthenticating: Boolean,
    authError: String?,
    alipayLoginUrl: String?,
    alipayStatusText: String?,
    emailCodeSentNotice: String?,
    hasPendingTwoFactor: Boolean = false,
    pendingTwoFactorHint: String? = null,
    onUpdateServerEndpoint: (String) -> Unit,
    onPasswordLogin: (username: String, password: String) -> Unit,
    onPasswordRegister: (username: String, password: String, firstName: String) -> Unit,
    onSubmitTwoFactor: (cloudPassword: String) -> Unit = {},
    onCancelTwoFactor: () -> Unit = {},
    onSendEmailCode: (email: String) -> Unit,
    onEmailRegister: (email: String, code: String, username: String, password: String, firstName: String) -> Unit,
    onStartAlipayLogin: () -> Unit,
    onPollAlipayLogin: () -> Unit,
    onToggleTheme: () -> Unit = {},
    onToggleLanguage: () -> Unit = {},
    onEnterOfflineLocalMode: () -> Unit,
) {
    val palette = LocalTelegramPalette.current
    val strings = LocalAppleUiStrings.current
    val context = LocalContext.current

    // Desktop v1.6.9 3-tab structure: 0 = Password (Sign In / Sign Up), 1 = Alipay QR, 2 = Email Register
    var selectedTab by remember { mutableIntStateOf(0) }
    var passwordRegisterMode by remember { mutableStateOf(false) }
    var editingServer by remember { mutableStateOf(false) }
    var serverDraft by remember(serverEndpoint) { mutableStateOf(serverEndpoint) }
    var username by remember { mutableStateOf("") }
    var password by remember { mutableStateOf("") }
    var firstName by remember { mutableStateOf("") }
    var email by remember { mutableStateOf("") }
    var emailCode by remember { mutableStateOf("") }
    var emailResendCooldown by remember { mutableIntStateOf(0) }
    var twoFactorPassword by remember { mutableStateOf("") }
    var copiedAlipayLink by remember { mutableStateOf(false) }

    // 60s email resend cooldown timer
    LaunchedEffect(emailResendCooldown) {
        if (emailResendCooldown > 0) {
            delay(1000L)
            emailResendCooldown -= 1
        }
    }

    // Auto-start Alipay QR flow on entering the Alipay tab (matching Desktop)
    LaunchedEffect(selectedTab) {
        if (selectedTab == 1 && alipayLoginUrl.isNullOrBlank() && !isAuthenticating) {
            onStartAlipayLogin()
        }
    }

    val inputColors = OutlinedTextFieldDefaults.colors(
        focusedTextColor = palette.primaryText,
        unfocusedTextColor = palette.primaryText,
        cursorColor = palette.searchCursorColor,
        focusedBorderColor = palette.primaryAccent,
        unfocusedBorderColor = palette.borderColor,
        focusedContainerColor = palette.inputBg,
        unfocusedContainerColor = palette.inputBg,
        focusedLabelColor = palette.primaryAccent,
        unfocusedLabelColor = palette.secondaryText,
    )

    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(palette.groupedBackground)
            .statusBarsPadding()
            .navigationBarsPadding()
            .imePadding()
            .testTag("telegram-auth-wizard"),
    ) {
        // Desktop-aligned top utility bar: Theme toggle + Language toggle
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 16.dp, vertical = 10.dp),
            horizontalArrangement = Arrangement.End,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Surface(
                shape = RoundedCornerShape(16.dp),
                color = palette.modalBg,
                modifier = Modifier
                    .border(0.5.dp, palette.borderColor, RoundedCornerShape(16.dp))
                    .clip(RoundedCornerShape(16.dp))
                    .clickable(onClick = onToggleTheme)
                    .testTag("auth-toggle-theme"),
            ) {
                Text(
                    text = if (palette.isDark) {
                        if (strings.isEnglish) "Light" else "日间"
                    } else {
                        if (strings.isEnglish) "Dark" else "深色"
                    },
                    color = palette.primaryText,
                    fontSize = 12.sp,
                    fontWeight = FontWeight.Medium,
                    modifier = Modifier.padding(horizontal = 12.dp, vertical = 6.dp),
                )
            }
            Spacer(Modifier.width(8.dp))
            Surface(
                shape = RoundedCornerShape(16.dp),
                color = palette.modalBg,
                modifier = Modifier
                    .border(0.5.dp, palette.borderColor, RoundedCornerShape(16.dp))
                    .clip(RoundedCornerShape(16.dp))
                    .clickable(onClick = onToggleLanguage)
                    .testTag("auth-toggle-language"),
            ) {
                Text(
                    text = if (strings.isEnglish) "中文" else "EN",
                    color = palette.primaryText,
                    fontSize = 12.sp,
                    fontWeight = FontWeight.Medium,
                    modifier = Modifier.padding(horizontal = 12.dp, vertical = 6.dp),
                )
            }
        }

        Box(
            modifier = Modifier
                .weight(1f)
                .fillMaxWidth(),
            contentAlignment = Alignment.Center,
        ) {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .verticalScroll(rememberScrollState())
                    .padding(horizontal = 20.dp, vertical = 10.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                // Brand emblem matching Desktop v1.6.9
                Box(
                    modifier = Modifier
                        .size(64.dp)
                        .clip(CircleShape)
                        .background(
                            Brush.verticalGradient(
                                listOf(palette.outgoingBubbleTop, palette.outgoingBubbleBottom),
                            ),
                        ),
                    contentAlignment = Alignment.Center,
                ) {
                    TelegramIcon(TelegramGlyph.CHAT_BUBBLE, tint = Color.White, size = 30.dp)
                }
                Spacer(Modifier.height(12.dp))
                Text(
                    text = "Fabushi",
                    color = palette.primaryText,
                    fontSize = 26.sp,
                    fontWeight = FontWeight.Bold,
                )
                Spacer(Modifier.height(4.dp))
                Text(
                    text = strings.authSubtitle,
                    color = palette.secondaryText,
                    fontSize = 13.sp,
                    textAlign = TextAlign.Center,
                )

                Spacer(Modifier.height(14.dp))

                // Server Endpoint Pill with status dot
                Surface(
                    shape = RoundedCornerShape(12.dp),
                    color = palette.modalBg,
                    modifier = Modifier
                        .fillMaxWidth()
                        .border(0.5.dp, palette.borderColor, RoundedCornerShape(12.dp))
                        .clip(RoundedCornerShape(12.dp))
                        .clickable { editingServer = !editingServer },
                ) {
                    Row(
                        modifier = Modifier.padding(horizontal = 12.dp, vertical = 10.dp),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.SpaceBetween,
                    ) {
                        Row(
                            verticalAlignment = Alignment.CenterVertically,
                            modifier = Modifier.weight(1f),
                        ) {
                            Box(
                                modifier = Modifier
                                    .size(8.dp)
                                    .clip(CircleShape)
                                    .background(palette.secretGreen),
                            )
                            Spacer(Modifier.width(8.dp))
                            Text(
                                text = serverEndpoint,
                                color = palette.secondaryText,
                                fontSize = 12.sp,
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                            )
                        }
                        Spacer(Modifier.width(8.dp))
                        Text(
                            text = if (editingServer) strings.done else if (strings.isEnglish) "Edit" else "修改",
                            color = palette.topBarActionColor,
                            fontSize = 12.sp,
                            fontWeight = FontWeight.SemiBold,
                        )
                    }
                }

                if (editingServer) {
                    Spacer(Modifier.height(8.dp))
                    Surface(
                        shape = RoundedCornerShape(14.dp),
                        color = palette.modalBg,
                        modifier = Modifier
                            .fillMaxWidth()
                            .border(0.5.dp, palette.borderColor, RoundedCornerShape(14.dp)),
                    ) {
                        Column(
                            modifier = Modifier.padding(12.dp),
                            verticalArrangement = Arrangement.spacedBy(8.dp),
                        ) {
                            OutlinedTextField(
                                value = serverDraft,
                                onValueChange = { serverDraft = it },
                                label = { Text(if (strings.isEnglish) "Server Endpoint URL" else "服务器地址") },
                                singleLine = true,
                                shape = RoundedCornerShape(12.dp),
                                colors = inputColors,
                                modifier = Modifier.fillMaxWidth(),
                            )
                            Row(
                                modifier = Modifier.fillMaxWidth(),
                                horizontalArrangement = Arrangement.spacedBy(8.dp),
                            ) {
                                OutlinedButton(
                                    onClick = {
                                        serverDraft = DEFAULT_DESKTOP_SERVER_URL
                                        onUpdateServerEndpoint(DEFAULT_DESKTOP_SERVER_URL)
                                        editingServer = false
                                    },
                                    shape = RoundedCornerShape(10.dp),
                                    modifier = Modifier.weight(1f),
                                ) {
                                    Text(strings.resetDefaultServer, color = palette.primaryAccent, fontSize = 13.sp)
                                }
                                Button(
                                    onClick = {
                                        val clean = serverDraft.trim().ifBlank { DEFAULT_DESKTOP_SERVER_URL }
                                        onUpdateServerEndpoint(clean)
                                        editingServer = false
                                    },
                                    colors = ButtonDefaults.buttonColors(
                                        containerColor = palette.primaryAccent,
                                        contentColor = Color.White,
                                    ),
                                    shape = RoundedCornerShape(10.dp),
                                    modifier = Modifier.weight(1f),
                                ) {
                                    Text(strings.save, fontSize = 13.sp)
                                }
                            }
                        }
                    }
                }

                Spacer(Modifier.height(14.dp))

                // Main Auth Card
                Surface(
                    shape = RoundedCornerShape(18.dp),
                    color = palette.modalBg,
                    shadowElevation = if (palette.isDark) 0.dp else 2.dp,
                    modifier = Modifier
                        .fillMaxWidth()
                        .border(0.5.dp, palette.borderColor, RoundedCornerShape(18.dp)),
                ) {
                    Column(
                        modifier = Modifier.padding(16.dp),
                        verticalArrangement = Arrangement.spacedBy(12.dp),
                    ) {
                        if (hasPendingTwoFactor) {
                            // Dedicated 2FA Cloud Password Step
                            Text(
                                text = strings.authTwoFactorTitle,
                                color = palette.primaryText,
                                fontSize = 18.sp,
                                fontWeight = FontWeight.Bold,
                            )
                            Text(
                                text = strings.authTwoFactorSubtitle,
                                color = palette.secondaryText,
                                fontSize = 13.sp,
                            )
                            if (!pendingTwoFactorHint.isNullOrBlank()) {
                                Surface(
                                    shape = RoundedCornerShape(10.dp),
                                    color = palette.inputBg,
                                    modifier = Modifier.fillMaxWidth(),
                                ) {
                                    Text(
                                        text = if (strings.isEnglish) {
                                            "Password hint: $pendingTwoFactorHint"
                                        } else {
                                            "密码提示：$pendingTwoFactorHint"
                                        },
                                        color = palette.secondaryText,
                                        fontSize = 12.sp,
                                        modifier = Modifier.padding(horizontal = 12.dp, vertical = 8.dp),
                                    )
                                }
                            }
                            if (!authError.isNullOrBlank()) {
                                AuthErrorBanner(authError, palette)
                            }
                            OutlinedTextField(
                                value = twoFactorPassword,
                                onValueChange = { twoFactorPassword = it },
                                label = { Text(if (strings.isEnglish) "Cloud Password" else "两步验证云密码") },
                                singleLine = true,
                                visualTransformation = PasswordVisualTransformation(),
                                shape = RoundedCornerShape(12.dp),
                                colors = inputColors,
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .testTag("auth-2fa-password-input"),
                            )
                            Button(
                                enabled = !isAuthenticating && twoFactorPassword.isNotBlank(),
                                onClick = { onSubmitTwoFactor(twoFactorPassword) },
                                colors = ButtonDefaults.buttonColors(
                                    containerColor = palette.primaryAccent,
                                    contentColor = Color.White,
                                ),
                                shape = RoundedCornerShape(12.dp),
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .height(48.dp)
                                    .testTag("auth-submit-2fa"),
                            ) {
                                if (isAuthenticating) {
                                    CircularProgressIndicator(
                                        modifier = Modifier.size(18.dp),
                                        color = Color.White,
                                        strokeWidth = 2.dp,
                                    )
                                } else {
                                    Text(
                                        if (strings.isEnglish) "Verify & Sign In" else "验证并登录",
                                        fontSize = 15.sp,
                                        fontWeight = FontWeight.SemiBold,
                                    )
                                }
                            }
                            TextButton(
                                onClick = {
                                    twoFactorPassword = ""
                                    onCancelTwoFactor()
                                },
                                modifier = Modifier.align(Alignment.CenterHorizontally),
                            ) {
                                Text(
                                    if (strings.isEnglish) "Back to Sign In" else "返回账号登录",
                                    color = palette.secondaryText,
                                    fontSize = 13.sp,
                                )
                            }
                        } else {
                            // Desktop v1.6.9 3-Tab Segmented Switcher
                            val tabs = if (strings.isEnglish) {
                                listOf("Password", "Alipay", "Email")
                            } else {
                                listOf("密码登录", "支付宝扫码", "邮箱注册")
                            }
                            Surface(
                                shape = RoundedCornerShape(10.dp),
                                color = palette.inputBg,
                                modifier = Modifier.fillMaxWidth(),
                            ) {
                                Row(
                                    modifier = Modifier
                                        .fillMaxWidth()
                                        .padding(3.dp),
                                    horizontalArrangement = Arrangement.spacedBy(2.dp),
                                ) {
                                    tabs.forEachIndexed { idx, label ->
                                        val selected = selectedTab == idx
                                        Surface(
                                            shape = RoundedCornerShape(8.dp),
                                            color = if (selected) palette.modalBg else Color.Transparent,
                                            shadowElevation = if (selected && !palette.isDark) 1.dp else 0.dp,
                                            modifier = Modifier
                                                .weight(1f)
                                                .clip(RoundedCornerShape(8.dp))
                                                .clickable { selectedTab = idx }
                                                .testTag("auth-tab-$idx"),
                                        ) {
                                            Text(
                                                text = label,
                                                color = if (selected) palette.primaryText else palette.secondaryText,
                                                fontSize = 12.sp,
                                                fontWeight = if (selected) FontWeight.SemiBold else FontWeight.Normal,
                                                textAlign = TextAlign.Center,
                                                maxLines = 1,
                                                modifier = Modifier.padding(vertical = 8.dp),
                                            )
                                        }
                                    }
                                }
                            }

                            if (!authError.isNullOrBlank()) {
                                AuthErrorBanner(authError, palette)
                            }

                            when (selectedTab) {
                                0 -> {
                                    if (passwordRegisterMode) {
                                        OutlinedTextField(
                                            value = firstName,
                                            onValueChange = { firstName = it },
                                            label = { Text(if (strings.isEnglish) "Display Name" else "昵称") },
                                            singleLine = true,
                                            shape = RoundedCornerShape(12.dp),
                                            colors = inputColors,
                                            modifier = Modifier.fillMaxWidth(),
                                        )
                                    }
                                    OutlinedTextField(
                                        value = username,
                                        onValueChange = { username = it },
                                        label = {
                                            Text(
                                                if (passwordRegisterMode) {
                                                    if (strings.isEnglish) "Username" else "用户名"
                                                } else {
                                                    strings.authIdentifierLabel
                                                },
                                            )
                                        },
                                        singleLine = true,
                                        shape = RoundedCornerShape(12.dp),
                                        colors = inputColors,
                                        modifier = Modifier
                                            .fillMaxWidth()
                                            .testTag("auth-username-input"),
                                    )
                                    OutlinedTextField(
                                        value = password,
                                        onValueChange = { password = it },
                                        label = { Text(if (strings.isEnglish) "Password" else "密码") },
                                        singleLine = true,
                                        visualTransformation = PasswordVisualTransformation(),
                                        shape = RoundedCornerShape(12.dp),
                                        colors = inputColors,
                                        modifier = Modifier
                                            .fillMaxWidth()
                                            .testTag("auth-password-input"),
                                    )
                                    Button(
                                        enabled = !isAuthenticating && username.isNotBlank() && password.isNotBlank(),
                                        onClick = {
                                            if (passwordRegisterMode) {
                                                onPasswordRegister(
                                                    username.trim(),
                                                    password,
                                                    firstName.trim().ifBlank { username.trim() },
                                                )
                                            } else {
                                                onPasswordLogin(username.trim(), password)
                                            }
                                        },
                                        colors = ButtonDefaults.buttonColors(
                                            containerColor = palette.primaryAccent,
                                            contentColor = Color.White,
                                        ),
                                        shape = RoundedCornerShape(12.dp),
                                        modifier = Modifier
                                            .fillMaxWidth()
                                            .height(48.dp)
                                            .testTag("auth-submit-login"),
                                    ) {
                                        if (isAuthenticating) {
                                            CircularProgressIndicator(
                                                modifier = Modifier.size(18.dp),
                                                color = Color.White,
                                                strokeWidth = 2.dp,
                                            )
                                        } else {
                                            Text(
                                                if (passwordRegisterMode) strings.authSignUpAction else strings.signIn,
                                                fontSize = 15.sp,
                                                fontWeight = FontWeight.SemiBold,
                                            )
                                        }
                                    }
                                    TextButton(
                                        onClick = { passwordRegisterMode = !passwordRegisterMode },
                                        modifier = Modifier
                                            .align(Alignment.CenterHorizontally)
                                            .testTag("auth-toggle-register-mode"),
                                    ) {
                                        Text(
                                            text = if (passwordRegisterMode) {
                                                strings.authSwitchToSignIn
                                            } else {
                                                strings.authSwitchToSignUp
                                            },
                                            color = palette.primaryAccent,
                                            fontSize = 13.sp,
                                        )
                                    }
                                }

                                1 -> {
                                    Column(
                                        modifier = Modifier.fillMaxWidth(),
                                        horizontalAlignment = Alignment.CenterHorizontally,
                                        verticalArrangement = Arrangement.spacedBy(10.dp),
                                    ) {
                                        Text(
                                            text = if (strings.isEnglish) {
                                                "Authorize with Alipay on this device or scan the QR code below."
                                            } else {
                                                "点击下方链接跳转支付宝授权登录，或使用另一台设备扫码完成验证。"
                                            },
                                            color = palette.secondaryText,
                                            fontSize = 13.sp,
                                            textAlign = TextAlign.Center,
                                        )

                                        if (!alipayLoginUrl.isNullOrBlank()) {
                                            QrCodeCanvas(
                                                payload = alipayLoginUrl,
                                                size = 184.dp,
                                                modifier = Modifier.testTag("auth-alipay-qr"),
                                            )
                                            Surface(
                                                shape = RoundedCornerShape(12.dp),
                                                color = palette.inputBg,
                                                modifier = Modifier
                                                    .fillMaxWidth()
                                                    .clip(RoundedCornerShape(12.dp))
                                                    .clickable { openAlipayUrl(context, alipayLoginUrl) }
                                                    .testTag("auth-alipay-url-card"),
                                            ) {
                                                Column(
                                                    modifier = Modifier.padding(12.dp),
                                                    verticalArrangement = Arrangement.spacedBy(4.dp),
                                                ) {
                                                    Text(
                                                        text = strings.openInAlipay,
                                                        color = palette.primaryAccent,
                                                        fontSize = 13.sp,
                                                        fontWeight = FontWeight.SemiBold,
                                                    )
                                                    Text(
                                                        text = alipayLoginUrl,
                                                        color = palette.secondaryText,
                                                        fontSize = 11.sp,
                                                        maxLines = 2,
                                                        overflow = TextOverflow.Ellipsis,
                                                    )
                                                }
                                            }
                                            Row(
                                                modifier = Modifier.fillMaxWidth(),
                                                horizontalArrangement = Arrangement.spacedBy(8.dp),
                                            ) {
                                                Button(
                                                    onClick = { openAlipayUrl(context, alipayLoginUrl) },
                                                    colors = ButtonDefaults.buttonColors(
                                                        containerColor = palette.primaryAccent,
                                                        contentColor = Color.White,
                                                    ),
                                                    shape = RoundedCornerShape(10.dp),
                                                    modifier = Modifier.weight(1f),
                                                ) {
                                                    Text(strings.openInAlipay, fontSize = 13.sp, maxLines = 1)
                                                }
                                                OutlinedButton(
                                                    onClick = {
                                                        val cm = context.getSystemService(Context.CLIPBOARD_SERVICE) as? ClipboardManager
                                                        cm?.setPrimaryClip(ClipData.newPlainText("alipay_login", alipayLoginUrl))
                                                        copiedAlipayLink = true
                                                    },
                                                    shape = RoundedCornerShape(10.dp),
                                                ) {
                                                    Text(
                                                        if (copiedAlipayLink) strings.copied else strings.copy,
                                                        color = palette.primaryAccent,
                                                        fontSize = 13.sp,
                                                    )
                                                }
                                            }
                                        }

                                        if (!alipayStatusText.isNullOrBlank()) {
                                            Text(
                                                text = alipayStatusText,
                                                color = palette.secondaryText,
                                                fontSize = 12.sp,
                                                textAlign = TextAlign.Center,
                                            )
                                        }

                                        Row(
                                            modifier = Modifier.fillMaxWidth(),
                                            horizontalArrangement = Arrangement.spacedBy(8.dp),
                                        ) {
                                            Button(
                                                enabled = !isAuthenticating,
                                                onClick = onStartAlipayLogin,
                                                colors = ButtonDefaults.buttonColors(
                                                    containerColor = palette.primaryAccent,
                                                    contentColor = Color.White,
                                                ),
                                                shape = RoundedCornerShape(12.dp),
                                                modifier = Modifier.weight(1f),
                                            ) {
                                                Text(
                                                    if (alipayLoginUrl.isNullOrBlank()) {
                                                        if (strings.isEnglish) "Start Alipay" else "发起支付宝登录"
                                                    } else {
                                                        if (strings.isEnglish) "Refresh QR" else "刷新二维码"
                                                    },
                                                    fontSize = 13.sp,
                                                )
                                            }
                                            if (!alipayLoginUrl.isNullOrBlank()) {
                                                OutlinedButton(
                                                    enabled = !isAuthenticating,
                                                    onClick = onPollAlipayLogin,
                                                    shape = RoundedCornerShape(12.dp),
                                                    modifier = Modifier.weight(1f),
                                                ) {
                                                    Text(
                                                        if (strings.isEnglish) "Check Now" else "立即检查",
                                                        color = palette.primaryAccent,
                                                        fontSize = 13.sp,
                                                    )
                                                }
                                            }
                                        }
                                    }
                                }

                                else -> {
                                    OutlinedTextField(
                                        value = email,
                                        onValueChange = { email = it },
                                        label = { Text(if (strings.isEnglish) "Email Address" else "电子邮箱") },
                                        singleLine = true,
                                        shape = RoundedCornerShape(12.dp),
                                        colors = inputColors,
                                        modifier = Modifier.fillMaxWidth(),
                                    )
                                    Row(
                                        modifier = Modifier.fillMaxWidth(),
                                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                                        verticalAlignment = Alignment.CenterVertically,
                                    ) {
                                        OutlinedTextField(
                                            value = emailCode,
                                            onValueChange = { emailCode = it },
                                            label = { Text(if (strings.isEnglish) "Code" else "验证码") },
                                            singleLine = true,
                                            shape = RoundedCornerShape(12.dp),
                                            colors = inputColors,
                                            modifier = Modifier.weight(1f),
                                        )
                                        OutlinedButton(
                                            enabled = !isAuthenticating && email.isNotBlank() && emailResendCooldown == 0,
                                            onClick = {
                                                emailResendCooldown = 60
                                                onSendEmailCode(email.trim())
                                            },
                                            shape = RoundedCornerShape(12.dp),
                                        ) {
                                            Text(
                                                if (emailResendCooldown > 0) {
                                                    "${emailResendCooldown}s"
                                                } else if (strings.isEnglish) {
                                                    "Send Code"
                                                } else {
                                                    "发送验证码"
                                                },
                                                color = palette.primaryAccent,
                                                fontSize = 13.sp,
                                            )
                                        }
                                    }
                                    if (!emailCodeSentNotice.isNullOrBlank()) {
                                        Text(emailCodeSentNotice, color = palette.statusGreenText, fontSize = 12.sp)
                                    }
                                    OutlinedTextField(
                                        value = firstName,
                                        onValueChange = { firstName = it },
                                        label = { Text(if (strings.isEnglish) "Display Name (optional)" else "昵称（可选）") },
                                        singleLine = true,
                                        shape = RoundedCornerShape(12.dp),
                                        colors = inputColors,
                                        modifier = Modifier.fillMaxWidth(),
                                    )
                                    OutlinedTextField(
                                        value = username,
                                        onValueChange = { username = it },
                                        label = { Text(if (strings.isEnglish) "Username" else "用户名") },
                                        singleLine = true,
                                        shape = RoundedCornerShape(12.dp),
                                        colors = inputColors,
                                        modifier = Modifier.fillMaxWidth(),
                                    )
                                    OutlinedTextField(
                                        value = password,
                                        onValueChange = { password = it },
                                        label = { Text(if (strings.isEnglish) "Password (min 8 chars)" else "设置密码（至少 8 位）") },
                                        singleLine = true,
                                        visualTransformation = PasswordVisualTransformation(),
                                        shape = RoundedCornerShape(12.dp),
                                        colors = inputColors,
                                        modifier = Modifier.fillMaxWidth(),
                                    )
                                    Button(
                                        enabled = !isAuthenticating && email.isNotBlank() && emailCode.isNotBlank() && username.isNotBlank() && password.isNotBlank(),
                                        onClick = {
                                            onEmailRegister(
                                                email.trim(),
                                                emailCode.trim(),
                                                username.trim(),
                                                password,
                                                firstName.trim().ifBlank { username.trim() },
                                            )
                                        },
                                        colors = ButtonDefaults.buttonColors(
                                            containerColor = palette.primaryAccent,
                                            contentColor = Color.White,
                                        ),
                                        shape = RoundedCornerShape(12.dp),
                                        modifier = Modifier
                                            .fillMaxWidth()
                                            .height(48.dp),
                                    ) {
                                        if (isAuthenticating) {
                                            CircularProgressIndicator(
                                                modifier = Modifier.size(18.dp),
                                                color = Color.White,
                                                strokeWidth = 2.dp,
                                            )
                                        } else {
                                            Text(
                                                if (strings.isEnglish) "Verify & Create Account" else "验证邮箱并注册登录",
                                                fontSize = 15.sp,
                                                fontWeight = FontWeight.SemiBold,
                                            )
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                Spacer(Modifier.height(12.dp))

                TextButton(
                    onClick = onEnterOfflineLocalMode,
                    modifier = Modifier.testTag("auth-enter-offline-mode"),
                ) {
                    Text(
                        text = if (strings.isEnglish) "Continue in Offline Local Mode" else "进入离线本地模式",
                        color = palette.secondaryText,
                        fontSize = 13.sp,
                    )
                }
            }
        }
    }
}

@Composable
private fun AuthErrorBanner(authError: String, palette: TelegramPalette) {
    Surface(
        shape = RoundedCornerShape(10.dp),
        color = palette.destructiveRed.copy(alpha = 0.12f),
        modifier = Modifier.fillMaxWidth(),
    ) {
        Text(
            text = authError,
            color = palette.destructiveRed,
            fontSize = 12.sp,
            modifier = Modifier.padding(horizontal = 12.dp, vertical = 9.dp),
        )
    }
}

private fun openAlipayUrl(context: Context, url: String) {
    runCatching {
        val intent = Intent(Intent.ACTION_VIEW, Uri.parse(url)).apply {
            addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        }
        context.startActivity(intent)
    }
}
