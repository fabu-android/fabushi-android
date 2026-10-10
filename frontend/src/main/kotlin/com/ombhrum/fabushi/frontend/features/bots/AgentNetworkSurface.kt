package com.ombhrum.fabushi

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectTransformGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.weight
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
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
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlin.math.roundToInt

@Composable
internal fun AgentNetworkSurface(
    bots: List<MobileBotSummaryAndroid>,
    appAgentSurface: FabushiAppAgentSurface,
    onBack: () -> Unit,
    onOpenBot: (MobileBotSummaryAndroid) -> Unit,
) {
    val nodes = remember(bots) { bots.filterNot { it.isHidden }.map { it.toAgentNetworkNode() } }
    val edges = remember(nodes) { buildAgentNetworkEdges(nodes) }
    var selectedId by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(nodes) {
        selectedId = reconcileAgentNetworkSelection(selectedId, nodes)
    }
    val selected = nodes.firstOrNull { it.id == selectedId }
    val botsById = remember(bots) { bots.associateBy { it.id } }

    LaunchedEffect(nodes, edges, selectedId, appAgentSurface) {
        val elements = mutableListOf(
            FabushiAppAgentSurface.Element("agent-network", "application", "Agent network"),
            FabushiAppAgentSurface.Element("agent-network-close", "button", "Close org chart"),
        )
        val actions = linkedMapOf<String, FabushiAppAgentSurface.Action>(
            "agent-network-close" to FabushiAppAgentSurface.Action(setOf("invoke")) { onBack() },
        )
        nodes.forEach { node ->
            val id = "agent-network-node-${node.id}".replace(Regex("[^A-Za-z0-9._:/@-]"), "-").take(200)
            elements += FabushiAppAgentSurface.Element(id, "button", node.name)
            actions[id] = FabushiAppAgentSurface.Action(setOf("invoke")) { selectedId = node.id }
        }
        selected?.let { node ->
            elements += FabushiAppAgentSurface.Element("agent-network-details", "complementary", "Org chart details for ${node.name}")
            elements += FabushiAppAgentSurface.Element("agent-network-open", "button", if (node.isGroup) "Open room" else "Open chat")
            actions["agent-network-open"] = FabushiAppAgentSurface.Action(setOf("invoke")) { botsById[node.id]?.let(onOpenBot) }
        }
        appAgentSurface.publish(screen = "agent-network", elements = elements, actions = actions)
    }
    DisposableEffect(appAgentSurface) { onDispose { appAgentSurface.clear() } }

    Column(Modifier.fillMaxSize().background(GrokMobileBackground).testTag("agent-network-surface")) {
        Row(
            Modifier.fillMaxWidth().padding(horizontal = 14.dp, vertical = 10.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text("‹", color = GrokMobileInk, fontSize = 34.sp, modifier = Modifier.clickable(onClick = onBack).padding(horizontal = 8.dp))
            Column(Modifier.weight(1f)) {
                Text("Org chart", color = GrokMobileInk, fontSize = 20.sp, fontWeight = FontWeight.Bold)
                Text(agentNetworkSummary(nodes, edges), color = GrokMobileMuted, fontSize = 12.sp, modifier = Modifier.testTag("agent-network-summary"))
            }
            TextButton(onClick = { selectedId = null }) { Text("Reset") }
        }
        Box(Modifier.weight(1f).fillMaxWidth()) {
            if (nodes.isEmpty()) {
                Text(
                    "No agents yet. Create a few teammates and the network draws itself.",
                    color = GrokMobileMuted,
                    modifier = Modifier.align(Alignment.Center).padding(24.dp).testTag("agent-network-empty"),
                )
            } else {
                AgentNetworkGraph(
                    nodes = nodes,
                    edges = edges,
                    selectedId = selectedId,
                    onSelect = { selectedId = if (selectedId == it) null else it },
                )
            }
            selected?.let { node ->
                AgentNetworkInspector(
                    node = node,
                    nodesById = nodes.associateBy { it.id },
                    onClose = { selectedId = null },
                    onOpen = { botsById[node.id]?.let(onOpenBot) },
                    modifier = Modifier.align(Alignment.BottomCenter),
                )
            }
        }
        Text(
            "Solid links are agent message history; dashed links are group membership. Pinch to zoom and drag to pan.",
            color = GrokMobileMuted,
            fontSize = 11.sp,
            modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 10.dp),
        )
    }
}

@Composable
private fun AgentNetworkGraph(
    nodes: List<AgentNetworkNode>,
    edges: List<AgentNetworkEdge>,
    selectedId: String?,
    onSelect: (String) -> Unit,
) {
    var viewport by remember { mutableStateOf(AgentNetworkViewport()) }
    BoxWithConstraints(Modifier.fillMaxSize().testTag("agent-network-graph")) {
        val widthPx = constraints.maxWidth.toFloat().coerceAtLeast(1f)
        val heightPx = constraints.maxHeight.toFloat().coerceAtLeast(1f)
        val positions = remember(nodes, widthPx, heightPx) { layoutAgentNetwork(nodes, widthPx, heightPx) }
        val byId = remember(nodes) { nodes.associateBy { it.id } }
        val now = System.currentTimeMillis()
        Box(
            Modifier
                .fillMaxSize()
                .graphicsLayer {
                    scaleX = viewport.scale
                    scaleY = viewport.scale
                    translationX = viewport.x
                    translationY = viewport.y
                }
                .pointerInput(widthPx, heightPx) {
                    detectTransformGestures { _, pan, zoom, _ ->
                        viewport = transformAgentNetworkViewport(viewport, zoom, pan.x, pan.y, widthPx, heightPx)
                    }
                },
        ) {
            Canvas(Modifier.fillMaxSize()) {
                edges.forEach { edge ->
                    val source = positions[edge.sourceId] ?: return@forEach
                    val target = positions[edge.targetId] ?: return@forEach
                    val activity = agentNetworkEdgeActivity(edge, byId, now)
                    val alpha = when (activity) {
                        AgentNetworkEdgeActivity.TALKING -> 0.9f
                        AgentNetworkEdgeActivity.RECENT -> 0.55f
                        AgentNetworkEdgeActivity.IDLE -> 0.22f
                    }
                    drawLine(
                        color = GrokMobileInk.copy(alpha = alpha),
                        start = Offset(source.x, source.y),
                        end = Offset(target.x, target.y),
                        strokeWidth = if (edge.kind == AgentNetworkEdgeKind.MESSAGE) 3f else 2f,
                        cap = StrokeCap.Round,
                    )
                }
            }
            nodes.forEach { node ->
                val point = positions[node.id] ?: return@forEach
                val activity = agentNetworkActivity(node)
                Column(
                    Modifier
                        .offset { IntOffset((point.x - 42f).roundToInt(), (point.y - 42f).roundToInt()) }
                        .widthIn(min = 84.dp, max = 118.dp)
                        .background(
                            if (selectedId == node.id) Color.White else Color.White.copy(alpha = 0.92f),
                            RoundedCornerShape(18.dp),
                        )
                        .clickable { onSelect(node.id) }
                        .padding(horizontal = 10.dp, vertical = 9.dp)
                        .testTag("agent-network-node-${node.id}"),
                    horizontalAlignment = Alignment.CenterHorizontally,
                ) {
                    Box(
                        Modifier.size(34.dp).background(if (node.isGroup) Color(0xFFD8E6FF) else Color(0xFFDFF5E7), CircleShape),
                        contentAlignment = Alignment.Center,
                    ) { Text(if (node.isGroup) "◫" else node.name.take(1).uppercase(), fontWeight = FontWeight.Bold) }
                    Spacer(Modifier.height(4.dp))
                    Text(node.name, color = GrokMobileInk, fontSize = 12.sp, fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    val caption = when (activity) {
                        AgentNetworkActivity.WAITING -> "Waiting for you"
                        AgentNetworkActivity.WORKING -> "Working…"
                        AgentNetworkActivity.IDLE -> if (node.isGroup) "${node.memberIds.size} members" else ""
                    }
                    if (caption.isNotBlank()) Text(caption, color = GrokMobileMuted, fontSize = 9.sp, maxLines = 1)
                }
            }
        }
    }
}

@Composable
private fun AgentNetworkInspector(
    node: AgentNetworkNode,
    nodesById: Map<String, AgentNetworkNode>,
    onClose: () -> Unit,
    onOpen: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val members = node.memberIds.mapNotNull(nodesById::get)
    Column(
        modifier.background(Color.White, RoundedCornerShape(topStart = 24.dp, topEnd = 24.dp)).padding(18.dp).testTag("agent-network-inspector"),
        verticalArrangement = Arrangement.spacedBy(7.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(node.name, color = GrokMobileInk, fontSize = 18.sp, fontWeight = FontWeight.Bold, modifier = Modifier.weight(1f))
            Text("×", color = GrokMobileMuted, fontSize = 24.sp, modifier = Modifier.clickable(onClick = onClose).padding(horizontal = 8.dp))
        }
        Text(
            when (agentNetworkActivity(node)) {
                AgentNetworkActivity.WAITING -> "Waiting for you"
                AgentNetworkActivity.WORKING -> "Working…"
                AgentNetworkActivity.IDLE -> "Idle"
            },
            color = GrokMobileMuted,
            fontSize = 12.sp,
        )
        if (node.description.isNotBlank()) Text(node.description, color = GrokMobileInk, fontSize = 13.sp, maxLines = 3, overflow = TextOverflow.Ellipsis)
        if (members.isNotEmpty()) Text("${members.size} members · ${members.joinToString { it.name }}", color = GrokMobileMuted, fontSize = 12.sp, maxLines = 2, overflow = TextOverflow.Ellipsis)
        if (node.lastMessage.isNotBlank()) Text(node.lastMessage, color = GrokMobileMuted, fontSize = 12.sp, maxLines = 2, overflow = TextOverflow.Ellipsis)
        Button(onClick = onOpen, modifier = Modifier.fillMaxWidth().testTag("agent-network-open")) {
            Text(if (node.isGroup) "Open room" else "Open chat")
        }
    }
}
