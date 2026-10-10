package com.ombhrum.fabushi

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.json.JSONArray
import org.json.JSONObject

class FrontendProductionModelParityTest {
    @Test
    fun miniAppAndUnknownConversationKindsAreNotProjectedAsChats() {
        assertEquals(ConversationKind.DIRECT, conversationKindFromWire("direct"))
        assertEquals(ConversationKind.GROUP, conversationKindFromWire("group"))
        assertNull(conversationKindFromWire("miniapp"))
        assertNull(conversationKindFromWire("future-kind"))
        assertNull(conversationKindFromWire(""))
    }

    private data class Agent(
        override val id: String,
        override val isPinned: Boolean,
    ) : SidebarOrderAgent

    @Test
    fun sidebarPartitionHonorsStoredOrderThenAppendsPinnedAgents() {
        val agents = listOf(
            Agent("a", true),
            Agent("b", false),
            Agent("c", true),
            Agent("d", true),
        )
        val partition = partitionSidebarAgents(agents, listOf("c", "missing", "c"))
        assertEquals(listOf("c", "a", "d"), partition.pinned.map { it.id })
        assertEquals(listOf("b"), partition.unpinned.map { it.id })
        assertEquals(
            listOf("c", "a", "d"),
            movePinnedAgent(
                storedIds = listOf("a", "c", "d"),
                movedId = "c",
                targetId = "a",
                position = PinnedMovePosition.BEFORE,
            ),
        )
    }

    @Test
    fun agentRowActionsAreDeterministicAndHiddenRowsHaveNoActions() {
        assertTrue(agentRowActions(isHidden = true, includeDelete = true).isEmpty())
        val actions = agentRowActions(
            isHidden = false,
            isPinned = true,
            hasUnread = true,
            includeCopy = true,
            includeDelete = true,
            includeDuplicate = true,
            includeMarkUnread = true,
            includePin = true,
        )
        assertEquals(
            listOf(
                AgentRowActionId.UNPIN_AGENT,
                AgentRowActionId.MARK_READ,
                AgentRowActionId.DUPLICATE_AGENT,
                AgentRowActionId.COPY_CONVERSATION_ID,
                AgentRowActionId.HIDE_FROM_SIDEBAR,
                AgentRowActionId.DELETE_AGENT,
            ),
            actions.map { it.id },
        )
        assertFalse(togglePinValue(actions.first()))
        assertFalse(markAgentUnreadValue(actions[1]))
        assertTrue(isHideFromSidebarAction(actions[4]))
    }

    @Test
    fun renameCommitTrimsRejectsEmptyAndIgnoresUnchangedValue() {
        assertEquals("Renamed", committedAgentName("Old", "  Renamed  "))
        assertNull(committedAgentName("Old", "   "))
        assertNull(committedAgentName("Old", "Old"))
    }

    @Test
    fun agentDeleteCopyDistinguishesSingleAgentAndGroupSemantics() {
        val single = agentDeleteDescription(
            AgentDeleteTarget(id = "agent-1", name = "Agent"),
        )
        val group = agentDeleteDescription(
            AgentDeleteTarget(id = "group-1", name = "Group", isGroup = true),
        )
        assertTrue(single.contains("agent and its chat history"))
        assertTrue(single.contains("can't be undone"))
        assertTrue(group.contains("group and its chat history"))
        assertTrue(group.contains("Bots in it are not deleted"))
    }

    @Test
    fun commandPaletteSearchFiltersAndScoresAgentsAndActions() {
        var activated = ""
        val entries = listOf(
            CommandPaletteEntry(
                id = "agent:a",
                kind = CommandPaletteEntryKind.AGENT,
                label = "Research Agent",
                searchText = "Research Agent analysis",
                activate = { activated = "agent" },
            ),
            CommandPaletteEntry(
                id = "command:settings",
                kind = CommandPaletteEntryKind.COMMAND,
                label = "Chat Settings",
                searchText = "Chat Settings notifications",
                activate = { activated = "settings" },
            ),
        )
        val agentResults = commandPaletteEntries(
            entries,
            CommandPaletteTab.AGENTS,
            "rsrch",
        )
        assertEquals(listOf("agent:a"), agentResults.map { it.id })
        assertTrue(activateCommandPaletteEntry(agentResults, 0))
        assertEquals("agent", activated)
        assertEquals(0, movePaletteHighlight(-1, 1, 1))
    }

    @Test
    fun commandPaletteSeparatesAgentsAndGroupsByShippingIdentity() {
        val agent = CommandPaletteEntry(
            id = "agent:solo",
            kind = CommandPaletteEntryKind.AGENT,
            label = "Solo",
            activate = {},
        )
        val group = CommandPaletteEntry(
            id = "agent:team",
            kind = CommandPaletteEntryKind.GROUP,
            label = "Team",
            activate = {},
        )

        assertEquals(
            listOf("agent:solo"),
            commandPaletteEntries(listOf(agent, group), CommandPaletteTab.AGENTS, "").map { it.id },
        )
        assertEquals(
            listOf("agent:team"),
            commandPaletteEntries(listOf(agent, group), CommandPaletteTab.GROUPS, "").map { it.id },
        )
        assertEquals(
            listOf("agent:team"),
            commandPaletteEntries(listOf(agent, group), CommandPaletteTab.GROUPS, "team").map { it.id },
        )
    }

    @Test
    fun commandPaletteCanonicalIdentityReplacesStaleRowsWithoutDuplicates() {
        val stale = CommandPaletteEntry(
            id = "human:ada",
            kind = CommandPaletteEntryKind.AGENT,
            label = "Ada (stale)",
            detail = "Old row",
            activate = {},
        )
        val other = CommandPaletteEntry(
            id = "human:grace",
            kind = CommandPaletteEntryKind.AGENT,
            label = "Grace",
            activate = {},
        )
        val current = CommandPaletteEntry(
            id = "human:ada",
            kind = CommandPaletteEntryKind.AGENT,
            label = "Ada",
            detail = "Current row",
            activate = {},
        )
        val canonical = dedupeCommandPaletteEntries(listOf(stale, other, current))
        assertEquals(listOf("human:ada", "human:grace"), canonical.map { it.id })
        assertEquals("Ada", canonical.first().label)
        assertEquals("Current row", canonical.first().detail)

        val results = commandPaletteEntries(
            listOf(stale, other, current),
            CommandPaletteTab.AGENTS,
            "ada",
        )
        assertEquals(1, results.count { it.id == "human:ada" })
        assertEquals("Ada", results.single { it.id == "human:ada" }.label)
    }

    @Test
    fun commandPaletteRootCommandsAreCurrentChatScoped() {
        val opened = mutableListOf<CommandPaletteInfoSection>()
        assertTrue(
            commandPaletteRootCommands(
                activeAgentIsGroup = null,
                activeAgentIsSharedRoom = false,
                hasChannels = true,
                openInfoSection = opened::add,
            ).isEmpty(),
        )

        val commands = commandPaletteRootCommands(
            activeAgentIsGroup = true,
            activeAgentIsSharedRoom = false,
            hasChannels = true,
            openInfoSection = opened::add,
        )
        assertEquals(
            listOf("info:members", "info:channels", "info:settings"),
            commands.map { it.id },
        )
        commands.first().activate()
        commands.last().activate()
        assertEquals(
            listOf(CommandPaletteInfoSection.MEMBERS, CommandPaletteInfoSection.SETTINGS),
            opened,
        )

        var requestedComputerAction: CommandPaletteComputerUpdateAction? = null
        val withComputerUpdate = commandPaletteRootCommands(
            activeAgentIsGroup = false,
            activeAgentIsSharedRoom = false,
            hasChannels = false,
            openInfoSection = opened::add,
            computerUpdateAction = CommandPaletteComputerUpdateAction.READY,
            openComputerUpdateConfirm = { requestedComputerAction = it },
        )
        val computer = withComputerUpdate.single { it.id == "update:computer" }
        assertEquals("Update Fabushi's Computer", computer.label)
        assertFalse(computer.label.contains("Grok", ignoreCase = true))
        computer.activate()
        assertEquals(CommandPaletteComputerUpdateAction.READY, requestedComputerAction)
    }

    @Test
    fun commandPaletteUpdateCommandUsesAndroidUpdaterStateMachine() {
        var checks = 0
        var installs = 0
        var opens = 0
        assertNull(
            commandPaletteUpdateCommand(
                state = AndroidUpdateUiState(
                    phase = AndroidUpdatePhase.DISABLED,
                    currentVersion = "1.0.0",
                ),
                check = { checks += 1 },
                install = { installs += 1 },
                openUpdates = { opens += 1 },
            ),
        )

        val available = commandPaletteUpdateCommand(
            state = AndroidUpdateUiState(
                phase = AndroidUpdatePhase.AVAILABLE,
                currentVersion = "1.0.0",
                availableVersion = "1.1.0",
            ),
            check = { checks += 1 },
            install = { installs += 1 },
            openUpdates = { opens += 1 },
        )
        assertEquals("Download Update…", available?.label)
        available?.activate?.invoke()
        assertEquals(1, installs)

        val error = commandPaletteUpdateCommand(
            state = AndroidUpdateUiState(
                phase = AndroidUpdatePhase.ERROR,
                currentVersion = "1.0.0",
            ),
            check = { checks += 1 },
            install = { installs += 1 },
            openUpdates = { opens += 1 },
        )
        error?.activate?.invoke()
        assertEquals(1, checks)
        assertEquals(1, opens)
    }

    @Test
    fun conversationReplyPreviewNormalizesAndBoundsMessageContent() {
        val message = ChatMessage(
            id = "m1",
            conversationId = "c1",
            text = "  hello    world  ",
            outgoing = false,
            time = "now",
        )
        assertEquals("hello world", replyPreviewLabel(message))
        assertEquals("回复…", replyComposerPlaceholder(message))
        assertEquals(
            "回复附件…",
            replyComposerPlaceholder(
                message.copy(contentType = "photo", mediaFileName = "photo.jpg"),
            ),
        )
        val long = message.copy(text = "x".repeat(200))
        assertTrue(replyPreviewLabel(long).endsWith("…"))
        assertTrue(replyPreviewLabel(long).length <= 96)
    }

    @Test
    fun coordinatorDraftAdapterUsesCanonicalSharedDraftAndAvoidsDuplicateWrites() {
        val shared = MessagingDraft(
            conversationId = "c1",
            text = "hello",
            replyToMessageId = "m1",
            updatedAtMs = 7,
        )
        val snapshot = coordinatorComposerDraftSnapshot("c1", shared)
        assertEquals("hello", snapshot.text)
        assertEquals("m1", snapshot.replyToMessageId)
        assertFalse(
            shouldPersistCoordinatorDraft(
                snapshot,
                text = "hello",
                replyToMessageId = "m1",
            ),
        )
        assertTrue(
            shouldPersistCoordinatorDraft(
                snapshot,
                text = "hello!",
                replyToMessageId = "m1",
            ),
        )
        assertTrue(composerDraftIsEmpty("", null))
        assertFalse(composerDraftIsEmpty("", "m1"))
    }

    @Test
    fun findInChatSearchesVisiblePayloadAndWrapsNavigation() {
        val messages = listOf(
            ChatMessage(
                id = "m1",
                conversationId = "c1",
                text = "alpha beta alpha",
                outgoing = false,
                time = "now",
            ),
            ChatMessage(
                id = "m2",
                conversationId = "c1",
                text = "",
                contentType = "poll",
                pollQuestion = "Choose Gamma",
                pollOptions = listOf(ChatPollOption("g", "Gamma", 0, false)),
                outgoing = false,
                time = "now",
            ),
        )
        val alpha = findInChatMatches(messages, "alpha")
        assertEquals(2, alpha.size)
        assertEquals(listOf("m1", "m1"), alpha.map { it.messageId })
        assertEquals("m2", findInChatMatches(messages, "gamma").single().messageId)
        assertEquals(1, stepFindInChatIndex(0, -1, 2))
        assertEquals(0, stepFindInChatIndex(1, 1, 2))
        assertEquals(-1, stepFindInChatIndex(0, 1, 0))
    }

    @Test
    fun permissionScopeRequiresStrictlyNewRevisionAfterAccountReentry() {
        val gate = LocalToolPermissionScopeGate()
        gate.enter("account-a")
        assertTrue(gate.accepts("account-a", 4))
        assertTrue(gate.accepts("account-a", 4))
        assertFalse(gate.accepts("account-a", 3))

        gate.enter(null)
        gate.enter("account-a")
        assertFalse(gate.accepts("account-a", 4))
        assertTrue(gate.accepts("account-a", 5))

        gate.dispose()
        assertFalse(gate.accepts("account-a", 6))
    }

    @Test
    fun disposalGuardDefersTerminalDisposeAndReplacementDisposesImmediately() {
        val queue = ArrayDeque<() -> Unit>()
        var firstDisposed = 0
        var secondDisposed = 0
        val guard = StrictModeDisposalGuard(defer = queue::addLast)
        val first = StrictModeDisposable { firstDisposed += 1 }
        val second = StrictModeDisposable { secondDisposed += 1 }

        val cleanupFirst = guard.attach(first)
        cleanupFirst()
        guard.attach(second)
        assertEquals(1, firstDisposed)

        while (queue.isNotEmpty()) queue.removeFirst().invoke()
        assertEquals(1, firstDisposed)
        assertEquals(0, secondDisposed)

        val cleanupSecond = guard.attach(second)
        cleanupSecond()
        while (queue.isNotEmpty()) queue.removeFirst().invoke()
        assertEquals(1, secondDisposed)
    }
    @Test
    fun deepLinkInfoModelUsesFabushiRouteAndSourceLabel() {
        val info = DeepLinkInfo(source = DeepLinkSource.PROTOCOL)
        assertEquals("fabushi://app/v1/info?topic=deep-links", deepLinkRoute(info))
        assertEquals(
            "Custom protocol (fabushi://)",
            deepLinkSourceLabel(DeepLinkSource.PROTOCOL),
        )
        assertEquals("HTTPS link", deepLinkSourceLabel(DeepLinkSource.HTTPS))
    }

    @Test
    fun canonicalAgentTranscriptIsIdentityFencedAndFailClosed() {
        val transcript = JSONArray()
            .put(
                JSONObject()
                    .put("kind", "message")
                    .put("id", "a-user")
                    .put("agentId", "agent-a")
                    .put("role", "user")
                    .put("content", "alpha question")
                    .put("timestampMs", 100L),
            )
            .put(
                JSONObject()
                    .put("kind", "message")
                    .put("id", "b-assistant")
                    .put("agentId", "agent-b")
                    .put("role", "assistant")
                    .put("content", "beta answer")
                    .put("timestampMs", 200L),
            )
            .put(
                JSONObject()
                    .put("kind", "message")
                    .put("id", "legacy-without-owner")
                    .put("role", "assistant")
                    .put("content", "must not leak"),
            )

        assertEquals(
            listOf("a-user"),
            canonicalMobileTranscriptForAgent(transcript, "agent-a").map { it.id },
        )
        assertEquals(
            listOf("b-assistant"),
            canonicalMobileTranscriptForAgent(transcript, "agent-b").map { it.id },
        )
        assertFalse(
            canonicalAgentTranscriptMessages(transcript)
                .any { it.id == "legacy-without-owner" },
        )
    }

    @Test
    fun canonicalTranscriptLateSnapshotPreservesOnlyMessagesArrivingDuringLoad() {
        val baseline = listOf(
            MobileChatMessage("old-local", MobileChatRole.USER, "old"),
        )
        val current = baseline + MobileChatMessage(
            "live-during-load",
            MobileChatRole.ASSISTANT,
            "new live event",
        )
        val canonical = listOf(
            MobileChatMessage("server-old", MobileChatRole.USER, "server"),
        )

        val merged = mergeCanonicalMobileTranscript(
            baselineEntryIds = baseline.mapTo(linkedSetOf(), MobileChatMessage::id),
            current = current,
            canonical = canonical,
        )
        assertEquals(
            listOf("server-old", "live-during-load"),
            merged.map { it.id },
        )
    }

    @Test
    fun commandPaletteMessageSearchCarriesAgentAndStableEntryIdentity() {
        val transcript = JSONArray()
            .put(
                JSONObject()
                    .put("kind", "message")
                    .put("id", "entry-1")
                    .put("agentId", "agent-a")
                    .put("role", "assistant")
                    .put("content", "Quarterly launch checklist ready")
                    .put("timestampMs", 1_000L),
            )
            .put(
                JSONObject()
                    .put("kind", "message")
                    .put("id", "entry-2")
                    .put("agentId", "agent-b")
                    .put("role", "assistant")
                    .put("content", "Unrelated note")
                    .put("timestampMs", 2_000L),
            )

        val results = commandPaletteMessagesFromTranscript(
            transcript = transcript,
            query = "launch checklist",
        )
        assertEquals(1, results.size)
        assertEquals("agent-a", results.single().agentId)
        assertEquals("entry-1", results.single().entryId)

        var opened: CommandPaletteMessage? = null
        val entries = commandPaletteMessageEntries(
            messages = results,
            agentNames = mapOf("agent-a" to "Research"),
            nowMs = 61_000L,
            onOpen = { opened = it },
        )
        assertEquals(listOf("message:agent-a:entry-1"), entries.map { it.id })
        assertTrue(activateCommandPaletteEntry(entries, 0))
        assertEquals("entry-1", opened?.entryId)
        assertEquals("agent-a", opened?.agentId)
    }

    @Test
    fun commandPaletteMessageRequestFenceRejectsSupersededResults() {
        val fence = CommandPaletteMessageRequestFence()
        val first = fence.begin()
        val second = fence.begin()
        assertFalse(fence.accepts(first))
        assertTrue(fence.accepts(second))
        fence.cancel()
        assertFalse(fence.accepts(second))
    }


    @Test
    fun commandPaletteRoutinesRequireCanonicalAgentOwnerAndKeepStableIdentity() {
        val raw = JSONArray()
            .put(
                JSONObject()
                    .put("id", "daily")
                    .put("name", "Daily brief")
                    .put("agent_id", "agent-a")
                    .put("schedule", "0 8 * * *")
                    .put("trigger_description", "Every day at 8:00 AM")
                    .put("created_at_ms", 10L)
                    .put("last_run_at_ms", 20L),
            )
            .put(
                JSONObject()
                    .put("id", "ownerless")
                    .put("name", "Legacy ownerless")
                    .put("schedule", "0 9 * * *")
                    .put("created_at_ms", 11L),
            )

        val routines = commandPaletteRoutinesFromAutomationList(raw)
        assertEquals(1, routines.size)
        assertEquals("agent-a", routines.single().agentId)
        assertEquals("daily", routines.single().automationId)

        var opened: String? = null
        val entries = commandPaletteRoutineEntries(
            routines = routines,
            agentNames = mapOf("agent-a" to "Research"),
            onOpenAgent = { opened = it },
        )
        assertEquals(listOf("routine:agent-a:daily"), entries.map { it.id })
        assertTrue(activateCommandPaletteEntry(entries, 0))
        assertEquals("agent-a", opened)
    }

    @Test
    fun commandPaletteRoutineFenceRejectsLateRefresh() {
        val fence = CommandPaletteRoutineRequestFence()
        val first = fence.begin()
        val second = fence.begin()
        assertFalse(fence.accepts(first))
        assertTrue(fence.accepts(second))
        fence.cancel()
        assertFalse(fence.accepts(second))
    }

}
