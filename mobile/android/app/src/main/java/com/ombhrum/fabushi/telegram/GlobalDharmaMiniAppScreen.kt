package com.ombhrum.fabushi.telegram

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
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
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableDoubleStateOf
import androidx.compose.runtime.mutableFloatStateOf
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
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.ombhrum.fabushi.MarketplacePlugin
import com.ombhrum.fabushi.MiniAppToolContract
import com.ombhrum.fabushi.MiniAppWebMcpSurface
import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.cos
import kotlin.math.hypot
import kotlin.math.sin
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import org.json.JSONObject

private enum class DharmaWorkspaceTab(val label: String) {
    GLOBE("🌍 3D 地球供养"),
    PRAYER_WHEEL("☸️ 摩尼转经轮"),
    SUTRAS_TIMER("📖 经藏禅修"),
    MERIT_AI("🏆 功德与法义AI"),
}

@Composable
fun GlobalDharmaMiniAppScreen(
    dharmaState: GlobalDharmaUiState,
    onSelectCountry: (String) -> Unit,
    onOfferDharma: (targetCode: String, blessingNote: String) -> Unit,
    onRecordWheelSpin: (Int) -> Unit,
    onTapWoodenFish: () -> Unit,
    onCompletePractice: (Int) -> Unit,
    onAskDharmaAi: (String) -> Unit,
    onBack: () -> Unit,
) {
    var showWebMcpHost by remember { mutableStateOf(false) }
    if (showWebMcpHost) {
        val plugin = remember {
            MarketplacePlugin(
                pluginId = "global-dharma",
                displayName = "全球法布施 · WebMCP 小程序",
                description = "240+ 国家地球供养与摩尼转经轮 WebMCP 容器",
                latestVersion = "2.0.0",
                tools = listOf(
                    MiniAppToolContract("selection.country", "切换地球供养目标国家", "none"),
                    MiniAppToolContract("transfer.merit", "发送祈福心灯回向", "confirm"),
                ),
            )
        }
        MiniAppWebMcpSurface(
            plugin = plugin,
            loadLocalHtml = { buildEmbeddedGlobalDharmaWebMcpHtml(dharmaState) },
            callRuntimeToolJson = { _, name, argsJson ->
                val args = runCatching { JSONObject(argsJson) }.getOrDefault(JSONObject())
                when (name) {
                    "selection.country" -> {
                        val code = args.optString("code", "CN")
                        onSelectCountry(code)
                        JSONObject().put("selected", code).toString()
                    }
                    "transfer.merit" -> {
                        val code = args.optString("code", dharmaState.selectedCountryCode)
                        val note = args.optString("note", "愿正法久住 · 十方吉祥")
                        onOfferDharma(code, note)
                        JSONObject().put("offered", true).put("country", code).toString()
                    }
                    else -> JSONObject().put("status", "ok").toString()
                }
            },
            onClose = { showWebMcpHost = false },
        )
        return
    }

    BackHandler(onBack = onBack)

    var activeTab by remember { mutableStateOf(DharmaWorkspaceTab.GLOBE) }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(Brush.verticalGradient(listOf(Color(0xFF0A1424), Color(0xFF132238), Color(0xFF1A1528))))
            .statusBarsPadding()
            .navigationBarsPadding()
            .testTag("telegram-global-dharma-screen"),
    ) {
        // Golden Lotus Header Bar
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 10.dp, vertical = 10.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Box(
                modifier = Modifier
                    .size(40.dp)
                    .clip(CircleShape)
                    .clickable(onClick = onBack),
                contentAlignment = Alignment.Center,
            ) {
                TelegramIcon(TelegramGlyph.BACK, tint = Color(0xFFF7D070), size = 22.dp)
            }
            Spacer(Modifier.width(6.dp))
            TelegramIcon(TelegramGlyph.DHARMA_WHEEL, tint = Color(0xFFF7D070), size = 26.dp)
            Spacer(Modifier.width(8.dp))
            Column(modifier = Modifier.weight(1f)) {
                Text(
                    text = "全球法布施 · 十方光明坛城",
                    color = Color(0xFFFFF4D4),
                    fontSize = 17.sp,
                    fontWeight = FontWeight.Bold,
                )
                Text(
                    text = "全球累计供养 ${"%,d".format(dharmaState.totalGlobalOfferings)} 盏 · 我的功德 ${dharmaState.myMeritPoints}",
                    color = Color(0xFFD9B468),
                    fontSize = 11.sp,
                )
            }
            Surface(
                shape = RoundedCornerShape(14.dp),
                color = Color(0xFFF7D070).copy(alpha = 0.16f),
                modifier = Modifier
                    .clickable { showWebMcpHost = true }
                    .testTag("global-dharma-open-webmcp"),
            ) {
                Text(
                    text = "WebMCP",
                    color = Color(0xFFF7D070),
                    fontSize = 12.sp,
                    fontWeight = FontWeight.Bold,
                    modifier = Modifier.padding(horizontal = 10.dp, vertical = 6.dp),
                )
            }
        }

        // 4 Sub-Tab Switcher
        LazyRow(
            contentPadding = PaddingValues(horizontal = 12.dp),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            modifier = Modifier.padding(bottom = 8.dp),
        ) {
            items(DharmaWorkspaceTab.entries) { tab ->
                val selected = activeTab == tab
                Surface(
                    shape = RoundedCornerShape(18.dp),
                    color = if (selected) Color(0xFFD99B26) else Color.White.copy(alpha = 0.08f),
                    modifier = Modifier.clickable { activeTab = tab },
                ) {
                    Text(
                        text = tab.label,
                        color = if (selected) Color(0xFF1A1205) else Color(0xFFE8DFC8),
                        fontWeight = FontWeight.Bold,
                        fontSize = 13.sp,
                        modifier = Modifier.padding(horizontal = 14.dp, vertical = 8.dp),
                    )
                }
            }
        }

        when (activeTab) {
            DharmaWorkspaceTab.GLOBE -> EarthOfferingGlobeTab(
                dharmaState = dharmaState,
                onSelectCountry = onSelectCountry,
                onOfferDharma = onOfferDharma,
            )

            DharmaWorkspaceTab.PRAYER_WHEEL -> ZenSpinningPrayerWheelTab(
                dharmaState = dharmaState,
                onRecordWheelSpin = onRecordWheelSpin,
            )

            DharmaWorkspaceTab.SUTRAS_TIMER -> SutrasAndMeditationTab(
                dharmaState = dharmaState,
                onTapWoodenFish = onTapWoodenFish,
                onCompletePractice = onCompletePractice,
            )

            DharmaWorkspaceTab.MERIT_AI -> MeritLeaderboardAndAiTab(
                dharmaState = dharmaState,
                onAskDharmaAi = onAskDharmaAi,
            )
        }
    }
}

// ============================================================================
// Tab 1: Interactive 3D Orthographic Earth Offering Globe (240+ Countries)
// ============================================================================

private data class ProjectedNode(
    val country: CountryOfferingNode,
    val screenX: Float,
    val screenY: Float,
    val zDepth: Double,
)

@Composable
private fun EarthOfferingGlobeTab(
    dharmaState: GlobalDharmaUiState,
    onSelectCountry: (String) -> Unit,
    onOfferDharma: (String, String) -> Unit,
) {
    var centerLonDeg by remember { mutableDoubleStateOf(-104.0) }
    var centerLatDeg by remember { mutableDoubleStateOf(22.0) }
    var zoomFactor by remember { mutableFloatStateOf(1.0f) }
    var autoRotate by remember { mutableStateOf(true) }
    var arcAnimPhase by remember { mutableFloatStateOf(0f) }
    var blessingInput by remember { mutableStateOf("愿以此心灯遍照十方，世界和平，众生安乐") }
    var countrySearch by remember { mutableStateOf("") }

    var projectedCache by remember { mutableStateOf<List<ProjectedNode>>(emptyList()) }

    LaunchedEffect(autoRotate) {
        while (isActive) {
            if (autoRotate) {
                centerLonDeg = ((centerLonDeg - 0.45) + 540.0) % 360.0 - 180.0
            }
            arcAnimPhase = (arcAnimPhase + 0.025f) % 1.0f
            delay(32)
        }
    }

    val selectedCountry = remember(dharmaState.countries, dharmaState.selectedCountryCode) {
        dharmaState.countries.firstOrNull { it.code == dharmaState.selectedCountryCode } ?: dharmaState.countries.first()
    }

    val filteredCountries = remember(dharmaState.countries, countrySearch) {
        if (countrySearch.isBlank()) dharmaState.countries.take(36)
        else dharmaState.countries.filter {
            it.nameZh.contains(countrySearch, ignoreCase = true) ||
                it.nameEn.contains(countrySearch, ignoreCase = true) ||
                it.code.contains(countrySearch, ignoreCase = true) ||
                it.region.contains(countrySearch, ignoreCase = true)
        }
    }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(horizontal = 14.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        // 3D Globe Canvas Box
        Box(
            modifier = Modifier
                .fillMaxWidth()
                .height(308.dp)
                .clip(RoundedCornerShape(22.dp))
                .background(Color(0xFF070F1C))
                .border(1.dp, Color(0x44F7D070), RoundedCornerShape(22.dp)),
        ) {
            Canvas(
                modifier = Modifier
                    .fillMaxSize()
                    .pointerInput(Unit) {
                        detectDragGestures(
                            onDragStart = { autoRotate = false },
                            onDrag = { change, dragAmount ->
                                change.consume()
                                centerLonDeg -= dragAmount.x * 0.28
                                centerLatDeg = (centerLatDeg + dragAmount.y * 0.22).coerceIn(-72.0, 72.0)
                            },
                        )
                    }
                    .pointerInput(projectedCache) {
                        detectTapGestures { tap ->
                            val hit = projectedCache
                                .filter { it.zDepth > 0.0 }
                                .minByOrNull { hypot((it.screenX - tap.x).toDouble(), (it.screenY - tap.y).toDouble()) }
                            if (hit != null && hypot(hit.screenX - tap.x, hit.screenY - tap.y) <= 44f) {
                                autoRotate = false
                                onSelectCountry(hit.country.code)
                            }
                        }
                    },
            ) {
                val cx = size.width / 2f
                val cy = size.height / 2f
                val radius = (size.minDimension * 0.41f) * zoomFactor

                // Outer Golden Lotus Atmosphere Halo
                drawCircle(
                    brush = Brush.radialGradient(
                        listOf(Color(0x38F7D070), Color(0x142AABEE), Color.Transparent),
                        center = Offset(cx, cy),
                        radius = radius * 1.28f,
                    ),
                    radius = radius * 1.28f,
                    center = Offset(cx, cy),
                )

                // Ocean Sphere Body
                drawCircle(
                    brush = Brush.radialGradient(
                        listOf(Color(0xFF1C4E80), Color(0xFF0F2D52), Color(0xFF08182E)),
                        center = Offset(cx - radius * 0.25f, cy - radius * 0.25f),
                        radius = radius * 1.2f,
                    ),
                    radius = radius,
                    center = Offset(cx, cy),
                )

                val lat0 = centerLatDeg * (PI / 180.0)
                val lon0 = -centerLonDeg * (PI / 180.0)

                fun project3D(latDeg: Double, lonDeg: Double): Triple<Float, Float, Double> {
                    val lat = latDeg * (PI / 180.0)
                    val lon = lonDeg * (PI / 180.0)
                    val dLon = lon - lon0
                    val x = cos(lat) * sin(dLon)
                    val y = cos(lat0) * sin(lat) - sin(lat0) * cos(lat) * cos(dLon)
                    val z = sin(lat0) * sin(lat) + cos(lat0) * cos(lat) * cos(dLon)
                    return Triple((cx + x * radius).toFloat(), (cy - y * radius).toFloat(), z)
                }

                // Draw Graticule Meridians & Parallels
                val graticuleColor = Color(0x268EC5FC)
                for (lonLine in -150..180 step 30) {
                    var prevPt: Offset? = null
                    for (latStep in -80..80 step 10) {
                        val (sx, sy, z) = project3D(latStep.toDouble(), lonLine.toDouble())
                        if (z > 0.0) {
                            val cur = Offset(sx, sy)
                            prevPt?.let { drawLine(graticuleColor, it, cur, 1f) }
                            prevPt = cur
                        } else {
                            prevPt = null
                        }
                    }
                }
                for (latLine in -60..60 step 30) {
                    var prevPt: Offset? = null
                    for (lonStep in -180..180 step 10) {
                        val (sx, sy, z) = project3D(latLine.toDouble(), lonStep.toDouble())
                        if (z > 0.0) {
                            val cur = Offset(sx, sy)
                            prevPt?.let { drawLine(graticuleColor, it, cur, 1f) }
                            prevPt = cur
                        } else {
                            prevPt = null
                        }
                    }
                }

                // Project 240+ Country & Territory Nodes
                val nextProjected = ArrayList<ProjectedNode>(dharmaState.countries.size)
                for (c in dharmaState.countries) {
                    val (sx, sy, z) = project3D(c.latitude, c.longitude)
                    nextProjected += ProjectedNode(c, sx, sy, z)
                    if (z > 0.02) {
                        val isSelected = c.code == selectedCountry.code
                        val dotColor = when {
                            isSelected -> Color(0xFFFFF59D)
                            c.lampLitByMe -> Color(0xFFFFB300)
                            else -> Color(0xFF64D2FF).copy(alpha = (0.35f + 0.55f * z.toFloat()).coerceIn(0.2f, 0.9f))
                        }
                        val dotR = when {
                            isSelected -> 6.8f
                            c.lampLitByMe -> 4.6f
                            else -> 2.8f
                        }
                        if (isSelected || c.lampLitByMe) {
                            drawCircle(dotColor.copy(alpha = 0.28f), radius = dotR * 2.4f, center = Offset(sx, sy))
                        }
                        drawCircle(dotColor, radius = dotR, center = Offset(sx, sy))
                    }
                }
                projectedCache = nextProjected

                // Draw Animated Golden Offering Arcs
                dharmaState.activeArcs.forEach { arc ->
                    val (x1, y1, z1) = project3D(arc.fromLat, arc.fromLon)
                    val (x2, y2, z2) = project3D(arc.toLat, arc.toLon)
                    if (z1 > -0.2 || z2 > -0.2) {
                        val ctrlX = (x1 + x2) / 2f
                        val ctrlY = minOf(y1, y2) - radius * 0.34f
                        val path = Path().apply {
                            moveTo(x1, y1)
                            quadraticBezierTo(ctrlX, ctrlY, x2, y2)
                        }
                        drawPath(
                            path = path,
                            brush = Brush.linearGradient(listOf(Color(0xFFFFD54F), Color(0xFFFF8F00))),
                            style = Stroke(width = 3.2f, cap = StrokeCap.Round),
                        )
                        // Traveling golden spark along Bezier
                        val t = arcAnimPhase
                        val omt = 1f - t
                        val sparkX = omt * omt * x1 + 2f * omt * t * ctrlX + t * t * x2
                        val sparkY = omt * omt * y1 + 2f * omt * t * ctrlY + t * t * y2
                        drawCircle(Color.White, radius = 4.5f, center = Offset(sparkX, sparkY))
                    }
                }
            }

            // Globe Overlay Controls
            Row(
                modifier = Modifier
                    .align(Alignment.TopStart)
                    .padding(10.dp),
                horizontalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                Surface(
                    shape = RoundedCornerShape(12.dp),
                    color = Color.Black.copy(alpha = 0.48f),
                ) {
                    Text(
                        text = "已收录 ${dharmaState.countries.size} 个国家与地区",
                        color = Color(0xFFF7D070),
                        fontSize = 11.sp,
                        modifier = Modifier.padding(horizontal = 9.dp, vertical = 4.dp),
                    )
                }
            }

            Row(
                modifier = Modifier
                    .align(Alignment.BottomEnd)
                    .padding(10.dp),
                horizontalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                Surface(
                    shape = RoundedCornerShape(10.dp),
                    color = Color.White.copy(alpha = 0.14f),
                    modifier = Modifier.clickable { autoRotate = !autoRotate },
                ) {
                    Text(
                        text = if (autoRotate) "⏸ 暂停自转" else "▶ 自动自转",
                        color = Color.White,
                        fontSize = 11.sp,
                        modifier = Modifier.padding(horizontal = 8.dp, vertical = 5.dp),
                    )
                }
                Surface(
                    shape = CircleShape,
                    color = Color.White.copy(alpha = 0.14f),
                    modifier = Modifier.size(28.dp).clickable { zoomFactor = (zoomFactor + 0.15f).coerceAtMost(1.6f) },
                ) {
                    Box(contentAlignment = Alignment.Center) { Text("+", color = Color.White, fontWeight = FontWeight.Bold) }
                }
                Surface(
                    shape = CircleShape,
                    color = Color.White.copy(alpha = 0.14f),
                    modifier = Modifier.size(28.dp).clickable { zoomFactor = (zoomFactor - 0.15f).coerceAtLeast(0.75f) },
                ) {
                    Box(contentAlignment = Alignment.Center) { Text("−", color = Color.White, fontWeight = FontWeight.Bold) }
                }
            }
        }

        // Selected Country Offering Card
        Surface(
            shape = RoundedCornerShape(18.dp),
            color = Color.White.copy(alpha = 0.08f),
            modifier = Modifier.fillMaxWidth(),
        ) {
            Column(modifier = Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Column {
                        Text(
                            text = "${selectedCountry.nameZh} · ${selectedCountry.nameEn} (${selectedCountry.code})",
                            color = Color(0xFFFFF4D4),
                            fontWeight = FontWeight.Bold,
                            fontSize = 16.sp,
                        )
                        Text(
                            text = "${selectedCountry.region} · 坐标 (%.1f°, %.1f°) · 已点亮 ${"%,d".format(selectedCountry.offeringsCount)} 盏".format(
                                selectedCountry.latitude,
                                selectedCountry.longitude,
                            ),
                            color = Color(0xFFB8C7DC),
                            fontSize = 12.sp,
                        )
                    }
                    if (selectedCountry.lampLitByMe) {
                        Surface(shape = RoundedCornerShape(10.dp), color = Color(0x33FFB300)) {
                            Text("🪔 我已供灯", color = Color(0xFFFFD54F), fontSize = 11.sp, modifier = Modifier.padding(horizontal = 8.dp, vertical = 4.dp))
                        }
                    }
                }

                OutlinedTextField(
                    value = blessingInput,
                    onValueChange = { blessingInput = it },
                    label = { Text("回向祈愿文", color = Color(0xFFD9B468)) },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth(),
                )

                Button(
                    onClick = { onOfferDharma(selectedCountry.code, blessingInput) },
                    colors = ButtonDefaults.buttonColors(containerColor = Color(0xFFD99B26), contentColor = Color(0xFF1C1304)),
                    modifier = Modifier
                        .fillMaxWidth()
                        .testTag("global-dharma-offer-button"),
                ) {
                    Text("🪔 向 ${selectedCountry.nameZh} 点亮祈福法灯 (+18 功德)", fontWeight = FontWeight.Bold)
                }
            }
        }

        // Quick Search & Country Selector Strip (240+ Regions)
        OutlinedTextField(
            value = countrySearch,
            onValueChange = { countrySearch = it },
            placeholder = { Text("搜索全球 242 个国家/地区名称或 ISO 代码…", color = Color.Gray) },
            singleLine = true,
            modifier = Modifier.fillMaxWidth(),
        )

        LazyRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            items(filteredCountries, key = { it.code }) { c ->
                val isSel = c.code == selectedCountry.code
                Surface(
                    shape = RoundedCornerShape(14.dp),
                    color = if (isSel) Color(0xFFD99B26) else Color.White.copy(alpha = 0.08f),
                    modifier = Modifier.clickable {
                        autoRotate = false
                        centerLonDeg = -c.longitude
                        centerLatDeg = c.latitude.coerceIn(-65.0, 65.0)
                        onSelectCountry(c.code)
                    },
                ) {
                    Text(
                        text = "${if (c.lampLitByMe) "🪔 " else ""}${c.nameZh} (${c.code})",
                        color = if (isSel) Color(0xFF1A1205) else Color.White,
                        fontSize = 12.sp,
                        fontWeight = FontWeight.Medium,
                        modifier = Modifier.padding(horizontal = 12.dp, vertical = 8.dp),
                    )
                }
            }
        }

        Spacer(Modifier.height(16.dp))
    }
}

// ============================================================================
// Tab 2: Zen Room Spinning Prayer Wheel (Angular Momentum Physics)
// ============================================================================

@Composable
private fun ZenSpinningPrayerWheelTab(
    dharmaState: GlobalDharmaUiState,
    onRecordWheelSpin: (Int) -> Unit,
) {
    val haptic = LocalHapticFeedback.current
    var angleRad by remember { mutableFloatStateOf(0f) }
    var angularVelocity by remember { mutableFloatStateOf(1.8f) } // rad/sec
    var autoSpinConstant by remember { mutableStateOf(true) }
    var accumulatedRad by remember { mutableFloatStateOf(0f) }

    LaunchedEffect(autoSpinConstant) {
        while (isActive) {
            val dt = 0.016f
            if (autoSpinConstant && angularVelocity < 2.2f) {
                angularVelocity = 2.2f
            }
            val delta = abs(angularVelocity) * dt
            angleRad = (angleRad + angularVelocity * dt) % (2f * PI.toFloat())
            accumulatedRad += delta
            if (accumulatedRad >= (2f * PI.toFloat())) {
                val revs = (accumulatedRad / (2f * PI.toFloat())).toInt()
                accumulatedRad -= revs * (2f * PI.toFloat())
                haptic.performHapticFeedback(HapticFeedbackType.TextHandleMove)
                onRecordWheelSpin(revs)
            }
            if (!autoSpinConstant) {
                angularVelocity *= 0.988f
                if (abs(angularVelocity) < 0.03f) angularVelocity = 0f
            }
            delay(16)
        }
    }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(16.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        Text(
            text = "嗡 嘛 呢 叭 咪 吽 · OM MANI PADME HUM",
            color = Color(0xFFF7D070),
            fontWeight = FontWeight.Bold,
            fontSize = 15.sp,
        )

        Box(
            modifier = Modifier
                .size(256.dp)
                .pointerInput(Unit) {
                    detectDragGestures { change, dragAmount ->
                        change.consume()
                        angularVelocity = (angularVelocity + dragAmount.x * 0.045f).coerceIn(-18f, 24f)
                    }
                }
                .clickable {
                    angularVelocity = (angularVelocity + 4.5f).coerceAtMost(24f)
                },
            contentAlignment = Alignment.Center,
        ) {
            Canvas(modifier = Modifier.fillMaxSize()) {
                val center = Offset(size.width / 2f, size.height / 2f)
                val r = size.minDimension * 0.42f

                // Golden Radiant Aura
                drawCircle(
                    brush = Brush.radialGradient(listOf(Color(0x44FFD54F), Color.Transparent), center = center, radius = r * 1.25f),
                    radius = r * 1.25f,
                    center = center,
                )

                // Outer & Inner Golden Rims
                drawCircle(Color(0xFFD99B26), radius = r, center = center, style = Stroke(width = 10f))
                drawCircle(Color(0xFFFFF0B3), radius = r * 0.84f, center = center, style = Stroke(width = 3f))
                drawCircle(Color(0xFF4A2E0B), radius = r * 0.80f, center = center)

                // 8 Rotating Dharma Spokes + Lotus Jewels
                for (spoke in 0 until 8) {
                    val theta = angleRad + spoke * (PI.toFloat() / 4f)
                    val end = Offset(center.x + cos(theta) * r * 0.92f, center.y + sin(theta) * r * 0.92f)
                    drawLine(Color(0xFFF7D070), center, end, strokeWidth = 5.5f, cap = StrokeCap.Round)
                    drawCircle(Color(0xFFFFECB3), radius = 6.5f, center = end)
                }

                // Hub Jewel
                drawCircle(Brush.radialGradient(listOf(Color(0xFFFFF9C4), Color(0xFFD99B26)), center, r * 0.2f), radius = r * 0.2f, center = center)
            }
        }

        Text(
            text = "左右滑动或轻触经轮加速顺时针旋转 · 当前转速 ${(abs(angularVelocity) * 9.55f).toInt()} RPM",
            color = Color(0xFFB8C7DC),
            fontSize = 12.sp,
        )

        Row(horizontalArrangement = Arrangement.spacedBy(12.dp), modifier = Modifier.fillMaxWidth()) {
            DharmaMetricBox("转经总圈数", "${dharmaState.prayerWheelSpins} 圈", Modifier.weight(1f))
            DharmaMetricBox("持诵真言数", "${dharmaState.mantraCount} 遍", Modifier.weight(1f))
            DharmaMetricBox("累积功德", "${dharmaState.myMeritPoints}", Modifier.weight(1f))
        }

        Row(horizontalArrangement = Arrangement.spacedBy(10.dp), modifier = Modifier.fillMaxWidth()) {
            Button(
                onClick = { angularVelocity = (angularVelocity + 6f).coerceAtMost(24f) },
                colors = ButtonDefaults.buttonColors(containerColor = Color(0xFFD99B26), contentColor = Color(0xFF1A1205)),
                modifier = Modifier.weight(1f),
            ) {
                Text("🙏 拨动经轮祈福", fontWeight = FontWeight.Bold)
            }
            Button(
                onClick = { autoSpinConstant = !autoSpinConstant },
                colors = ButtonDefaults.buttonColors(containerColor = Color.White.copy(alpha = 0.14f), contentColor = Color.White),
                modifier = Modifier.weight(1f),
            ) {
                Text(if (autoSpinConstant) "♾ 恒转模式：开启" else "♾ 恒转模式：手动")
            }
        }
    }
}

// ============================================================================
// Tab 3: Sutras Reader, Wooden Fish & Practice Meditation Timer
// ============================================================================

@Composable
private fun SutrasAndMeditationTab(
    dharmaState: GlobalDharmaUiState,
    onTapWoodenFish: () -> Unit,
    onCompletePractice: (Int) -> Unit,
) {
    val haptic = LocalHapticFeedback.current
    var selectedSutraId by remember { mutableStateOf(dharmaState.sutras.first().id) }
    var sutraFontSp by remember { mutableIntStateOf(17) }
    var selectedMinutes by remember { mutableIntStateOf(15) }
    var timerRunning by remember { mutableStateOf(false) }
    var remainingSeconds by remember { mutableIntStateOf(15 * 60) }

    LaunchedEffect(timerRunning) {
        while (isActive && timerRunning) {
            delay(1000)
            if (remainingSeconds > 1) {
                remainingSeconds -= 1
            } else {
                timerRunning = false
                remainingSeconds = selectedMinutes * 60
                onCompletePractice(selectedMinutes)
            }
        }
    }

    val activeSutra = dharmaState.sutras.firstOrNull { it.id == selectedSutraId } ?: dharmaState.sutras.first()

    Column(
        modifier = Modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(14.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        LazyRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            items(dharmaState.sutras, key = { it.id }) { sutra ->
                val sel = sutra.id == selectedSutraId
                Surface(
                    shape = RoundedCornerShape(14.dp),
                    color = if (sel) Color(0xFFD99B26) else Color.White.copy(alpha = 0.08f),
                    modifier = Modifier.clickable { selectedSutraId = sutra.id },
                ) {
                    Text(
                        text = sutra.title,
                        color = if (sel) Color(0xFF1A1205) else Color.White,
                        fontSize = 13.sp,
                        fontWeight = FontWeight.SemiBold,
                        modifier = Modifier.padding(horizontal = 12.dp, vertical = 8.dp),
                    )
                }
            }
        }

        // Sutra Text Card
        Surface(shape = RoundedCornerShape(18.dp), color = Color.White.copy(alpha = 0.07f), modifier = Modifier.fillMaxWidth()) {
            Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween, verticalAlignment = Alignment.CenterVertically) {
                    Column(modifier = Modifier.weight(1f)) {
                        Text(activeSutra.title, color = Color(0xFFFFF4D4), fontWeight = FontWeight.Bold, fontSize = 17.sp)
                        Text(activeSutra.tradition, color = Color(0xFFD9B468), fontSize = 12.sp)
                    }
                    Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                        Surface(shape = CircleShape, color = Color.White.copy(alpha = 0.12f), modifier = Modifier.size(30.dp).clickable { sutraFontSp = (sutraFontSp - 2).coerceAtLeast(13) }) {
                            Box(contentAlignment = Alignment.Center) { Text("A-", color = Color.White, fontSize = 12.sp) }
                        }
                        Surface(shape = CircleShape, color = Color.White.copy(alpha = 0.12f), modifier = Modifier.size(30.dp).clickable { sutraFontSp = (sutraFontSp + 2).coerceAtMost(26) }) {
                            Box(contentAlignment = Alignment.Center) { Text("A+", color = Color.White, fontSize = 12.sp) }
                        }
                    }
                }
                Text(
                    text = activeSutra.content,
                    color = Color(0xFFF5EFE0),
                    fontSize = sutraFontSp.sp,
                    lineHeight = (sutraFontSp * 1.65f).sp,
                )
                Surface(shape = RoundedCornerShape(10.dp), color = Color(0x26FFD54F), modifier = Modifier.fillMaxWidth()) {
                    Text(
                        text = "🙏 回向偈：${activeSutra.dedicationVerse}",
                        color = Color(0xFFFFECB3),
                        fontSize = 13.sp,
                        modifier = Modifier.padding(10.dp),
                    )
                }
            }
        }

        // Electronic Wooden Fish + Practice Timer Card
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp), modifier = Modifier.fillMaxWidth()) {
            Surface(
                shape = RoundedCornerShape(18.dp),
                color = Color(0x22D99B26),
                modifier = Modifier
                    .weight(1f)
                    .clickable {
                        haptic.performHapticFeedback(HapticFeedbackType.TextHandleMove)
                        onTapWoodenFish()
                    },
            ) {
                Column(
                    modifier = Modifier.padding(14.dp),
                    horizontalAlignment = Alignment.CenterHorizontally,
                ) {
                    Text("🪵 电子木鱼", color = Color(0xFFF7D070), fontWeight = FontWeight.Bold, fontSize = 15.sp)
                    Spacer(Modifier.height(6.dp))
                    Text("${dharmaState.woodenFishHits}", color = Color.White, fontWeight = FontWeight.Bold, fontSize = 26.sp)
                    Text("轻触敲击 · 功德 +1", color = Color(0xFFB8C7DC), fontSize = 11.sp)
                }
            }

            Surface(
                shape = RoundedCornerShape(18.dp),
                color = Color.White.copy(alpha = 0.08f),
                modifier = Modifier.weight(1.3f),
            ) {
                Column(modifier = Modifier.padding(14.dp), horizontalAlignment = Alignment.CenterHorizontally) {
                    Text("🪷 正念共修计时 (累计 ${dharmaState.practiceMinutesTotal}m)", color = Color(0xFFF7D070), fontSize = 12.sp, fontWeight = FontWeight.Bold)
                    Spacer(Modifier.height(4.dp))
                    Text(
                        text = "%02d:%02d".format(remainingSeconds / 60, remainingSeconds % 60),
                        color = Color.White,
                        fontSize = 24.sp,
                        fontWeight = FontWeight.Bold,
                    )
                    Spacer(Modifier.height(6.dp))
                    Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                        listOf(5, 15, 30).forEach { m ->
                            Surface(
                                shape = RoundedCornerShape(8.dp),
                                color = if (selectedMinutes == m) Color(0xFFD99B26) else Color.White.copy(alpha = 0.12f),
                                modifier = Modifier.clickable {
                                    selectedMinutes = m
                                    remainingSeconds = m * 60
                                },
                            ) {
                                Text("${m}m", color = if (selectedMinutes == m) Color.Black else Color.White, fontSize = 11.sp, modifier = Modifier.padding(horizontal = 7.dp, vertical = 3.dp))
                            }
                        }
                    }
                    Spacer(Modifier.height(8.dp))
                    Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                        Button(
                            onClick = { timerRunning = !timerRunning },
                            colors = ButtonDefaults.buttonColors(containerColor = Color(0xFFD99B26), contentColor = Color.Black),
                            contentPadding = PaddingValues(horizontal = 10.dp, vertical = 4.dp),
                        ) {
                            Text(if (timerRunning) "暂停" else "开始", fontSize = 12.sp, fontWeight = FontWeight.Bold)
                        }
                        Button(
                            onClick = {
                                timerRunning = false
                                onCompletePractice(selectedMinutes)
                            },
                            colors = ButtonDefaults.buttonColors(containerColor = Color.White.copy(alpha = 0.14f), contentColor = Color.White),
                            contentPadding = PaddingValues(horizontal = 10.dp, vertical = 4.dp),
                        ) {
                            Text("圆满回向", fontSize = 12.sp)
                        }
                    }
                }
            }
        }
    }
}

// ============================================================================
// Tab 4: Merit Leaderboard, Achievements & AI Dharma Q&A
// ============================================================================

@Composable
private fun MeritLeaderboardAndAiTab(
    dharmaState: GlobalDharmaUiState,
    onAskDharmaAi: (String) -> Unit,
) {
    var question by remember { mutableStateOf("") }
    LazyColumn(
        modifier = Modifier.fillMaxSize(),
        contentPadding = PaddingValues(14.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        item {
            Text("🏅 修行成就勋章", color = Color(0xFFF7D070), fontWeight = FontWeight.Bold, fontSize = 15.sp)
            Spacer(Modifier.height(8.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.fillMaxWidth()) {
                dharmaState.achievements.forEach { ach ->
                    Surface(
                        shape = RoundedCornerShape(14.dp),
                        color = if (ach.unlocked) Color(0x28FFD54F) else Color.White.copy(alpha = 0.05f),
                        modifier = Modifier.weight(1f),
                    ) {
                        Column(modifier = Modifier.padding(10.dp), horizontalAlignment = Alignment.CenterHorizontally) {
                            Text(ach.icon, fontSize = 24.sp)
                            Text(ach.title, color = Color.White, fontSize = 11.sp, fontWeight = FontWeight.Bold, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        }
                    }
                }
            }
        }

        item {
            Text("🌍 全球共修功德随喜榜", color = Color(0xFFF7D070), fontWeight = FontWeight.Bold, fontSize = 15.sp)
        }
        items(dharmaState.leaderboard, key = { it.rank }) { row ->
            Surface(shape = RoundedCornerShape(14.dp), color = Color.White.copy(alpha = 0.07f), modifier = Modifier.fillMaxWidth()) {
                Row(
                    modifier = Modifier.padding(12.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.SpaceBetween,
                ) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Text("#${row.rank}", color = Color(0xFFF7D070), fontWeight = FontWeight.Bold, fontSize = 15.sp, modifier = Modifier.width(32.dp))
                        Column {
                            Text("${row.name} (${row.countryCode})", color = Color.White, fontWeight = FontWeight.SemiBold, fontSize = 14.sp)
                            Text("转经 ${row.wheelsSpun} 圈", color = Color(0xFFB8C7DC), fontSize = 11.sp)
                        }
                    }
                    Text("${"%,d".format(row.meritPoints)} 功德", color = Color(0xFFFFD54F), fontWeight = FontWeight.Bold, fontSize = 13.sp)
                }
            }
        }

        item {
            Text("🪷 AI 法义善知识问答", color = Color(0xFFF7D070), fontWeight = FontWeight.Bold, fontSize = 15.sp)
        }
        items(dharmaState.aiDharmaMessages) { (isUser, msg) ->
            Surface(
                shape = RoundedCornerShape(14.dp),
                color = if (isUser) Color(0x332AABEE) else Color.White.copy(alpha = 0.09f),
                modifier = Modifier.fillMaxWidth(),
            ) {
                Text(
                    text = "${if (isUser) "🙋 请益：" else "☸️ 答疑："}$msg",
                    color = Color.White,
                    fontSize = 13.sp,
                    lineHeight = 20.sp,
                    modifier = Modifier.padding(12.dp),
                )
            }
        }
        item {
            Row(verticalAlignment = Alignment.CenterVertically) {
                OutlinedTextField(
                    value = question,
                    onValueChange = { question = it },
                    placeholder = { Text("请益《心经》《金刚经》或日常正念修持…", color = Color.Gray) },
                    singleLine = true,
                    modifier = Modifier.weight(1f),
                )
                Spacer(Modifier.width(8.dp))
                Button(
                    onClick = {
                        onAskDharmaAi(question)
                        question = ""
                    },
                    colors = ButtonDefaults.buttonColors(containerColor = Color(0xFFD99B26), contentColor = Color.Black),
                ) {
                    Text("请益", fontWeight = FontWeight.Bold)
                }
            }
        }
    }
}

@Composable
private fun DharmaMetricBox(label: String, value: String, modifier: Modifier = Modifier) {
    Surface(shape = RoundedCornerShape(14.dp), color = Color.White.copy(alpha = 0.08f), modifier = modifier) {
        Column(modifier = Modifier.padding(10.dp), horizontalAlignment = Alignment.CenterHorizontally) {
            Text(value, color = Color(0xFFFFF4D4), fontWeight = FontWeight.Bold, fontSize = 15.sp, maxLines = 1)
            Text(label, color = Color(0xFFB8C7DC), fontSize = 11.sp)
        }
    }
}

private fun buildEmbeddedGlobalDharmaWebMcpHtml(state: GlobalDharmaUiState): String = """
<!doctype html>
<html>
<head>
  <meta charset="utf-8"/>
  <meta name="viewport" content="width=device-width,initial-scale=1"/>
  <style>
    body { margin:0; font-family:system-ui,-apple-system,sans-serif; background:#0d1829; color:#fff5d6; padding:18px; }
    .card { background:rgba(255,255,255,0.08); border:1px solid rgba(247,208,112,0.3); border-radius:16px; padding:16px; margin-bottom:14px; }
    button { background:#d99b26; color:#1a1205; border:0; border-radius:12px; padding:10px 16px; font-weight:700; margin-right:8px; }
  </style>
</head>
<body>
  <div class="card">
    <h3>☸️ 全球法布施 · 本地 WebMCP 宿主桥</h3>
    <p>全球供养：<b>${state.totalGlobalOfferings}</b> 盏 · 我的功德：<b>${state.myMeritPoints}</b></p>
    <button onclick="window.__fabushiWebMcp.call('offline.capabilities',{}).then(r=>document.getElementById('out').innerText=JSON.stringify(r,null,2))">探测宿主能力</button>
    <button onclick="window.__fabushiWebMcp.call('transfer.merit',{code:'${state.selectedCountryCode}',note:'WebMCP 祈福供灯'}).then(r=>document.getElementById('out').innerText=JSON.stringify(r,null,2))">WebMCP 供灯祈福</button>
    <pre id="out" style="margin-top:12px;font-size:12px;color:#8be8ff;">就绪 · 点击按钮调用原生 WebMCP 工具</pre>
  </div>
</body>
</html>
""".trimIndent()
