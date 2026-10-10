package com.ombhrum.fabushi

import androidx.compose.foundation.layout.Box
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag

/**
 * Android/Compose counterpart of Grok's production AgentRowActions.
 *
 * Desktop context-menu affordances become an explicit overflow control on Android while preserving
 * action availability and semantics. The renderer owns only menu state; mutations are delegated.
 */
@Composable
internal fun AgentRowActions(
    agentId: String,
    agentName: String,
    isGroup: Boolean = false,
    isPinned: Boolean = false,
    hasUnread: Boolean = false,
    isHidden: Boolean = false,
    onEditName: ((String) -> Unit)? = null,
    onShowFullConversation: ((String) -> Unit)? = null,
    onHideFromSidebar: (String) -> Unit,
    onCopyConversationId: ((String) -> Unit)? = null,
    onDuplicateAgent: ((String) -> Unit)? = null,
    onTogglePin: ((String, Boolean) -> Unit)? = null,
    onSetAgentUnread: ((String, Boolean) -> Unit)? = null,
    onRequestDelete: ((AgentDeleteTarget) -> Unit)? = null,
    modifier: Modifier = Modifier,
) {
    var expanded by remember(agentId) { mutableStateOf(false) }
    val actions = agentRowActions(
        isHidden = isHidden,
        isPinned = isPinned,
        hasUnread = hasUnread,
        includeCopy = onCopyConversationId != null,
        includeDelete = onRequestDelete != null,
        includeDuplicate = onDuplicateAgent != null,
        includeMarkUnread = onSetAgentUnread != null,
        includePin = onTogglePin != null,
    )

    if (isHidden || (actions.isEmpty() && onEditName == null && onShowFullConversation == null)) return

    Box(modifier) {
        TextButton(
            onClick = { expanded = true },
            modifier = Modifier.testTag("agent-row-actions-$agentId"),
        ) {
            Text("⋮")
        }
        DropdownMenu(
            expanded = expanded,
            onDismissRequest = { expanded = false },
        ) {
            if (onEditName != null) {
                DropdownMenuItem(
                    text = { Text("Rename") },
                    onClick = {
                        expanded = false
                        onEditName(agentId)
                    },
                )
            }
            if (onShowFullConversation != null) {
                DropdownMenuItem(
                    text = { Text("Show full conversation") },
                    onClick = {
                        expanded = false
                        onShowFullConversation(agentId)
                    },
                )
            }
            actions.forEach { action ->
                DropdownMenuItem(
                    text = { Text(action.label) },
                    onClick = {
                        expanded = false
                        when {
                            isTogglePinAction(action) ->
                                onTogglePin?.invoke(agentId, togglePinValue(action))
                            isDuplicateAgentAction(action) ->
                                onDuplicateAgent?.invoke(agentId)
                            isCopyConversationIdAction(action) ->
                                onCopyConversationId?.invoke(agentId)
                            isMarkAgentUnreadAction(action) ->
                                onSetAgentUnread?.invoke(agentId, markAgentUnreadValue(action))
                            isHideFromSidebarAction(action) ->
                                onHideFromSidebar(agentId)
                            isDeleteAgentAction(action) ->
                                onRequestDelete?.invoke(
                                    AgentDeleteTarget(
                                        id = agentId,
                                        name = agentName,
                                        isGroup = isGroup,
                                    ),
                                )
                        }
                    },
                )
            }
        }
    }
}
