package ai.multisoul.norma.quest

import android.os.Handler
import android.os.Looper
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import ai.multisoul.norma.quest.core.CommandResult
import ai.multisoul.norma.quest.core.DecisionCommand
import ai.multisoul.norma.quest.core.HostError
import ai.multisoul.norma.quest.core.Protocol
import ai.multisoul.norma.quest.core.SessionSnapshot
import ai.multisoul.norma.quest.core.SessionStore
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import java.net.URI
import java.util.UUID
import java.util.concurrent.TimeUnit

enum class ConnectionStatus { DISCONNECTED, CONNECTING, CONNECTED, ERROR }

data class ConnectionState(
    val status: ConnectionStatus = ConnectionStatus.DISCONNECTED,
    val snapshot: SessionSnapshot? = null,
    val message: String = "连接 Mac，查看任务与终端",
    val pendingDecisions: Set<String> = emptySet(),
)

class HostConnection {
    var state by mutableStateOf(ConnectionState())
        private set
    private val main = Handler(Looper.getMainLooper())
    private val store = SessionStore()
    private val requests = mutableMapOf<String, String>()
    private val client = OkHttpClient.Builder()
        .connectTimeout(10, TimeUnit.SECONDS).readTimeout(0, TimeUnit.MILLISECONDS)
        .pingInterval(15, TimeUnit.SECONDS).build()
    private var socket: WebSocket? = null
    private var generation = 0

    fun connect(address: String) {
        disconnect()
        val uri = runCatching { URI(address.trim()) }.getOrNull()
        val loopback = uri?.host in setOf("127.0.0.1", "localhost", "::1", "[::1]")
        val permitted = uri?.scheme == "wss" || (BuildConfig.DEBUG && uri?.scheme == "ws" && loopback)
        if (!permitted || uri?.host.isNullOrBlank() || uri?.userInfo != null || uri?.path != "/session") {
            state = state.copy(status = ConnectionStatus.ERROR, message = "请输入 wss://主机/session；开发版可连接本机 mock")
            return
        }
        val request = runCatching { Request.Builder().url(address.trim()).build() }.getOrElse {
            state = state.copy(status = ConnectionStatus.ERROR, message = "连接地址无效")
            return
        }
        store.reset()
        state = ConnectionState(status = ConnectionStatus.CONNECTING, message = "正在连接 Mac…")
        val active = generation
        socket = client.newWebSocket(request, object : WebSocketListener() {
            override fun onOpen(webSocket: WebSocket, response: Response) {
                webSocket.send(Protocol.HELLO)
            }
            override fun onMessage(webSocket: WebSocket, text: String) = dispatch(active) {
                runCatching { consume(text) }.onFailure { fail("Mac 返回了不兼容的会话数据") }
            }
            override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) = dispatch(active) {
                fail("无法连接 Mac，请检查开发服务和 USB 转发")
            }
            override fun onClosing(webSocket: WebSocket, code: Int, reason: String) {
                webSocket.close(code, reason)
            }
            override fun onClosed(webSocket: WebSocket, code: Int, reason: String) = dispatch(active) {
                disconnect()
            }
        })
        main.postDelayed({
            if (active == generation && state.status == ConnectionStatus.CONNECTING) fail("等待 Mac 会话超时，请重试")
        }, 12_000)
    }

    private fun consume(text: String) {
        when (val message = Protocol.decode(text)) {
            is SessionSnapshot -> if (store.accept(message)) {
                state = state.copy(status = ConnectionStatus.CONNECTED, snapshot = message, message = "已连接 ${message.hostName}")
            }
            is CommandResult -> {
                val decisionId = requests.remove(message.requestId)
                state = state.copy(
                    pendingDecisions = state.pendingDecisions - setOfNotNull(decisionId),
                    message = if (message.accepted) "Mac 已确认决定" else "该决定未被接受，请查看最新任务状态",
                )
            }
            is HostError -> fail("会话协议错误：${message.message.take(100)}")
        }
    }

    fun decide(decisionId: String, optionId: String) {
        if (state.status != ConnectionStatus.CONNECTED || decisionId in state.pendingDecisions) return
        val decision = state.snapshot?.decisions?.find { it.id == decisionId } ?: return
        if (decision.options.none { it.id == optionId }) return
        val requestId = UUID.randomUUID().toString()
        val command = Protocol.encodeCommand(DecisionCommand(requestId, decisionId, optionId))
        if (socket?.send(command) != true) { fail("连接已断开，决定尚未提交"); return }
        requests[requestId] = decisionId
        state = state.copy(pendingDecisions = state.pendingDecisions + decisionId, message = "等待 Mac 确认…")
        val active = generation
        main.postDelayed({
            if (active == generation && requests.remove(requestId) != null) {
                state = state.copy(pendingDecisions = state.pendingDecisions - decisionId, message = "确认超时，请重新连接以获取最新状态")
            }
        }, 10_000)
    }

    fun disconnect() {
        generation++
        socket?.cancel()
        socket = null
        requests.clear()
        main.removeCallbacksAndMessages(null)
        state = state.copy(status = ConnectionStatus.DISCONNECTED, pendingDecisions = emptySet(), message = "未连接 Mac")
    }

    private fun fail(message: String) {
        disconnect()
        state = state.copy(status = ConnectionStatus.ERROR, message = message)
    }

    private fun dispatch(active: Int, action: () -> Unit) {
        main.post { if (active == generation) action() }
    }

    fun close() {
        disconnect()
        client.dispatcher.executorService.shutdown()
        client.connectionPool.evictAll()
    }
}
