package com.ombhrum.fabushi.telegram

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.ombhrum.fabushi.ConversationKind

/**
 * Apple-styled conversation / secret-chat creation dialog.
 * Legacy ModalNavigationDrawer sheet has been retired in favor of the unified 5-tab Apple shell.
 */
@Composable
fun CreateConversationOrSecretDialog(
    kind: ConversationKind?,
    onDismiss: () -> Unit,
    onCreateGroupOrChannel: (ConversationKind, String, String, Boolean) -> Unit,
    onCreateSecretChat: (String) -> Unit,
) {
    if (kind == null) return
    val palette = LocalTelegramPalette.current
    val strings = LocalAppleUiStrings.current
    var title by remember(kind) { mutableStateOf("") }
    var desc by remember(kind) { mutableStateOf("") }
    var forumTopicsEnabled by remember(kind) { mutableStateOf(false) }

    val canSubmit = title.isNotBlank()

    AlertDialog(
        onDismissRequest = onDismiss,
        containerColor = palette.modalBg,
        titleContentColor = palette.primaryText,
        textContentColor = palette.primaryText,
        title = {
            Row(verticalAlignment = Alignment.CenterVertically) {
                val glyph = when (kind) {
                    ConversationKind.SECRET -> TelegramGlyph.SECRET_SHIELD
                    ConversationKind.CHANNEL -> TelegramGlyph.COMPOSE_SQUARE_PEN
                    else -> TelegramGlyph.CONTACTS
                }
                TelegramIcon(glyph = glyph, tint = palette.primaryAccent, size = 20.dp)
                Spacer(Modifier.width(8.dp))
                Text(
                    when (kind) {
                        ConversationKind.SECRET -> strings.newSecretChatAction
                        ConversationKind.CHANNEL -> strings.newChannelAction
                        else -> strings.newGroupAction
                    },
                )
            }
        },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                OutlinedTextField(
                    value = title,
                    onValueChange = { title = it },
                    label = {
                        Text(
                            if (kind == ConversationKind.SECRET) {
                                if (strings.isEnglish) "Peer @username or display name" else "对方 @用户名或昵称"
                            } else {
                                if (strings.isEnglish) "Name" else "名称"
                            },
                        )
                    },
                    singleLine = true,
                    colors = OutlinedTextFieldDefaults.colors(
                        focusedTextColor = palette.primaryText,
                        unfocusedTextColor = palette.primaryText,
                        cursorColor = palette.searchCursorColor,
                    ),
                    modifier = Modifier.fillMaxWidth(),
                )
                if (kind != ConversationKind.SECRET) {
                    OutlinedTextField(
                        value = desc,
                        onValueChange = { desc = it },
                        label = { Text(if (strings.isEnglish) "Description (optional)" else "简介说明（可选）") },
                        colors = OutlinedTextFieldDefaults.colors(
                            focusedTextColor = palette.primaryText,
                            unfocusedTextColor = palette.primaryText,
                            cursorColor = palette.searchCursorColor,
                        ),
                        modifier = Modifier.fillMaxWidth(),
                    )
                } else {
                    Text(
                        text = if (strings.isEnglish) {
                            "Uses X3DH (X25519 + Ed25519) key agreement and Double Ratchet encryption with 4-emoji safety verification."
                        } else {
                            "采用 X3DH (X25519 + Ed25519) 密钥协商与 Double Ratchet 双棘轮加密，支持 4-Emoji 安全指纹核验与阅后即焚。"
                        },
                        fontSize = 13.sp,
                        color = palette.secondaryText,
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
                        TelegramIcon(
                            glyph = if (forumTopicsEnabled) TelegramGlyph.CHECK_CIRCLE else TelegramGlyph.CIRCLE_OUTLINE,
                            tint = if (forumTopicsEnabled) palette.primaryAccent else palette.secondaryText,
                            size = 20.dp,
                        )
                        Spacer(Modifier.width(8.dp))
                        Text(
                            text = if (strings.isEnglish) "Enable Forum Topics" else "启用超级群组多话题模式",
                            fontSize = 14.sp,
                            color = palette.primaryText,
                        )
                    }
                }
            }
        },
        confirmButton = {
            Button(
                enabled = canSubmit,
                onClick = {
                    val cleanTitle = title.trim()
                    if (kind == ConversationKind.SECRET) {
                        onCreateSecretChat(cleanTitle)
                    } else {
                        onCreateGroupOrChannel(kind, cleanTitle, desc.trim(), forumTopicsEnabled)
                    }
                    onDismiss()
                },
                colors = ButtonDefaults.buttonColors(
                    containerColor = palette.primaryAccent,
                    contentColor = Color.White,
                ),
            ) {
                Text(if (strings.isEnglish) "Create" else "创建")
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) {
                Text(strings.cancel, color = palette.topBarActionColor)
            }
        },
    )
}
