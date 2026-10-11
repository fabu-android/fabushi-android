package com.ombhrum.fabushi

import android.app.Application
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.compose.viewModel
import com.ombhrum.fabushi.androidpreload.runtime.AndroidCoordinatorPort
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONObject
import java.io.File

data class GlobalDharmaCommerceState(
    val loading: Boolean = true,
    val allowed: Boolean = false,
    val reason: String = "loading",
    val lifetimeCatalogValid: Boolean = false,
    val activeRails: List<String> = emptyList(),
    val testMode: Boolean = BuildConfig.CI_ACCOUNT_SESSION_IMPORT_ENABLED,
    val busy: Boolean = false,
    val message: String? = null,
)

class GlobalDharmaCommerceViewModel(application: Application) : AndroidViewModel(application) {
    private val coordinator: AndroidCoordinatorPort = CoordinatorClient.presentation()
    private val bridge = MiniAppPlatformBridge(coordinator)
    private val mutableState = MutableStateFlow(GlobalDharmaCommerceState())
    val state: StateFlow<GlobalDharmaCommerceState> = mutableState.asStateFlow()
    private val commerceJournal = CommerceOperationJournal(
        File(application.filesDir, "commerce-operation-journal.properties"),
    )

    init {
        refresh()
    }

    fun refresh() {
        viewModelScope.launch {
            runCatching { withContext(Dispatchers.IO) { readCanonicalState() } }
                .onSuccess { mutableState.value = it.copy(message = mutableState.value.message) }
                .onFailure { error ->
                    mutableState.value = mutableState.value.copy(
                        loading = false,
                        allowed = false,
                        reason = "entitlement_read_failed",
                        message = error.message ?: "权益读取失败",
                    )
                }
        }
    }

    fun purchaseLifetimeTest() {
        val snapshot = mutableState.value
        if (snapshot.busy || !snapshot.testMode || !snapshot.lifetimeCatalogValid) return
        mutableState.value = snapshot.copy(busy = true, message = "正在通过 Fabushi Pay 测试账本购买 ¥1080 买断权益…")
        viewModelScope.launch {
            runCatching {
                withContext(Dispatchers.IO) {
                    val accountFence = currentCommerceAccountFence()
                    val idempotencyKey = commerceJournal.stableIdempotencyKey(
                        accountFence = accountFence,
                        pluginId = MiniAppPlatformBridge.GLOBAL_DHARMA_ID,
                        sku = MiniAppPlatformBridge.PRAYER_WHEEL_LIFETIME_SKU,
                    )
                    bridge.purchase(
                        pluginId = MiniAppPlatformBridge.GLOBAL_DHARMA_ID,
                        sku = MiniAppPlatformBridge.PRAYER_WHEEL_LIFETIME_SKU,
                        idempotencyKey = idempotencyKey,
                    )
                    val refreshed = readCanonicalState()
                    if (refreshed.allowed) {
                        commerceJournal.markConfirmed(
                            accountFence = accountFence,
                            pluginId = MiniAppPlatformBridge.GLOBAL_DHARMA_ID,
                            sku = MiniAppPlatformBridge.PRAYER_WHEEL_LIFETIME_SKU,
                            idempotencyKey = idempotencyKey,
                        )
                    }
                    refreshed
                }
            }.onSuccess { refreshed ->
                mutableState.value = refreshed.copy(
                    busy = false,
                    message = if (refreshed.allowed) "¥1080 买断权益已由服务端确认。" else "订单已处理，但权益仍未生效。",
                )
            }.onFailure { error ->
                mutableState.value = mutableState.value.copy(
                    busy = false,
                    allowed = false,
                    message = "测试购买失败：${error.message ?: "unknown error"}",
                )
            }
        }
    }

    fun restore() {
        if (mutableState.value.busy) return
        mutableState.value = mutableState.value.copy(busy = true, message = "正在从 canonical purchase ledger 恢复权益…")
        viewModelScope.launch {
            runCatching {
                withContext(Dispatchers.IO) {
                    bridge.restorePurchases()
                    readCanonicalState()
                }
            }.onSuccess { refreshed ->
                mutableState.value = refreshed.copy(
                    busy = false,
                    message = if (refreshed.allowed) "权益恢复完成。" else "恢复完成，但没有有效的本地转经轮权益。",
                )
            }.onFailure { error ->
                mutableState.value = mutableState.value.copy(
                    busy = false,
                    allowed = false,
                    message = "恢复失败：${error.message ?: "unknown error"}",
                )
            }
        }
    }

    private fun currentCommerceAccountFence(): String {
        val auth = coordinator.authStatus()
        check(auth.optBoolean("loggedIn", false)) { "commerce_account_not_logged_in" }
        val user = auth.optJSONObject("user") ?: error("commerce_account_identity_missing")
        val identity = user.optString("id").ifBlank { user.optString("username") }.ifBlank { user.optString("email") }
        check(identity.isNotBlank() && identity.length <= 320) { "commerce_account_identity_missing" }
        return identity
    }

    private fun readCanonicalState(): GlobalDharmaCommerceState {
        val response = bridge.entitlement(
            MiniAppPlatformBridge.GLOBAL_DHARMA_ID,
            MiniAppPlatformBridge.PRAYER_WHEEL_CAPABILITY,
        )
        val access = response.optJSONObject("access")
            ?: error("Canonical entitlement response is missing access")
        check(access.optBoolean("protected", false)) {
            "Canonical service did not mark local.prayer-wheel.start protected"
        }
        val options = response.optJSONArray("purchaseOptions")
        var validLifetime = false
        var rails = emptyList<String>()
        if (options != null) {
            for (index in 0 until options.length()) {
                val option = options.optJSONObject(index) ?: continue
                if (option.optString("sku") != MiniAppPlatformBridge.PRAYER_WHEEL_LIFETIME_SKU) continue
                val currency = option.optString("currency")
                val amount = option.optLong("amount", -1L)
                check(currency == "CNY" && amount == MiniAppPlatformBridge.PRAYER_WHEEL_LIFETIME_CNY_MINOR) {
                    "Server lifetime SKU drifted from the governed CNY 1080 contract"
                }
                validLifetime = true
                val active = option.optJSONArray("activeRails")
                rails = buildList {
                    if (active != null) for (railIndex in 0 until active.length()) {
                        active.optString(railIndex).takeIf(String::isNotBlank)?.let(::add)
                    }
                }
                break
            }
        }
        return GlobalDharmaCommerceState(
            loading = false,
            allowed = access.optBoolean("allowed", false),
            reason = access.optString("reason", "unknown"),
            lifetimeCatalogValid = validLifetime,
            activeRails = rails,
            testMode = BuildConfig.CI_ACCOUNT_SESSION_IMPORT_ENABLED,
            busy = false,
        )
    }

}

@Composable
fun GlobalDharmaCommercePanel(
    modifier: Modifier = Modifier,
    model: GlobalDharmaCommerceViewModel = viewModel(),
) {
    val state by model.state.collectAsState()
    Surface(
        tonalElevation = 4.dp,
        modifier = modifier.fillMaxWidth(),
    ) {
        Column(
            verticalArrangement = Arrangement.spacedBy(6.dp),
            modifier = Modifier.padding(horizontal = 12.dp, vertical = 8.dp),
        ) {
            Text(
                text = if (state.allowed) "本地转经轮：已解锁" else "本地转经轮：未解锁",
                style = MaterialTheme.typography.titleSmall,
                modifier = Modifier.testTag("global-dharma-entitlement-status"),
            )
            Text(
                text = when {
                    state.loading -> "正在读取 Fabushi Pay 权益…"
                    state.allowed -> "授权源：canonical server entitlement · ${state.reason}"
                    state.testMode -> "Android 测试模式 · ¥1080 买断 · ${state.reason}"
                    state.activeRails.isEmpty() -> "生产支付 rail 尚未 active；功能保持 fail-closed。"
                    else -> "可用生产 rail：${state.activeRails.joinToString()} · 当前 Android 不会绕过 provider checkout。"
                },
                style = MaterialTheme.typography.bodySmall,
            )
            state.message?.let { Text(it, style = MaterialTheme.typography.bodySmall) }
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Button(
                    onClick = model::purchaseLifetimeTest,
                    enabled = state.testMode && state.lifetimeCatalogValid && !state.allowed && !state.busy,
                    modifier = Modifier.testTag("global-dharma-buy-lifetime"),
                ) {
                    Text(if (state.testMode) "¥1080 买断（测试）" else "¥1080 买断")
                }
                OutlinedButton(
                    onClick = model::restore,
                    enabled = !state.busy,
                    modifier = Modifier.testTag("global-dharma-restore"),
                ) {
                    Text("恢复购买")
                }
            }
        }
    }
}
