package com.ombhrum.fabushi

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

@Composable
internal fun SharedRoomsSurface(
    bots: List<MobileBotSummaryAndroid>,
    onBack: () -> Unit,
) {
    val owner = remember { SharedRoomsPresentationOwner(CoordinatorClient.presentation()) }
    val scope = rememberCoroutineScope()
    var snapshot by remember { mutableStateOf(MobileSharedRoomsSnapshot()) }
    var loading by remember { mutableStateOf(true) }
    var error by remember { mutableStateOf<String?>(null) }
    var joinLink by remember { mutableStateOf("") }
    var generation by remember { mutableLongStateOf(0L) }
    val shareableAgents = remember(bots) {
        bots.filter { bot -> !bot.isHidden && !bot.isGroup && !bot.isSharedRoom }
    }

    fun runAction(action: () -> MobileSharedRoomsSnapshot) {
        val token = ++generation
        loading = true
        error = null
        scope.launch {
            val result = withContext(Dispatchers.IO) { runCatching(action) }
            if (token != generation) return@launch
            loading = false
            result.onSuccess { snapshot = it }
                .onFailure { throwable -> error = throwable.message ?: "Shared rooms unavailable" }
        }
    }

    fun refresh() = runAction(owner::refresh)

    LaunchedEffect(Unit) { refresh() }

    Column(
        Modifier
            .fillMaxSize()
            .background(GrokMobileBackground)
            .testTag("shared-rooms-surface"),
    ) {
        Row(
            Modifier.fillMaxWidth().padding(horizontal = 14.dp, vertical = 10.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            TextButton(onClick = onBack, modifier = Modifier.testTag("shared-rooms-back")) { Text("Back") }
            Column(Modifier.weight(1f)) {
                Text("Shared rooms", color = GrokMobileInk, fontSize = 20.sp, fontWeight = FontWeight.Bold)
                Text(
                    when {
                        !snapshot.available -> "Unavailable"
                        !snapshot.environmentAllowed -> "Blocked by environment policy"
                        !snapshot.canonicalRoomManagerBound -> "Room manager not ready"
                        snapshot.enabled -> "On · ${snapshot.rooms.size} rooms"
                        else -> "Off"
                    },
                    color = GrokMobileMuted,
                    fontSize = 12.sp,
                    modifier = Modifier.testTag("shared-rooms-status"),
                )
            }
            TextButton(onClick = { refresh() }, enabled = !loading) { Text("Refresh") }
        }

        error?.let {
            Text(
                it,
                color = GrokMobileMuted,
                modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 6.dp)
                    .testTag("shared-rooms-error"),
            )
        }

        if (snapshot.available && snapshot.environmentAllowed && snapshot.canonicalRoomManagerBound) {
            Row(
                Modifier.fillMaxWidth().padding(horizontal = 16.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                Button(
                    onClick = { runAction { owner.setEnabled(!snapshot.enabled) } },
                    enabled = !loading,
                    modifier = Modifier.testTag("shared-rooms-toggle"),
                ) { Text(if (snapshot.enabled) "Turn off" else "Turn on") }
                Button(
                    onClick = { runAction(owner::reconcile) },
                    enabled = snapshot.enabled && !loading,
                    modifier = Modifier.testTag("shared-rooms-reconcile"),
                ) { Text("Reconcile") }
            }
        }

        if (snapshot.enabled) {
            Spacer(Modifier.height(10.dp))
            OutlinedTextField(
                value = joinLink,
                onValueChange = { joinLink = it.take(2048) },
                label = { Text("Invite link") },
                singleLine = true,
                modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp)
                    .testTag("shared-rooms-join-link"),
            )
            Button(
                onClick = {
                    val link = joinLink.trim()
                    if (link.isNotEmpty()) {
                        runAction {
                            owner.join(link)
                            owner.refresh()
                        }
                    }
                },
                enabled = joinLink.isNotBlank() && !loading,
                modifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp)
                    .testTag("shared-rooms-join"),
            ) { Text("Join room") }
        }

        val metadata = buildString {
            if (snapshot.pendingJoinRequests.isNotEmpty()) {
                append("${snapshot.pendingJoinRequests.size} pending")
            }
            if (snapshot.typingUserCount > 0) {
                if (isNotEmpty()) append(" · ")
                append("${snapshot.typingUserCount} typing")
            }
        }
        if (metadata.isNotEmpty()) {
            Text(
                metadata,
                color = GrokMobileMuted,
                fontSize = 12.sp,
                modifier = Modifier.padding(horizontal = 16.dp, vertical = 4.dp),
            )
        }

        LazyColumn(
            Modifier.fillMaxWidth().weight(1f).padding(horizontal = 16.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            if (snapshot.enabled && snapshot.pendingJoinRequests.isNotEmpty()) {
                item(key = "pending-join-heading") {
                    Text("Pending requests", color = GrokMobileInk, fontWeight = FontWeight.SemiBold)
                }
                items(snapshot.pendingJoinRequests, key = { "join:${it.requestId}" }) { request ->
                    Column(
                        Modifier.fillMaxWidth()
                            .background(androidx.compose.ui.graphics.Color.White, RoundedCornerShape(16.dp))
                            .padding(14.dp)
                            .testTag("shared-room-join-request-${request.requestId}"),
                    ) {
                        Text(request.requesterName, color = GrokMobileInk, fontWeight = FontWeight.SemiBold)
                        Text(request.roomName, color = GrokMobileMuted, fontSize = 12.sp)
                        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            TextButton(
                                onClick = { runAction { owner.respond(request.requestId, false) } },
                                enabled = !loading,
                                modifier = Modifier.testTag("shared-room-deny-${request.requestId}"),
                            ) { Text("Deny") }
                            TextButton(
                                onClick = { runAction { owner.respond(request.requestId, true) } },
                                enabled = !loading,
                                modifier = Modifier.testTag("shared-room-approve-${request.requestId}"),
                            ) { Text("Approve") }
                        }
                    }
                }
            }

            if (snapshot.enabled && shareableAgents.isNotEmpty()) {
                item(key = "share-agent-heading") {
                    Text("Share an Agent", color = GrokMobileInk, fontWeight = FontWeight.SemiBold)
                }
                items(shareableAgents, key = { "share-agent:${it.id}" }) { bot ->
                    Row(
                        Modifier.fillMaxWidth()
                            .background(androidx.compose.ui.graphics.Color.White, RoundedCornerShape(16.dp))
                            .padding(horizontal = 14.dp, vertical = 10.dp)
                            .testTag("shared-room-share-agent-${bot.id}"),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Column(Modifier.weight(1f)) {
                            Text(bot.name, color = GrokMobileInk, fontWeight = FontWeight.SemiBold)
                            if (bot.description.isNotBlank()) {
                                Text(bot.description, color = GrokMobileMuted, fontSize = 12.sp)
                            }
                        }
                        TextButton(
                            onClick = {
                                runAction {
                                    val result = owner.createRoomFromAgent(
                                        agentId = bot.id,
                                        agentName = bot.name,
                                        avatarDataUrl = bot.avatarDataUrl,
                                    )
                                    check(result.optString("status") == "ok") {
                                        result.optString("message").ifBlank { "Shared room could not be created" }
                                    }
                                    owner.refresh()
                                }
                            },
                            enabled = !loading,
                            modifier = Modifier.testTag("shared-room-share-${bot.id}"),
                        ) { Text("Share") }
                    }
                }
            }

            items(snapshot.rooms, key = { it.id }) { room ->
                Column(
                    Modifier.fillMaxWidth()
                        .background(androidx.compose.ui.graphics.Color.White, RoundedCornerShape(16.dp))
                        .padding(14.dp)
                        .testTag("shared-room-${room.id}"),
                ) {
                    Text(room.name, color = GrokMobileInk, fontWeight = FontWeight.SemiBold)
                    Text(
                        listOfNotNull(
                            room.hostName?.let { "Host: $it" },
                            "${room.memberCount} members",
                        ).joinToString(" · "),
                        color = GrokMobileMuted,
                        fontSize = 12.sp,
                    )
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        TextButton(
                            onClick = {
                                scope.launch {
                                    val result = withContext(Dispatchers.IO) {
                                        runCatching { owner.invite(room.id) }
                                    }
                                    result.onFailure { throwable ->
                                        error = throwable.message ?: "Invite unavailable"
                                    }
                                }
                            },
                            enabled = !loading,
                        ) { Text("Invite") }
                        TextButton(
                            onClick = { runAction { owner.leave(room.id) } },
                            enabled = !loading,
                        ) { Text("Leave") }
                    }
                }
            }
            if (!loading && snapshot.enabled && snapshot.rooms.isEmpty()) {
                item {
                    Text(
                        "No shared rooms yet. Join with an invite link or share an Agent.",
                        color = GrokMobileMuted,
                        modifier = Modifier.fillMaxWidth().padding(20.dp)
                            .testTag("shared-rooms-empty"),
                    )
                }
            }
        }
    }
}
