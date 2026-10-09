package com.ombhrum.fabushi

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertTextContains
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTextInput
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test

class FabushiScreenTest {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun homeMatchesConversationReferenceAndSearchesMessages() {
        compose.setContent {
            FabushiMessagingSurface(
                state = MarketplaceUiState(),
                onQueryChange = {},
                onSearch = {},
                onInstall = {},
                onOpen = {},
                onApprovePermissions = {},
                onDenyPermissions = {},
                onSubmitPluginVariables = {},
                onCancelPluginVariables = {},
            )
        }

        compose.onNodeWithTag(TestTags.AppShell).assertIsDisplayed()
        compose.onNodeWithTag(TestTags.ProfileAvatar).assertIsDisplayed()
        compose.onNodeWithTag(TestTags.HomeSearchButton).assertIsDisplayed()
        compose.onNodeWithTag(TestTags.AddButton).assertIsDisplayed()
        compose.onNodeWithTag(TestTags.ConversationList).assertIsDisplayed()
        assertEquals(0, compose.onAllNodesWithTag(TestTags.ConversationRow).fetchSemanticsNodes().size)
        compose.onNodeWithTag(TestTags.AddButton).performClick()
        compose.onNodeWithText("新消息").assertIsDisplayed().performClick()
        compose.onNodeWithText("暂无可用联系人").assertIsDisplayed()
        assertEquals(0, compose.onAllNodesWithText("Chief of Staff").fetchSemanticsNodes().size)
    }

    @Test
    fun signedOutMobileSurfaceUsesSingleNativeLoginAction() {
        var loginRequests = 0
        compose.setContent {
            FabushiMessagingSurface(
                state = MarketplaceUiState(
                    authResolved = true,
                    loggedIn = false,
                    onboardingStep = 3,
                ),
                onQueryChange = {},
                onSearch = {},
                onInstall = {},
                onOpen = {},
                onApprovePermissions = {},
                onDenyPermissions = {},
                onSubmitPluginVariables = {},
                onCancelPluginVariables = {},
                authGateEnabled = true,
                onBeginBrowserLogin = { loginRequests += 1 },
            )
        }

        compose.onNodeWithTag(TestTags.MobileLogin).assertIsDisplayed()
        compose.onNodeWithText("Fabushi").assertIsDisplayed()
        compose.onNodeWithText("你的常驻智能体团队，持续完成工作。").assertIsDisplayed()
        compose.onNodeWithTag(TestTags.MobileLoginBrowser).assertIsDisplayed().performClick()
        assertEquals(1, loginRequests)
        assertEquals(0, compose.onAllNodesWithText("账号状态加载失败").fetchSemanticsNodes().size)
    }

    @Test
    fun addMenuOpensMarketplaceAndKeepsMarketplaceCallbacks() {
        var query by mutableStateOf("")
        var searches = 0
        var installed: MarketplacePlugin? = null
        var opened: MarketplacePlugin? = null
        val plugin = MarketplacePlugin("example-plugin", "示例插件", "描述", "1.0.0")
        compose.setContent {
            FabushiMessagingSurface(
                state = MarketplaceUiState(message = "ready", query = query, plugins = listOf(plugin)),
                onQueryChange = { query = it },
                onSearch = { searches += 1 },
                onInstall = { installed = it },
                onOpen = { opened = it },
                onApprovePermissions = {},
                onDenyPermissions = {},
                onSubmitPluginVariables = {},
                onCancelPluginVariables = {},
            )
        }

        compose.onNodeWithTag(TestTags.ProfileAvatar).performClick()
        compose.onNodeWithTag(TestTags.MarketplaceEntry).assertIsDisplayed().performClick()
        compose.onNodeWithTag(TestTags.RuntimeBadge).assertTextContains("Compose", substring = true)
        compose.onNodeWithTag(TestTags.HostStatus).assertIsDisplayed()
        compose.onNodeWithText("ready").assertIsDisplayed()
        compose.onNodeWithTag(TestTags.SearchField).performTextInput("telegram")
        compose.onNodeWithTag(TestTags.SearchButton).performClick()
        assertEquals("telegram", query)
        assertEquals(1, searches)

        compose.onNodeWithTag(TestTags.plugin(plugin.pluginId)).assertIsDisplayed()
        compose.onNodeWithTag(TestTags.open(plugin.pluginId)).assertIsDisplayed().performClick()
        assertEquals(plugin, opened)
        compose.onNodeWithTag(TestTags.install(plugin.pluginId)).assertIsDisplayed().performClick()
        assertEquals(plugin, installed)
    }

    @Test
    fun addMenuOpensAndClosesRestrictedRemoteComputerSurface() {
        compose.setContent {
            FabushiMessagingSurface(
                state = MarketplaceUiState(),
                onQueryChange = {},
                onSearch = {},
                onInstall = {},
                onOpen = {},
                onApprovePermissions = {},
                onDenyPermissions = {},
                onSubmitPluginVariables = {},
                onCancelPluginVariables = {},
            )
        }

        compose.onNodeWithTag(TestTags.ProfileAvatar).performClick()
        compose.onNodeWithTag(TestTags.RemoteComputerEntry).assertIsDisplayed().performClick()
        compose.onNodeWithTag(TestTags.RemoteComputerSurface).assertIsDisplayed()
        compose.onNodeWithTag(TestTags.RemoteComputerClose).assertIsDisplayed().performClick()
        compose.onNodeWithTag(TestTags.AppShell).assertIsDisplayed()
    }

    @Test
    fun availableUpdateAppearsOnHomeAndStartsInstall() {
        var installRequests = 0
        compose.setContent {
            FabushiMessagingSurface(
                state = MarketplaceUiState(),
                onQueryChange = {},
                onSearch = {},
                onInstall = {},
                onOpen = {},
                onApprovePermissions = {},
                onDenyPermissions = {},
                onSubmitPluginVariables = {},
                onCancelPluginVariables = {},
                updateState = AndroidUpdateUiState(
                    phase = AndroidUpdatePhase.AVAILABLE,
                    currentVersion = "1.0.4",
                    availableVersion = "1.0.5",
                    availableVersionCode = 3,
                ),
                onInstallUpdate = { installRequests += 1 },
            )
        }

        compose.onNodeWithTag(TestTags.UpdateCard).assertIsDisplayed()
        compose.onNodeWithText("发现新版本 1.0.5").assertIsDisplayed()
        compose.onNodeWithTag(TestTags.UpdateAction).performClick()
        assertEquals(1, installRequests)
    }

    @Test
    fun permissionDialogHasStableApproveAndDenyControls() {
        compose.setContent {
            FabushiMessagingSurface(
                state = MarketplaceUiState(
                    permissionRequest = PermissionRequest(
                        pluginId = "example-plugin",
                        runtime = "deepseek-js",
                        permissions = listOf("network.request"),
                    ),
                ),
                onQueryChange = {},
                onSearch = {},
                onInstall = {},
                onOpen = {},
                onApprovePermissions = {},
                onDenyPermissions = {},
                onSubmitPluginVariables = {},
                onCancelPluginVariables = {},
            )
        }

        compose.onNodeWithTag(TestTags.PermissionDialog).assertIsDisplayed()
        compose.onNodeWithTag(TestTags.PermissionApprove).assertIsDisplayed()
        compose.onNodeWithTag(TestTags.PermissionDeny).assertIsDisplayed()
    }
    @Test
    fun appAgentSurfaceNavigatesWithStableSemanticIdsWithoutScreenshotCoordinates() {
        val surface = FabushiAppAgentSurface()
        compose.setContent {
            FabushiMessagingSurface(
                state = MarketplaceUiState(message = "ready"),
                onQueryChange = {},
                onSearch = {},
                onInstall = {},
                onOpen = {},
                onApprovePermissions = {},
                onDenyPermissions = {},
                onSubmitPluginVariables = {},
                onCancelPluginVariables = {},
                appAgentSurface = surface,
            )
        }
        compose.waitForIdle()
        var snapshot = surface.snapshot()
        assertEquals("home", snapshot.screen)
        assertEquals(TestTags.ProfileAvatar, surface.find(agentId = TestTags.ProfileAvatar).single().agentId)

        compose.runOnIdle {
            surface.action(snapshot.generation, TestTags.ProfileAvatar, "invoke")
        }
        compose.waitForIdle()
        snapshot = surface.snapshot()
        assertEquals(TestTags.MarketplaceEntry, surface.find(agentId = TestTags.MarketplaceEntry).single().agentId)

        compose.runOnIdle {
            surface.action(snapshot.generation, TestTags.MarketplaceEntry, "invoke")
        }
        compose.onNodeWithTag(TestTags.RuntimeBadge).assertIsDisplayed()
        compose.waitForIdle()
        assertEquals("marketplace", surface.snapshot().screen)
    }

    @Test
    fun conversationTranscriptRendersMessageAndPollChoices() {
        val poll = ChatMessage(
            id = "poll-1",
            conversationId = "c1",
            text = "",
            contentType = "poll",
            pollQuestion = "Choose",
            pollOptions = listOf(
                ChatPollOption("a", "A", 2, false),
                ChatPollOption("b", "B", 1, true),
            ),
            outgoing = false,
            time = "now",
        )
        compose.setContent {
            ConversationTranscript(
                conversationTitle = "Room",
                messages = listOf(
                    ChatMessage(
                        id = "m1",
                        conversationId = "c1",
                        text = "hello transcript",
                        outgoing = false,
                        time = "now",
                    ),
                    poll,
                ),
                searchQuery = "",
                playingVoiceMessageId = null,
                onPlayVoice = {},
                onOpenMedia = {},
                onVotePoll = { _, _ -> },
                onSelectMessage = {},
                onReply = {},
            )
        }

        compose.onNodeWithText("hello transcript").assertIsDisplayed()
        compose.onNodeWithText("Choose").assertIsDisplayed()
        compose.onNodeWithText("A").assertIsDisplayed()
        compose.onNodeWithText("B").assertIsDisplayed()
    }

    @Test
    fun conversationComposerSendsDraftAndRoutesAttachmentPicker() {
        var draft by mutableStateOf("")
        var sent: Pair<String, String?>? = null
        var selectedMime: String? = null
        compose.setContent {
            ConversationComposer(
                draft = draft,
                editingMessage = null,
                replyTarget = null,
                isRecordingVoice = false,
                recordingSeconds = 0,
                voiceError = null,
                onDraftChange = { draft = it },
                onTypingChanged = {},
                onClearContext = {},
                onPickAttachment = { selectedMime = it },
                onRequestLocation = {},
                onRequestContact = {},
                onRequestPoll = {},
                onCancelRecording = {},
                onFinishRecording = {},
                onStartRecording = {},
                onEdit = { _, _ -> },
                onSend = { text, replyTo -> sent = text to replyTo },
                onRequestSendModes = {},
            )
        }

        compose.onNodeWithTag("conversation-composer").performTextInput("hello")
        compose.onNodeWithTag("conversation-send").assertIsDisplayed().performClick()
        assertEquals("hello" to null, sent)

        compose.onNodeWithTag("conversation-attach").performClick()
        compose.onNodeWithText("照片").assertIsDisplayed().performClick()
        assertEquals("image/*", selectedMime)
    }

}
