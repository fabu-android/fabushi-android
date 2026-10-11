package com.ombhrum.fabushi

import android.Manifest
import android.annotation.SuppressLint
import android.content.Context
import android.content.pm.PackageManager
import android.location.LocationManager
import android.os.Build
import android.os.CancellationSignal
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.gestures.detectHorizontalDragGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.FloatingActionButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.content.ContextCompat
import kotlinx.coroutines.delay


@Composable
internal fun AndroidSectionSurface(
    section: AndroidMobileSection,
    messagingState: MessagingUiState,
    onBack: () -> Unit,
    onCreateDirect: (MessagingContact) -> Unit,
    onOpenConversation: (ConversationSummary) -> Unit,
    onUnarchive: (ConversationSummary) -> Unit,
    onUpsertFolder: (MessagingFolder) -> Unit,
    onDeleteFolder: (String) -> Unit,
) {
    var showFolderEditor by remember { mutableStateOf(false) }
    var folderTitle by remember { mutableStateOf("") }
    var folderConversationIds by remember { mutableStateOf(setOf<String>()) }
    var folderIncludeGroups by remember { mutableStateOf(false) }
    var folderIncludeChannels by remember { mutableStateOf(false) }
    var openedFolder by remember { mutableStateOf<MessagingFolder?>(null) }

    if (showFolderEditor) {
        AlertDialog(
            onDismissRequest = { showFolderEditor = false },
            title = { Text("新建文件夹") },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    OutlinedTextField(folderTitle, { folderTitle = it }, label = { Text("文件夹名称") }, singleLine = true)
                    Row(Modifier.fillMaxWidth().clickable { folderIncludeGroups = !folderIncludeGroups }.padding(vertical = 4.dp)) { Text(if (folderIncludeGroups) "●" else "○", color = homeAccent); Text("自动包含群组", color = homePrimaryText, modifier = Modifier.padding(start = 8.dp)) }
                    Row(Modifier.fillMaxWidth().clickable { folderIncludeChannels = !folderIncludeChannels }.padding(vertical = 4.dp)) { Text(if (folderIncludeChannels) "●" else "○", color = homeAccent); Text("自动包含频道", color = homePrimaryText, modifier = Modifier.padding(start = 8.dp)) }
                    Text("选择会话", color = homeSecondaryText, style = MaterialTheme.typography.bodySmall)
                    messagingState.conversations.filter { !it.isArchived }.take(14).forEach { conversation ->
                        val selected = conversation.id in folderConversationIds
                        Row(Modifier.fillMaxWidth().clickable { folderConversationIds = if (selected) folderConversationIds - conversation.id else folderConversationIds + conversation.id }.padding(vertical = 4.dp)) {
                            Text(if (selected) "●" else "○", color = if (selected) homeAccent else homeSecondaryText); Text(conversation.title, color = homePrimaryText, modifier = Modifier.padding(start = 8.dp))
                        }
                    }
                }
            },
            confirmButton = {
                Button(onClick = {
                    onUpsertFolder(MessagingFolder(id = "folder-${System.nanoTime()}", title = folderTitle.trim(), icon = "folder", conversationIds = folderConversationIds.toList(), includeGroups = folderIncludeGroups, includeChannels = folderIncludeChannels, excludeArchived = true))
                    showFolderEditor = false
                }, enabled = folderTitle.isNotBlank()) { Text("创建") }
            },
            dismissButton = { OutlinedButton(onClick = { showFolderEditor = false }) { Text("取消") } },
        )
    }

    openedFolder?.let { folder ->
        val rows = messagingState.conversations.filter { conversation ->
            (!conversation.isArchived || !folder.excludeArchived) && (!conversation.isMuted || !folder.excludeMuted) && (conversation.unreadCount > 0 || !folder.excludeRead) &&
                (conversation.id in folder.conversationIds || (folder.includeGroups && conversation.kind == ConversationKind.GROUP) || (folder.includeChannels && conversation.kind == ConversationKind.CHANNEL))
        }
        Scaffold(containerColor = homeBackground) { padding ->
            Column(Modifier.fillMaxSize().padding(padding)) {
                Row(Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 10.dp), verticalAlignment = Alignment.CenterVertically) {
                    Text("‹", color = homePrimaryText, fontSize = 34.sp, modifier = Modifier.clickable { openedFolder = null }.padding(8.dp)); Text(folder.title, color = homePrimaryText, fontWeight = FontWeight.SemiBold, fontSize = 20.sp)
                }
                LazyColumn(Modifier.fillMaxSize()) {
                    if (rows.isEmpty()) item { Text("暂无会话", color = homeSecondaryText, modifier = Modifier.fillMaxWidth().padding(48.dp)) }
                    items(rows, key = { it.id }) { conversation -> ConversationRow(conversation, onClick = { onOpenConversation(conversation) }) }
                }
            }
        }
        return
    }

    val conversations = when (section) {
        AndroidMobileSection.GROUPS -> messagingState.conversations.filter { it.kind == ConversationKind.GROUP && !it.isArchived }
        AndroidMobileSection.CHANNELS -> messagingState.conversations.filter { it.kind == ConversationKind.CHANNEL && !it.isArchived }
        AndroidMobileSection.SAVED -> messagingState.conversations.filter { it.kind == ConversationKind.SAVED_MESSAGES }
        AndroidMobileSection.ARCHIVE -> messagingState.conversations.filter { it.isArchived }
        else -> emptyList()
    }
    val contacts = when (section) {
        AndroidMobileSection.CONTACTS -> messagingState.contacts
        AndroidMobileSection.BOTS -> messagingState.contacts.filter { it.kind == "bot" || it.kind == "assistant" }
        else -> emptyList()
    }
    Scaffold(containerColor = homeBackground) { padding ->
        Column(Modifier.fillMaxSize().padding(padding)) {
            Row(Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 10.dp), verticalAlignment = Alignment.CenterVertically) {
                Text("‹", color = homePrimaryText, fontSize = 34.sp, modifier = Modifier.clickable(onClick = onBack).padding(8.dp))
                Text(section.label, color = homePrimaryText, fontWeight = FontWeight.SemiBold, fontSize = 20.sp, modifier = Modifier.weight(1f))
                if (section == AndroidMobileSection.FOLDERS) Text("＋", color = homeAccent, fontSize = 26.sp, modifier = Modifier.clickable { folderTitle = ""; folderConversationIds = emptySet(); folderIncludeGroups = false; folderIncludeChannels = false; showFolderEditor = true }.padding(8.dp))
            }
            LazyColumn(Modifier.fillMaxSize()) {
                if (section == AndroidMobileSection.FOLDERS && messagingState.folders.isNotEmpty()) items(messagingState.folders, key = { it.id }) { folder ->
                    Row(Modifier.fillMaxWidth().padding(horizontal = 18.dp, vertical = 11.dp), verticalAlignment = Alignment.CenterVertically) {
                        Text("▣", color = homeAccent, fontSize = 22.sp)
                        Text(folder.title, color = homePrimaryText, modifier = Modifier.weight(1f).clickable { openedFolder = folder }.padding(start = 12.dp, top = 8.dp, bottom = 8.dp))
                        Text("删除", color = Color(0xFFFF6B6B), modifier = Modifier.clickable { onDeleteFolder(folder.id) }.padding(8.dp))
                    }
                }
                if (contacts.isNotEmpty()) items(contacts, key = { it.id }) { contact ->
                    Row(Modifier.fillMaxWidth().clickable { onCreateDirect(contact) }.padding(horizontal = 18.dp, vertical = 11.dp), verticalAlignment = Alignment.CenterVertically) {
                        Box(Modifier.size(46.dp).background(homeAccent, CircleShape), contentAlignment = Alignment.Center) { Text(contact.displayName.take(1).uppercase(), color = Color.Black, fontWeight = FontWeight.Bold) }
                        Column(Modifier.padding(start = 13.dp)) { Text(contact.displayName, color = homePrimaryText); Text(contact.username?.let { "@$it" } ?: contact.kind, color = homeSecondaryText, style = MaterialTheme.typography.bodySmall) }
                    }
                }
                if (conversations.isNotEmpty()) items(conversations, key = { it.id }) { conversation ->
                    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                        Box(Modifier.weight(1f)) { ConversationRow(conversation, onClick = { onOpenConversation(conversation) }) }
                        if (section == AndroidMobileSection.ARCHIVE) Text("恢复", color = homeAccent, modifier = Modifier.clickable { onUnarchive(conversation) }.padding(14.dp))
                    }
                }
                if (contacts.isEmpty() && conversations.isEmpty() && (section != AndroidMobileSection.FOLDERS || messagingState.folders.isEmpty())) item {
                    Column(Modifier.fillMaxWidth().padding(top = 96.dp), horizontalAlignment = Alignment.CenterHorizontally) {
                        Text(section.label, color = homePrimaryText, fontWeight = FontWeight.SemiBold)
                        Text(
                            when (section) {
                                AndroidMobileSection.CALLS -> "暂无通话记录"
                                AndroidMobileSection.FOLDERS -> "暂无会话文件夹"
                                AndroidMobileSection.SETTINGS -> "设置由统一账户配置提供"
                                else -> "暂无内容"
                            },
                            color = homeSecondaryText,
                            modifier = Modifier.padding(top = 8.dp),
                        )
                    }
                }
            }
        }
    }
}

@Composable
internal fun ProfileAvatar(onClick: () -> Unit) {
    Box(
        modifier = Modifier
            .size(56.dp)
            .background(homeSurface, CircleShape)
            .border(1.dp, homeBorder, CircleShape)
            .testTag(TestTags.ProfileAvatar)
            .clickable(onClick = onClick)
            .semantics { contentDescription = "个人头像" },
        contentAlignment = Alignment.Center,
    ) {
        Text("✦", color = homeAccent, fontSize = 34.sp, fontWeight = FontWeight.Bold)
    }
}

@Composable
internal fun CircularActionButton(
    tag: String,
    description: String,
    onClick: () -> Unit,
    content: @Composable () -> Unit,
) {
    Box(
        modifier = Modifier
            .size(54.dp)
            .background(Color(0xFF101011), CircleShape)
            .border(1.dp, homeBorder, CircleShape)
            .clickable(onClick = onClick)
            .testTag(tag)
            .semantics { contentDescription = description },
        contentAlignment = Alignment.Center,
    ) {
        content()
    }
}

@Composable
internal fun SearchGlyph() {
    Canvas(Modifier.size(24.dp)) {
        val stroke = 2.2.dp.toPx()
        drawCircle(
            color = homePrimaryText,
            radius = size.minDimension * 0.28f,
            center = Offset(size.width * 0.43f, size.height * 0.42f),
            style = Stroke(width = stroke),
        )
        drawLine(
            color = homePrimaryText,
            start = Offset(size.width * 0.63f, size.height * 0.63f),
            end = Offset(size.width * 0.84f, size.height * 0.84f),
            strokeWidth = stroke,
        )
    }
}

@Composable
internal fun PlusGlyph() {
    Canvas(Modifier.size(25.dp)) {
        val stroke = 2.dp.toPx()
        drawLine(
            color = homePrimaryText,
            start = Offset(size.width * 0.5f, size.height * 0.14f),
            end = Offset(size.width * 0.5f, size.height * 0.86f),
            strokeWidth = stroke,
        )
        drawLine(
            color = homePrimaryText,
            start = Offset(size.width * 0.14f, size.height * 0.5f),
            end = Offset(size.width * 0.86f, size.height * 0.5f),
            strokeWidth = stroke,
        )
    }
}


