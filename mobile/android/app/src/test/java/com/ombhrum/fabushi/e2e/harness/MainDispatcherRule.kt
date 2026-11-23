@file:OptIn(kotlinx.coroutines.ExperimentalCoroutinesApi::class)

package com.ombhrum.fabushi.e2e.harness

import com.ombhrum.fabushi.telegram.TelegramUiState
import com.ombhrum.fabushi.telegram.TelegramViewModel
import java.util.concurrent.ExecutorService
import java.util.concurrent.Executors
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.asCoroutineDispatcher
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.setMain
import org.junit.rules.TestWatcher
import org.junit.runner.Description

/**
 * JUnit [TestWatcher] wiring [Dispatchers.Main] to a dedicated real single-thread executor so
 * [TelegramViewModel], [com.ombhrum.fabushi.FabushiRemoteDeviceGateway], real JNI `Dispatchers.IO`,
 * and OkHttp WebSocket threads interact without virtual-time deadlocks or `Looper.getMainLooper()` crashes.
 */
class MainDispatcherRule : TestWatcher() {
    private var executor: ExecutorService? = null

    override fun starting(description: Description) {
        val exec = Executors.newSingleThreadExecutor { r ->
            Thread(r, "fabushi-e2e-main").apply { isDaemon = true }
        }
        executor = exec
        Dispatchers.setMain(exec.asCoroutineDispatcher())
    }

    override fun finished(description: Description) {
        Dispatchers.resetMain()
        executor?.shutdownNow()
        executor = null
    }
}

/**
 * Deterministic polling helper for [TelegramViewModel.state].
 */
fun awaitState(
    vm: TelegramViewModel,
    timeoutMs: Long = 5_000L,
    pollMs: Long = 20L,
    description: String = "TelegramUiState predicate",
    predicate: (TelegramUiState) -> Boolean,
): TelegramUiState {
    val deadline = System.currentTimeMillis() + timeoutMs
    while (System.currentTimeMillis() < deadline) {
        val snap = vm.state.value
        if (predicate(snap)) return snap
        Thread.sleep(pollMs)
    }
    val finalSnap = vm.state.value
    check(predicate(finalSnap)) {
        "Timed out after ${timeoutMs}ms waiting for $description. Final snapshot: " +
            "auth=${finalSnap.isAuthenticated}, sync=${finalSnap.syncStatusLabel}, ws=${finalSnap.serverWsConnected}, " +
            "endpoint=${finalSnap.serverEndpoint}, chats=${finalSnap.chats.size}, bots=${finalSnap.fabushiBots.size}, " +
            "nodes=${finalSnap.cloudComputer.nodes.size}, toast=${finalSnap.toastBanner}, authErr=${finalSnap.authError}"
    }
    return finalSnap
}

/**
 * Deterministic polling helper for arbitrary opaque-box conditions (server requests, WS connect counts, etc.).
 */
fun awaitCondition(
    timeoutMs: Long = 5_000L,
    pollMs: Long = 20L,
    description: String = "condition",
    predicate: () -> Boolean,
) {
    val deadline = System.currentTimeMillis() + timeoutMs
    while (System.currentTimeMillis() < deadline) {
        if (predicate()) return
        Thread.sleep(pollMs)
    }
    check(predicate()) { "Timed out after ${timeoutMs}ms waiting for $description" }
}
