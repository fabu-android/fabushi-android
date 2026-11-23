package com.ombhrum.fabushi.telegram

import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.ViewModelStore
import com.ombhrum.fabushi.LoopbackTelegramTestServer
import com.ombhrum.fabushi.M3MainDispatcherRule
import com.ombhrum.fabushi.M3TestApplication
import com.ombhrum.fabushi.MockHttpResponse
import com.ombhrum.fabushi.awaitCondition
import com.ombhrum.fabushi.awaitState
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

/**
 * Verifies M3 Features F10 (Real Bots & Coding Agents) and F11 (Real Remote Control,
 * Cloud Computers, UU-Remote Assist, Active Sessions & OAuth 2.1 MCP Grants).
 */
class TelegramRealBotsAndRemoteControlTest {
    @get:Rule
    val mainDispatcherRule = M3MainDispatcherRule()

    @Test
    fun realBotsCodingAgentsComputersAssistSessionsAndMcpGrantsRoundTrip() {
        val app = M3TestApplication()
        val store = ViewModelStore()
        var allowAssist = false
        var assistCode = "Init1234"
        var execForbidden = false

        val server = LoopbackTelegramTestServer(
            httpHandler = { req ->
                when {
                    req.path == "/api/v1/auth/login" -> MockHttpResponse(
                        body = """{"user_id":7,"username":"dev","first_name":"Dev","session_id":"s7","token":"tok_bots_remote_1234567890123"}""",
                    )
                    req.path == "/api/v1/auth/me" -> MockHttpResponse(body = """{"user_id":7,"username":"dev","first_name":"Dev","session_id":"s7"}""")
                    req.path == "/api/v1/chats" -> MockHttpResponse(
                        body = """{"chats":[{"id":301,"title":"Coder Bot","kind":"direct","is_bot":true,"peer_user_id":9001,"last_message_date_ms":2000}],"total":1}""",
                    )
                    req.path == "/api/v1/chats/301/messages" -> MockHttpResponse(
                        body = """{"chat_id":301,"has_more":false,"messages":[{"id":801,"chat_id":301,"sender_id":9001,"sender_name":"Coder Bot","text":"Choose action","date_ms":2000,"inline_keyboard":[[{"text":"Run Check","callback_data":"run:check"}]]}]}""",
                    )
                    req.path == "/api/v1/chats/301/callback" -> MockHttpResponse(
                        body = """{"toast":"Check passed","updated_text":"Check complete!"}""",
                    )
                    req.path == "/api/v1/bots/my" -> MockHttpResponse(body = """{"bots":[],"total":0}""")
                    req.path == "/api/v1/fabushi-bots" && req.method == "GET" -> MockHttpResponse(
                        body = """{"bots":[{"bot_user_id":9001,"agent_id":"ag-1","username":"coder_bot","display_name":"Coder Bot","execution_state":"idle","avatar_shape":"hexagon","avatar_color":"#34C759","companion_chat_id":301}],"total":1}""",
                    )
                    req.path == "/api/v1/fabushi-bots" && req.method == "POST" -> MockHttpResponse(
                        body = """{"bot_user_id":9002,"agent_id":"ag-2","username":"zen_bot","display_name":"Zen Bot","execution_state":"idle","avatar_shape":"star","avatar_color":"#AF52DE","companion_chat_id":302,"bot_token":"srv_tok_9002"}""",
                    )
                    req.path == "/api/v1/fabushi-bots/9002" && req.method == "PATCH" -> MockHttpResponse(
                        body = """{"bot_user_id":9002,"agent_id":"ag-2","username":"zen_bot","display_name":"Zen Bot","execution_state":"idle","avatar_shape":"diamond","avatar_color":"#FF9500","preferred_model":"gpt-5.4","companion_chat_id":302}""",
                    )
                    req.path == "/api/v1/coding-agents/probe" -> MockHttpResponse(
                        body = """{"probes":[{"provider_kind":"claude_code","installed":false},{"provider_kind":"codex","installed":true,"version":"0.42.0","executable_path":"/usr/local/bin/codex"}]}""",
                    )
                    req.path == "/api/v1/bots/9002/computer" && req.method == "PUT" -> MockHttpResponse(
                        body = """{"bound_node_id":"cloud:7:prod-1"}""",
                    )
                    req.path == "/api/v1/computers" && req.method == "GET" -> MockHttpResponse(
                        body = """{"computers":[{"node_id":"cloud:7:prod-1","owner_user_id":7,"kind":"cloud","display_name":"Prod Box","host":"10.0.0.8","ssh_port":22,"os_name":"Ubuntu 24.04","resolution":"1920x1080","online":false,"run_policy":"ask_every_time","has_saved_credential":true}]}""",
                    )
                    req.path == "/api/v1/computers/cloud" && req.method == "POST" -> MockHttpResponse(
                        body = """{"node_id":"cloud:7:prod-2","owner_user_id":7,"kind":"cloud","display_name":"GPU 2","host":"10.0.0.9","ssh_port":22,"os_name":"Debian 12","resolution":"1280x720","online":true,"run_policy":"auto","has_saved_credential":true}""",
                    )
                    req.path == "/api/v1/computers/cloud:7:prod-1/test" -> MockHttpResponse(
                        body = """{"command_id":"t1","node_id":"cloud:7:prod-1","ok":true,"exit_code":0,"stdout":"SSH OK","stderr":"","entries":[],"executed_by":"ssh","duration_ms":8}""",
                    )
                    req.path == "/api/v1/computers/cloud:7:prod-1/policy" && req.method == "PATCH" -> MockHttpResponse(
                        body = """{"node_id":"cloud:7:prod-1","owner_user_id":7,"kind":"cloud","display_name":"Prod Box","os_name":"Ubuntu 24.04","resolution":"1920x1080","online":true,"run_policy":"auto","has_saved_credential":true}""",
                    )
                    req.path == "/api/v1/computers/exec" -> {
                        if (execForbidden) {
                            MockHttpResponse(status = 403, body = """{"error":"disabled","message":"run_policy is disabled"}""")
                        } else {
                            MockHttpResponse(
                                body = """{"command_id":"e1","node_id":"cloud:7:prod-1","ok":true,"exit_code":0,"stdout":"uptime 42d","stderr":"","entries":[],"executed_by":"ssh","duration_ms":15}""",
                            )
                        }
                    }
                    req.path == "/api/v1/remote/assist/info" -> MockHttpResponse(
                        body = """{"assist_id":"987654321","allow_assist":$allowAssist,"code_mode":"custom","current_code":"$assistCode"}""",
                    )
                    req.path == "/api/v1/remote/assist/allow" -> {
                        allowAssist = req.bodyJsonOrNull()?.optBoolean("allow_assist", req.bodyJsonOrNull()?.optBoolean("allowAssist", false) ?: false) ?: false
                        MockHttpResponse(body = """{"assist_id":"987654321","allow_assist":$allowAssist,"code_mode":"custom","current_code":"$assistCode"}""")
                    }
                    req.path == "/api/v1/remote/assist/set-code" -> {
                        assistCode = req.bodyJsonOrNull()?.optString("code", assistCode) ?: assistCode
                        MockHttpResponse(body = """{"assist_id":"987654321","allow_assist":$allowAssist,"code_mode":"custom","current_code":"$assistCode"}""")
                    }
                    req.path == "/api/v1/remote/assist/start" -> MockHttpResponse(
                        body = """{"session_id":"rsess-assist-2","target_node_id":"cloud:7:prod-1","target_device_id":"dev-1","temp_relay_token":"rly_tok_1","requires_host_confirm":false}""",
                    )
                    req.path == "/api/v1/remote/sessions" -> MockHttpResponse(
                        body = """{"sessions":[{"session_id":"rsess-1","target_node_id":"cloud:7:prod-1","controller_kind":"human","controller_name":"Android","relay_mode":"desktop"}]}""",
                    )
                    req.path == "/api/v1/remote/sessions/disconnect" -> MockHttpResponse(body = """{"disconnected":true,"session_id":"rsess-assist-2"}""")
                    req.path == "/api/v1/mcp/grants" -> MockHttpResponse(
                        body = """{"grants":[{"family_id":"fam-srv-1","client_name":"Claude Desktop","scopes":["remote.read"],"allowed_node_ids":["cloud:7:prod-1"]}]}""",
                    )
                    req.path == "/api/v1/mcp/grants/fam-srv-1" && req.method == "DELETE" -> MockHttpResponse(body = """{"revoked":true,"family_id":"fam-srv-1"}""")
                    else -> MockHttpResponse(body = """{"ok":true,"folders":[],"contacts":[],"sessions":[],"pins":[]}""")
                }
            },
        )

        try {
            val vm = ViewModelProvider(store, ViewModelProvider.AndroidViewModelFactory(app))[TelegramViewModel::class.java]
            awaitState(vm) { it.syncStatusLabel != "同步中…" }
            vm.updateDesktopServerEndpoint(server.baseUrl)
            awaitState(vm) { it.serverEndpoint == server.baseUrl }
            vm.loginWithPassword("dev", "pw")
            val authed = awaitState(vm) { it.isAuthenticated && it.fabushiBots.size == 1 && it.cloudComputer.nodes.any { n -> n.id == "cloud:7:prod-1" } }

            // 1. Verify real bot hydration & local-android-mika honest online=false (no MikaApp bridge attached)
            assertEquals("coder_bot", authed.fabushiBots.first().username)
            val localNode = authed.cloudComputer.nodes.firstOrNull { it.id == "local-android-mika" }
            assertTrue(localNode != null && !localNode.online)

            // 2. Create Fabushi bot + PATCH + probe coding agents + bind computer
            vm.createFabushiBot("Zen Bot", "zen_bot", "Zen assistant", avatarShape = "star", avatarColor = "#AF52DE")
            val createdBotState = awaitState(vm) { it.fabushiBots.any { b -> b.botUserId == 9002L } }
            assertEquals("srv_tok_9002", createdBotState.lastCreatedBotToken)

            vm.patchFabushiBot("9002", avatarShape = "diamond", avatarColor = "#FF9500", preferredModel = "gpt-5.4")
            awaitState(vm) { it.fabushiBots.firstOrNull { b -> b.botUserId == 9002L }?.avatarShape == "diamond" }
            assertTrue(server.recordedRequests.any { it.method == "PATCH" && it.path == "/api/v1/fabushi-bots/9002" })

            vm.probeCodingAgents()
            val probed = awaitState(vm) { it.codingAgentProbes.size == 2 }
            assertFalse(probed.codingAgentProbes.first { it.providerKind == CodingProviderKindUi.CLAUDE_CODE }.installed)
            assertTrue(probed.codingAgentProbes.first { it.providerKind == CodingProviderKindUi.CODEX }.installed)

            vm.setBotComputer("9002", "cloud:7:prod-1")
            awaitState(vm) { it.botComputerBindings["9002"] == "cloud:7:prod-1" }

            // 3. Inline callback query applies PeerBotActionResult
            vm.openChat("301")
            val chat301 = awaitState(vm) { it.messagesByChat["301"]?.size == 1 }
            val btn = chat301.messagesByChat["301"]!!.first().inlineKeyboard.first().buttons.first()
            vm.clickInlineButton("301", btn, messageId = "801")
            awaitState(vm) { it.messagesByChat["301"]?.firstOrNull { m -> m.id == "801" }?.text == "Check complete!" }

            // 4. Computers: test, patch policy, exec (success + 403 fail closed), upsert
            vm.testComputer("cloud:7:prod-1")
            awaitState(vm) { it.cloudComputer.nodes.first { n -> n.id == "cloud:7:prod-1" }.online }

            vm.patchComputerPolicy("cloud:7:prod-1", runPolicy = "auto")
            awaitState(vm) { it.cloudComputer.nodes.first { n -> n.id == "cloud:7:prod-1" }.runPolicy == "auto" }

            vm.selectComputerNode("cloud:7:prod-1")
            vm.executeComputerOp("uptime")
            awaitState(vm) { it.cloudComputer.lastOpOutput.contains("uptime 42d") }

            execForbidden = true
            vm.clearToastBanner()
            vm.executeComputerOp("rm -rf /")
            val after403 = awaitState(vm) { it.toastBanner?.contains("远程执行失败") == true }
            assertFalse(after403.cloudComputer.lastOpOutput.contains("执行完成 (0)"))

            vm.upsertCloudComputer("GPU 2", "10.0.0.9")
            awaitState(vm) { it.cloudComputer.nodes.any { n -> n.id == "cloud:7:prod-2" } }

            // 5. UU-Remote Assist: reject invalid code (<8 / non-alnum), accept valid 8..16 alnum, start & disconnect session
            vm.clearToastBanner()
            vm.updateRemoteAssistCode("short7")
            assertTrue(vm.state.value.toastBanner.orEmpty().contains("8–16"))
            vm.toggleRemoteAssist(true)
            awaitState(vm) { it.cloudComputer.assistConfig.allowAssist }
            vm.updateRemoteAssistCode("Abcd1234")
            awaitState(vm) { it.cloudComputer.assistConfig.currentCode == "Abcd1234" }
            vm.startRemoteAssist("987654321", "Abcd1234")
            awaitState(vm) { it.cloudComputer.remoteSessions.any { s -> s.sessionId == "rsess-assist-2" } }
            vm.disconnectRemoteSession("rsess-assist-2")
            awaitState(vm) {
                it.cloudComputer.remoteSessions.none { s -> s.sessionId == "rsess-assist-2" } &&
                    it.toastBanner?.contains("已断开远程会话 rsess-assist-2") == true
            }

            // 6. MCP Grants: issueRemoteMcpGrant fails closed, revokeRemoteMcpGrant deletes server grant
            vm.clearToastBanner()
            vm.issueRemoteMcpGrant("Fake Client")
            awaitState(vm) { it.toastBanner?.contains("OAUTH_FLOW_REQUIRED") == true }
            assertEquals(1, vm.state.value.cloudComputer.mcpGrants.size)

            vm.revokeRemoteMcpGrant("fam-srv-1")
            awaitState(vm) { it.cloudComputer.mcpGrants.isEmpty() }
            awaitCondition { server.recordedRequests.any { it.method == "DELETE" && it.path == "/api/v1/mcp/grants/fam-srv-1" } }
        } finally {
            store.clear()
            server.close()
            app.cleanup()
        }
    }
}
