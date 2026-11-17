package com.ombhrum.fabushi.telegram

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.Canvas
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
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import java.security.MessageDigest

@Composable
fun TonWalletScreen(
    walletState: TonWalletUiState,
    onCreateNewWallet: () -> Unit,
    onImportMnemonic: (String) -> Unit,
    onSendTon: (recipient: String, amountTon: Double, memo: String) -> Unit,
    onSwapTonToUsdt: (Double) -> Unit,
    onStakeTon: (Double) -> Unit,
    onEmulateTonConnect: (String) -> Unit,
    onResolveTonConnect: (Boolean) -> Unit,
    onBack: () -> Unit,
) {
    val palette = LocalTelegramPalette.current
    val context = LocalContext.current
    BackHandler(onBack = onBack)

    var activeSection by remember { mutableIntStateOf(0) } // 0 = Activity, 1 = Collectibles, 2 = Mnemonic/Security
    var showSendDialog by remember { mutableStateOf(false) }
    var showReceiveQrDialog by remember { mutableStateOf(false) }
    var showSwapDialog by remember { mutableStateOf(false) }
    var showStakeDialog by remember { mutableStateOf(false) }
    var showImportDialog by remember { mutableStateOf(false) }
    var tonConnectInput by remember { mutableStateOf("tc://?v=2&id=fabushi_merit_dapp&r=mint_lotus_sbt") }

    val totalUsdEstimate = walletState.tonBalance * 5.42 + walletState.usdtBalance + walletState.starsBalance * 0.018 + walletState.stakedTon * 5.42

    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(palette.appBackground)
            .testTag("telegram-ton-wallet-screen"),
    ) {
        // Top AppBar
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .background(palette.topBarBackground)
                .statusBarsPadding()
                .height(56.dp)
                .padding(horizontal = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Box(
                modifier = Modifier.size(40.dp).clip(CircleShape).clickable(onClick = onBack),
                contentAlignment = Alignment.Center,
            ) {
                TelegramIcon(TelegramGlyph.BACK, tint = palette.topBarContent, size = 22.dp)
            }
            Spacer(Modifier.width(8.dp))
            TelegramIcon(TelegramGlyph.TON_DIAMOND, tint = Color(0xFF64D2FF), size = 22.dp)
            Spacer(Modifier.width(8.dp))
            Column(modifier = Modifier.weight(1f)) {
                Text("TON Space & Stars 钱包", color = palette.topBarContent, fontWeight = FontWeight.Bold, fontSize = 17.sp)
                Text("自托管 Wallet v4R2 · Ed25519 硬件级签名", color = palette.topBarSubtitle, fontSize = 11.sp)
            }
            TextButton(onClick = { showImportDialog = true }) {
                Text("导入助记词", color = Color.White, fontSize = 12.sp)
            }
        }

        LazyColumn(
            modifier = Modifier.fillMaxSize().navigationBarsPadding(),
            contentPadding = PaddingValues(14.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            if (!walletState.statusBanner.isNullOrBlank()) {
                item {
                    Surface(shape = RoundedCornerShape(12.dp), color = palette.primaryAccent.copy(alpha = 0.15f), modifier = Modifier.fillMaxWidth()) {
                        Text(walletState.statusBanner, color = palette.primaryAccent, fontSize = 13.sp, fontWeight = FontWeight.Medium, modifier = Modifier.padding(10.dp))
                    }
                }
            }

            // Hero Balance Card
            item {
                Box(
                    modifier = Modifier
                        .fillMaxWidth()
                        .clip(RoundedCornerShape(22.dp))
                        .background(Brush.linearGradient(listOf(Color(0xFF0088CC), Color(0xFF1A4F8B), Color(0xFF13294B))))
                        .padding(18.dp),
                ) {
                    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                        Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween, verticalAlignment = Alignment.CenterVertically) {
                            Text("总资产估值", color = Color.White.copy(alpha = 0.78f), fontSize = 13.sp)
                            Surface(
                                shape = RoundedCornerShape(12.dp),
                                color = Color.White.copy(alpha = 0.14f),
                                modifier = Modifier.clickable { copyAddress(context, walletState.addressFriendly) },
                            ) {
                                Text(
                                    text = "${walletState.addressFriendly.take(10)}…${walletState.addressFriendly.takeLast(6)} 📋",
                                    color = Color.White,
                                    fontFamily = FontFamily.Monospace,
                                    fontSize = 11.sp,
                                    modifier = Modifier.padding(horizontal = 10.dp, vertical = 4.dp),
                                )
                            }
                        }

                        Text(
                            text = "$${"%,.2f".format(totalUsdEstimate)} USD",
                            color = Color.White,
                            fontSize = 30.sp,
                            fontWeight = FontWeight.Bold,
                        )

                        Row(horizontalArrangement = Arrangement.spacedBy(10.dp), modifier = Modifier.fillMaxWidth()) {
                            AssetPill("💎 TON", "%.2f".format(walletState.tonBalance), Modifier.weight(1f))
                            AssetPill("💵 USDT", "%.2f".format(walletState.usdtBalance), Modifier.weight(1f))
                            AssetPill("⭐ Stars", "${walletState.starsBalance}", Modifier.weight(1f))
                        }

                        // Quick Action Buttons
                        Row(horizontalArrangement = Arrangement.spacedBy(10.dp), modifier = Modifier.fillMaxWidth()) {
                            WalletActionButton("↗ 转账", Modifier.weight(1f)) { showSendDialog = true }
                            WalletActionButton("↙ 收款码", Modifier.weight(1f)) { showReceiveQrDialog = true }
                            WalletActionButton("⇄ 闪兑", Modifier.weight(1f)) { showSwapDialog = true }
                            WalletActionButton("🥩 质押", Modifier.weight(1f)) { showStakeDialog = true }
                        }
                    }
                }
            }

            // TonConnect 2.0 Sandbox Emulator Card
            item {
                Surface(shape = RoundedCornerShape(16.dp), color = palette.surfaceColor, modifier = Modifier.fillMaxWidth()) {
                    Column(modifier = Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        Text("🔗 TonConnect 2.0 DApp 交易沙箱预演", fontWeight = FontWeight.Bold, fontSize = 14.sp, color = palette.primaryText)
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            OutlinedTextField(
                                value = tonConnectInput,
                                onValueChange = { tonConnectInput = it },
                                singleLine = true,
                                label = { Text("tc:// 连接或交易请求 URI") },
                                modifier = Modifier.weight(1f),
                            )
                            Spacer(Modifier.width(8.dp))
                            Button(onClick = { onEmulateTonConnect(tonConnectInput) }) {
                                Text("预演")
                            }
                        }
                    }
                }
            }

            // Section Tabs
            item {
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    listOf(0 to "📜 链上流水", 1 to "🎁 NFT 与星礼", 2 to "🔑 24词助记词管理").forEach { (idx, label) ->
                        val sel = activeSection == idx
                        Surface(
                            shape = RoundedCornerShape(16.dp),
                            color = if (sel) palette.primaryAccent else palette.surfaceColor,
                            modifier = Modifier.clickable { activeSection = idx },
                        ) {
                            Text(
                                text = label,
                                color = if (sel) Color.White else palette.primaryText,
                                fontSize = 12.sp,
                                fontWeight = FontWeight.Bold,
                                modifier = Modifier.padding(horizontal = 12.dp, vertical = 8.dp),
                            )
                        }
                    }
                }
            }

            when (activeSection) {
                0 -> {
                    items(walletState.transactions, key = { it.id }) { tx ->
                        Surface(shape = RoundedCornerShape(14.dp), color = palette.surfaceColor, modifier = Modifier.fillMaxWidth()) {
                            Row(
                                modifier = Modifier.padding(12.dp),
                                verticalAlignment = Alignment.CenterVertically,
                                horizontalArrangement = Arrangement.SpaceBetween,
                            ) {
                                Column(modifier = Modifier.weight(1f)) {
                                    Text(tx.title, fontWeight = FontWeight.Bold, fontSize = 14.sp, color = palette.primaryText)
                                    Text("${tx.peerAddress} · ${tx.timestampLabel}", color = palette.secondaryText, fontSize = 12.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
                                    if (tx.memo.isNotBlank()) {
                                        Text("备注：${tx.memo}", color = palette.hintText, fontSize = 11.sp)
                                    }
                                }
                                Text(
                                    text = tx.amountLabel,
                                    color = if (tx.amountLabel.startsWith("+")) Color(0xFF2FA84F) else palette.primaryText,
                                    fontWeight = FontWeight.Bold,
                                    fontSize = 14.sp,
                                )
                            }
                        }
                    }
                }

                1 -> {
                    items(walletState.collectibles, key = { it.id }) { nft ->
                        Surface(shape = RoundedCornerShape(14.dp), color = palette.surfaceColor, modifier = Modifier.fillMaxWidth()) {
                            Row(modifier = Modifier.padding(12.dp), verticalAlignment = Alignment.CenterVertically) {
                                Box(
                                    modifier = Modifier.size(48.dp).clip(RoundedCornerShape(12.dp)).background(palette.primaryAccent.copy(alpha = 0.14f)),
                                    contentAlignment = Alignment.Center,
                                ) {
                                    Text(nft.badgeEmoji, fontSize = 26.sp)
                                }
                                Spacer(Modifier.width(12.dp))
                                Column(modifier = Modifier.weight(1f)) {
                                    Text(nft.name, fontWeight = FontWeight.Bold, fontSize = 14.sp, color = palette.primaryText)
                                    Text("${nft.collection}${if (nft.isGift) " · Telegram Star Gift" else " · TON NFT"}", color = palette.secondaryText, fontSize = 12.sp)
                                }
                                Text("地板价 ${nft.floorTon} TON", color = palette.primaryAccent, fontWeight = FontWeight.Bold, fontSize = 12.sp)
                            }
                        }
                    }
                }

                else -> {
                    item {
                        var revealMnemonic by remember { mutableStateOf(false) }
                        Surface(shape = RoundedCornerShape(16.dp), color = palette.surfaceColor, modifier = Modifier.fillMaxWidth()) {
                            Column(modifier = Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                                Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween, verticalAlignment = Alignment.CenterVertically) {
                                    Text("🔐 BIP39 24 词根私钥备份", fontWeight = FontWeight.Bold, fontSize = 14.sp, color = palette.primaryText)
                                    TextButton(onClick = { revealMnemonic = !revealMnemonic }) {
                                        Text(if (revealMnemonic) "隐藏助记词" else "点击显示")
                                    }
                                }
                                walletState.mnemonicWords.chunked(3).forEachIndexed { rIdx, rowWords ->
                                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.fillMaxWidth()) {
                                        rowWords.forEachIndexed { cIdx, w ->
                                            val num = rIdx * 3 + cIdx + 1
                                            Surface(shape = RoundedCornerShape(8.dp), color = palette.elevatedSurface, modifier = Modifier.weight(1f)) {
                                                Text(
                                                    text = "$num. ${if (revealMnemonic) w else "••••••"}",
                                                    fontSize = 12.sp,
                                                    fontFamily = FontFamily.Monospace,
                                                    color = palette.primaryText,
                                                    modifier = Modifier.padding(horizontal = 8.dp, vertical = 6.dp),
                                                )
                                            }
                                        }
                                    }
                                }
                                Button(onClick = onCreateNewWallet, modifier = Modifier.fillMaxWidth()) {
                                    Text("✨ 重新生成全新 24 词自托管钱包")
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Pending TonConnect Approval Sheet
    walletState.pendingTonConnect?.let { preview ->
        AlertDialog(
            onDismissRequest = { onResolveTonConnect(false) },
            title = { Text("🔐 TonConnect 2.0 签名确认") },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text("DApp：${preview.dappName} (${preview.dappDomain})", fontWeight = FontWeight.Bold, fontSize = 14.sp)
                    Text("请求操作：${preview.actionSummary}", fontSize = 13.sp)
                    Text("资产变动预估：${preview.estimatedDelta}", color = palette.primaryAccent, fontWeight = FontWeight.SemiBold, fontSize = 13.sp)
                    Text("预估网络矿工费：${preview.networkFeeTon}", fontSize = 12.sp, color = palette.secondaryText)
                    Surface(shape = RoundedCornerShape(8.dp), color = Color(0x222FA84F)) {
                        Text("✓ ${preview.riskNotice}", color = Color(0xFF2FA84F), fontSize = 12.sp, modifier = Modifier.padding(8.dp))
                    }
                }
            },
            confirmButton = {
                Button(onClick = { onResolveTonConnect(true) }) { Text("签名批准") }
            },
            dismissButton = {
                TextButton(onClick = { onResolveTonConnect(false) }) { Text("拒绝") }
            },
        )
    }

    // Send TON Dialog
    if (showSendDialog) {
        var toAddr by remember { mutableStateOf("") }
        var amountStr by remember { mutableStateOf("5.0") }
        var memo by remember { mutableStateOf("") }
        AlertDialog(
            onDismissRequest = { showSendDialog = false },
            title = { Text("↗ 发送 TON 链上转账") },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    OutlinedTextField(value = toAddr, onValueChange = { toAddr = it }, label = { Text("收款 TON 地址或 .ton 域名") }, singleLine = true)
                    OutlinedTextField(value = amountStr, onValueChange = { amountStr = it }, label = { Text("数量 (可用 %.2f TON)".format(walletState.tonBalance)) }, singleLine = true)
                    OutlinedTextField(value = memo, onValueChange = { memo = it }, label = { Text("链上 Memo 备注（可选）") }, singleLine = true)
                }
            },
            confirmButton = {
                Button(onClick = {
                    onSendTon(toAddr.ifBlank { "UQDr_receiver_demo_v4r2" }, amountStr.toDoubleOrNull() ?: 0.0, memo)
                    showSendDialog = false
                }) { Text("签名并发送") }
            },
            dismissButton = { TextButton(onClick = { showSendDialog = false }) { Text("取消") } },
        )
    }

    // Receive QR Code Dialog
    if (showReceiveQrDialog) {
        AlertDialog(
            onDismissRequest = { showReceiveQrDialog = false },
            title = { Text("↙ TON 收款二维码") },
            text = {
                Column(horizontalAlignment = Alignment.CenterHorizontally, modifier = Modifier.fillMaxWidth()) {
                    DeterministicAddressQrCanvas(payload = "ton://transfer/${walletState.addressFriendly}")
                    Spacer(Modifier.height(10.dp))
                    Text(walletState.addressFriendly, fontFamily = FontFamily.Monospace, fontSize = 11.sp)
                }
            },
            confirmButton = {
                Button(onClick = {
                    copyAddress(context, walletState.addressFriendly)
                    showReceiveQrDialog = false
                }) { Text("复制收款地址") }
            },
        )
    }

    // Swap Dialog
    if (showSwapDialog) {
        var swapAmt by remember { mutableStateOf("10.0") }
        AlertDialog(
            onDismissRequest = { showSwapDialog = false },
            title = { Text("⇄ STON.fi DEX 链上闪兑") },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    OutlinedTextField(value = swapAmt, onValueChange = { swapAmt = it }, label = { Text("卖出 TON 数量") }, singleLine = true)
                    val est = (swapAmt.toDoubleOrNull() ?: 0.0) * 5.42
                    Text("预计获得：%.2f USDT (汇率 1 TON ≈ 5.42 USDT)".format(est), fontSize = 13.sp, fontWeight = FontWeight.SemiBold)
                }
            },
            confirmButton = {
                Button(onClick = {
                    onSwapTonToUsdt(swapAmt.toDoubleOrNull() ?: 0.0)
                    showSwapDialog = false
                }) { Text("确认闪兑") }
            },
            dismissButton = { TextButton(onClick = { showSwapDialog = false }) { Text("取消") } },
        )
    }

    // Stake Dialog
    if (showStakeDialog) {
        var stakeAmt by remember { mutableStateOf("25.0") }
        AlertDialog(
            onDismissRequest = { showStakeDialog = false },
            title = { Text("🥩 TON 流动性质押 (${walletState.stakingApyPercent}% APY)") },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text("当前已质押：${walletState.stakedTon} TON", fontSize = 13.sp)
                    OutlinedTextField(value = stakeAmt, onValueChange = { stakeAmt = it }, label = { Text("新增质押 TON 数量") }, singleLine = true)
                }
            },
            confirmButton = {
                Button(onClick = {
                    onStakeTon(stakeAmt.toDoubleOrNull() ?: 0.0)
                    showStakeDialog = false
                }) { Text("立即质押") }
            },
            dismissButton = { TextButton(onClick = { showStakeDialog = false }) { Text("取消") } },
        )
    }

    // Import 24-Word Mnemonic Dialog
    if (showImportDialog) {
        var rawWords by remember { mutableStateOf(walletState.mnemonicWords.joinToString(" ")) }
        AlertDialog(
            onDismissRequest = { showImportDialog = false },
            title = { Text("🔑 导入 24 词 BIP39 助记词") },
            text = {
                OutlinedTextField(
                    value = rawWords,
                    onValueChange = { rawWords = it },
                    label = { Text("以空格分隔的 24 个英文单词") },
                    modifier = Modifier.fillMaxWidth(),
                )
            },
            confirmButton = {
                Button(onClick = {
                    onImportMnemonic(rawWords)
                    showImportDialog = false
                }) { Text("校验并导入") }
            },
            dismissButton = { TextButton(onClick = { showImportDialog = false }) { Text("取消") } },
        )
    }
}

@Composable
private fun AssetPill(title: String, amount: String, modifier: Modifier = Modifier) {
    Surface(shape = RoundedCornerShape(12.dp), color = Color.White.copy(alpha = 0.12f), modifier = modifier) {
        Column(modifier = Modifier.padding(horizontal = 10.dp, vertical = 8.dp)) {
            Text(title, color = Color.White.copy(alpha = 0.75f), fontSize = 11.sp)
            Text(amount, color = Color.White, fontWeight = FontWeight.Bold, fontSize = 15.sp, maxLines = 1)
        }
    }
}

@Composable
private fun WalletActionButton(label: String, modifier: Modifier = Modifier, onClick: () -> Unit) {
    Button(
        onClick = onClick,
        colors = ButtonDefaults.buttonColors(containerColor = Color.White.copy(alpha = 0.18f), contentColor = Color.White),
        contentPadding = PaddingValues(horizontal = 6.dp, vertical = 8.dp),
        modifier = modifier,
    ) {
        Text(label, fontSize = 12.sp, fontWeight = FontWeight.Bold, maxLines = 1)
    }
}

@Composable
private fun DeterministicAddressQrCanvas(payload: String) {
    val digest = remember(payload) {
        MessageDigest.getInstance("SHA-256").digest(payload.toByteArray(Charsets.UTF_8))
    }
    Canvas(
        modifier = Modifier
            .size(176.dp)
            .clip(RoundedCornerShape(12.dp))
            .background(Color.White)
            .padding(12.dp),
    ) {
        val grid = 21
        val cell = size.width / grid
        fun drawFinder(r0: Int, c0: Int) {
            for (r in 0..6) {
                for (c in 0..6) {
                    val border = r == 0 || r == 6 || c == 0 || c == 6
                    val inner = r in 2..4 && c in 2..4
                    if (border || inner) {
                        drawRect(Color.Black, Offset((c0 + c) * cell, (r0 + r) * cell), Size(cell, cell))
                    }
                }
            }
        }
        drawFinder(0, 0)
        drawFinder(0, grid - 7)
        drawFinder(grid - 7, 0)

        for (r in 0 until grid) {
            for (c in 0 until grid) {
                val inFinder = (r < 8 && c < 8) || (r < 8 && c >= grid - 8) || (r >= grid - 8 && c < 8)
                if (!inFinder) {
                    val bitIdx = (r * grid + c) % (digest.size * 8)
                    val on = ((digest[bitIdx / 8].toInt() ushr (bitIdx % 8)) and 1) == 1
                    if (on) {
                        drawRect(Color(0xFF0A2540), Offset(c * cell, r * cell), Size(cell * 0.92f, cell * 0.92f))
                    }
                }
            }
        }
    }
}

private fun copyAddress(context: Context, address: String) {
    val cm = context.getSystemService(Context.CLIPBOARD_SERVICE) as? ClipboardManager ?: return
    cm.setPrimaryClip(ClipData.newPlainText("TONAddress", address))
}
