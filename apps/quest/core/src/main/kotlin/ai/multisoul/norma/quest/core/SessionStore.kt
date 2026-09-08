package ai.multisoul.norma.quest.core

/** Single-connection store. The Mac owns state; the client never optimistically approves work. */
class SessionStore {
    var snapshot: SessionSnapshot? = null
        private set

    fun receive(text: String): Boolean {
        val incoming = Protocol.decode(text) as? SessionSnapshot ?: return false
        return accept(incoming)
    }

    fun accept(incoming: SessionSnapshot): Boolean {
        val current = snapshot
        if (current != null && current.sessionId == incoming.sessionId && incoming.sequence <= current.sequence) return false
        snapshot = incoming
        return true
    }

    fun reset() { snapshot = null }
}
