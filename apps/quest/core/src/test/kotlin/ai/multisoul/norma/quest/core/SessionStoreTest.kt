package ai.multisoul.norma.quest.core

import org.junit.Assert.*
import org.junit.Test

class SessionStoreTest {
    // 场景：Quest 消费 Mac 端同一份协议 fixture，收到新快照后又收到旧快照。
    // 从资源读取两个待办的初始数据，将第二个版本的待办清空，再重放第一版。
    // 预期：保留第二版状态；旧消息不能恢复已经处理的待办。
    @Test fun sharedFixtureAndOutOfOrderMessages() {
        val store = SessionStore()
        val fixture = javaClass.getResource("/snapshot.v1.json")!!.readText()
        assertTrue("首个合法快照应被接受", store.receive(fixture))
        assertEquals("两个待办都应解析出来", 2, store.snapshot!!.decisions.size)
        val newer = store.snapshot!!.copy(sequence = 2, decisions = emptyList())
        assertTrue("较新版本应被接受", store.receive(Protocol.encodeSnapshot(newer)))
        assertFalse("乱序旧快照必须被忽略", store.receive(fixture))
        assertTrue("已处理待办不能被旧快照恢复", store.snapshot!!.decisions.isEmpty())
    }

    // 场景：Mac 发来不带版本号的快照，不能自动当作 v1 执行。
    // 从公共 fixture 移除 version 字段，先载入合法快照再消费非法数据。
    // 预期：拒绝非法消息，同时保持已知的权威状态。
    @Test fun missingProtocolVersionDoesNotSilentlyBecomeV1() {
        val store = SessionStore()
        val fixture = javaClass.getResource("/snapshot.v1.json")!!.readText()
        store.receive(fixture)
        val original = store.snapshot
        val missingVersion = fixture.replace("\"version\": 1,", "")
        assertThrows("缺少 version 必须拒绝", IllegalArgumentException::class.java) { store.receive(missingVersion) }
        assertSame("错误消息不得改变现有会话", original, store.snapshot)
    }
}
