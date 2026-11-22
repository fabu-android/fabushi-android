package com.ombhrum.fabushi.telegram

import androidx.compose.animation.AnimatedVisibility
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
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
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
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/**
 * Desktop `bhrumom/gpui-telegram` AuthWizardView aligned onboarding & login screen (Apple Messages UI/UX).
 * Connects directly to real `gpui_telegram_server` `/api/v1/auth` endpoints with zero fake data.
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
    var selectedTab by remember { mutableIntStateOf(0) } // 0=Login, 1=Register, 2=Email, 3=Alipay
    var editingServer by remember { mutableStateOf(false) }
    var serverDraft by remember(serverEndpoint) { mutableStateOf(serverEndpoint) }
    var username by remember { mutableStateOf("") }
    var password by remember { mutableStateOf("") }
    var firstName by remember { mutableStateOf("") }
    var email by remember { mutableStateOf("") }
    var emailCode by remember { mutableStateOf("") }

    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(palette.appBackground)
            .statusBarsPadding()
            .navigationBarsPadding()
            .imePadding()
            .testTag("telegram-auth-wizard"),
        contentAlignment = Alignment.Center,
    ) {
        Column(
            modifier = Modifier
                .widthIn(max = 440.dp)
                .fillMaxWidth()
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 20.dp, vertical = 24.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            // Apple Messages style emblem
            Box(
                modifier = Modifier
                    .size(68.dp)
                    .clip(CircleShape)
                    .background(
                        Brush.verticalGradient(
                            colors = listOf(palette.outgoingBubbleTop, palette.outgoingBubbleBottom),
                        ),
                    ),
                contentAlignment = Alignment.Center,
            ) {
                Text("✈", color = Color.White, fontSize = 30.sp, fontWeight = FontWeight.Bold)
            }

            Spacer(Modifier.height(14.dp))
            Text(
                text = "Fabushi 信息",
                color = palette.primaryText,
                fontSize = 24.sp,
                fontWeight = FontWeight.Bold,
            )
            Spacer(Modifier.height(4.dp))
            Text(
                text = "登录与桌面端同步的真实账号 · 零假数据",
                color = palette.secondaryText,
                fontSize = 13.sp,
                textAlign = TextAlign.Center,
            )

            Spacer(Modifier.height(14.dp))

            // Editable server pill (matches desktop AuthWizardView server URL bar)
            Surface(
                shape = RoundedCornerShape(999.dp),
                color = palette.elevatedSurface,
                modifier = Modifier
                    .border(0.75.dp, palette.dividerColor, RoundedCornerShape(999.dp))
                    .clickable { editingServer = !editingServer },
            ) {
                Row(
                    modifier = Modifier.padding(horizontal = 14.dp, vertical = 7.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(
                        text = "服务器：$serverEndpoint",
                        color = palette.secondaryText,
                        fontSize = 12.sp,
                        maxLines = 1,
                    )
                    Spacer(Modifier.width(8.dp))
                    Text(
                        text = if (editingServer) "收起" else "修改",
                        color = palette.primaryAccent,
                        fontSize = 12.sp,
                        fontWeight = FontWeight.SemiBold,
                    )
                }
            }

            AnimatedVisibility(visible = editingServer) {
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(top = 10.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    OutlinedTextField(
                        value = serverDraft,
                        onValueChange = { serverDraft = it },
                        label = { Text("gpui_telegram_server URL") },
                        singleLine = true,
                        modifier = Modifier.weight(1f),
                    )
                    Button(
                        onClick = {
                            onUpdateServerEndpoint(serverDraft.trim())
                            editingServer = false
                        },
                        shape = RoundedCornerShape(12.dp),
                    ) {
                        Text("保存")
                    }
                }
            }

            Spacer(Modifier.height(18.dp))

            // Main frosted Apple card
            Surface(
                shape = RoundedCornerShape(20.dp),
                color = palette.surfaceColor,
                modifier = Modifier
                    .fillMaxWidth()
                    .border(0.75.dp, palette.dividerColor, RoundedCornerShape(20.dp)),
            ) {
                Column(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(18.dp),
                    verticalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    // Segmented pill tabs matching desktop AuthWizardTab
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .clip(RoundedCornerShape(12.dp))
                            .background(palette.elevatedSurface)
                            .padding(3.dp),
                        horizontalArrangement = Arrangement.spacedBy(3.dp),
                    ) {
                        listOf("密码登录", "注册", "邮箱", "支付宝").forEachIndexed { idx, label ->
                            val active = selectedTab == idx
                            Box(
                                modifier = Modifier
                                    .weight(1f)
                                    .clip(RoundedCornerShape(9.dp))
                                    .background(if (active) palette.surfaceColor else Color.Transparent)
                                    .clickable { selectedTab = idx }
                                    .padding(vertical = 8.dp),
                                contentAlignment = Alignment.Center,
                            ) {
                                Text(
                                    text = label,
                                    color = if (active) palette.primaryText else palette.secondaryText,
                                    fontSize = 12.5.sp,
                                    fontWeight = if (active) FontWeight.SemiBold else FontWeight.Medium,
                                )
                            }
                        }
                    }

                    if (!authError.isNullOrBlank()) {
                        Box(
                            modifier = Modifier
                                .fillMaxWidth()
                                .clip(RoundedCornerShape(12.dp))
                                .background(palette.destructiveRed.copy(alpha = 0.12f))
                                .padding(horizontal = 12.dp, vertical = 9.dp),
                        ) {
                            Text(
                                text = authError,
                                color = palette.destructiveRed,
                                fontSize = 12.5.sp,
                            )
                        }
                    }

                    when (selectedTab) {
                        0 -> {
                            OutlinedTextField(
                                value = username,
                                onValueChange = { username = it },
                                label = { Text("用户名 / @username") },
                                singleLine = true,
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .testTag("auth-username-input"),
                            )
                            OutlinedTextField(
                                value = password,
                                onValueChange = { password = it },
                                label = { Text("密码") },
                                singleLine = true,
                                visualTransformation = PasswordVisualTransformation(),
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .testTag("auth-password-input"),
                            )
                            Button(
                                onClick = { onPasswordLogin(username.trim(), password) },
                                enabled = !isAuthenticating && username.isNotBlank() && password.isNotBlank(),
                                shape = RoundedCornerShape(14.dp),
                                colors = ButtonDefaults.buttonColors(containerColor = palette.primaryAccent),
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .height(46.dp)
                                    .testTag("auth-submit-login"),
                            ) {
                                if (isAuthenticating) {
                                    CircularProgressIndicator(
                                        color = Color.White,
                                        strokeWidth = 2.dp,
                                        modifier = Modifier.size(18.dp),
                                    )
                                } else {
                                    Text("登录", color = Color.White, fontWeight = FontWeight.SemiBold, fontSize = 15.sp)
                                }
                            }
                        }

                        1 -> {
                            OutlinedTextField(
                                value = firstName,
                                onValueChange = { firstName = it },
                                label = { Text("显示昵称") },
                                singleLine = true,
                                modifier = Modifier.fillMaxWidth(),
                            )
                            OutlinedTextField(
                                value = username,
                                onValueChange = { username = it },
                                label = { Text("用户名") },
                                singleLine = true,
                                modifier = Modifier.fillMaxWidth(),
                            )
                            OutlinedTextField(
                                value = password,
                                onValueChange = { password = it },
                                label = { Text("设置密码（至少 6 位）") },
                                singleLine = true,
                                visualTransformation = PasswordVisualTransformation(),
                                modifier = Modifier.fillMaxWidth(),
                            )
                            Button(
                                onClick = { onPasswordRegister(username.trim(), password, firstName.trim()) },
                                enabled = !isAuthenticating && username.isNotBlank() && password.length >= 6,
                                shape = RoundedCornerShape(14.dp),
                                colors = ButtonDefaults.buttonColors(containerColor = palette.primaryAccent),
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .height(46.dp),
                            ) {
                                Text(if (isAuthenticating) "注册中…" else "创建账号并登录", color = Color.White, fontWeight = FontWeight.SemiBold)
                            }
                        }

                        2 -> {
                            Row(
                                modifier = Modifier.fillMaxWidth(),
                                horizontalArrangement = Arrangement.spacedBy(8.dp),
                                verticalAlignment = Alignment.CenterVertically,
                            ) {
                                OutlinedTextField(
                                    value = email,
                                    onValueChange = { email = it },
                                    label = { Text("邮箱地址") },
                                    singleLine = true,
                                    modifier = Modifier.weight(1f),
                                )
                                OutlinedButton(
                                    onClick = { onSendEmailCode(email.trim()) },
                                    enabled = !isAuthenticating && email.contains("@"),
                                    shape = RoundedCornerShape(12.dp),
                                ) {
                                    Text("发验证码")
                                }
                            }
                            if (!emailCodeSentNotice.isNullOrBlank()) {
                                Text(emailCodeSentNotice, color = palette.primaryAccent, fontSize = 12.sp)
                            }
                            OutlinedTextField(
                                value = emailCode,
                                onValueChange = { emailCode = it },
                                label = { Text("邮箱验证码") },
                                singleLine = true,
                                modifier = Modifier.fillMaxWidth(),
                            )
                            OutlinedTextField(
                                value = username,
                                onValueChange = { username = it },
                                label = { Text("用户名") },
                                singleLine = true,
                                modifier = Modifier.fillMaxWidth(),
                            )
                            OutlinedTextField(
                                value = firstName,
                                onValueChange = { firstName = it },
                                label = { Text("显示昵称") },
                                singleLine = true,
                                modifier = Modifier.fillMaxWidth(),
                            )
                            OutlinedTextField(
                                value = password,
                                onValueChange = { password = it },
                                label = { Text("密码") },
                                singleLine = true,
                                visualTransformation = PasswordVisualTransformation(),
                                modifier = Modifier.fillMaxWidth(),
                            )
                            Button(
                                onClick = {
                                    onEmailRegister(email.trim(), emailCode.trim(), username.trim(), password, firstName.trim())
                                },
                                enabled = !isAuthenticating && email.contains("@") && emailCode.isNotBlank() && username.isNotBlank() && password.length >= 6,
                                shape = RoundedCornerShape(14.dp),
                                colors = ButtonDefaults.buttonColors(containerColor = palette.primaryAccent),
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .height(46.dp),
                            ) {
                                Text("验证邮箱并注册", color = Color.White, fontWeight = FontWeight.SemiBold)
                            }
                        }

                        3 -> {
                            Text(
                                text = alipayStatusText?.ifBlank { null } ?: "通过服务器发起支付宝 OAuth 授权登录",
                                color = palette.secondaryText,
                                fontSize = 13.sp,
                            )
                            if (!alipayLoginUrl.isNullOrBlank()) {
                                Text(
                                    text = alipayLoginUrl,
                                    color = palette.primaryAccent,
                                    fontSize = 11.5.sp,
                                    maxLines = 2,
                                )
                            }
                            Row(
                                modifier = Modifier.fillMaxWidth(),
                                horizontalArrangement = Arrangement.spacedBy(8.dp),
                            ) {
                                Button(
                                    onClick = onStartAlipayLogin,
                                    enabled = !isAuthenticating,
                                    shape = RoundedCornerShape(14.dp),
                                    colors = ButtonDefaults.buttonColors(containerColor = Color(0xFF1677FF)),
                                    modifier = Modifier.weight(1f),
                                ) {
                                    Text("发起支付宝登录", color = Color.White)
                                }
                                OutlinedButton(
                                    onClick = onPollAlipayLogin,
                                    enabled = !isAuthenticating && !alipayLoginUrl.isNullOrBlank(),
                                    shape = RoundedCornerShape(14.dp),
                                ) {
                                    Text("检查状态")
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
                    text = "暂不登录 · 仅使用本机离线工作台（Mika / 工具 / 本地会话）",
                    color = palette.secondaryText,
                    fontSize = 12.5.sp,
                )
            }
        }
    }
}
