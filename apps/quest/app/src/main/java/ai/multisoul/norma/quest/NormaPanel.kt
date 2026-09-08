package ai.multisoul.norma.quest

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.meta.spatial.uiset.theme.LocalColorScheme
import com.meta.spatial.uiset.theme.SpatialTheme
import com.meta.spatial.uiset.theme.darkSpatialColorScheme

enum class PanelKind { WORKSPACE, AGENTS, DECISIONS }

@Composable
fun NormaPanel(kind: PanelKind, connection: HostConnection) {
    SpatialTheme(colorScheme = darkSpatialColorScheme()) {
        MaterialTheme(colorScheme = darkColorScheme(primary = Color(0xFFB8D9FB))) {
            Column(
                Modifier.fillMaxSize().clip(SpatialTheme.shapes.large)
                    .background(LocalColorScheme.current.panel).padding(28.dp),
                verticalArrangement = Arrangement.spacedBy(16.dp),
            ) {
                when (kind) {
                    PanelKind.WORKSPACE -> WorkspacePanel(connection)
                    PanelKind.AGENTS -> AgentsPanel(connection.state)
                    PanelKind.DECISIONS -> DecisionsPanel(connection)
                }
            }
        }
    }
}

@Composable
private fun ColumnScope.WorkspacePanel(connection: HostConnection) {
    val state = connection.state
    var endpoint by remember { mutableStateOf(if (BuildConfig.DEBUG) "ws://127.0.0.1:8787/session" else "") }
    Text("Norma", style = MaterialTheme.typography.headlineLarge)
    Text(state.message, style = MaterialTheme.typography.bodyLarge)
    if (state.status != ConnectionStatus.CONNECTED) {
        OutlinedTextField(
            value = endpoint, onValueChange = { endpoint = it }, label = { Text("Mac 会话地址") },
            singleLine = true, modifier = Modifier.fillMaxWidth(),
            enabled = state.status != ConnectionStatus.CONNECTING,
        )
        Button(
            onClick = { connection.connect(endpoint) },
            enabled = state.status != ConnectionStatus.CONNECTING && endpoint.isNotBlank(),
        ) { Text(if (state.status == ConnectionStatus.CONNECTING) "正在连接…" else "连接 Mac") }
    } else {
        Row(horizontalArrangement = Arrangement.spacedBy(16.dp)) {
            Text(if (state.snapshot?.source == "mock") "MOCK · 开发会话" else "Mac 会话", color = MaterialTheme.colorScheme.primary)
            TextButton(onClick = { connection.disconnect() }) { Text("断开") }
        }
    }
    val transcript = state.snapshot?.terminal
    if (transcript == null) {
        Text("Agent 在 Mac 上执行。Quest 呈现终端、任务进度与需要你决定的事项。", style = MaterialTheme.typography.bodyLarge)
        Text("开发联调：先在 Mac 启动 mock 服务，并通过 USB 调试转发端口。", style = MaterialTheme.typography.bodyMedium)
    } else {
        Text(transcript.title, style = MaterialTheme.typography.titleMedium)
        SelectionContainer(Modifier.weight(1f).fillMaxWidth().verticalScroll(rememberScrollState())) {
            Text(transcript.lines.joinToString("\n"), fontFamily = FontFamily.Monospace, fontSize = 18.sp, lineHeight = 29.sp)
        }
    }
}

@Composable
private fun ColumnScope.AgentsPanel(state: ConnectionState) {
    Text("正在执行", style = MaterialTheme.typography.headlineSmall)
    val agents = state.snapshot?.agents.orEmpty()
    Text(if (state.status == ConnectionStatus.CONNECTED) "${agents.count { it.status == "working" }} 个 Agent 运行中" else "连接后显示 Mac 任务")
    LazyColumn(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(24.dp)) {
        items(agents, key = { it.id }) { agent ->
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(agent.name, style = MaterialTheme.typography.titleLarge)
                Text(agent.task, style = MaterialTheme.typography.bodyLarge)
                Text(
                    when (agent.status) { "working" -> "运行中"; "waiting" -> "等待你决定"; "done" -> "已完成"; else -> agent.status },
                    color = MaterialTheme.colorScheme.primary,
                )
                Text(agent.step, style = MaterialTheme.typography.bodyMedium)
            }
        }
    }
    if (state.status != ConnectionStatus.CONNECTED && agents.isNotEmpty()) Text("上次连接时的状态", color = MaterialTheme.colorScheme.outline)
}

@Composable
private fun ColumnScope.DecisionsPanel(connection: HostConnection) {
    val state = connection.state
    val decisions = state.snapshot?.decisions.orEmpty()
    var selected by remember { mutableStateOf<String?>(null) }
    Text("需要你决定", style = MaterialTheme.typography.headlineSmall)
    Text(if (state.snapshot == null) "连接 Mac 后同步待办" else "${decisions.size} 项待办")
    LazyColumn(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        items(decisions, key = { it.id }) { decision ->
            Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                TextButton(
                    onClick = { selected = if (selected == decision.id) null else decision.id },
                    modifier = Modifier.fillMaxWidth(),
                ) { Text(decision.title, style = MaterialTheme.typography.titleMedium) }
                if (selected == decision.id) {
                    Text(decision.context, style = MaterialTheme.typography.bodyLarge)
                    decision.options.forEach { option ->
                        OutlinedButton(
                            onClick = { connection.decide(decision.id, option.id) },
                            enabled = state.status == ConnectionStatus.CONNECTED && decision.id !in state.pendingDecisions,
                            modifier = Modifier.fillMaxWidth(),
                        ) { Text(option.label) }
                    }
                    if (decision.id in state.pendingDecisions) Text("等待 Mac 确认…")
                }
                HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
            }
        }
        if (decisions.isEmpty() && state.snapshot != null) item { Text("当前没有需要你决定的事项。") }
    }
}
