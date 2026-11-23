package com.ombhrum.fabushi.telegram

import android.annotation.SuppressLint
import android.content.Context
import android.hardware.biometrics.BiometricPrompt
import android.os.Build
import android.os.CancellationSignal
import androidx.compose.foundation.Canvas
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
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Surface
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
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

// ============================================================================
// 1. Honest VoIP Call Unavailable Overlay (zero fake participants/screen-share)
// ============================================================================

@Composable
fun TelegramCallOverlay(
    callState: TelegramCallUiState,
    onToggleMute: () -> Unit,
    onToggleSpeaker: () -> Unit,
    onToggleVideo: () -> Unit,
    onToggleScreenShare: () -> Unit,
    onToggleMinimize: () -> Unit,
    onEndCall: () -> Unit,
) {
    if (callState.phase == CallPhase.IDLE) return
    val palette = LocalTelegramPalette.current

    if (callState.isMinimized) {
        Surface(
            modifier = Modifier
                .fillMaxWidth()
                .statusBarsPadding()
                .padding(horizontal = 12.dp, vertical = 6.dp)
                .clickable(onClick = onToggleMinimize),
            shape = RoundedCornerShape(14.dp),
            color = palette.cardBg,
            shadowElevation = 6.dp,
        ) {
            Row(
                modifier = Modifier.padding(horizontal = 14.dp, vertical = 10.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                TelegramIcon(TelegramGlyph.PHONE_CALL, tint = palette.secondaryText, size = 18.dp)
                Spacer(Modifier.width(10.dp))
                Text(
                    text = "实时音视频通话服务暂不可用",
                    color = palette.primaryText,
                    fontSize = 13.sp,
                    modifier = Modifier.weight(1f),
                )
                TextButton(onClick = onEndCall) {
                    Text("关闭", color = palette.destructiveRed)
                }
            }
        }
        return
    }

    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(palette.appBackground.copy(alpha = 0.96f))
            .statusBarsPadding()
            .navigationBarsPadding()
            .padding(24.dp),
        contentAlignment = Alignment.Center,
    ) {
        Surface(
            shape = RoundedCornerShape(20.dp),
            color = palette.modalBg,
            shadowElevation = 12.dp,
            modifier = Modifier.fillMaxWidth(),
        ) {
            Column(
                modifier = Modifier.padding(24.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                Box(
                    modifier = Modifier
                        .size(56.dp)
                        .clip(CircleShape)
                        .background(palette.inputBg),
                    contentAlignment = Alignment.Center,
                ) {
                    TelegramIcon(TelegramGlyph.PHONE_CALL, tint = palette.secondaryText, size = 26.dp)
                }
                Text(
                    text = "音视频通话暂不可用",
                    color = palette.primaryText,
                    fontSize = 18.sp,
                    fontWeight = FontWeight.Bold,
                )
                Text(
                    text = "当前服务端尚未开启原生 WebRTC 音视频通话通道 (CALL_TRANSPORT_UNAVAILABLE)。请使用文字、语音消息或「远程」标签页进行实时远程协助。",
                    color = palette.secondaryText,
                    fontSize = 13.sp,
                    lineHeight = 19.sp,
                )
                Button(
                    onClick = onEndCall,
                    colors = ButtonDefaults.buttonColors(
                        containerColor = palette.primaryAccent,
                        contentColor = Color.White,
                    ),
                    shape = RoundedCornerShape(12.dp),
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    Text("返回对话")
                }
            }
        }
    }
}

// ============================================================================
// 2. Honest Voice Assistant Fallback Overlay (zero fake <60ms VAD/echoes)
// ============================================================================

@Composable
fun TelegramTalkModeOrbOverlay(
    orbState: TalkModeOrbState,
    onClose: () -> Unit,
    onSimulateBargeIn: () -> Unit,
    onSubmitUtterance: (String) -> Unit,
) {
    if (!orbState.visible) return
    val palette = LocalTelegramPalette.current

    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(Color.Black.copy(alpha = 0.56f))
            .clickable(onClick = onClose)
            .padding(24.dp),
        contentAlignment = Alignment.Center,
    ) {
        Surface(
            shape = RoundedCornerShape(20.dp),
            color = palette.modalBg,
            shadowElevation = 12.dp,
            modifier = Modifier.fillMaxWidth(),
        ) {
            Column(
                modifier = Modifier.padding(22.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                TelegramIcon(TelegramGlyph.TALK_WAVE, tint = palette.primaryAccent, size = 34.dp)
                Text(
                    text = "实时语音对话暂未接入",
                    color = palette.primaryText,
                    fontSize = 17.sp,
                    fontWeight = FontWeight.Bold,
                )
                Text(
                    text = "实时全双工语音流需要配置专用音频传输通道。您可以在对话中按住麦克风发送真实语音消息，或在「智能体」中配置 Magpie AI 通道。",
                    color = palette.secondaryText,
                    fontSize = 13.sp,
                    lineHeight = 19.sp,
                )
                Button(
                    onClick = onClose,
                    colors = ButtonDefaults.buttonColors(
                        containerColor = palette.primaryAccent,
                        contentColor = Color.White,
                    ),
                    shape = RoundedCornerShape(12.dp),
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    Text("关闭")
                }
            }
        }
    }
}

// ============================================================================
// 3. E2EE Secret Chat 8x8 Identicon + 4-Emoji Fingerprint Dialog
// ============================================================================

@Composable
fun TelegramSecretChatDialog(
    chat: TelegramChatItem?,
    onVerify: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    if (chat == null) return
    val palette = LocalTelegramPalette.current
    val fp = remember(chat.secretFingerprintSeed) {
        CallsEmojiFingerprint.deriveSecretChatIdenticon(chat.secretFingerprintSeed)
    }
    val cellPalette = listOf(
        Color(0xFFE3F2FD),
        Color(0xFF64B5F6),
        Color(0xFF1E88E5),
        Color(0xFF0D47A1),
    )

    AlertDialog(
        onDismissRequest = onDismiss,
        containerColor = palette.modalBg,
        titleContentColor = palette.primaryText,
        textContentColor = palette.primaryText,
        title = {
            Row(verticalAlignment = Alignment.CenterVertically) {
                TelegramIcon(TelegramGlyph.SECRET_SHIELD, tint = palette.secretGreen, size = 20.dp)
                Spacer(Modifier.width(8.dp))
                Text("端到端加密密钥指纹核验", fontSize = 17.sp, fontWeight = FontWeight.SemiBold)
            }
        },
        text = {
            Column(
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(12.dp),
                modifier = Modifier.fillMaxWidth(),
            ) {
                Canvas(
                    modifier = Modifier
                        .size(144.dp)
                        .clip(RoundedCornerShape(14.dp))
                        .border(1.dp, palette.borderColor, RoundedCornerShape(14.dp)),
                ) {
                    val cellW = size.width / 8f
                    val cellH = size.height / 8f
                    for (r in 0 until 8) {
                        for (c in 0 until 8) {
                            val v = fp.cells8x8[r * 8 + c].coerceIn(0, 3)
                            drawRect(
                                color = cellPalette[v],
                                topLeft = Offset(c * cellW, r * cellH),
                                size = Size(cellW + 0.5f, cellH + 0.5f),
                            )
                        }
                    }
                }
                Text(fp.emojis4.joinToString("   "), fontSize = 28.sp)
                Text(
                    text = fp.hexBlocks,
                    fontFamily = FontFamily.Monospace,
                    fontSize = 11.sp,
                    lineHeight = 16.sp,
                    color = palette.secondaryText,
                )
                Text(
                    text = fp.protocolSummary,
                    fontSize = 12.sp,
                    color = palette.secretGreen,
                    fontWeight = FontWeight.SemiBold,
                )
            }
        },
        confirmButton = {
            Button(
                onClick = { onVerify(chat.id) },
                colors = ButtonDefaults.buttonColors(
                    containerColor = palette.primaryAccent,
                    contentColor = Color.White,
                ),
            ) {
                Text(if (chat.secretVerified) "已核验一致" else "标记为已核验")
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) { Text("关闭", color = palette.topBarActionColor) }
        },
    )
}

// ============================================================================
// 4. Apple iOS Local 4-Digit Passcode (Rust Argon2id) & Biometric Lock Screen
// ============================================================================

@Composable
fun TelegramPasscodeScreen(
    errorMessage: String?,
    biometricEnabled: Boolean,
    onVerifyPin: (String) -> Boolean,
    onBiometricSuccess: () -> Unit,
) {
    val palette = LocalTelegramPalette.current
    val strings = LocalAppleUiStrings.current
    val context = LocalContext.current
    var enteredPin by remember { mutableStateOf("") }

    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(palette.appBackground)
            .statusBarsPadding()
            .navigationBarsPadding()
            .testTag("telegram-passcode-screen"),
        contentAlignment = Alignment.Center,
    ) {
        Column(
            horizontalAlignment = Alignment.CenterHorizontally,
            modifier = Modifier.padding(28.dp),
        ) {
            Box(
                modifier = Modifier
                    .size(64.dp)
                    .clip(CircleShape)
                    .background(palette.primaryAccent.copy(alpha = 0.14f)),
                contentAlignment = Alignment.Center,
            ) {
                TelegramIcon(TelegramGlyph.LOCK_CLOSED, tint = palette.primaryAccent, size = 30.dp)
            }
            Spacer(Modifier.height(16.dp))
            Text(
                text = if (strings.isEnglish) "Enter Passcode" else "输入应用锁密码",
                color = palette.primaryText,
                fontSize = 20.sp,
                fontWeight = FontWeight.Bold,
            )
            Spacer(Modifier.height(6.dp))
            Text(
                text = errorMessage ?: if (strings.isEnglish) {
                    "Unlock messages, TON wallet, and remote controls"
                } else {
                    "解锁消息、TON 钱包与远程控制权限"
                },
                color = if (errorMessage != null) palette.destructiveRed else palette.secondaryText,
                fontSize = 13.sp,
            )

            Spacer(Modifier.height(24.dp))

            Row(horizontalArrangement = Arrangement.spacedBy(18.dp)) {
                repeat(4) { index ->
                    val filled = index < enteredPin.length
                    Box(
                        modifier = Modifier
                            .size(14.dp)
                            .clip(CircleShape)
                            .background(if (filled) palette.primaryAccent else Color.Transparent)
                            .border(2.dp, palette.primaryAccent, CircleShape),
                    )
                }
            }

            Spacer(Modifier.height(32.dp))

            val rows = listOf(
                listOf("1", "2", "3"),
                listOf("4", "5", "6"),
                listOf("7", "8", "9"),
                listOf(if (strings.isEnglish) "Bio" else "生物", "0", "⌫"),
            )
            Column(verticalArrangement = Arrangement.spacedBy(14.dp)) {
                rows.forEach { row ->
                    Row(horizontalArrangement = Arrangement.spacedBy(24.dp)) {
                        row.forEach { key ->
                            val isBio = key == "生物" || key == "Bio"
                            Box(
                                modifier = Modifier
                                    .size(70.dp)
                                    .clip(CircleShape)
                                    .background(palette.inputBg)
                                    .clickable {
                                        when {
                                            key == "⌫" -> if (enteredPin.isNotEmpty()) enteredPin = enteredPin.dropLast(1)
                                            isBio -> if (biometricEnabled) promptNativeBiometric(context, onBiometricSuccess)
                                            else -> if (enteredPin.length < 4) {
                                                val next = enteredPin + key
                                                enteredPin = next
                                                if (next.length == 4) {
                                                    if (!onVerifyPin(next)) enteredPin = ""
                                                }
                                            }
                                        }
                                    },
                                contentAlignment = Alignment.Center,
                            ) {
                                Text(
                                    text = key,
                                    color = if (isBio) palette.primaryAccent else palette.primaryText,
                                    fontSize = if (isBio) 14.sp else 24.sp,
                                    fontWeight = FontWeight.SemiBold,
                                )
                            }
                        }
                    }
                }
            }
        }
    }
}

@SuppressLint("MissingPermission")
private fun promptNativeBiometric(context: Context, onSuccess: () -> Unit) {
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
        runCatching {
            val prompt = BiometricPrompt.Builder(context)
                .setTitle("解锁 Fabushi")
                .setSubtitle("验证指纹或面容以解锁加密工作台")
                .setNegativeButton("使用数字密码", context.mainExecutor) { _, _ -> }
                .build()
            prompt.authenticate(
                CancellationSignal(),
                context.mainExecutor,
                object : BiometricPrompt.AuthenticationCallback() {
                    override fun onAuthenticationSucceeded(result: BiometricPrompt.AuthenticationResult?) {
                        onSuccess()
                    }
                },
            )
        }
    }
}
