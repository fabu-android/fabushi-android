package com.ombhrum.fabushi

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Popup
import androidx.compose.ui.window.PopupProperties
import com.ombhrum.fabushi.androidpreload.runtime.AndroidPresentationRuntimePort
import com.ombhrum.fabushi.androidpreload.runtime.RootConnectionTransportState
import java.net.URI
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.ExecutorService
import java.util.concurrent.Executors
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import org.json.JSONArray
import org.json.JSONObject

internal sealed interface RootShellNotificationAction {
    val label: String

    data class OpenUrl(
        override val label: String,
        val url: String,
    ) : RootShellNotificationAction

    data class DashboardAction(
        override val label: String,
        val action: String,
        val args: Map<String, String>,
        val successMessage: String?,
    ) : RootShellNotificationAction
}

internal data class RootShellNotificationTray(
    val id: String,
    val title: String,
    val detail: String? = null,
    val requestId: String? = null,
    val errorKind: String? = null,
    val actions: List<RootShellNotificationAction> = emptyList(),
    val count: Long? = null,
)

internal fun projectRootShellNotificationAction(value: JSONObject): RootShellNotificationAction? {
    val kind = value.optString("kind").trim()
    val label = value.optString("label").trim()
    if (label.isEmpty()) return null
    return when (kind) {
        "open-url" -> {
            val url = value.optString("url").trim()
            val scheme = runCatching { URI(url).scheme?.lowercase() }.getOrNull()
            if (scheme != "http" && scheme != "https") null
            else RootShellNotificationAction.OpenUrl(label, url)
        }
        "dashboard-action" -> {
            val action = value.optString("action").trim()
            val rawArgs = value.optJSONObject("args") ?: return null
            if (action.isEmpty()) return null
            val args = linkedMapOf<String, String>()
            val keys = rawArgs.keys()
            while (keys.hasNext()) {
                val key = keys.next()
                val arg = rawArgs.opt(key) as? String ?: return null
                args[key] = arg
            }
            RootShellNotificationAction.DashboardAction(
                label = label,
                action = action,
                args = args,
                successMessage = value.opt("successMessage") as? String,
            )
        }
        else -> null
    }
}

internal fun projectRootShellNotificationTray(value: JSONObject): RootShellNotificationTray? {
    if (value.optString("kind") != "error") return null
    val id = value.optString("id").trim()
    val title = value.optString("title").trim()
    if (id.isEmpty() || title.isEmpty()) return null
    val actions = buildList {
        val raw = value.optJSONArray("actions") ?: JSONArray()
        for (index in 0 until raw.length()) {
            if (size >= 3) break
            val action = raw.optJSONObject(index)?.let(::projectRootShellNotificationAction)
            if (action != null) add(action)
        }
    }
    val count = value.optLong("count", 0L).takeIf { it > 1L }
    return RootShellNotificationTray(
        id = id,
        title = title,
        detail = value.optString("detail").trim().takeIf(String::isNotEmpty),
        requestId = value.optString("requestId").trim().takeIf(String::isNotEmpty),
        errorKind = value.optString("errorKind").trim().takeIf(String::isNotEmpty),
        actions = actions,
        count = count,
    )
}

internal fun projectRootShellNotificationTrays(value: JSONArray): List<RootShellNotificationTray> =
    buildList {
        for (index in 0 until value.length()) {
            value.optJSONObject(index)?.let(::projectRootShellNotificationTray)?.let(::add)
        }
    }

internal fun reduceRootShellNotificationEvent(
    current: List<RootShellNotificationTray>,
    event: JSONObject,
): List<RootShellNotificationTray> {
    return when (event.optString("type")) {
        "tray.cleared", "cleared" -> emptyList()
        "tray.dismissed", "dismissed" -> {
            val id = event.optString("id")
            current.filterNot { it.id == id }
        }
        "tray.pushed", "pushed" -> {
            val next = event.optJSONObject("tray")?.let(::projectRootShellNotificationTray)
                ?: return current
            val index = current.indexOfFirst { it.id == next.id }
            if (index < 0) current + next
            else current.mapIndexed { itemIndex, tray -> if (itemIndex == index) next else tray }
        }
        else -> current
    }
}

internal class RootShellNotificationController(
    private val runtimePort: AndroidPresentationRuntimePort,
    private val executor: ExecutorService = Executors.newSingleThreadExecutor { runnable ->
        Thread(runnable, "fabushi-root-trays").apply { isDaemon = true }
    },
) : AutoCloseable {
    private val _trays = MutableStateFlow<List<RootShellNotificationTray>>(emptyList())
    val trays: StateFlow<List<RootShellNotificationTray>> = _trays
    private val _pendingActions = MutableStateFlow<Set<String>>(emptySet())
    val pendingActions: StateFlow<Set<String>> = _pendingActions
    private val _actionNotices = MutableStateFlow<Map<String, String>>(emptyMap())
    val actionNotices: StateFlow<Map<String, String>> = _actionNotices

    private val queuedEvents = CopyOnWriteArrayList<JSONObject>()
    private var eventSubscription: AutoCloseable? = null
    private var active = false
    private var loggedIn = false
    private var connected = false
    private var fetching = false
    private var generation = 0L

    @Synchronized
    fun start() {
        if (active) return
        active = true
        eventSubscription = runtimePort.addRootNotificationEventListener(::acceptEvent)
    }

    @Synchronized
    fun observeAccount(isLoggedIn: Boolean) {
        if (loggedIn == isLoggedIn) return
        loggedIn = isLoggedIn
        generation += 1
        fetching = false
        queuedEvents.clear()
        _trays.value = emptyList()
        _pendingActions.value = emptySet()
        _actionNotices.value = emptyMap()
        if (loggedIn) refreshLocked()
    }

    @Synchronized
    fun observeTransport(state: RootConnectionTransportState) {
        val nextConnected = state == RootConnectionTransportState.CONNECTED
        val becameConnected = !connected && nextConnected
        connected = nextConnected
        if (becameConnected) refreshLocked()
    }

    fun dismiss(id: String) {
        executor.execute { runCatching { runtimePort.dismissRootNotificationTray(id) } }
    }

    fun clear() {
        executor.execute { runCatching { runtimePort.clearRootNotificationTrays() } }
    }

    fun runAction(trayId: String, action: RootShellNotificationAction) {
        val key = actionKey(trayId, action)
        synchronized(this) {
            if (key in _pendingActions.value) return
            _pendingActions.value = _pendingActions.value + key
            _actionNotices.value = _actionNotices.value - key
        }
        executor.execute {
            val notice = runCatching {
                when (action) {
                    is RootShellNotificationAction.OpenUrl -> {
                        if (!runtimePort.launchExternalUrl(action.url)) "Couldn’t open link — try again"
                        else null
                    }
                    is RootShellNotificationAction.DashboardAction -> {
                        val args = JSONObject()
                        action.args.forEach(args::put)
                        val result = runtimePort.invokeRootNotificationDashboardAction(action.action, args)
                        if (result.optBoolean("ok", false)) {
                            result.optString("message").trim().takeIf(String::isNotEmpty)
                                ?: action.successMessage
                        } else {
                            result.optString("message").trim().takeIf(String::isNotEmpty)
                                ?: "Couldn’t complete the action — try again"
                        }
                    }
                }
            }.getOrElse { "Couldn’t complete the action — try again" }
            synchronized(this) {
                _pendingActions.value = _pendingActions.value - key
                if (notice != null) _actionNotices.value = _actionNotices.value + (key to notice)
            }
        }
    }

    private fun acceptEvent(event: JSONObject) {
        synchronized(this) {
            if (!active || !loggedIn) return
            val copy = JSONObject(event.toString())
            if (fetching) queuedEvents += copy
            else _trays.value = reduceRootShellNotificationEvent(_trays.value, copy)
        }
    }

    @Synchronized
    private fun refreshLocked() {
        if (!active || !loggedIn || fetching) return
        val requestGeneration = ++generation
        fetching = true
        queuedEvents.clear()
        executor.execute {
            val snapshot = runCatching { runtimePort.rootNotificationTrays() }.getOrNull()
            synchronized(this) {
                if (!active || requestGeneration != generation) return@synchronized
                if (snapshot != null) {
                    var next = projectRootShellNotificationTrays(snapshot)
                    queuedEvents.forEach { event ->
                        next = reduceRootShellNotificationEvent(next, event)
                    }
                    _trays.value = next
                }
                queuedEvents.clear()
                fetching = false
            }
        }
    }

    internal fun actionKey(trayId: String, action: RootShellNotificationAction): String =
        trayId + ":" + when (action) {
            is RootShellNotificationAction.OpenUrl -> "url:" + action.url
            is RootShellNotificationAction.DashboardAction -> "dashboard:" + action.action
        }

    @Synchronized
    override fun close() {
        if (!active) return
        active = false
        generation += 1
        fetching = false
        queuedEvents.clear()
        eventSubscription?.close()
        eventSubscription = null
        _trays.value = emptyList()
        _pendingActions.value = emptySet()
        _actionNotices.value = emptyMap()
        executor.shutdownNow()
    }
}

@Composable
internal fun RootShellNotificationHost(
    runtimePort: AndroidPresentationRuntimePort,
    loggedIn: Boolean,
    transport: RootConnectionTransportState,
) {
    val controller = remember(runtimePort) { RootShellNotificationController(runtimePort) }
    val trays by controller.trays.collectAsState()
    val pending by controller.pendingActions.collectAsState()
    val notices by controller.actionNotices.collectAsState()
    val clipboard = LocalClipboardManager.current

    DisposableEffect(controller) {
        controller.start()
        onDispose(controller::close)
    }
    LaunchedEffect(loggedIn) { controller.observeAccount(loggedIn) }
    LaunchedEffect(transport) { controller.observeTransport(transport) }

    if (trays.isEmpty()) return

    Popup(properties = PopupProperties(focusable = false)) {
        Column(
            verticalArrangement = Arrangement.spacedBy(8.dp),
            modifier = Modifier.padding(12.dp),
        ) {
            if (trays.size > 1) {
                TextButton(onClick = controller::clear) { Text("Clear all") }
            }
            trays.forEach { tray ->
                Card {
                    Column(
                        verticalArrangement = Arrangement.spacedBy(6.dp),
                        modifier = Modifier.padding(12.dp),
                    ) {
                        Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                            Text(tray.title)
                            tray.count?.let { Text("×" + it) }
                        }
                        tray.detail?.let { Text(it) }
                        tray.requestId?.let { requestId ->
                            TextButton(
                                onClick = { clipboard.setText(AnnotatedString(requestId)) },
                            ) { Text("Copy request ID") }
                        }
                        tray.actions.forEach { action ->
                            val key = controller.actionKey(tray.id, action)
                            Button(
                                enabled = key !in pending,
                                onClick = { controller.runAction(tray.id, action) },
                            ) { Text(action.label) }
                            notices[key]?.let { Text(it) }
                        }
                        TextButton(onClick = { controller.dismiss(tray.id) }) {
                            Text("Dismiss")
                        }
                    }
                }
            }
        }
    }
}
