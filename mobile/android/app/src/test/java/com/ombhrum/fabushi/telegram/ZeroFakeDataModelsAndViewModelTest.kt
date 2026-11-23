package com.ombhrum.fabushi.telegram

import androidx.lifecycle.ViewModelStore
import androidx.lifecycle.ViewModelProvider
import com.ombhrum.fabushi.M3MainDispatcherRule
import com.ombhrum.fabushi.M3TestApplication
import com.ombhrum.fabushi.awaitState
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

/**
 * Verifies M3 Feature F8 (Zero Fake Data & Honest Fail-Closed Defaults/Fallbacks in Kotlin).
 */
class ZeroFakeDataModelsAndViewModelTest {
    @get:Rule
    val mainDispatcherRule = M3MainDispatcherRule()

    @Test
    fun allDefaultModelsHaveTruthfulZeroFakeDefaults() {
        val ui = TelegramUiState()
        assertFalse(ui.isAuthenticated)
        assertTrue(ui.chats.isEmpty())
        assertTrue(ui.accounts.isEmpty())
        assertTrue(ui.stories.isEmpty())
        assertTrue(ui.callHistory.isEmpty())
        assertTrue(ui.fabushiBots.isEmpty())
        assertTrue(ui.codingAgentBots.isEmpty())
        assertTrue(ui.serverSessions.isEmpty())
        assertEquals(DEFAULT_DESKTOP_SERVER_URL, ui.serverEndpoint)

        assertTrue(MagpiePresetCatalog.PRESETS_48.all { !it.configured && !it.healthy })
        assertFalse(MagpieGatewayUiState().liveReady)

        val hermes = HermesStudioUiState()
        assertEquals("", hermes.activeSessionId)
        assertEquals("", hermes.soulMarkdown)
        assertTrue(hermes.memoryBlocks.isEmpty())
        assertTrue(hermes.terminalLines.isEmpty())
        assertEquals("", HermesCronItem("c1", "t", "* * * * *", "p").lastRunStatus)
        assertEquals(0, HermesSessionItem("s1", "t", "openai", "gpt").turnCount)

        val comp = CloudComputerUiState()
        assertEquals("", comp.selectedNodeId)
        assertEquals("", comp.lastOpOutput)
        assertEquals(0, comp.actionBusCategoryCount)
        val node = ComputerNodeItem("n1", "N1", ComputerNodeKind.CLOUD_SERVER, "Linux")
        assertFalse(node.online)
        assertEquals(0, node.cpuLoadPercent)
        assertEquals("", node.memorySummary)

        val acc = TelegramAccount("a1", 0, "Alice", "@alice", "")
        assertFalse(acc.isPremium)
        assertEquals("", acc.statusText)

        val story = TelegramStoryItem("st1", "a1", "Alice", "a1", "cap", "head")
        assertEquals(0, story.viewCount)
        assertEquals(0, story.reactionsCount)

        val msg = TelegramMessageItem("m1", "c1", "u1", "Alice", "hi", outgoing = true, time = "10:00")
        assertEquals("sending", msg.deliveryState)

        val mika = MikaBridgeUiSnapshot()
        assertFalse(mika.available)
        assertFalse(mika.sandboxReady)

        val orb = TalkModeOrbState()
        assertEquals(0, orb.vadLatencyMs)
        assertEquals(0f, orb.rmsLevel)
        assertEquals("", orb.assistantReply)

        val dharma = GlobalDharmaUiState()
        assertTrue(dharma.countries.isEmpty())
        assertTrue(GlobalDharmaCatalog.build240Countries().none { it.nameZh.startsWith("十方洲岛") || it.code.matches(Regex("R\\d+")) })
    }

    @Test
    fun failClosedHonestyForUnsupportedCallsWalletTranslateDharmaPasscodeCacheAndPermissions() {
        val app = M3TestApplication()
        val store = ViewModelStore()
        try {
            // Seed a legacy plaintext PIN key to verify migration removes it on startup
            app.telegramPrefs().edit().putString("passcode_pin", "9999").apply()
            // Create a real file in cacheDir to verify real cache byte measurement & deletion
            val cacheFile = File(app.cacheDir, "sample-media.bin").apply { writeBytes(ByteArray(1024 * 512)) }
            assertTrue(cacheFile.exists())

            val vm = ViewModelProvider(store, ViewModelProvider.AndroidViewModelFactory(app))[TelegramViewModel::class.java]
            val hydrated = awaitState(vm) { it.syncStatusLabel != "同步中…" }
            assertFalse(hydrated.isAuthenticated)
            assertTrue(hydrated.chats.isEmpty())
            assertFalse(app.telegramPrefs().contains("passcode_pin"))
            assertTrue("Expected real cache > 0 MB", hydrated.storageCacheMb > 0.1)

            // 1. Clear cache removes real files and drops storageCacheMb to ~0.0 (NOT 0.4)
            vm.clearLocalMediaCache()
            assertFalse(cacheFile.exists())
            assertEquals(0.0, vm.state.value.storageCacheMb, 0.0001)

            // 2. Argon2id Passcode: zero plaintext PIN in SharedPreferences, never auto-sets 0000
            vm.lockAppNow()
            assertFalse(vm.state.value.isLocked)
            vm.configurePasscode("4321")
            awaitState(vm) { it.passcodeConfigured }
            assertFalse(app.telegramPrefs().contains("passcode_pin"))
            assertFalse(app.telegramPrefs().all.values.any { it.toString().contains("4321") })
            assertTrue(vm.verifyPasscode("4321"))
            assertFalse(vm.verifyPasscode("0000"))

            // 3. startCall fails closed with CALL_TRANSPORT_UNAVAILABLE, stays IDLE
            vm.startCall("101", "Bob", video = true, groupCall = true)
            val afterCall = awaitState(vm) { it.toastBanner?.contains("CALL_TRANSPORT_UNAVAILABLE") == true }
            assertEquals(CallPhase.IDLE, afterCall.callState.phase)
            assertTrue(afterCall.callState.participants.isEmpty())

            // 4. TON swap / stake / TonConnect fail closed with zero fake balance mutation
            vm.swapTonToUsdt(2.0)
            assertEquals(0.0, vm.state.value.tonWallet.usdtBalance, 0.0)
            vm.stakeTon(2.0)
            assertEquals(0.0, vm.state.value.tonWallet.stakedTon, 0.0)
            vm.resolveTonConnect(approve = true)
            awaitState(vm) { it.toastBanner?.contains("TONCONNECT_UNAVAILABLE") == true }

            // 5. Dharma AI fails closed with PROVIDER_NOT_CONFIGURED (zero Heart Sutra fallback)
            vm.clearToastBanner()
            vm.askDharmaAi("如何修心？")
            val afterDharma = awaitState(vm) { it.toastBanner != null }
            assertTrue(afterDharma.globalDharma.aiDharmaMessages.none { !it.first && it.second.contains("应无所住而生其心") })

            // 6. RECORD_AUDIO & Location permission denial guards
            vm.clearToastBanner()
            vm.startVoiceOrVideoRecording()
            assertFalse(vm.state.value.recordingState.isRecording)
            assertTrue(vm.state.value.recordingState.liveBars.isEmpty())
            assertFalse(vm.canSendCurrentLocation())

            // 7. Language preference persists cleanly
            vm.updateLanguageAndTranslate("en-US", showTranslateButton = false, autoTranslate = true)
            assertEquals("en-US", vm.state.value.uiLanguage)
            assertEquals("en-US", app.telegramPrefs().getString("ui_language", null))

            // 8. Translation without configured Magpie key never prepends fake prefix
            vm.createSecretChat("Alice")
            val secretChat = awaitState(vm) { it.chats.isNotEmpty() }.chats.first()
            val secMsg = vm.state.value.messagesByChat[secretChat.id]!!.first()
            vm.clearToastBanner()
            vm.translateMessage(secretChat.id, secMsg.id)
            awaitState(vm) { it.toastBanner != null }
            val updatedMsg = vm.state.value.messagesByChat[secretChat.id]!!.first { it.id == secMsg.id }
            assertNull(updatedMsg.translatedText)
        } finally {
            store.clear()
            app.cleanup()
        }
    }
}
