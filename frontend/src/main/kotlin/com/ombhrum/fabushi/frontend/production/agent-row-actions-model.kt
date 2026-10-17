package com.ombhrum.fabushi

internal enum class AgentRowActionId {
    PIN_AGENT,
    UNPIN_AGENT,
    HIDE_FROM_SIDEBAR,
    DELETE_AGENT,
    COPY_CONVERSATION_ID,
    DUPLICATE_AGENT,
    MARK_READ,
    MARK_UNREAD,
}

internal data class AgentRowAction(
    val id: AgentRowActionId,
    val label: String,
)

internal const val AGENT_ROW_ACTIONS_LABEL = "Agent actions"

internal fun agentRowActions(
    isHidden: Boolean,
    isPinned: Boolean = false,
    hasUnread: Boolean = false,
    includeCopy: Boolean = false,
    includeDelete: Boolean = false,
    includeDuplicate: Boolean = false,
    includeMarkUnread: Boolean = false,
    includePin: Boolean = false,
): List<AgentRowAction> {
    if (isHidden) return emptyList()
    return buildList {
        if (includePin) {
            add(
                if (isPinned) AgentRowAction(AgentRowActionId.UNPIN_AGENT, "Unpin")
                else AgentRowAction(AgentRowActionId.PIN_AGENT, "Pin"),
            )
        }
        if (includeMarkUnread) {
            add(
                if (hasUnread) AgentRowAction(AgentRowActionId.MARK_READ, "Mark as Read")
                else AgentRowAction(AgentRowActionId.MARK_UNREAD, "Mark as Unread"),
            )
        }
        if (includeDuplicate) add(AgentRowAction(AgentRowActionId.DUPLICATE_AGENT, "Duplicate"))
        if (includeCopy) add(AgentRowAction(AgentRowActionId.COPY_CONVERSATION_ID, "Copy conversation ID"))
        add(AgentRowAction(AgentRowActionId.HIDE_FROM_SIDEBAR, "Hide from sidebar"))
        if (includeDelete) add(AgentRowAction(AgentRowActionId.DELETE_AGENT, "Delete"))
    }
}

internal fun isHideFromSidebarAction(action: AgentRowAction) =
    action.id == AgentRowActionId.HIDE_FROM_SIDEBAR

internal fun isTogglePinAction(action: AgentRowAction) =
    action.id == AgentRowActionId.PIN_AGENT || action.id == AgentRowActionId.UNPIN_AGENT

internal fun togglePinValue(action: AgentRowAction) =
    action.id == AgentRowActionId.PIN_AGENT

internal fun isDeleteAgentAction(action: AgentRowAction) =
    action.id == AgentRowActionId.DELETE_AGENT

internal fun isCopyConversationIdAction(action: AgentRowAction) =
    action.id == AgentRowActionId.COPY_CONVERSATION_ID

internal fun isDuplicateAgentAction(action: AgentRowAction) =
    action.id == AgentRowActionId.DUPLICATE_AGENT

internal fun isMarkAgentUnreadAction(action: AgentRowAction) =
    action.id == AgentRowActionId.MARK_READ || action.id == AgentRowActionId.MARK_UNREAD

internal fun markAgentUnreadValue(action: AgentRowAction) =
    action.id == AgentRowActionId.MARK_UNREAD
