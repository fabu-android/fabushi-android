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

