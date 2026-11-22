package com.ombhrum.fabushi.telegram

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.ombhrum.fabushi.ClothGhostAvatarAndroid

// ============================================================================
// 1. @BotFather & Natural-Language Companion Bot Creator Workspace
// ============================================================================

@Composable
fun BotFatherWorkspaceScreen(
    existingBots: List<TelegramChatItem>,
    codingBots: List<CodingAgentBotItem> = emptyList(),
    onCreateBot: (name: String, description: String, handleHint: String) -> Unit,
    onCreateCodingBot: (
        provider: CodingProviderKindUi,
        name: String,
        handle: String,
        workspacePath: String,
        permissionMode: CodingPermissionModeUi,
        useWorktree: Boolean,
    ) -> Unit = { _, _, _, _, _, _ -> },
    onCycleCodingPermission: (botId: String, nextMode: CodingPermissionModeUi) -> Unit = { _, _ -> },
    onOpenBotChat: (String) -> Unit,
    onBack: () -> Unit,
) {
    val palette = LocalTelegramPalette.current
    BackHandler(onBack = onBack)

    var botName by remember { mutableStateOf("法布施随身禅心官") }
    var botHandle by remember { mutableStateOf("@dharma_companion_bot") }
    var botDescription by remember { mutableStateOf("支持每日早晚课提醒、经典导读、WebMCP 小程序联动与 Mika 本机自动化。") }
    var selectedCodingProvider by remember { mutableStateOf(CodingProviderKindUi.CLAUDE_CODE) }
    var selectedCodingPermission by remember { mutableStateOf(CodingPermissionModeUi.ASK) }
    var codingWorkspacePath by remember { mutableStateOf("~/workspace") }
    var codingUseWorktree by remember { mutableStateOf(true) }

    val templates = listOf(
        Triple("Mika 跨应用生活助理", "@mika_life_bot", "自动帮你比价、设闹钟、汇总通知并回传结果卡片"),
        Triple("TON 链上金库守卫", "@ton_sentinel_bot", "监控 TON / Stars 异动与 TonConnect 合约安全审计"),
        Triple("Hermes 代码架构师", "@hermes_arch_bot", "结合 /moa 多模型协同审查 Rust 与 Kotlin 架构"),
    )

    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(palette.appBackground)
            .testTag("telegram-botfather-screen"),
    ) {
        WorkspaceTopBar(
            title = "@BotFather 智能机器人中心",
            subtitle = "自然语言创建 Telegram Bot · 原生 Coding Agent (Claude Code / Codex / ACP)",
            glyph = TelegramGlyph.BOT_ROBOT,
            onBack = onBack,
        )

        LazyColumn(
            modifier = Modifier.fillMaxSize().navigationBarsPadding(),
            contentPadding = PaddingValues(14.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            item {
                Surface(shape = RoundedCornerShape(18.dp), color = palette.surfaceColor, modifier = Modifier.fillMaxWidth()) {
                    Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            ClothGhostAvatarAndroid(
                                botId = botHandle.ifBlank { "preview-bot" },
                                size = 58.dp,
                                active = true,
                                badge = palette.primaryAccent,
                            )
                            Spacer(Modifier.width(12.dp))
                            Column(modifier = Modifier.weight(1f)) {
                                Text("✨ /newbot 自然语言一键生成", fontWeight = FontWeight.Bold, fontSize = 16.sp, color = palette.primaryText)
                                Text("自动生成专属 API Token、内联键盘与动态布料幽灵头像", fontSize = 12.sp, color = palette.secondaryText)
                            }
                        }

                        LazyRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            items(templates) { (tName, tHandle, tDesc) ->
                                Surface(
                                    shape = RoundedCornerShape(12.dp),
                                    color = palette.primaryAccent.copy(alpha = 0.12f),
                                    modifier = Modifier.clickable {
                                        botName = tName
                                        botHandle = tHandle
                                        botDescription = tDesc
                                    },
                                ) {
                                    Text("模板：$tName", color = palette.primaryAccent, fontSize = 12.sp, fontWeight = FontWeight.SemiBold, modifier = Modifier.padding(horizontal = 10.dp, vertical = 6.dp))
                                }
                            }
                        }

                        OutlinedTextField(value = botName, onValueChange = { botName = it }, label = { Text("机器人显示名称") }, singleLine = true, modifier = Modifier.fillMaxWidth())
                        OutlinedTextField(value = botHandle, onValueChange = { botHandle = it }, label = { Text("@用户名 (以 _bot 结尾)") }, singleLine = true, modifier = Modifier.fillMaxWidth())
                        OutlinedTextField(value = botDescription, onValueChange = { botDescription = it }, label = { Text("自然语言职责与系统提示词") }, modifier = Modifier.fillMaxWidth())

                        Button(
                            onClick = { onCreateBot(botName, botDescription, botHandle) },
                            modifier = Modifier.fillMaxWidth().testTag("botfather-create-button"),
                        ) {
                            Text("🤖 立即创建并加入对话列表", fontWeight = FontWeight.Bold)
                        }
                    }
                }
            }

            // Desktop v1.5/v1.6 parity: Native Third-Party Coding Agent Bots (Claude Code, Codex, OpenCode, Cursor CLI, Antigravity, ACP)
            item {
                Surface(shape = RoundedCornerShape(18.dp), color = palette.surfaceColor, modifier = Modifier.fillMaxWidth().testTag("botfather-coding-agent-card")) {
                    Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                        Text("💻 第三方编程智能体原生 Bot (Desktop v1.6 对齐)", fontWeight = FontWeight.Bold, fontSize = 15.sp, color = palette.primaryText)
                        Text("直接将 Claude Code、Codex、OpenCode、Cursor CLI、Antigravity 或自定义 ACP 接入为原生 Telegram Bot，支持 Git Worktree 隔离与检查点回滚。", fontSize = 12.sp, color = palette.secondaryText)
                        LazyRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            items(CodingProviderKindUi.entries) { prov ->
                                val sel = selectedCodingProvider == prov
                                Surface(
                                    shape = RoundedCornerShape(12.dp),
                                    color = if (sel) palette.primaryAccent else palette.elevatedSurface,
                                    modifier = Modifier.clickable { selectedCodingProvider = prov },
                                ) {
                                    Text(
                                        text = if (prov.supportsAcp) "${prov.label} (ACP)" else prov.label,
                                        color = if (sel) Color.White else palette.primaryText,
                                        fontSize = 12.sp,
                                        fontWeight = FontWeight.SemiBold,
                                        modifier = Modifier.padding(horizontal = 10.dp, vertical = 6.dp),
                                    )
                                }
                            }
                        }
                        LazyRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            items(CodingPermissionModeUi.entries) { mode ->
                                val sel = selectedCodingPermission == mode
                                Surface(
                                    shape = RoundedCornerShape(10.dp),
                                    color = if (sel) palette.primaryAccent.copy(alpha = 0.20f) else palette.elevatedSurface,
                                    modifier = Modifier.clickable { selectedCodingPermission = mode },
                                ) {
                                    Text(
                                        text = "权限：${mode.label}",
                                        color = if (sel) palette.primaryAccent else palette.secondaryText,
                                        fontSize = 11.sp,
                                        fontWeight = FontWeight.Bold,
                                        modifier = Modifier.padding(horizontal = 10.dp, vertical = 5.dp),
                                    )
                                }
                            }
                        }
                        OutlinedTextField(
                            value = codingWorkspacePath,
                            onValueChange = { codingWorkspacePath = it },
                            label = { Text("绑定宿主机工作区路径") },
                            singleLine = true,
                            modifier = Modifier.fillMaxWidth(),
                        )
                        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.SpaceBetween, modifier = Modifier.fillMaxWidth()) {
                            Text("自动创建隔离 Git Worktree 分支", fontSize = 13.sp, color = palette.primaryText)
                            Switch(checked = codingUseWorktree, onCheckedChange = { codingUseWorktree = it })
                        }
                        Button(
                            onClick = {
                                onCreateCodingBot(
                                    selectedCodingProvider,
                                    "${selectedCodingProvider.label} Coding Bot",
                                    selectedCodingProvider.recommendedHandle,
                                    codingWorkspacePath,
                                    selectedCodingPermission,
                                    codingUseWorktree,
                                )
                            },
                            modifier = Modifier.fillMaxWidth().testTag("botfather-create-coding-bot"),
                        ) {
                            Text("⚡ 创建 ${selectedCodingProvider.label} 原生编程 Bot", fontWeight = FontWeight.Bold)
                        }

                        if (codingBots.isNotEmpty()) {
                            codingBots.forEach { cb ->
                                Surface(shape = RoundedCornerShape(12.dp), color = palette.elevatedSurface, modifier = Modifier.fillMaxWidth()) {
                                    Row(modifier = Modifier.padding(10.dp), verticalAlignment = Alignment.CenterVertically) {
                                        Column(modifier = Modifier.weight(1f)) {
                                            Text("${cb.displayName} (@${cb.username})", fontWeight = FontWeight.Bold, fontSize = 13.sp, color = palette.primaryText)
                                            Text("${cb.provider.label} · ${cb.workspacePath} · 分支 ${cb.activeBranch} · ${cb.totalTurns} 轮", fontSize = 11.sp, color = palette.secondaryText)
                                        }
                                        val nextMode = CodingPermissionModeUi.entries[(cb.permissionMode.ordinal + 1) % CodingPermissionModeUi.entries.size]
                                        TextButton(onClick = { onCycleCodingPermission(cb.botId, nextMode) }) {
                                            Text(cb.permissionMode.label, fontSize = 11.sp)
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            item {
                Text("已部署的机器人 (${existingBots.size})", fontWeight = FontWeight.Bold, fontSize = 14.sp, color = palette.secondaryText)
            }

            items(existingBots, key = { it.id }) { bot ->
                Surface(
                    shape = RoundedCornerShape(14.dp),
                    color = palette.surfaceColor,
                    modifier = Modifier.fillMaxWidth().clickable { onOpenBotChat(bot.id) },
                ) {
                    Row(modifier = Modifier.padding(12.dp), verticalAlignment = Alignment.CenterVertically) {
                        TelegramAvatar(
                            idSeed = bot.id,
                            title = bot.title,
                            size = 46.dp,
                            online = true,
                            isCompanionGhost = bot.isCompanionGhost,
                        )
                        Spacer(Modifier.width(12.dp))
                        Column(modifier = Modifier.weight(1f)) {
                            Text(bot.title, fontWeight = FontWeight.Bold, fontSize = 15.sp, color = palette.primaryText)
                            Text(bot.subtitle, fontSize = 12.sp, color = palette.secondaryText, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        }
                        Button(onClick = { onOpenBotChat(bot.id) }, contentPadding = PaddingValues(horizontal = 12.dp, vertical = 6.dp)) {
                            Text("对话", fontSize = 12.sp)
                        }
                    }
                }
            }
        }
    }
}

// ============================================================================
// 2. Hermes 6-Tab Sovereign AI Studio
// ============================================================================

@Composable
fun HermesStudioScreen(
    studioState: HermesStudioUiState,
    onSelectTab: (HermesStudioTab) -> Unit,
    onBranchSession: (String) -> Unit,
    onRunMoaPrompt: (String) -> Unit,
    onSaveSoul: (String) -> Unit,
    onToggleSkill: (String) -> Unit,
    onExecuteTerminal: (String) -> Unit,
    onAddCronJob: (title: String, expr: String, prompt: String) -> Unit,
    onAdvanceKanban: (String) -> Unit,
    onDecideApproval: (String, Boolean) -> Unit,
    onBack: () -> Unit,
) {
    val palette = LocalTelegramPalette.current
    BackHandler(onBack = onBack)

    var moaInput by remember { mutableStateOf("") }
    var soulDraft by remember(studioState.soulMarkdown) { mutableStateOf(studioState.soulMarkdown) }
    var ttyInput by remember { mutableStateOf("") }
    var cronTitle by remember { mutableStateOf("") }
    var cronExpr by remember { mutableStateOf("0 8 * * *") }
    var cronPrompt by remember { mutableStateOf("汇总今日重要频道消息与待办") }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(palette.appBackground)
            .testTag("telegram-hermes-studio-screen"),
    ) {
        WorkspaceTopBar(
            title = "Hermes AI 6-Tab 主权工作室",
            subtitle = "61 Slash 指令 · 会话分叉 · /moa 多模型协同 · HITL 审批",
            glyph = TelegramGlyph.HERMES_SPARK,
            onBack = onBack,
        )

        // 6-Tab Horizontal Bar
        LazyRow(
            contentPadding = PaddingValues(horizontal = 12.dp, vertical = 8.dp),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            items(HermesStudioTab.entries) { tab ->
                val selected = studioState.activeTab == tab
                Surface(
                    shape = RoundedCornerShape(16.dp),
                    color = if (selected) palette.primaryAccent else palette.surfaceColor,
                    modifier = Modifier.clickable { onSelectTab(tab) },
                ) {
                    Row(modifier = Modifier.padding(horizontal = 12.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                        Text(
                            text = tab.title,
                            color = if (selected) Color.White else palette.primaryText,
                            fontSize = 13.sp,
                            fontWeight = FontWeight.Bold,
                        )
                        Spacer(Modifier.width(5.dp))
                        Text(
                            text = tab.badge,
                            color = if (selected) Color.White.copy(alpha = 0.85f) else palette.primaryAccent,
                            fontSize = 10.sp,
                            fontWeight = FontWeight.SemiBold,
                        )
                    }
                }
            }
        }

        LazyColumn(
            modifier = Modifier.fillMaxSize().navigationBarsPadding(),
            contentPadding = PaddingValues(14.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            when (studioState.activeTab) {
                HermesStudioTab.SESSIONS_PROVIDERS -> {
                    item {
                        Surface(shape = RoundedCornerShape(16.dp), color = palette.surfaceColor, modifier = Modifier.fillMaxWidth()) {
                            Column(modifier = Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                                Text("🧠 Mixture-of-Agents (/moa) 并行辩论推理", fontWeight = FontWeight.Bold, fontSize = 15.sp, color = palette.primaryText)
                                Row(verticalAlignment = Alignment.CenterVertically) {
                                    OutlinedTextField(
                                        value = moaInput,
                                        onValueChange = { moaInput = it },
                                        placeholder = { Text("输入复杂架构或自动化议题…") },
                                        singleLine = true,
                                        modifier = Modifier.weight(1f),
                                    )
                                    Spacer(Modifier.width(8.dp))
                                    Button(onClick = {
                                        onRunMoaPrompt(moaInput)
                                        moaInput = ""
                                    }) { Text("/moa") }
                                }
                            }
                        }
                    }
                    items(studioState.sessions, key = { it.id }) { sess ->
                        Surface(shape = RoundedCornerShape(14.dp), color = palette.surfaceColor, modifier = Modifier.fillMaxWidth()) {
                            Column(modifier = Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                                Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween, verticalAlignment = Alignment.CenterVertically) {
                                    Column(modifier = Modifier.weight(1f)) {
                                        Text(sess.title, fontWeight = FontWeight.Bold, fontSize = 14.sp, color = palette.primaryText)
                                        Text("${sess.provider} · ${sess.model} · ${sess.turnCount} 轮", fontSize = 12.sp, color = palette.primaryAccent)
                                    }
                                    TextButton(onClick = { onBranchSession(sess.id) }) {
                                        Text("🌿 分叉 Branch")
                                    }
                                }
                                if (sess.lastOutput.isNotBlank()) {
                                    Text(sess.lastOutput, fontSize = 12.sp, color = palette.secondaryText)
                                }
                            }
                        }
                    }
                }

                HermesStudioTab.MEMORY_SOUL -> {
                    item {
                        Surface(shape = RoundedCornerShape(16.dp), color = palette.surfaceColor, modifier = Modifier.fillMaxWidth()) {
                            Column(modifier = Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                                Text("📜 SOUL.md 主权人格宪章", fontWeight = FontWeight.Bold, fontSize = 15.sp, color = palette.primaryText)
                                OutlinedTextField(
                                    value = soulDraft,
                                    onValueChange = { soulDraft = it },
                                    modifier = Modifier.fillMaxWidth().height(150.dp),
                                )
                                Button(onClick = { onSaveSoul(soulDraft) }, modifier = Modifier.align(Alignment.End)) {
                                    Text("持久化保存 SOUL.md")
                                }
                            }
                        }
                    }
                    items(studioState.memoryBlocks, key = { it.first }) { (k, v) ->
                        Surface(shape = RoundedCornerShape(14.dp), color = palette.surfaceColor, modifier = Modifier.fillMaxWidth()) {
                            Column(modifier = Modifier.padding(12.dp)) {
                                Text("长期记忆槽 · $k", fontWeight = FontWeight.Bold, fontSize = 13.sp, color = palette.primaryAccent)
                                Spacer(Modifier.height(4.dp))
                                Text(v, fontSize = 13.sp, color = palette.primaryText)
                            }
                        }
                    }
                }

                HermesStudioTab.SKILLS_HUB -> {
                    items(studioState.skills, key = { it.id }) { skill ->
                        Surface(shape = RoundedCornerShape(14.dp), color = palette.surfaceColor, modifier = Modifier.fillMaxWidth()) {
                            Row(modifier = Modifier.padding(12.dp), verticalAlignment = Alignment.CenterVertically) {
                                Column(modifier = Modifier.weight(1f)) {
                                    Text("${skill.name} · [${skill.category}]", fontWeight = FontWeight.Bold, fontSize = 14.sp, color = palette.primaryText)
                                    Text(skill.description, fontSize = 12.sp, color = palette.secondaryText)
                                }
                                Switch(checked = skill.enabled, onCheckedChange = { onToggleSkill(skill.id) })
                            }
                        }
                    }
                }

                HermesStudioTab.TOOLSETS_TERMINAL -> {
                    item {
                        Surface(shape = RoundedCornerShape(16.dp), color = Color(0xFF0D141D), modifier = Modifier.fillMaxWidth()) {
                            Column(modifier = Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                                Text("Hermes Sovereign TTY & Slash Console", color = Color(0xFF4EF2D2), fontFamily = FontFamily.Monospace, fontWeight = FontWeight.Bold, fontSize = 12.sp)
                                studioState.terminalLines.forEach { line ->
                                    Text(line, color = Color(0xFFE2E8F0), fontFamily = FontFamily.Monospace, fontSize = 12.sp)
                                }
                                Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                                    listOf("/help", "uname -a", "/moa status").forEach { preset ->
                                        Surface(
                                            shape = RoundedCornerShape(8.dp),
                                            color = Color.White.copy(alpha = 0.12f),
                                            modifier = Modifier.clickable { onExecuteTerminal(preset) },
                                        ) {
                                            Text(preset, color = Color.White, fontSize = 11.sp, fontFamily = FontFamily.Monospace, modifier = Modifier.padding(horizontal = 8.dp, vertical = 4.dp))
                                        }
                                    }
                                }
                                Row(verticalAlignment = Alignment.CenterVertically) {
                                    OutlinedTextField(
                                        value = ttyInput,
                                        onValueChange = { ttyInput = it },
                                        placeholder = { Text("输入 /slash 或沙箱命令…", color = Color.Gray) },
                                        singleLine = true,
                                        modifier = Modifier.weight(1f),
                                    )
                                    Spacer(Modifier.width(8.dp))
                                    Button(onClick = {
                                        onExecuteTerminal(ttyInput)
                                        ttyInput = ""
                                    }) { Text("运行") }
                                }
                            }
                        }
                    }
                }

                HermesStudioTab.CRON_HEARTBEAT -> {
                    item {
                        Surface(shape = RoundedCornerShape(16.dp), color = palette.surfaceColor, modifier = Modifier.fillMaxWidth()) {
                            Column(modifier = Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                                Text("⏰ 新建自治定时任务 (Cron / Heartbeat)", fontWeight = FontWeight.Bold, fontSize = 14.sp, color = palette.primaryText)
                                OutlinedTextField(value = cronTitle, onValueChange = { cronTitle = it }, label = { Text("任务名称") }, singleLine = true, modifier = Modifier.fillMaxWidth())
                                OutlinedTextField(value = cronExpr, onValueChange = { cronExpr = it }, label = { Text("Cron 表达式") }, singleLine = true, modifier = Modifier.fillMaxWidth())
                                OutlinedTextField(value = cronPrompt, onValueChange = { cronPrompt = it }, label = { Text("触发执行指令") }, singleLine = true, modifier = Modifier.fillMaxWidth())
                                Button(onClick = {
                                    onAddCronJob(cronTitle.ifBlank { "每日摘要巡检" }, cronExpr, cronPrompt)
                                    cronTitle = ""
                                }) { Text("+ 添加定时任务") }
                            }
                        }
                    }
                    items(studioState.cronJobs, key = { it.id }) { job ->
                        Surface(shape = RoundedCornerShape(14.dp), color = palette.surfaceColor, modifier = Modifier.fillMaxWidth()) {
                            Column(modifier = Modifier.padding(12.dp)) {
                                Text("${job.title} (${job.scheduleExpr})", fontWeight = FontWeight.Bold, fontSize = 14.sp, color = palette.primaryText)
                                Text(job.prompt, fontSize = 12.sp, color = palette.secondaryText)
                                Text("上次运行：${job.lastRunStatus}", fontSize = 11.sp, color = Color(0xFF2FA84F))
                            }
                        }
                    }
                }

                HermesStudioTab.KANBAN_APPROVALS -> {
                    item {
                        Text("🛡 高风险动作人工审批队列 (HITL Approvals)", fontWeight = FontWeight.Bold, fontSize = 14.sp, color = palette.primaryText)
                    }
                    items(studioState.approvals, key = { it.id }) { req ->
                        Surface(shape = RoundedCornerShape(14.dp), color = palette.surfaceColor, modifier = Modifier.fillMaxWidth()) {
                            Column(modifier = Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                                Row(horizontalArrangement = Arrangement.SpaceBetween, modifier = Modifier.fillMaxWidth()) {
                                    Text("${req.toolName} · ${req.riskTier.label}", fontWeight = FontWeight.Bold, fontSize = 13.sp, color = palette.destructiveRed)
                                    Text(req.status.uppercase(), fontSize = 11.sp, fontWeight = FontWeight.Bold, color = palette.primaryAccent)
                                }
                                Text(req.summary, fontSize = 13.sp, color = palette.primaryText)
                                if (req.status == "pending") {
                                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                        Button(onClick = { onDecideApproval(req.id, true) }) { Text("批准执行") }
                                        TextButton(onClick = { onDecideApproval(req.id, false) }) { Text("拒绝", color = palette.destructiveRed) }
                                    }
                                }
                            }
                        }
                    }
                    item {
                        Spacer(Modifier.height(4.dp))
                        Text("📋 多智能体协同看板 (点击卡片推进状态流转)", fontWeight = FontWeight.Bold, fontSize = 14.sp, color = palette.primaryText)
                    }
                    items(studioState.kanbanCards, key = { it.id }) { card ->
                        Surface(
                            shape = RoundedCornerShape(14.dp),
                            color = palette.surfaceColor,
                            modifier = Modifier.fillMaxWidth().clickable { onAdvanceKanban(card.id) },
                        ) {
                            Row(modifier = Modifier.padding(12.dp), horizontalArrangement = Arrangement.SpaceBetween, verticalAlignment = Alignment.CenterVertically) {
                                Column(modifier = Modifier.weight(1f)) {
                                    Text(card.title, fontWeight = FontWeight.SemiBold, fontSize = 14.sp, color = palette.primaryText)
                                    Text("负责智能体：${card.assigneeAgent}", fontSize = 12.sp, color = palette.secondaryText)
                                }
                                Surface(shape = RoundedCornerShape(10.dp), color = palette.primaryAccent.copy(alpha = 0.15f)) {
                                    Text(card.column.label, color = palette.primaryAccent, fontSize = 11.sp, fontWeight = FontWeight.Bold, modifier = Modifier.padding(horizontal = 9.dp, vertical = 4.dp))
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ============================================================================
// 3. Magpie Zero-Subscription AI Gateway Console (48 Provider Presets)
// ============================================================================

@Composable
fun MagpieGatewayScreen(
    gatewayState: MagpieGatewayUiState,
    onSelectStrategy: (MagpieRoutingStrategy) -> Unit,
    onConfigureProvider: (presetId: String, apiKey: String) -> Unit,
    onImportEnvOrUri: (String) -> Unit,
    onTestPiiMask: (String) -> Unit,
    onSyncModelsDev: () -> Unit = {},
    onScanLocalSubscriptions: () -> Unit = {},
    onBack: () -> Unit,
) {
    val palette = LocalTelegramPalette.current
    BackHandler(onBack = onBack)

    var searchQuery by remember { mutableStateOf("") }
    var selectedRegion by remember { mutableStateOf("ALL") }
    var editingPreset by remember { mutableStateOf<MagpieProviderPreset?>(null) }
    var showBulkImportDialog by remember { mutableStateOf(false) }
    var piiSample by remember { mutableStateOf("联系邮箱 dev@ombhrum.com 手机 13800138000 密钥 sk-demoSecretToken123456") }

    val filteredPresets = remember(gatewayState.presets, searchQuery, selectedRegion) {
        gatewayState.presets.filter { p ->
            val regionOk = selectedRegion == "ALL" || p.region.equals(selectedRegion, ignoreCase = true)
            val queryOk = searchQuery.isBlank() ||
                p.name.contains(searchQuery, ignoreCase = true) ||
                p.flagshipModel.contains(searchQuery, ignoreCase = true) ||
                p.id.contains(searchQuery, ignoreCase = true)
            regionOk && queryOk
        }
    }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(palette.appBackground)
            .testTag("telegram-magpie-gateway-screen"),
    ) {
        WorkspaceTopBar(
            title = "Magpie 免订阅 AI 网关 (${gatewayState.presets.size} 通道)",
            subtitle = "Rosetta 协议互转 · models.dev 实时目录 · Vertex/Gemini 并行工具对齐",
            glyph = TelegramGlyph.MAGPIE_GATEWAY,
            onBack = onBack,
        )

        LazyColumn(
            modifier = Modifier.fillMaxSize().navigationBarsPadding(),
            contentPadding = PaddingValues(14.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            // Usage & Cost Ledger Banner
            item {
                Surface(shape = RoundedCornerShape(18.dp), color = palette.surfaceColor, modifier = Modifier.fillMaxWidth()) {
                    Column(modifier = Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween, verticalAlignment = Alignment.CenterVertically) {
                            Text("📊 Token 成本账本与 Rosetta 路由追踪", fontWeight = FontWeight.Bold, fontSize = 14.sp, color = palette.primaryText)
                            TextButton(onClick = { showBulkImportDialog = true }) {
                                Text("📥 批量导入 .env / URI")
                            }
                        }
                        Row(horizontalArrangement = Arrangement.SpaceBetween, modifier = Modifier.fillMaxWidth()) {
                            GatewayMetric("总请求", "${gatewayState.totalRequests}")
                            GatewayMetric("总 Tokens", "${(gatewayState.totalPromptTokens + gatewayState.totalCompletionTokens) / 1000}K")
                            GatewayMetric("累计直连成本", "$%.4f".format(gatewayState.totalCostUsd))
                            GatewayMetric("PII 拦截", "${gatewayState.piiRedactedCount} 次")
                        }
                        Surface(shape = RoundedCornerShape(10.dp), color = palette.elevatedSurface, modifier = Modifier.fillMaxWidth()) {
                            Text(gatewayState.lastRouteExplanation, fontSize = 12.sp, color = palette.primaryAccent, modifier = Modifier.padding(8.dp))
                        }
                    }
                }
            }

            // Desktop v1.3.2-v1.6.2 parity: Live models.dev catalog & Local Subscription Sync
            item {
                Surface(shape = RoundedCornerShape(18.dp), color = palette.surfaceColor, modifier = Modifier.fillMaxWidth().testTag("magpie-models-dev-sync-card")) {
                    Column(modifier = Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        Text("🌐 实时模型目录 (models.dev) & 本地订阅同步", fontWeight = FontWeight.Bold, fontSize = 14.sp, color = palette.primaryText)
                        Text(gatewayState.modelsDevStatusLabel, fontSize = 12.sp, color = palette.secondaryText)
                        Text(gatewayState.localSubscriptionsSummary, fontSize = 11.sp, color = palette.hintText)
                        Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.fillMaxWidth()) {
                            Button(onClick = onSyncModelsDev, modifier = Modifier.weight(1f)) {
                                Text("刷新 models.dev", fontSize = 12.sp)
                            }
                            Button(onClick = onScanLocalSubscriptions, modifier = Modifier.weight(1f)) {
                                Text("同步本地订阅", fontSize = 12.sp)
                            }
                        }
                        if (gatewayState.discoveredModels.isNotEmpty()) {
                            LazyRow(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                                items(gatewayState.discoveredModels.take(12)) { modelId ->
                                    Surface(shape = RoundedCornerShape(8.dp), color = palette.elevatedSurface) {
                                        Text(modelId, fontSize = 11.sp, fontFamily = FontFamily.Monospace, color = palette.primaryAccent, modifier = Modifier.padding(horizontal = 8.dp, vertical = 4.dp))
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // 6 Routing Strategies
            item {
                LazyRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    items(MagpieRoutingStrategy.entries) { st ->
                        val sel = gatewayState.strategy == st
                        Surface(
                            shape = RoundedCornerShape(14.dp),
                            color = if (sel) palette.primaryAccent else palette.surfaceColor,
                            modifier = Modifier.clickable { onSelectStrategy(st) },
                        ) {
                            Text(
                                text = st.label,
                                color = if (sel) Color.White else palette.primaryText,
                                fontSize = 12.sp,
                                fontWeight = FontWeight.Bold,
                                modifier = Modifier.padding(horizontal = 12.dp, vertical = 8.dp),
                            )
                        }
                    }
                }
            }

            // Live PII & API Key Masking Sandbox
            item {
                Surface(shape = RoundedCornerShape(16.dp), color = palette.surfaceColor, modifier = Modifier.fillMaxWidth()) {
                    Column(modifier = Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                        Text("🛡 端侧 PII 隐私与密钥自动脱敏网关测试", fontWeight = FontWeight.Bold, fontSize = 13.sp, color = palette.primaryText)
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            OutlinedTextField(value = piiSample, onValueChange = { piiSample = it }, singleLine = true, modifier = Modifier.weight(1f))
                            Spacer(Modifier.width(8.dp))
                            Button(onClick = { onTestPiiMask(piiSample) }) { Text("脱敏") }
                        }
                        if (gatewayState.lastMaskedPreview.isNotBlank()) {
                            Text("脱敏输出：${gatewayState.lastMaskedPreview}", fontSize = 12.sp, fontFamily = FontFamily.Monospace, color = Color(0xFF2FA84F))
                        }
                    }
                }
            }

            // Search & Region Filters for 48 Presets
            item {
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    OutlinedTextField(
                        value = searchQuery,
                        onValueChange = { searchQuery = it },
                        placeholder = { Text("搜索 48 家全球/国产/云端/本地模型供应商…") },
                        singleLine = true,
                        modifier = Modifier.fillMaxWidth(),
                    )
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        listOf("ALL" to "全部 (${gatewayState.presets.size})", "Global" to "国际前沿", "CN" to "国产旗舰", "Cloud" to "云厂商", "Local" to "本地离线").forEach { (reg, label) ->
                            val sel = selectedRegion == reg
                            Surface(
                                shape = RoundedCornerShape(12.dp),
                                color = if (sel) palette.primaryAccent.copy(alpha = 0.2f) else palette.surfaceColor,
                                modifier = Modifier.clickable { selectedRegion = reg },
                            ) {
                                Text(label, color = if (sel) palette.primaryAccent else palette.secondaryText, fontSize = 12.sp, fontWeight = FontWeight.SemiBold, modifier = Modifier.padding(horizontal = 10.dp, vertical = 6.dp))
                            }
                        }
                    }
                }
            }

            items(filteredPresets, key = { it.id }) { preset ->
                Surface(
                    shape = RoundedCornerShape(14.dp),
                    color = palette.surfaceColor,
                    modifier = Modifier.fillMaxWidth().clickable { editingPreset = preset },
                ) {
                    Row(modifier = Modifier.padding(12.dp), verticalAlignment = Alignment.CenterVertically) {
                        Column(modifier = Modifier.weight(1f)) {
                            Row(verticalAlignment = Alignment.CenterVertically) {
                                Text(preset.name, fontWeight = FontWeight.Bold, fontSize = 14.sp, color = palette.primaryText)
                                Spacer(Modifier.width(6.dp))
                                Surface(shape = RoundedCornerShape(6.dp), color = palette.elevatedSurface) {
                                    Text(preset.protocol, fontSize = 10.sp, color = palette.primaryAccent, modifier = Modifier.padding(horizontal = 6.dp, vertical = 2.dp))
                                }
                            }
                            Text("${preset.flagshipModel} · $${preset.inputUsdPer1M}/1M · ${preset.defaultBaseUrl}", fontSize = 11.sp, color = palette.secondaryText, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        }
                        Surface(
                            shape = RoundedCornerShape(10.dp),
                            color = if (preset.configured) Color(0x222FA84F) else palette.elevatedSurface,
                        ) {
                            Text(
                                text = if (preset.configured) "✓ 已就绪" else "配置 Key",
                                color = if (preset.configured) Color(0xFF2FA84F) else palette.primaryAccent,
                                fontSize = 11.sp,
                                fontWeight = FontWeight.Bold,
                                modifier = Modifier.padding(horizontal = 9.dp, vertical = 5.dp),
                            )
                        }
                    }
                }
            }
        }
    }

    editingPreset?.let { preset ->
        var keyInput by remember(preset.id) { mutableStateOf("") }
        AlertDialog(
            onDismissRequest = { editingPreset = null },
            title = { Text("🔑 配置 ${preset.name}") },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text("端点：${preset.defaultBaseUrl}\n推荐模型：${preset.flagshipModel}\n密钥仅保存在本机 Android Keystore，绝不硬编码或上传第三方服务器。", fontSize = 12.sp, color = palette.secondaryText)
                    OutlinedTextField(
                        value = keyInput,
                        onValueChange = { keyInput = it },
                        label = { Text("输入您自己的 API Key") },
                        visualTransformation = PasswordVisualTransformation(),
                        singleLine = true,
                        modifier = Modifier.fillMaxWidth(),
                    )
                }
            },
            confirmButton = {
                Button(onClick = {
                    onConfigureProvider(preset.id, keyInput)
                    editingPreset = null
                }) { Text("安全保存") }
            },
            dismissButton = { TextButton(onClick = { editingPreset = null }) { Text("取消") } },
        )
    }

    if (showBulkImportDialog) {
        var rawImport by remember { mutableStateOf("") }
        AlertDialog(
            onDismissRequest = { showBulkImportDialog = false },
            title = { Text("📥 批量导入 magpie://import 或 .env") },
            text = {
                OutlinedTextField(
                    value = rawImport,
                    onValueChange = { rawImport = it },
                    placeholder = { Text("粘贴 .env 环境变量或 magpie://import?...") },
                    modifier = Modifier.fillMaxWidth().height(120.dp),
                )
            },
            confirmButton = {
                Button(onClick = {
                    onImportEnvOrUri(rawImport)
                    showBulkImportDialog = false
                }) { Text("解析导入") }
            },
            dismissButton = { TextButton(onClick = { showBulkImportDialog = false }) { Text("取消") } },
        )
    }
}

// ============================================================================
// 4. Cloud Computer Control, Action Bus (R0..R4) & 7-State Global Task FSM
// ============================================================================

@Composable
fun CloudComputerWorkbenchScreen(
    computerState: CloudComputerUiState,
    onSelectNode: (String) -> Unit,
    onAcquireLease: (String) -> Unit,
    onEmergencyTakeover: (String) -> Unit,
    onExecuteOp: (String) -> Unit,
    onAdvanceTaskFsm: (String) -> Unit,
    onToggleRemoteAssist: (Boolean) -> Unit = {},
    onUpdateAssistCode: (String) -> Unit = {},
    onUpdateStreamSettings: (RemoteStreamSettingsUi) -> Unit = {},
    onIssueMcpGrant: (String) -> Unit = {},
    onRevokeMcpGrant: (String) -> Unit = {},
    onOpenMikaWorkspace: () -> Unit,
    onBack: () -> Unit,
) {
    val palette = LocalTelegramPalette.current
    BackHandler(onBack = onBack)

    var opCommand by remember { mutableStateOf("df -h && uptime") }
    var assistCodeDraft by remember(computerState.assistConfig.currentCode) { mutableStateOf(computerState.assistConfig.currentCode) }
    var mcpClientNameDraft by remember { mutableStateOf("Claude Desktop MCP") }
    val selectedNode = computerState.nodes.firstOrNull { it.id == computerState.selectedNodeId } ?: computerState.nodes.firstOrNull()

    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(palette.appBackground)
            .testTag("telegram-cloud-computer-screen"),
    ) {
        WorkspaceTopBar(
            title = "Cloud Computer 云电脑与 UU-Remote 智控",
            subtitle = "9 位远程协助 · 4K/120fps 串流 · OAuth 2.1 MCP · Action Bus (${computerState.actionBusCategoryCount} 域)",
            glyph = TelegramGlyph.CLOUD_COMPUTER,
            onBack = onBack,
        )

        LazyColumn(
            modifier = Modifier.fillMaxSize().navigationBarsPadding(),
            contentPadding = PaddingValues(14.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            // Desktop v1.5.0 parity: UU-Remote 9-Digit Remote Assist & Stream Settings
            item {
                Surface(shape = RoundedCornerShape(18.dp), color = palette.surfaceColor, modifier = Modifier.fillMaxWidth().testTag("cloud-computer-uu-remote-card")) {
                    Column(modifier = Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.SpaceBetween, modifier = Modifier.fillMaxWidth()) {
                            Column(modifier = Modifier.weight(1f)) {
                                Text("🛰 UU-Remote 远程协助与高清串流 (v1.5+)", fontWeight = FontWeight.Bold, fontSize = 15.sp, color = palette.primaryText)
                                Text("9 位永久设备协助 ID：${computerState.assistConfig.assistId}", fontFamily = FontFamily.Monospace, fontWeight = FontWeight.Bold, fontSize = 13.sp, color = palette.primaryAccent)
                            }
                            Switch(
                                checked = computerState.assistConfig.allowAssist,
                                onCheckedChange = onToggleRemoteAssist,
                            )
                        }
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            OutlinedTextField(
                                value = assistCodeDraft,
                                onValueChange = { assistCodeDraft = it.take(16) },
                                label = { Text("远程协助验证码 (${computerState.assistConfig.codeMode})") },
                                singleLine = true,
                                modifier = Modifier.weight(1f),
                            )
                            Spacer(Modifier.width(8.dp))
                            Button(onClick = { onUpdateAssistCode(assistCodeDraft) }) { Text("更新") }
                        }
                        val ss = computerState.streamSettings
                        LazyRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            items(listOf("auto", "h264", "h265", "vp8")) { codec ->
                                val sel = ss.codec == codec
                                Surface(
                                    shape = RoundedCornerShape(10.dp),
                                    color = if (sel) palette.primaryAccent else palette.elevatedSurface,
                                    modifier = Modifier.clickable { onUpdateStreamSettings(ss.copy(codec = codec)) },
                                ) {
                                    Text(codec.uppercase(), color = if (sel) Color.White else palette.primaryText, fontSize = 11.sp, fontWeight = FontWeight.Bold, modifier = Modifier.padding(horizontal = 10.dp, vertical = 5.dp))
                                }
                            }
                            items(listOf(30, 60, 120)) { fps ->
                                val sel = ss.targetFps == fps
                                Surface(
                                    shape = RoundedCornerShape(10.dp),
                                    color = if (sel) palette.primaryAccent else palette.elevatedSurface,
                                    modifier = Modifier.clickable { onUpdateStreamSettings(ss.copy(targetFps = fps)) },
                                ) {
                                    Text("${fps}fps", color = if (sel) Color.White else palette.primaryText, fontSize = 11.sp, fontWeight = FontWeight.Bold, modifier = Modifier.padding(horizontal = 10.dp, vertical = 5.dp))
                                }
                            }
                        }
                        Row(horizontalArrangement = Arrangement.SpaceBetween, verticalAlignment = Alignment.CenterVertically, modifier = Modifier.fillMaxWidth()) {
                            Text("隐私黑屏保护 (Privacy Screen) · 音频/麦克风透传", fontSize = 12.sp, color = palette.secondaryText, modifier = Modifier.weight(1f))
                            Switch(
                                checked = ss.privacyScreen,
                                onCheckedChange = { onUpdateStreamSettings(ss.copy(privacyScreen = it)) },
                            )
                        }
                    }
                }
            }

            // Desktop v1.5.0 parity: Official OAuth 2.1 Remote MCP Grants
            item {
                Surface(shape = RoundedCornerShape(18.dp), color = palette.surfaceColor, modifier = Modifier.fillMaxWidth().testTag("cloud-computer-mcp-grants-card")) {
                    Column(modifier = Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        Text("🔑 Official OAuth 2.1 Remote MCP 授权 (${computerState.mcpGrants.size})", fontWeight = FontWeight.Bold, fontSize = 14.sp, color = palette.primaryText)
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            OutlinedTextField(
                                value = mcpClientNameDraft,
                                onValueChange = { mcpClientNameDraft = it },
                                label = { Text("MCP 客户端名称") },
                                singleLine = true,
                                modifier = Modifier.weight(1f),
                            )
                            Spacer(Modifier.width(8.dp))
                            Button(onClick = { onIssueMcpGrant(mcpClientNameDraft) }) { Text("签发") }
                        }
                        computerState.mcpGrants.forEach { g ->
                            Surface(shape = RoundedCornerShape(10.dp), color = palette.elevatedSurface, modifier = Modifier.fillMaxWidth()) {
                                Row(modifier = Modifier.padding(10.dp), verticalAlignment = Alignment.CenterVertically) {
                                    Column(modifier = Modifier.weight(1f)) {
                                        Text(g.clientName, fontWeight = FontWeight.Bold, fontSize = 13.sp, color = palette.primaryText)
                                        Text("Scopes: ${g.scopes.joinToString(", ")}", fontSize = 11.sp, color = palette.secondaryText)
                                    }
                                    TextButton(onClick = { onRevokeMcpGrant(g.familyId) }) {
                                        Text("撤销", color = palette.destructiveRed, fontSize = 12.sp)
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Node Selector Cards
            item {
                LazyRow(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                    items(computerState.nodes, key = { it.id }) { node ->
                        val sel = node.id == computerState.selectedNodeId
                        Surface(
                            shape = RoundedCornerShape(16.dp),
                            color = if (sel) palette.primaryAccent else palette.surfaceColor,
                            modifier = Modifier.width(220.dp).clickable { onSelectNode(node.id) },
                        ) {
                            Column(modifier = Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                                Text(node.name, fontWeight = FontWeight.Bold, fontSize = 14.sp, color = if (sel) Color.White else palette.primaryText)
                                Text("${node.kind.label} · ${node.osInfo}", fontSize = 11.sp, color = if (sel) Color.White.copy(alpha = 0.82f) else palette.secondaryText)
                                Text("CPU ${node.cpuLoadPercent}% · RAM ${node.memorySummary}", fontSize = 11.sp, color = if (sel) Color.White else palette.primaryAccent)
                            }
                        }
                    }
                }
            }

            // CUA Lease & Emergency Human Takeover Panel
            selectedNode?.let { node ->
                item {
                    Surface(shape = RoundedCornerShape(18.dp), color = palette.surfaceColor, modifier = Modifier.fillMaxWidth()) {
                        Column(modifier = Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                            Text("🖥 节点控制权租约 · ${node.name}", fontWeight = FontWeight.Bold, fontSize = 15.sp, color = palette.primaryText)
                            Text(
                                text = when {
                                    node.humanTakeoverActive -> "🚨 状态：人工紧急接管生效中（自动智能体已冻结）"
                                    node.leaseOwner != null -> "🤖 当前持有租约：${node.leaseOwner}"
                                    else -> "🟢 空闲待命（无活跃自动租约）"
                                },
                                fontSize = 13.sp,
                                color = if (node.humanTakeoverActive) palette.destructiveRed else palette.primaryAccent,
                                fontWeight = FontWeight.SemiBold,
                            )
                            Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.fillMaxWidth()) {
                                Button(onClick = { onAcquireLease(node.id) }, modifier = Modifier.weight(1f)) {
                                    Text("授予 CUA 租约")
                                }
                                Button(
                                    onClick = { onEmergencyTakeover(node.id) },
                                    colors = ButtonDefaults.buttonColors(containerColor = palette.destructiveRed, contentColor = Color.White),
                                    modifier = Modifier.weight(1f).testTag("computer-emergency-takeover"),
                                ) {
                                    Text("🚨 紧急人工接管", fontWeight = FontWeight.Bold)
                                }
                            }
                            if (node.kind == ComputerNodeKind.LOCAL_DEVICE) {
                                TextButton(onClick = onOpenMikaWorkspace) {
                                    Text("📱 前往 Mika 本机操控中心查看无障碍与虚拟屏幕详情 →")
                                }
                            }

                            Row(verticalAlignment = Alignment.CenterVertically) {
                                OutlinedTextField(value = opCommand, onValueChange = { opCommand = it }, label = { Text("节点远程/本机指令") }, singleLine = true, modifier = Modifier.weight(1f))
                                Spacer(Modifier.width(8.dp))
                                Button(onClick = { onExecuteOp(opCommand) }) { Text("执行") }
                            }
                            Surface(shape = RoundedCornerShape(10.dp), color = Color(0xFF0D141D), modifier = Modifier.fillMaxWidth()) {
                                Text(computerState.lastOpOutput, color = Color(0xFF4EF2D2), fontFamily = FontFamily.Monospace, fontSize = 12.sp, modifier = Modifier.padding(10.dp))
                            }
                        }
                    }
                }
            }

            // Global Tasks 7-State FSM
            item {
                Text("🔄 全局任务 7 态有限状态机 (Global Tasks FSM · 点击推进)", fontWeight = FontWeight.Bold, fontSize = 14.sp, color = palette.primaryText)
            }
            items(computerState.globalTasks, key = { it.id }) { task ->
                Surface(
                    shape = RoundedCornerShape(14.dp),
                    color = palette.surfaceColor,
                    modifier = Modifier.fillMaxWidth().clickable { onAdvanceTaskFsm(task.id) },
                ) {
                    Column(modifier = Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                        Row(horizontalArrangement = Arrangement.SpaceBetween, modifier = Modifier.fillMaxWidth()) {
                            Text(task.title, fontWeight = FontWeight.Bold, fontSize = 14.sp, color = palette.primaryText, modifier = Modifier.weight(1f))
                            Text(task.state.label, fontSize = 12.sp, fontWeight = FontWeight.Bold, color = palette.primaryAccent)
                        }
                        Text(task.detail, fontSize = 12.sp, color = palette.secondaryText)
                        LinearProgressIndicator(
                            progress = { (task.progressPercent / 100f).coerceIn(0f, 1f) },
                            modifier = Modifier.fillMaxWidth().height(5.dp).clip(RoundedCornerShape(3.dp)),
                        )
                    }
                }
            }

            // Action Bus R0..R4 Explorer
            item {
                Text("⚡ Action Bus 统一风险分级总线 (R0 ~ R4)", fontWeight = FontWeight.Bold, fontSize = 14.sp, color = palette.primaryText)
            }
            items(computerState.actionBusTools, key = { it.name }) { tool ->
                val tierColor = when (tool.riskTier) {
                    ActionRiskTier.R0_READ, ActionRiskTier.R1_LOW -> Color(0xFF2FA84F)
                    ActionRiskTier.R2_MEDIUM -> Color(0xFFD99B26)
                    ActionRiskTier.R3_HIGH, ActionRiskTier.R4_CRITICAL -> palette.destructiveRed
                }
                Surface(shape = RoundedCornerShape(14.dp), color = palette.surfaceColor, modifier = Modifier.fillMaxWidth()) {
                    Column(modifier = Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                        Row(horizontalArrangement = Arrangement.SpaceBetween, modifier = Modifier.fillMaxWidth()) {
                            Text(tool.name, fontFamily = FontFamily.Monospace, fontWeight = FontWeight.Bold, fontSize = 13.sp, color = palette.primaryText)
                            Surface(shape = RoundedCornerShape(8.dp), color = tierColor.copy(alpha = 0.16f)) {
                                Text(tool.riskTier.label, color = tierColor, fontSize = 10.sp, fontWeight = FontWeight.Bold, modifier = Modifier.padding(horizontal = 8.dp, vertical = 2.dp))
                            }
                        }
                        Text(tool.description, fontSize = 12.sp, color = palette.secondaryText)
                        Text(tool.schemaPreview, fontFamily = FontFamily.Monospace, fontSize = 11.sp, color = palette.hintText)
                    }
                }
            }
        }
    }
}

@Composable
internal fun WorkspaceTopBar(
    title: String,
    subtitle: String,
    glyph: TelegramGlyph,
    onBack: () -> Unit,
) {
    val palette = LocalTelegramPalette.current
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .background(palette.topBarBackground)
            .statusBarsPadding()
            .height(58.dp)
            .padding(horizontal = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(
            modifier = Modifier.size(40.dp).clip(CircleShape).clickable(onClick = onBack),
            contentAlignment = Alignment.Center,
        ) {
            TelegramIcon(TelegramGlyph.BACK, tint = palette.topBarContent, size = 22.dp)
        }
        Spacer(Modifier.width(6.dp))
        TelegramIcon(glyph, tint = palette.topBarContent, size = 22.dp)
        Spacer(Modifier.width(10.dp))
        Column(modifier = Modifier.weight(1f)) {
            Text(title, color = palette.topBarContent, fontWeight = FontWeight.Bold, fontSize = 16.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(subtitle, color = palette.topBarSubtitle, fontSize = 11.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
    }
}

@Composable
private fun GatewayMetric(title: String, value: String) {
    val palette = LocalTelegramPalette.current
    Column(horizontalAlignment = Alignment.CenterHorizontally) {
        Text(value, fontWeight = FontWeight.Bold, fontSize = 15.sp, color = palette.primaryText)
        Text(title, fontSize = 11.sp, color = palette.secondaryText)
    }
}
