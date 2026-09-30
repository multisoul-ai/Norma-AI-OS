import XCTest
@testable import NormaCore

final class WorkThreadStoreTests: XCTestCase {
    /// 线程输入草稿重启可读；发送只清理对应原文，不删除另一线程或后来编辑的草稿。
    /// 数据构造：1 个 Workspace、2 个线程，草稿分别为「 第一条 \n」和「第二条」；发送时正文会去首尾空白。
    /// 执行过程：保存草稿 → 重建 Store → 发第一条 → 核对第二条 → 保存新草稿 → 重试旧消息 → 拒绝不存在的线程。
    /// 预期结果：草稿原样恢复；同文发送与清理原子完成；新草稿和另一线程保留；无效写入不损坏旧草稿。
    func testDraftRestartAndAtomicSendPreserveOtherEdits() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let url = directory.appendingPathComponent("workspaces.sqlite")
        let store = CollaborationWorkspaceStore(databaseURL: url)
        let workspace = try await store.create(name: "移动端体验")
        let first = try await store.createThread(workspaceID: workspace.id, title: "第一件事")
        let second = try await store.createThread(workspaceID: workspace.id, title: "第二件事")
        let originalDraft = WorkThreadDraft(text: " 第一条 \n")
        try await store.saveMessageDraft(originalDraft, threadID: first.id)
        try await store.saveMessageDraft(WorkThreadDraft(text: "第二条"), threadID: second.id)
        let reopened = CollaborationWorkspaceStore(databaseURL: url)
        let restored = try await reopened.messageDrafts()
        XCTAssertEqual(restored[first.id]?.text, " 第一条 \n", "重启不能改写输入中的空白或换行")
        XCTAssertEqual(restored[second.id]?.text, "第二条", "第二线程草稿应独立保留")
        let id = UUID().uuidString
        _ = try await reopened.appendMessage(threadID: first.id, id: id, text: " 第一条 \n", clearingDraftRevision: originalDraft.revision)
        let sentDrafts = try await store.messageDrafts()
        XCTAssertNil(sentDrafts[first.id], "消息落盘和清理对应草稿须在同一事务中完成")
        XCTAssertEqual(sentDrafts[second.id]?.text, "第二条", "发送第一线程不能清空第二线程")
        try await reopened.saveMessageDraft(WorkThreadDraft(text: "下一条"), threadID: first.id)
        _ = try await reopened.appendMessage(threadID: first.id, id: id, text: "第一条")
        do {
            try await reopened.saveMessageDraft(WorkThreadDraft(text: "不存在的线程"), threadID: UUID().uuidString)
            XCTFail("未绑定真实线程的草稿必须拒绝")
        } catch { XCTAssertTrue(error is WorkThreadError, "非法身份应返回线程错误") }
        let finalDrafts = try await store.messageDrafts()
        let messages = try await store.messages(threadID: first.id)
        XCTAssertEqual(finalDrafts[first.id]?.text, "下一条", "旧消息重试不能删除后来输入的新草稿")
        XCTAssertEqual(finalDrafts[second.id]?.text, "第二条", "失败替换必须回滚，不能丢失已有草稿")
        XCTAssertEqual(messages.map(\.text), ["第一条"], "草稿和消息分开保存，重复请求不能新增消息")
        XCTAssertFalse(messages.contains { $0.text == "下一条" }, "未点发送的草稿不得变成消息")
    }

    /// 新编辑版本即使与已发送原文相同也保留；超长草稿失败只影响本线程。
    /// 数据构造：2 个线程；新旧草稿均为「继续」但 revision 不同；100001 = 100000 字符上限 + 1。
    /// 执行过程：保存旧草稿 → 同文新版本 → 用旧版本发送 → 另一线程超限保存 → 保存有效草稿 → 重启。
    /// 预期结果：新版本不被清理；超限不覆盖旧数据；另一线程仍能保存，失败原文不截断写入。
    func testSameTextRevisionAndOversizedDraftAreIsolated() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let url = directory.appendingPathComponent("workspaces.sqlite")
        let store = CollaborationWorkspaceStore(databaseURL: url)
        let workspace = try await store.create(name: "测试")
        let first = try await store.createThread(workspaceID: workspace.id, title: "首屏")
        let second = try await store.createThread(workspaceID: workspace.id, title: "导航")
        let old = WorkThreadDraft(text: "继续")
        let revised = WorkThreadDraft(text: "继续")
        try await store.saveMessageDraft(old, threadID: first.id)
        try await store.saveMessageDraft(revised, threadID: first.id)
        _ = try await store.appendMessage(threadID: first.id, id: UUID().uuidString, text: old.text,
                                          clearingDraftRevision: old.revision)
        do {
            try await store.saveMessageDraft(WorkThreadDraft(text: String(repeating: "字", count: 100_001)), threadID: second.id)
            XCTFail("超限原文应拒绝而非截断保存")
        } catch { XCTAssertTrue(error is WorkThreadError, "超限应返回内容错误") }
        try await store.saveMessageDraft(WorkThreadDraft(text: "有效草稿"), threadID: second.id)
        let restored = try await CollaborationWorkspaceStore(databaseURL: url).messageDrafts()
        XCTAssertEqual(restored[first.id]?.revision, revised.revision, "同文的新编辑版本不能被旧发送删除")
        XCTAssertNotEqual(restored[first.id]?.revision, old.revision, "不能恢复为旧的发送版本")
        XCTAssertEqual(restored[second.id]?.text, "有效草稿", "一次超限失败不能阻塞后续独立保存")
        XCTAssertFalse(restored.values.contains { $0.text.count > 100_000 }, "无效超长草稿不得写入数据库")
    }

    /// 从想法建线程后原件仍在；两个线程隔离，消息按稳定 ID 去重，重启保留。
    /// 数据构造：1 条想法、Workspace A/B 各 1 个、A 的线程 2 个；重复提交同一 ID 两次只能保存 1 条。
    /// 执行过程：记录想法 → 引用建 A1 → 建 A2 → A1 留言并重试 → 冲突重试 → 重建 Store。
    /// 预期结果：想法仍独立存在；A1 有原消息，A2/B 无该消息；来源快照不因想法编辑而改变。
    func testThreadSourceAndMessageIsolationSurviveRestart() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let ideas = IdeaStore(databaseURL: directory.appendingPathComponent("ideas.sqlite"),
                              recordingsDirectory: directory.appendingPathComponent("recordings"))
        let idea = try await ideas.createText("首屏信息太多")
        let url = directory.appendingPathComponent("workspaces.sqlite")
        let store = CollaborationWorkspaceStore(databaseURL: url)
        let a = try await store.create(name: "移动端体验")
        let b = try await store.create(name: "发布质量")
        let first = try await store.createThread(workspaceID: a.id, title: "改进首屏交接", source: idea)
        let second = try await store.createThread(workspaceID: a.id, title: "检查导航")
        let messageID = UUID().uuidString
        let message = try await store.appendMessage(threadID: first.id, id: messageID, text: "先梳理层级")
        let duplicate = try await store.appendMessage(threadID: first.id, id: messageID, text: "先梳理层级")
        XCTAssertEqual(duplicate, message, "同 ID 同内容重试应返回原消息而不是新增")
        do { _ = try await store.appendMessage(threadID: first.id, id: messageID, text: "覆盖原文"); XCTFail("同 ID 不同内容必须拒绝") }
        catch { XCTAssertTrue(error is WorkThreadError, "冲突应为明确线程错误") }
        _ = try await ideas.updateBody(id: idea.id, body: "修改后的想法")
        let restarted = CollaborationWorkspaceStore(databaseURL: url)
        let messages = try await restarted.messages(threadID: first.id)
        let otherMessages = try await restarted.messages(threadID: second.id)
        let otherThreads = try await restarted.threads(workspaceID: b.id)
        let threads = try await restarted.threads(workspaceID: a.id)
        let originals = try await ideas.all()
        XCTAssertEqual(messages.map(\.text), ["先梳理层级"], "重试不能新增或覆盖消息")
        XCTAssertEqual(messages.first?.state, .pending, "无 Mac 回执的消息不得记为已送达")
        XCTAssertTrue(otherMessages.isEmpty, "第二线程不得继承第一线程的消息")
        XCTAssertTrue(otherThreads.isEmpty, "另一 Workspace 不得继承线程")
        XCTAssertEqual(threads.first(where: { $0.id == first.id })?.sourceIdeaID, idea.id, "来源 ID 应长期保留")
        XCTAssertEqual(threads.first(where: { $0.id == first.id })?.sourceExcerpt, "首屏信息太多", "历史引用不随原想法编辑而改写")
        XCTAssertEqual(originals.first?.body, "修改后的想法", "引用不能移动或锁死原想法")
    }

    /// 目标限定在线程内，决定在 Workspace 内共享；来源与版本冲突拒绝，归档仍可读。
    /// 数据构造：A 有两个线程，B 有一个线程；决定版本从 1 修订到 2，旧版本 1 再写是冲突。
    /// 执行过程：A1 写消息/目标/决定 → A2 读取 → 修订决定 → 旧版本重写/跨空间引用 → 归档 A。
    /// 预期结果：A2 能读决定但不继承 A1 目标；B 不能引用 A 的消息；归档拒绝新消息而保留旧版本。
    func testFactsKeepVersionsAndRejectForeignSourcesOrArchivedWrites() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let store = CollaborationWorkspaceStore(databaseURL: directory.appendingPathComponent("workspaces.sqlite"))
        var a = try await store.create(name: "移动端体验")
        let b = try await store.create(name: "发布质量")
        let a1 = try await store.createThread(workspaceID: a.id, title: "首屏")
        let a2 = try await store.createThread(workspaceID: a.id, title: "导航")
        let b1 = try await store.createThread(workspaceID: b.id, title: "发布")
        let source = try await store.appendMessage(threadID: a1.id, id: UUID().uuidString, text: "只突出下一步")
        let goal = try await store.saveFact(threadID: a1.id, kind: .goal, text: "看懂首屏", sourceMessageID: source.id)
        let decision = try await store.saveFact(threadID: a1.id, kind: .decision, text: "一个主操作", sourceMessageID: source.id)
        let revised = try await store.saveFact(threadID: a1.id, kind: .decision, text: "保留一个明确主操作", replacing: decision)
        let crossThreadFact = try await store.saveFact(threadID: a2.id, kind: .fact, text: "讨论来自首屏线程", sourceMessageID: source.id)
        let sourceFromWorkspace = try await store.sourceMessage(id: source.id, workspaceID: a.id)
        XCTAssertEqual(sourceFromWorkspace.threadID, a1.id, "共同事实可引用同 Workspace 其他线程消息，回看来源须打开原线程")
        XCTAssertEqual(crossThreadFact.sourceMessageID, source.id, "跨线程事实保留准确来源 ID")
        do { _ = try await store.sourceMessage(id: source.id, workspaceID: b.id); XCTFail("不能越过 Workspace 读取来源消息") }
        catch { XCTAssertTrue(error is WorkThreadError, "跨 Workspace 读取来源须显式拒绝") }
        XCTAssertEqual(revised.version, 2, "首次修订必须递增为版本 2")
        do { _ = try await store.saveFact(threadID: a1.id, kind: .decision, text: "过期编辑", replacing: decision); XCTFail("旧版本不能覆盖新版") }
        catch { XCTAssertTrue(error is WorkThreadError, "过期版本应显式失败") }
        do { _ = try await store.saveFact(threadID: b1.id, kind: .decision, text: "错误归属", sourceMessageID: source.id); XCTFail("B 不能引用 A 的消息") }
        catch { XCTAssertTrue(error is WorkThreadError, "跨 Workspace 来源应拒绝") }
        let shared = try await store.facts(threadID: a2.id)
        let foreign = try await store.facts(threadID: b1.id)
        XCTAssertEqual(shared.first(where: { $0.id == decision.id })?.text, revised.text, "同 Workspace 应共享当前决定")
        XCTAssertFalse(shared.contains { $0.id == goal.id }, "A1 目标不能成为 A2 的目标")
        XCTAssertTrue(foreign.isEmpty, "B 不得出现 A 的共同事实")
        a.archived = true; try await store.save(a)
        do { _ = try await store.appendMessage(threadID: a1.id, id: UUID().uuidString, text: "归档后新增"); XCTFail("归档后必须先重开再发消息") }
        catch { XCTAssertTrue(error is WorkThreadError, "归档写入应显式拒绝") }
        let history = try await store.factHistory(id: decision.id)
        XCTAssertEqual(history.map(\.version), [1, 2], "归档不能删除旧修订")
        XCTAssertEqual(history.first?.sourceMessageID, source.id, "旧修订来源不能丢失")
        XCTAssertNil(history.last?.sourceMessageID, "手动修订不能冒用旧消息来源")
        let remaining = try await store.messages(threadID: a1.id)
        XCTAssertEqual(remaining.map(\.id), [source.id], "归档后原消息可读，新消息未写入")
    }
}
