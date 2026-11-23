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

/**
 * Apple iOS styled onboarding & login wizard connected to real `gpui_telegram_server` `/api/v1/auth`
 * endpoints with clickable Alipay URL + scannable ISO-18004 QR code.
 */
@Composable
fun TelegramAuthWizardScreen(
    serverEndpoint: String,
    isAuthenticating: Boolean,
    authError: String?,
    alipayLoginUrl: String?,
    alipayStatusText: String?,
    emailCodeSentNotice: String?,
    onUpdateServerEndpoint: (String) -> Unit,
    onPasswordLogin: (username: String, password: String) -> Unit,
    onPasswordRegister: (username: String, password: String, firstName: String) -> Unit,
    onSendEmailCode: (email: String) -> Unit,
    onEmailRegister: (email: String, code: String, username: String, password: String, firstName: String) -> Unit,
    onStartAlipayLogin: () -> Unit,
    onPollAlipayLogin: () -> Unit,
    onEnterOfflineLocalMode: () -> Unit,
) {
    val palette = LocalTelegramPalette.current
    val strings = LocalAppleUiStrings.current
    val context = LocalContext.current

    var selectedTab by remember { mutableIntStateOf(0) } // 0=Login, 1=Register, 2=Email, 3=Alipay
    var editingServer by remember { mutableStateOf(false) }
    var serverDraft by remember(serverEndpoint) { mutableStateOf(serverEndpoint) }
    var username by remember { mutableStateOf("") }
    var password by remember { mutableStateOf("") }
    var firstName by remember { mutableStateOf("") }
    var email by remember { mutableStateOf("") }
    var emailCode by remember { mutableStateOf("") }
    var copiedAlipayLink by remember { mutableStateOf(false) }

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

    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(palette.groupedBackground)
            .statusBarsPadding()
            .navigationBarsPadding()
            .imePadding()
            .testTag("telegram-auth-wizard"),
        contentAlignment = Alignment.Center,
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 20.dp, vertical = 20.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            // Apple Messages style emblem
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
            Spacer(Modifier.height(14.dp))
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

            Spacer(Modifier.height(16.dp))

            // Server Endpoint Pill
            Surface(
                shape = RoundedCornerShape(12.dp),
                color = palette.modalBg,
                modifier = Modifier
                    .fillMaxWidth()
                    .border(0.5.dp, palette.borderColor, RoundedCornerShape(12.dp))
                    .clickable { editingServer = !editingServer },
            ) {
                Row(
                    modifier = Modifier.padding(horizontal = 12.dp, vertical = 10.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.SpaceBetween,
                ) {
                    Text(
                        text = if (strings.isEnglish) "Server: $serverEndpoint" else "服务器：$serverEndpoint",
                        color = palette.secondaryText,
                        fontSize = 12.sp,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.weight(1f),
                    )
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
                    modifier = Modifier.fillMaxWidth(),
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

            Spacer(Modifier.height(16.dp))

            // Main iOS Auth Card
            Surface(
                shape = RoundedCornerShape(18.dp),
                color = palette.modalBg,
                shadowElevation = if (palette.isDark) 0.dp else 4.dp,
                modifier = Modifier
                    .fillMaxWidth()
                    .border(0.5.dp, palette.borderColor, RoundedCornerShape(18.dp)),
            ) {
                Column(
                    modifier = Modifier.padding(16.dp),
                    verticalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    // iOS UISegmentedControl tabs
                    val tabs = if (strings.isEnglish) {
                        listOf("Sign In", "Sign Up", "Email", "Alipay")
                    } else {
                        listOf("密码登录", "注册", "邮箱", "支付宝")
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
                                        .clickable { selectedTab = idx },
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

                    when (selectedTab) {
                        0 -> {
                            OutlinedTextField(
                                value = username,
                                onValueChange = { username = it },
                                label = { Text(if (strings.isEnglish) "Username" else "用户名") },
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
                                onClick = { onPasswordLogin(username.trim(), password) },
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
                                    Text(strings.signIn, fontSize = 15.sp, fontWeight = FontWeight.SemiBold)
                                }
                            }
                        }

                        1 -> {
                            OutlinedTextField(
                                value = firstName,
                                onValueChange = { firstName = it },
                                label = { Text(if (strings.isEnglish) "Display Name" else "昵称") },
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
                                label = { Text(if (strings.isEnglish) "Password" else "密码") },
                                singleLine = true,
                                visualTransformation = PasswordVisualTransformation(),
                                shape = RoundedCornerShape(12.dp),
                                colors = inputColors,
                                modifier = Modifier.fillMaxWidth(),
                            )
                            Button(
                                enabled = !isAuthenticating && username.isNotBlank() && password.isNotBlank(),
                                onClick = {
                                    onPasswordRegister(
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
                                Text(
                                    if (strings.isEnglish) "Create Account" else "注册并登录",
                                    fontSize = 15.sp,
                                    fontWeight = FontWeight.SemiBold,
                                )
                            }
                        }

                        2 -> {
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
                                    enabled = !isAuthenticating && email.isNotBlank(),
                                    onClick = { onSendEmailCode(email.trim()) },
                                    shape = RoundedCornerShape(12.dp),
                                ) {
                                    Text(
                                        if (strings.isEnglish) "Send Code" else "发送验证码",
                                        color = palette.primaryAccent,
                                        fontSize = 13.sp,
                                    )
                                }
                            }
                            if (!emailCodeSentNotice.isNullOrBlank()) {
                                Text(emailCodeSentNotice, color = palette.statusGreenText, fontSize = 12.sp)
                            }
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
                                label = { Text(if (strings.isEnglish) "Password" else "设置密码") },
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
                                Text(
                                    if (strings.isEnglish) "Verify & Sign In" else "验证邮箱并登录",
                                    fontSize = 15.sp,
                                    fontWeight = FontWeight.SemiBold,
                                )
                            }
                        }

                        else -> {
                            Column(
                                modifier = Modifier.fillMaxWidth(),
                                horizontalAlignment = Alignment.CenterHorizontally,
                                verticalArrangement = Arrangement.spacedBy(10.dp),
                            ) {
                                Text(
                                    text = if (strings.isEnglish) {
                                        "Authorize with Alipay OAuth on this device or scan the QR code below."
                                    } else {
                                        "点击下方链接跳转支付宝授权登录，或使用另一台设备扫码完成验证。"
                                    },
                                    color = palette.secondaryText,
                                    fontSize = 13.sp,
                                    textAlign = TextAlign.Center,
                                )

                                if (!alipayLoginUrl.isNullOrBlank()) {
                                    // Real scannable ISO/IEC 18004 QR code
                                    QrCodeCanvas(
                                        payload = alipayLoginUrl,
                                        size = 184.dp,
                                        modifier = Modifier.testTag("auth-alipay-qr"),
                                    )

                                    // Clickable URL card + Copy action
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
                                                if (strings.isEnglish) "Check Status" else "检查授权状态",
                                                color = palette.primaryAccent,
                                                fontSize = 13.sp,
                                            )
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            Spacer(Modifier.height(14.dp))

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

private fun openAlipayUrl(context: Context, url: String) {
    runCatching {
        val intent = Intent(Intent.ACTION_VIEW, Uri.parse(url)).apply {
            addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        }
        context.startActivity(intent)
    }
}
