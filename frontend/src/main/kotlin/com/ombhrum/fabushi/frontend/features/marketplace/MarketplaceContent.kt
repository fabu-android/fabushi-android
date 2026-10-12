package com.ombhrum.fabushi

import android.Manifest
import android.annotation.SuppressLint
import android.content.Context
import android.content.pm.PackageManager
import android.location.LocationManager
import android.os.Build
import android.os.CancellationSignal
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.gestures.detectHorizontalDragGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.FloatingActionButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.content.ContextCompat
import kotlinx.coroutines.delay


@Composable
internal fun MarketplaceContent(
    state: MarketplaceUiState,
    onQueryChange: (String) -> Unit,
    onSearch: () -> Unit,
    onInstall: (MarketplacePlugin) -> Unit,
    onOpen: (MarketplacePlugin) -> Unit,
    onSubmitVariables: (Map<String, String>) -> Unit,
    onCancelVariables: () -> Unit,
    onSaveMcpServerInstructions: (String, String) -> Unit,
    onLogoutMcpAccount: (String, String) -> Unit,
    onRenameMcpAccount: (String, String, String) -> Unit,
    onRemoveMcpAccount: (String, String) -> Unit,
    onOpenMcpServerTools: (String) -> Unit,
    onRetryMcpServerTools: (String) -> Unit,
    onToggleMcpServerTool: (String, String) -> Unit,
    onBack: () -> Unit,
) {
    state.variableRequest?.let { request ->
        var values by remember(request.plugin.pluginId) {
            mutableStateOf(request.fields.associate { field -> field.key to field.defaultValue.orEmpty() })
        }
        val requiredReady = request.fields.all { field ->
            !field.isRequired || values[field.key].orEmpty().trim().isNotEmpty() || !field.defaultValue.isNullOrBlank()
        }
        AlertDialog(
            onDismissRequest = onCancelVariables,
            title = { Text("配置 " + request.plugin.displayName) },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    request.fields.forEach { field ->
                        OutlinedTextField(
                            value = values[field.key].orEmpty(),
                            onValueChange = { value -> values = values + (field.key to value) },
                            modifier = Modifier.fillMaxWidth().testTag("plugin-variable-" + field.key),
                            label = { Text(field.label + if (field.isRequired) " *" else "") },
                            placeholder = { Text(field.placeholder) },
                            supportingText = field.hint?.let { hint -> ({ Text(hint) }) },
                            singleLine = true,
                            visualTransformation = if (field.isSecret) PasswordVisualTransformation() else VisualTransformation.None,
                        )
                    }
                }
            },
            confirmButton = {
                Button(
                    onClick = { onSubmitVariables(values) },
                    enabled = requiredReady,
                    modifier = Modifier.testTag("plugin-variable-submit"),
                ) { Text("保存并继续") }
            },
            dismissButton = {
                TextButton(
                    onClick = onCancelVariables,
                    modifier = Modifier.testTag("plugin-variable-cancel"),
                ) { Text("取消") }
            },
        )
    }
    Scaffold(modifier = Modifier.fillMaxSize().testTag(TestTags.AppShell)) { padding ->
        LazyColumn(
            modifier = Modifier.fillMaxSize().padding(padding).padding(horizontal = 20.dp),
            verticalArrangement = Arrangement.spacedBy(14.dp),
        ) {
            item {
                Spacer(Modifier.height(12.dp))
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Column {
                        Text("MAHAYANA RUST HOST", style = MaterialTheme.typography.labelSmall)
                        Text("全球法布施", style = MaterialTheme.typography.headlineLarge, fontWeight = FontWeight.Bold)
                    }
                    OutlinedButton(onClick = onBack, modifier = Modifier.testTag(TestTags.MarketplaceBack)) { Text("返回消息") }
                }
            }

            item {
                Text(
                    "Compose · Rust",
                    modifier = Modifier.testTag(TestTags.RuntimeBadge).semantics { contentDescription = "Android native runtime" },
                    style = MaterialTheme.typography.labelMedium,
                )
            }

            item {
                Card(modifier = Modifier.fillMaxWidth()) {
                    Column(modifier = Modifier.padding(18.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                        Text("本地插件市场", style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.SemiBold)
                        Text("Android 主壳使用 Jetpack Compose；MiniApp 使用受控 WebMCP Surface；代码从 GitHub 固定版本拉取并由共享 Mahayana Rust Host 校验、安装、更新。")
                        OutlinedTextField(
                            value = state.query,
                            onValueChange = onQueryChange,
                            modifier = Modifier.fillMaxWidth().testTag(TestTags.SearchField),
                            label = { Text("搜索插件") },
                            singleLine = true,
                        )
                        Button(
                            onClick = onSearch,
                            modifier = Modifier.fillMaxWidth().testTag(TestTags.SearchButton),
                            enabled = !state.loading,
                        ) {
                            if (state.loading) {
                                CircularProgressIndicator(modifier = Modifier.height(20.dp))
                            } else {
                                Text("搜索")
                            }
                        }
                    }
                }
            }

            item {
                Card(modifier = Modifier.fillMaxWidth().testTag(TestTags.HostStatus)) {
                    Column(modifier = Modifier.padding(16.dp)) {
                        Text("Host 状态", fontWeight = FontWeight.Bold)
                        Text(state.message)
                    }
                }
            }

            item {
                Card(
                    modifier = Modifier
                        .fillMaxWidth()
                        .testTag("mcp-server-catalog"),
                ) {
                    Column(
                        modifier = Modifier.padding(16.dp),
                        verticalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        Text("连接器", style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.SemiBold)
                        Text(
                            "账户范围的 MCP server catalog 由 Rust Host 投影；此处仅显示连接状态元数据，不暴露凭据。",
                            style = MaterialTheme.typography.bodySmall,
                        )
                        when {
                            state.mcpServersBusy -> CircularProgressIndicator(modifier = Modifier.height(20.dp))
                            state.mcpServersError != null -> Text(
                                "连接器目录不可用：${state.mcpServersError}",
                                style = MaterialTheme.typography.bodySmall,
                            )
                            state.mcpServers.isEmpty() -> Text(
                                "当前账户没有已启用连接器。",
                                style = MaterialTheme.typography.bodySmall,
                            )
                            else -> state.mcpServers.forEach { server ->
                                Column(
                                    modifier = Modifier
                                        .fillMaxWidth()
                                        .testTag("mcp-server-${server.id}"),
                                    verticalArrangement = Arrangement.spacedBy(2.dp),
                                ) {
                                    Text(server.name, fontWeight = FontWeight.Medium)
                                    Text(
                                        buildString {
                                            append(server.transport.uppercase())
                                            server.url?.let { append(" · "); append(it) }
                                            if (server.disabledByTeamAdminPolicy) append(" · 受团队策略限制")
                                        },
                                        style = MaterialTheme.typography.labelSmall,
                                    )
                                    var instructionsDraft by remember(
                                        server.id,
                                        state.mcpServerInstructions[server.id],
                                    ) {
                                        mutableStateOf(state.mcpServerInstructions[server.id].orEmpty())
                                    }
                                    OutlinedTextField(
                                        value = instructionsDraft,
                                        onValueChange = { instructionsDraft = it },
                                        modifier = Modifier
                                            .fillMaxWidth()
                                            .testTag("mcp-server-instructions-${server.id}"),
                                        label = { Text("自定义说明") },
                                        supportingText = {
                                            state.mcpServerInstructionsError[server.id]?.let { error ->
                                                Text("保存失败：$error")
                                            }
                                        },
                                        minLines = 2,
                                        maxLines = 6,
                                    )
                                    Button(
                                        onClick = {
                                            onSaveMcpServerInstructions(server.id, instructionsDraft)
                                        },
                                        enabled = server.id !in state.mcpServerInstructionsBusy,
                                        modifier = Modifier.testTag("mcp-server-instructions-save-${server.id}"),
                                    ) {
                                        Text(
                                            if (server.id in state.mcpServerInstructionsBusy) {
                                                "保存中…"
                                            } else {
                                                "保存说明"
                                            },
                                        )
                                    }
                                    val serverAccounts = state.mcpServerAccounts[server.id].orEmpty()
                                    if (serverAccounts.isNotEmpty()) {
                                        Text("已连接账户", style = MaterialTheme.typography.labelMedium)
                                        serverAccounts.forEach { account ->
                                            val mutationKey = server.id + "\u0000" + account.accountKey
                                            val accountBusy = mutationKey in state.mcpServerAccountBusy
                                            var accountNameDraft by remember(server.id, account.accountKey) {
                                                mutableStateOf(account.accountKey)
                                            }
                                            Column(
                                                modifier = Modifier
                                                    .fillMaxWidth()
                                                    .testTag("mcp-account-${server.id}-${account.accountKey}"),
                                                verticalArrangement = Arrangement.spacedBy(4.dp),
                                            ) {
                                                OutlinedTextField(
                                                    value = accountNameDraft,
                                                    onValueChange = { accountNameDraft = it },
                                                    modifier = Modifier
                                                        .fillMaxWidth()
                                                        .testTag("mcp-account-name-${server.id}-${account.accountKey}"),
                                                    label = { Text("账户名称") },
                                                    singleLine = true,
                                                    enabled = !accountBusy,
                                                )
                                                Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                                                    TextButton(
                                                        onClick = {
                                                            onRenameMcpAccount(
                                                                server.id,
                                                                account.accountKey,
                                                                accountNameDraft,
                                                            )
                                                        },
                                                        enabled = !accountBusy &&
                                                            accountNameDraft.trim().isNotEmpty() &&
                                                            accountNameDraft.trim() != account.accountKey,
                                                        modifier = Modifier.testTag(
                                                            "mcp-account-rename-${server.id}-${account.accountKey}",
                                                        ),
                                                    ) { Text("重命名") }
                                                    TextButton(
                                                        onClick = {
                                                            onLogoutMcpAccount(server.id, account.accountKey)
                                                        },
                                                        enabled = !accountBusy,
                                                        modifier = Modifier.testTag(
                                                            "mcp-account-logout-${server.id}-${account.accountKey}",
                                                        ),
                                                    ) { Text("退出") }
                                                    TextButton(
                                                        onClick = {
                                                            onRemoveMcpAccount(server.id, account.accountKey)
                                                        },
                                                        enabled = !accountBusy,
                                                        modifier = Modifier.testTag(
                                                            "mcp-account-remove-${server.id}-${account.accountKey}",
                                                        ),
                                                    ) { Text("移除") }
                                                }
                                                state.mcpServerAccountError[mutationKey]?.let { error ->
                                                    Text(
                                                        "账户操作失败：$error",
                                                        style = MaterialTheme.typography.bodySmall,
                                                    )
                                                }
                                            }
                                        }
                                    }
                                    val toolsState = state.mcpServerTools[server.id]
                                    TextButton(
                                        onClick = { onOpenMcpServerTools(server.id) },
                                        enabled = !server.disabledByTeamAdminPolicy && toolsState?.pendingTool == null,
                                        modifier = Modifier.testTag("mcp-server-tools-${server.id}"),
                                    ) { Text(if (toolsState == null) "查看工具" else "刷新工具") }
                                    when (toolsState?.status) {
                                        McpServerToolsUiStatus.LOADING -> CircularProgressIndicator(modifier = Modifier.height(18.dp))
                                        McpServerToolsUiStatus.FAILED -> {
                                            Text("工具列表不可用：${toolsState.failure.orEmpty()}", style = MaterialTheme.typography.bodySmall)
                                            TextButton(
                                                onClick = { onRetryMcpServerTools(server.id) },
                                                modifier = Modifier.testTag("mcp-server-tools-retry-${server.id}"),
                                            ) { Text("重试") }
                                        }
                                        McpServerToolsUiStatus.READY -> {
                                            if (toolsState.tools.isEmpty()) {
                                                Text("此连接器没有可用工具。", style = MaterialTheme.typography.bodySmall)
                                            } else {
                                                toolsState.tools.forEach { tool ->
                                                    Row(
                                                        modifier = Modifier.fillMaxWidth(),
                                                        horizontalArrangement = Arrangement.SpaceBetween,
                                                        verticalAlignment = Alignment.CenterVertically,
                                                    ) {
                                                        Column(modifier = Modifier.weight(1f)) {
                                                            Text(tool.title?.takeIf(String::isNotBlank) ?: tool.name)
                                                            tool.description?.takeIf(String::isNotBlank)?.let {
                                                                Text(it, style = MaterialTheme.typography.bodySmall)
                                                            }
                                                        }
                                                        TextButton(
                                                            onClick = { onToggleMcpServerTool(server.id, tool.name) },
                                                            enabled = toolsState.pendingTool == null,
                                                            modifier = Modifier.testTag("mcp-tool-${server.id}-${tool.name}"),
                                                        ) { Text(if (tool.isDisabled) "启用" else "停用") }
                                                    }
                                                }
                                            }
                                        }
                                        null -> Unit
                                    }
                                    toolsState?.failure
                                        ?.takeIf { toolsState.status == McpServerToolsUiStatus.READY && it.isNotBlank() }
                                        ?.let { Text("上次工具更新失败：$it", style = MaterialTheme.typography.bodySmall) }
                                }
                            }
                        }
                    }
                }
            }

            if (state.plugins.isEmpty() && !state.loading) {
                item { Text("没有匹配的 Android 插件。", modifier = Modifier.padding(16.dp)) }
            }

            items(state.plugins, key = { it.pluginId }) { plugin ->
                Card(modifier = Modifier.fillMaxWidth().testTag(TestTags.plugin(plugin.pluginId))) {
                    Column(modifier = Modifier.padding(18.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        Text(plugin.pluginId, style = MaterialTheme.typography.labelSmall)
                        Text(plugin.displayName, style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.SemiBold)
                        Text(plugin.description)
                        plugin.latestVersion?.let { version ->
                            Text(
                                "$version · GitHub ${plugin.sourceRef?.take(9) ?: "待确认"}",
                                style = MaterialTheme.typography.labelMedium,
                            )
                        }
                        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            Button(
                                onClick = { onOpen(plugin) },
                                modifier = Modifier.weight(1f).testTag(TestTags.open(plugin.pluginId)),
                            ) {
                                Text("打开 WebMCP")
                            }
                            Button(
                                onClick = { onInstall(plugin) },
                                modifier = Modifier.weight(1f).testTag(TestTags.install(plugin.pluginId)),
                                enabled = plugin.latestVersion != null && state.installingPluginId == null,
                            ) {
                                Text(if (state.installingPluginId == plugin.pluginId) "处理中…" else "安装 / 更新")
                            }
                        }
                    }
                }
            }

            item { Spacer(Modifier.height(24.dp)) }
        }
    }
}

