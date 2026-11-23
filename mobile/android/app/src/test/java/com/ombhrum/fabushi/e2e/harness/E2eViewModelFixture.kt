package com.ombhrum.fabushi.e2e.harness

import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.ViewModelStore
import com.ombhrum.fabushi.telegram.TelegramViewModel
import java.io.Closeable

/**
 * Lifecycle-safe pure-JVM fixture for real [TelegramViewModel] + real JNI [com.ombhrum.fabushi.core.MahayanaHost].
 *
 * Enforces M3 / TEST_INFRA.md invariants:
 * - ALWAYS instantiates `ViewModelProvider.AndroidViewModelFactory(app)` constructor (NEVER `getInstance(app)`!).
 * - Pre-seeds `"server_endpoint"` in [InMemorySharedPreferences] before constructing [TelegramViewModel] when requested
 *   so cold-restart `init -> refreshAllFromHost()` hits the loopback mock server on the exact same port.
 * - Sets `vm.wsBaseBackoffMs` (default `150L`) for fast deterministic WebSocket reconnect verification.
 * - Awaits initial `init -> refreshAllFromHost()` hydration before returning control to the test.
 * - Guarantees `store.clear()` (closing OkHttp WS + native JNI handle) and optional temp dir cleanup in [close].
 */
class E2eViewModelFixture(
    val app: TestFabushiApplication = TestFabushiApplication(),
    preseedEndpoint: String? = null,
    wsBaseBackoffMs: Long = 150L,
    expectAuthenticated: Boolean = false,
    private val deleteRootDirOnClose: Boolean = true,
) : Closeable {
    val store: ViewModelStore = ViewModelStore()
    val vm: TelegramViewModel

    init {
        if (!preseedEndpoint.isNullOrBlank()) {
            app.telegramPrefs().edit().putString("server_endpoint", preseedEndpoint).commit()
        }
        vm = ViewModelProvider(store, ViewModelProvider.AndroidViewModelFactory(app))[TelegramViewModel::class.java]
        vm.wsBaseBackoffMs = wsBaseBackoffMs
        awaitState(
            vm = vm,
            timeoutMs = 6_000L,
            description = "initial ViewModel hydration (expectAuthenticated=$expectAuthenticated)",
        ) { snap ->
            snap.syncStatusLabel != "同步中…" && (!expectAuthenticated || snap.isAuthenticated)
        }
    }

    /**
     * Configures the loopback endpoint and signs in via real `POST /api/v1/auth/login`, awaiting
     * authenticated state and (optionally) WebSocket connection.
     */
    fun loginAndConnect(
        server: MockGpuiTelegramServer,
        username: String = "alice",
        password: String = "password123",
        awaitWs: Boolean = true,
        awaitHydration: Boolean = true,
    ) {
        val grantsCountBefore = server.recordedRequests.count { it.path == "/api/v1/mcp/grants" }
        vm.updateDesktopServerEndpoint(server.baseUrl)
        awaitState(vm, description = "serverEndpoint update") { it.serverEndpoint == server.baseUrl }
        vm.loginWithPassword(username, password)
        awaitState(vm, timeoutMs = 6_000L, description = "login + WS connect") { snap ->
            snap.isAuthenticated && (!awaitWs || snap.serverWsConnected)
        }
        if (awaitHydration) {
            awaitCondition(timeoutMs = 6_000L, description = "post-login refreshAllFromHost HTTP batch") {
                server.recordedRequests.count { it.path == "/api/v1/mcp/grants" } > grantsCountBefore
            }
            awaitState(vm, timeoutMs = 6_000L, description = "post-login syncStatusLabel settled") { snap ->
                snap.syncStatusLabel != "同步中…"
            }
        }
    }

    /**
     * Tears down the current ViewModel + JNI handle without deleting [TestFabushiApplication.rootDir]
     * so a subsequent cold-restarted [E2eViewModelFixture] can reopen the exact same SQLite WAL database.
     */
    fun closeKeepingDataDir() {
        store.clear()
    }

    override fun close() {
        try {
            store.clear()
        } finally {
            if (deleteRootDirOnClose) {
                app.cleanup()
            }
        }
    }
}
