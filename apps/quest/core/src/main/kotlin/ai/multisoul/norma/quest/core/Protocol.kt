package ai.multisoul.norma.quest.core

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.intOrNull

@Serializable
sealed interface HostMessage { val version: Int }

@Serializable
@SerialName("session.snapshot")
data class SessionSnapshot(
    override val version: Int = 1,
    val source: String,
    val sessionId: String,
    val sequence: Long,
    val hostName: String,
    val agents: List<AgentSummary>,
    val decisions: List<Decision>,
    val terminal: TerminalTranscript,
) : HostMessage

@Serializable
data class AgentSummary(
    val id: String, val name: String, val taskId: String,
    val task: String, val status: String, val step: String,
)

@Serializable
data class Decision(
    val id: String, val taskId: String, val agentId: String,
    val title: String, val context: String, val options: List<DecisionOption>,
)

@Serializable
data class DecisionOption(val id: String, val label: String)

@Serializable
data class TerminalTranscript(val agentId: String, val title: String, val lines: List<String>)

@Serializable
@SerialName("command.result")
data class CommandResult(
    override val version: Int = 1, val requestId: String, val accepted: Boolean, val message: String,
) : HostMessage

@Serializable
@SerialName("error")
data class HostError(override val version: Int = 1, val message: String) : HostMessage

@Serializable
data class DecisionCommand(
    val requestId: String, val decisionId: String, val optionId: String,
    val version: Int = 1, val type: String = "decision.resolve",
)

object Protocol {
    private val json = Json { ignoreUnknownKeys = true; encodeDefaults = true }
    const val HELLO = """{"version":1,"type":"hello"}"""

    fun decode(text: String): HostMessage {
        require(text.length <= 262144) { "Message exceeds development protocol limit" }
        val envelope = json.parseToJsonElement(text).jsonObject
        require(envelope["version"]?.jsonPrimitive?.intOrNull == 1) { "Missing or unsupported protocol version" }
        val message = json.decodeFromString<HostMessage>(text)
        require(message.version == 1) { "Unsupported protocol version" }
        if (message is SessionSnapshot) {
            require(message.sequence >= 0 && message.sessionId.isNotBlank()) { "Invalid session identity" }
            require(message.agents.size <= 100 && message.decisions.size <= 100) { "Too many items" }
            require(message.terminal.lines.size <= 200) { "Transcript exceeds development limit" }
        }
        return message
    }

    fun encodeSnapshot(snapshot: SessionSnapshot): String = json.encodeToString<HostMessage>(snapshot)
    fun encodeCommand(command: DecisionCommand): String = json.encodeToString(command)
}
