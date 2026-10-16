package com.ombhrum.fabushi

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/**
 * Android conversation identity/header counterpart. The header owns only UI menu state; all
 * mutations are injected from the conversation presentation boundary.
 */
@Composable
internal fun ConversationChatHeader(
    conversation: ConversationSummary,
    searchOpen: Boolean,
    onBack: () -> Unit,
    onOpenInfo: () -> Unit,
    onToggleSearch: () -> Unit,
    onToggleMute: () -> Unit,
    onTogglePin: () -> Unit,
    onArchive: () -> Unit,
) {
    var menuOpen by remember(conversation.id) { mutableStateOf(false) }
    Row(
        Modifier
            .fillMaxWidth()
            .padding(horizontal = 12.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            "‹",
            color = homePrimaryText,
            fontSize = 34.sp,
            modifier = Modifier
                .clickable(onClick = onBack)
                .padding(8.dp)
                .testTag("conversation-back"),
        )
        Column(
            Modifier
                .weight(1f)
                .clickable(onClick = onOpenInfo)
                .padding(vertical = 4.dp)
                .testTag("conversation-header-identity"),
        ) {
            Text(
                conversation.title,
                color = homePrimaryText,
                fontWeight = FontWeight.SemiBold,
                fontSize = 18.sp,
            )
            Text(
                "${conversation.participants.size} 位成员 · ${conversation.kind.label}",
                color = homeSecondaryText,
                style = MaterialTheme.typography.bodySmall,
            )
        }
        Text(
            if (searchOpen) "×" else "⌕",
            color = homePrimaryText,
            fontSize = 23.sp,
            modifier = Modifier
                .clickable(onClick = onToggleSearch)
                .padding(8.dp)
                .testTag("conversation-find-toggle"),
        )
        Box {
            Text(
                "⋯",
                color = homePrimaryText,
                fontSize = 28.sp,
                modifier = Modifier
                    .clickable { menuOpen = true }
                    .padding(8.dp)
                    .testTag("conversation-header-menu"),
            )
            DropdownMenu(
                expanded = menuOpen,
                onDismissRequest = { menuOpen = false },
                containerColor = homeSurface,
            ) {
                DropdownMenuItem(
                    text = {
                        Text(
                            if (conversation.isMuted) "取消静音" else "静音",
                            color = homePrimaryText,
                        )
                    },
                    onClick = {
                        menuOpen = false
                        onToggleMute()
                    },
                )
                DropdownMenuItem(
                    text = {
                        Text(
                            if (conversation.isPinned) "取消置顶" else "置顶",
                            color = homePrimaryText,
                        )
                    },
                    onClick = {
                        menuOpen = false
                        onTogglePin()
                    },
                )
                DropdownMenuItem(
                    text = { Text("归档", color = homePrimaryText) },
                    onClick = {
                        menuOpen = false
                        onArchive()
                    },
                )
            }
        }
    }
}
