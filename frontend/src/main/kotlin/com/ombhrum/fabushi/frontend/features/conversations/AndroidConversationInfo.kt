package com.ombhrum.fabushi

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

@Composable
internal fun AndroidConversationInfo(
    conversation: ConversationSummary,
    contacts: List<MessagingContact>,
    currentActorId: String,
    onBack: () -> Unit,
    onUpdateInfo: (String, String) -> Unit,
    onSetParticipant: (String, String) -> Unit,
    onRemoveParticipant: (String) -> Unit,
) {
    var editing by remember(conversation.id) { mutableStateOf(false) }
    var titleDraft by remember(conversation.id) { mutableStateOf(conversation.title) }
    var descriptionDraft by remember(conversation.id) { mutableStateOf(conversation.description) }
    val currentRole = conversation.participants.firstOrNull { it.actorId == currentActorId }?.role
    val canManage = conversation.kind in setOf(ConversationKind.GROUP, ConversationKind.CHANNEL) && currentRole in setOf("owner", "admin")
    val availableContacts = contacts.filter { contact -> conversation.participants.none { it.actorId == contact.id } }

    Column(Modifier.fillMaxSize().background(homeBackground)) {
        Row(Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 10.dp), verticalAlignment = Alignment.CenterVertically) {
            Text("‹", color = homePrimaryText, fontSize = 34.sp, modifier = Modifier.clickable(onClick = onBack).padding(8.dp))
            Text("详情", color = homePrimaryText, fontSize = 20.sp, fontWeight = FontWeight.SemiBold)
        }
        LazyColumn(Modifier.fillMaxSize()) {
            item {
                Column(Modifier.fillMaxWidth().padding(20.dp), horizontalAlignment = Alignment.CenterHorizontally) {
                    Box(Modifier.size(72.dp).background(homeAccent, CircleShape), contentAlignment = Alignment.Center) {
                        Text(conversation.badge, color = Color.Black, fontSize = 26.sp, fontWeight = FontWeight.Bold)
                    }
                    Text(conversation.title, color = homePrimaryText, style = MaterialTheme.typography.titleLarge, modifier = Modifier.padding(top = 12.dp))
                    Text("${conversation.participants.size} 位成员 · ${conversation.kind.label}", color = homeSecondaryText, style = MaterialTheme.typography.bodySmall)
                }
            }
            if (canManage) {
                item {
                    Column(Modifier.fillMaxWidth().background(homeSurface).padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        Text("资料", color = homeSecondaryText, style = MaterialTheme.typography.bodySmall)
                        if (editing) {
                            OutlinedTextField(titleDraft, { titleDraft = it }, label = { Text("名称") }, singleLine = true, modifier = Modifier.fillMaxWidth())
                            OutlinedTextField(descriptionDraft, { descriptionDraft = it }, label = { Text("描述") }, minLines = 2, maxLines = 5, modifier = Modifier.fillMaxWidth())
                            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                Button(onClick = { if (titleDraft.isNotBlank()) { onUpdateInfo(titleDraft.trim(), descriptionDraft); editing = false } }, enabled = titleDraft.isNotBlank()) { Text("保存") }
                                OutlinedButton(onClick = { titleDraft = conversation.title; descriptionDraft = conversation.description; editing = false }) { Text("取消") }
                            }
                        } else {
                            Text(conversation.title, color = homePrimaryText, fontWeight = FontWeight.SemiBold)
                            if (conversation.description.isNotBlank()) Text(conversation.description, color = homeSecondaryText)
                            OutlinedButton(onClick = { titleDraft = conversation.title; descriptionDraft = conversation.description; editing = true }) { Text("编辑资料") }
                        }
                    }
                }
            } else if (conversation.description.isNotBlank()) {
                item { Text(conversation.description, color = homeSecondaryText, modifier = Modifier.fillMaxWidth().padding(18.dp)) }
            }
            item { Text("成员", color = homeSecondaryText, style = MaterialTheme.typography.bodySmall, modifier = Modifier.padding(start = 18.dp, top = 18.dp, bottom = 6.dp)) }
            items(conversation.participants, key = { it.actorId }) { participant ->
                val name = if (participant.actorId == currentActorId) "我" else contacts.firstOrNull { it.id == participant.actorId }?.displayName ?: participant.actorId
                val manageable = participant.actorId != currentActorId && participant.actorId != conversation.ownerId && when (currentRole) {
                    "owner" -> true
                    "admin" -> participant.role == "member" || participant.role == "restricted"
                    else -> false
                }
                Row(Modifier.fillMaxWidth().padding(horizontal = 18.dp, vertical = 9.dp), verticalAlignment = Alignment.CenterVertically) {
                    Box(Modifier.size(42.dp).background(homeAccent, CircleShape), contentAlignment = Alignment.Center) { Text(name.take(1).uppercase(), color = Color.Black, fontWeight = FontWeight.Bold) }
                    Column(Modifier.weight(1f).padding(start = 12.dp)) {
                        Text(name, color = homePrimaryText)
                        Text(roleLabel(participant.role), color = homeSecondaryText, style = MaterialTheme.typography.bodySmall)
                    }
                    if (manageable) {
                        if (currentRole == "owner") {
                            Text(if (participant.role == "admin") "设为成员" else "设为管理员", color = homeAccent, style = MaterialTheme.typography.bodySmall, modifier = Modifier.clickable { onSetParticipant(participant.actorId, if (participant.role == "admin") "member" else "admin") }.padding(7.dp))
                        }
                        Text("移除", color = Color(0xFFFF6B6B), style = MaterialTheme.typography.bodySmall, modifier = Modifier.clickable { onRemoveParticipant(participant.actorId) }.padding(7.dp))
                    }
                }
            }
            if (canManage && availableContacts.isNotEmpty()) {
                item { Text("添加成员", color = homeSecondaryText, style = MaterialTheme.typography.bodySmall, modifier = Modifier.padding(start = 18.dp, top = 18.dp, bottom = 6.dp)) }
                items(availableContacts, key = { it.id }) { contact ->
                    Row(Modifier.fillMaxWidth().clickable { onSetParticipant(contact.id, "member") }.padding(horizontal = 18.dp, vertical = 10.dp), verticalAlignment = Alignment.CenterVertically) {
                        Text("＋", color = homeAccent, fontSize = 22.sp)
                        Text(contact.displayName, color = homePrimaryText, modifier = Modifier.padding(start = 12.dp))
                    }
                }
            }
        }
    }
}

private fun roleLabel(role: String): String = when (role) {
    "owner" -> "群主"
    "admin" -> "管理员"
    "restricted" -> "受限成员"
    else -> "成员"
}
