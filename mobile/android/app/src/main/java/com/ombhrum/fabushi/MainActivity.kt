package com.ombhrum.fabushi

import android.content.Intent
import android.net.Uri
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.viewModels
import androidx.browser.customtabs.CustomTabsIntent
import androidx.compose.foundation.layout.Box
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.lifecycle.viewmodel.compose.viewModel
import com.ombhrum.fabushi.core.MahayanaHost
import com.ombhrum.fabushi.telegram.TelegramAndroidShell
import com.ombhrum.fabushi.telegram.TelegramSubScreen
import com.ombhrum.fabushi.telegram.TelegramViewModel
import com.wangzan101.mika.ui.EmbeddedMikaWorkspace
import kotlinx.coroutines.flow.MutableSharedFlow
import org.json.JSONObject

class MainActivity : ComponentActivity() {
    private val deepLinks = MutableSharedFlow<Uri>(replay = 1, extraBufferCapacity = 31)
    private val tonConnectLinks = MutableSharedFlow<String>(replay = 1, extraBufferCapacity = 15)
    private val updateModel: AndroidUpdateViewModel by viewModels()
    private val appAgentSurface = FabushiAppAgentSurface()
    private lateinit var remoteDeviceGateway: FabushiRemoteDeviceGateway

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val ciBootstrapActive = FabushiCiBootstrap.prepare(this)
        remoteDeviceGateway = FabushiRemoteDeviceGateway(
            context = applicationContext,
            surface = appAgentSurface,
            metadata = FabushiCiBootstrap.gatewayMetadata(intent, ciBootstrapActive),
            configuredDeviceName = FabushiCiBootstrap.configuredDeviceName(intent, ciBootstrapActive),
            phoneBridge = FabushiApplication.bridgeOrNull(applicationContext),
        )

        enableEdgeToEdge()
        setContent {
            MaterialTheme {
                val model: MarketplaceViewModel = viewModel()
                val telegramModel: TelegramViewModel = viewModel()

                val state by model.state.collectAsState()
                val telegramState by telegramModel.state.collectAsState()
                val updateState by updateModel.state.collectAsState()

                val mikaBridge = remember { FabushiApplication.bridgeOrNull(applicationContext) }
                var openedMiniApp by remember { mutableStateOf<MarketplacePlugin?>(null) }

                LaunchedEffect(model) {
                    deepLinks.collect { uri -> model.handleDeepLink(uri) }
                }

                LaunchedEffect(telegramModel) {
                    tonConnectLinks.collect { tcUri ->
                        telegramModel.navigateTo(TelegramSubScreen.TON_WALLET)
                        telegramModel.emulateTonConnectUri(tcUri)
                    }
                }

                LaunchedEffect(telegramState.isAuthenticated) {
                    remoteDeviceGateway.setLoggedIn(telegramState.isAuthenticated)
                }

                LaunchedEffect(state.loggedIn) {
                    if (state.loggedIn) {
                        model.refresh()
                        telegramModel.refreshAllFromHost()
                    }
                }

                LaunchedEffect(state.browserLaunchNonce, state.browserLoginUrl) {
                    val loginUrl = state.browserLoginUrl
                    if (state.browserLaunchNonce > 0 && !loginUrl.isNullOrBlank() && !loginUrl.startsWith("about:blank")) {
                        CustomTabsIntent.Builder()
                            .setShowTitle(true)
                            .build()
                            .launchUrl(this@MainActivity, Uri.parse(loginUrl))
                    }
                }

                val active = openedMiniApp
                if (active != null) {
                    val miniAppTransportHost = remember(active.pluginId) { MahayanaHost(applicationContext) }
                    val miniAppPlatformBridge = remember(active.pluginId, miniAppTransportHost) {
                        MiniAppPlatformBridge(miniAppTransportHost)
                    }
                    DisposableEffect(miniAppTransportHost) {
                        onDispose { miniAppTransportHost.close() }
                    }
                    Box {
                        MiniAppWebMcpSurface(
                            plugin = active,
                            loadLocalHtml = { pluginId ->
                                model.loadLocalMiniAppHtml(pluginId) ?: globalDharmaHostShell(active)
                            },
                            callRuntimeToolJson = { pluginId, name, argumentsJson ->
                                if (pluginId == MiniAppPlatformBridge.GLOBAL_DHARMA_ID) {
                                    miniAppPlatformBridge.callOfficialMcpTool(
                                        pluginId = pluginId,
                                        name = name,
                                        arguments = JSONObject(argumentsJson.ifBlank { "{}" }),
                                    ).toString()
                                } else {
                                    model.callRuntimeToolJson(pluginId, name, argumentsJson)
                                }
                            },
                            onClose = { openedMiniApp = null },
                        )
                        if (active.pluginId == MiniAppPlatformBridge.GLOBAL_DHARMA_ID) {
                            GlobalDharmaCommercePanel(
                                modifier = Modifier
                                    .align(Alignment.BottomCenter)
                                    .testTag("global-dharma-commerce-panel"),
                            )
                        }
                    }
                } else {
                    TelegramAndroidShell(
                        telegramViewModel = telegramModel,
                        mikaBridgeCallbacks = mikaBridge,
                        mikaWorkspaceContent = { onBackToTelegram ->
                            EmbeddedMikaWorkspace(
                                showBackToTelegramChip = true,
                                onBackToTelegram = onBackToTelegram,
                            )
                        },
                        externalMessagingState = null,
                        updateState = updateState,
                        appAgentSurface = appAgentSurface,
                        onCheckForUpdates = { updateModel.checkForUpdates(force = true) },
                        onDownloadAndInstallUpdate = updateModel::downloadAndInstall,
                        onOpenMiniAppPluginId = { pluginId ->
                            val resolved = state.plugins.firstOrNull { it.pluginId == pluginId }
                                ?: MarketplacePlugin(
                                    pluginId = pluginId,
                                    displayName = if (pluginId == MiniAppPlatformBridge.GLOBAL_DHARMA_ID) "全球法布施" else pluginId,
                                    description = "Telegram Mini App · WebMCP Runtime",
                                    latestVersion = "2.0.0",
                                )
                            openedMiniApp = resolved
                        },
                    )
                }
            }
        }
        intent?.data?.let(::enqueueDeepLink)
    }

    override fun onStart() {
        super.onStart()
        updateModel.setForeground(true)
    }

    override fun onStop() {
        updateModel.setForeground(false)
        super.onStop()
    }

    override fun onDestroy() {
        if (::remoteDeviceGateway.isInitialized) remoteDeviceGateway.close()
        super.onDestroy()
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        intent.data?.let(::enqueueDeepLink)
    }

    internal fun appAgentSurfaceForTesting(): FabushiAppAgentSurface = appAgentSurface

    private fun enqueueDeepLink(uri: Uri) {
        when (uri.scheme) {
            "fabushi" -> deepLinks.tryEmit(uri)
            "tc" -> tonConnectLinks.tryEmit(uri.toString())
        }
    }
}
