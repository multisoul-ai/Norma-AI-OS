import XCTest
@testable import NormaCore

@MainActor final class ChatDraftTests: XCTestCase {
    private func scope(machine: String = "a", project: String = "project-a", route: String = "route-a", node: UInt64 = 7, run: String? = "run-a", session: String? = "session-a") -> ChatDraft.Scope {
        ChatDraft.Scope(machineID: "mac-" + String(repeating: machine, count: 64), workspaceID: project, routeID: route, nodeID: node, runID: run, sessionID: session)
    }
    private func directory() throws -> URL {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        return url
    }

    /// 原生 Codex 在首次提交前只有 PTY 运行身份，草稿仍应可保存。
    ///
    /// 数据构造：node7/run-a，无 session；对照为非法 run ID、缺少 session 或 run 的成果反馈。
    /// 执行过程：编辑 → 落盘 → 重建客户端 → 核验草稿范围及非法身份。
    /// 预期结果：首次输入完整恢复，不自动进入随后创建的会话；非法身份和无会话反馈被拒绝。
    func testNativePromptDraftBeforeSessionStarts() async throws {
        let dir = try directory(); defer { try? FileManager.default.removeItem(at: dir) }
        let url = dir.appendingPathComponent("drafts.json")
        let pending = scope(session: nil)
        let book = ChatDraftBook(store: ChatDraftStore(url: url)); await book.load()
        book.update("第一次输入", for: pending)
        try await book.flush()
        let reopened = ChatDraftBook(store: ChatDraftStore(url: url)); await reopened.load()
        XCTAssertEqual(reopened.draft(for: pending)?.text, "第一次输入", "原生空提示符的草稿必须落盘并恢复")
        XCTAssertNil(reopened.draft(for: scope()), "新会话不能自动继承旧范围草稿")
        XCTAssertThrowsError(try scope(run: "invalid/run").validate(), "允许尚无运行的历史会话不能放松 ID 格式校验")
        var feedback = pending
        feedback.feedbackFingerprint = String(repeating: "a", count: 64)
        XCTAssertThrowsError(try feedback.validate(), "成果反馈必须绑定已存在的会话")
        var inactiveFeedback = scope(run: nil)
        inactiveFeedback.feedbackFingerprint = String(repeating: "a", count: 64)
        XCTAssertThrowsError(try inactiveFeedback.validate(), "成果反馈仍须绑定准确运行，不能仅凭历史会话")
    }

    /// 草稿在重建客户端后恢复，并按电脑/项目/窗口/卡片/运行/会话完整隔离。
    ///
    /// 数据构造：两台证书身份不同的Mac，局部卡片ID都为7；快速编辑三次，最终正文含首尾空白、LF和Tab。
    /// 执行过程：加载空库 → 连续编辑 → 切到另一Mac编辑 → flush → 重建读取 → 查询逐项不同的范围。
    /// 预期结果：最后一次编辑完整恢复；另一Mac正文独立；任一范围不符不能自动带入旧草稿。
    func testDraftsSurviveReloadWithoutCrossingIdentity() async throws {
        let dir = try directory(); defer { try? FileManager.default.removeItem(at: dir) }
        let store = ChatDraftStore(url: dir.appendingPathComponent("drafts.json"))
        let book = ChatDraftBook(store: store)
        await book.load()
        book.update("first", for: scope(), label: "Mac A · 项目 A · Claude")
        book.update("second", for: scope())
        book.update(" \n最终\t正文 \n", for: scope())
        book.update("另一台电脑", for: scope(machine: "b"))
        try await book.flush()
        #if os(macOS)
        let attributes = try FileManager.default.attributesOfItem(atPath: dir.appendingPathComponent("drafts.json").path)
        // 0600 = owner read(0400) + write(0200), no group/other access.
        XCTAssertEqual(attributes[.posixPermissions] as? Int, 0o600, "macOS原生测试草稿仅当前用户可读写")
        #endif
        let reopened = ChatDraftBook(store: ChatDraftStore(url: dir.appendingPathComponent("drafts.json")))
        await reopened.load()
        XCTAssertEqual(reopened.draft(for: scope())?.text, " \n最终\t正文 \n", "快速写入不能倒序覆盖或trim原文")
        XCTAssertEqual(reopened.draft(for: scope())?.label, "Mac A · 项目 A · Claude", "离线时也必须能识别旧草稿来自哪里")
        XCTAssertEqual(reopened.draft(for: scope(machine: "b"))?.text, "另一台电脑", "相同局部ID不能串电脑")
        XCTAssertNil(reopened.draft(for: scope(project: "project-b")), "项目不同不能带入正文")
        XCTAssertNil(reopened.draft(for: scope(route: "route-b")), "重开的项目窗口不能自动复用草稿")
        XCTAssertNil(reopened.draft(for: scope(node: 8)), "8=7+1，不同卡片不能带入正文")
        XCTAssertNil(reopened.draft(for: scope(run: "run-b")), "运行不同不能带入正文")
        XCTAssertNil(reopened.draft(for: scope(session: "session-b")), "会话不同不能带入正文")
        XCTAssertNil(reopened.draft(for: scope(run: nil, session: nil)), "空卡与已启动运行不能混淆")
    }

    /// 输入控件同值回写不能阻止已发送草稿被准确成功回执清空。
    /// 数据构造：原卡7/run-a/session-a，一条“继续”；发送后控件重复写回相同原文，另一电脑草稿作对照。
    /// 执行过程：保存并绑定草稿 → 控件同值回写 → 收到成功回执 → 重读磁盘。
    /// 预期结果：同值回写不生成新编辑；本条已发送草稿消失，另一电脑的未发送原文保留。
    func testSuccessfulReceiptClearsDraftAfterUnchangedInputCallback() async throws {
        let dir = try directory(); defer { try? FileManager.default.removeItem(at: dir) }
        let url = dir.appendingPathComponent("drafts.json")
        let book = ChatDraftBook(store: ChatDraftStore(url: url)); await book.load()
        book.update("继续", for: scope())
        book.update("保留另一台电脑的草稿", for: scope(machine: "b"))
        let draft = try XCTUnwrap(book.draft(for: scope()))
        let card = try JSONDecoder().decode(CardSnapshot.self, from: Data(#"{"node_id":7,"profile":"Claude","title":"Agent","mobile_control":true,"state":"idle","run_id":"run-a","session_id":"session-a","turn_id":null,"task_title":null}"#.utf8))
        let command = try PendingMobileCommand(machineID: draft.scope.machineID, deviceID: "phone-a", workspaceID: "project-a", routeID: "route-a", revision: 1, sending: card, text: draft.text, draftID: draft.id, operationID: "message-a")
        book.update("继续", for: scope())
        let receipt = try JSONDecoder().decode(MobileCommandReceipt.self, from: Data(#"{"device_id":"phone-a","operation_id":"message-a","workspace_id":"project-a","route_id":"route-a","state":"succeeded","result":{"node_id":7,"run_id":"run-a","session_id":"session-a","turn_id":"turn-a"},"error":null}"#.utf8))
        try await book.confirm(command, receipt: receipt)
        XCTAssertNil(book.draft(for: scope()), "相同输入回写不能让已发送正文继续占据输入框")
        let disk = try await ChatDraftStore(url: url).load()
        XCTAssertFalse(disk.contains { $0.scope == scope() }, "发送成功后重启也不能恢复已发送输入")
        XCTAssertEqual(book.draft(for: scope(machine: "b"))?.text, "保留另一台电脑的草稿", "其他电脑的未发送草稿不能被清除")
    }

    /// 旧消息回执不能清除后来重写的同文草稿；只有准确成功回执可清除对应草稿版本。
    ///
    /// 数据构造：空卡7，草稿先写“继续”，编辑为“继续检查”再改回“继续”，产生新的编辑版本；另一电脑作对照。
    /// 执行过程：绑定第一版 → 写第二版 → 旧成功/新未知 → 损坏文件 → 成功清除两次失败 → 修复并重试 → 重读。
    /// 预期结果：同文新版本保留，未知结果保留，清除失败不能假成功；准确成功只清其版本，其他电脑不受影响。
    func testReceiptOnlyClearsItsExactDraftRevision() async throws {
        let dir = try directory(); defer { try? FileManager.default.removeItem(at: dir) }
        let book = ChatDraftBook(store: ChatDraftStore(url: dir.appendingPathComponent("drafts.json")))
        await book.load()
        let empty = scope(run: nil, session: nil)
        book.update("继续", for: empty)
        book.update("必须保留", for: scope(machine: "b"))
        let firstID = try XCTUnwrap(book.draft(for: empty)?.id)
        let card = try JSONDecoder().decode(CardSnapshot.self, from: Data(#"{"node_id":7,"profile":"Claude","title":"Agent","mobile_control":true,"state":"idle","run_id":null,"session_id":null,"turn_id":null,"task_title":null}"#.utf8))
        func command(_ draftID: String) throws -> PendingMobileCommand {
            try PendingMobileCommand(machineID: empty.machineID, deviceID: "phone-a", workspaceID: empty.workspaceID, routeID: empty.routeID, revision: 1, sending: card, text: "继续", draftID: draftID, operationID: "message-a")
        }
        let commands = PendingCommandStore(url: dir.appendingPathComponent("commands.json"))
        try await commands.save(command(firstID))
        let savedCommands = try await commands.load()
        XCTAssertEqual(savedCommands.first?.draftID, firstID, "发送记录重读必须保留对应草稿版本")
        let request = try JSONSerialization.jsonObject(with: JSONEncoder().encode(command(firstID).request)) as? [String: Any]
        XCTAssertNil(request?["draftID"], "本地草稿版本不能改变服务器命令协议")
        let success = try JSONDecoder().decode(MobileCommandReceipt.self, from: Data(#"{"device_id":"phone-a","operation_id":"message-a","workspace_id":"project-a","route_id":"route-a","state":"succeeded","result":{"node_id":7,"run_id":"new-run","session_id":"new-session","turn_id":"turn-a"},"error":null}"#.utf8))
        book.update("继续检查", for: empty)
        book.update("继续", for: empty)
        let secondID = try XCTUnwrap(book.draft(for: empty)?.id)
        XCTAssertNotEqual(firstID, secondID, "真正编辑后即使改回原文，也必须与旧发送版本区分")
        try await book.confirm(try XCTUnwrap(savedCommands.first), receipt: success)
        XCTAssertEqual(book.draft(for: empty)?.id, secondID, "迟到成功回执不能清除新草稿")
        let unknown = try JSONDecoder().decode(MobileCommandReceipt.self, from: Data(#"{"device_id":"phone-a","operation_id":"message-a","workspace_id":"project-a","route_id":"route-a","state":"outcome_unknown","result":null,"error":"outcome_unknown"}"#.utf8))
        try await book.confirm(command(secondID), receipt: unknown)
        XCTAssertEqual(book.draft(for: empty)?.id, secondID, "结果未知不能清除输入")
        try await book.flush()
        let url = dir.appendingPathComponent("drafts.json")
        let saved = try Data(contentsOf: url)
        try Data("[".utf8).write(to: url)
        do { try await book.confirm(command(secondID), receipt: success); XCTFail("损坏文件时清除必须失败") } catch {}
        do { try await book.confirm(command(secondID), receipt: success); XCTFail("内存已移除也不能掩盖上一次磁盘清除失败") } catch {}
        XCTAssertEqual(try Data(contentsOf: url), Data("[".utf8), "重复核验不能覆盖损坏文件")
        try saved.write(to: url)
        await book.retry()
        try await book.flush()
        try await book.confirm(command(secondID), receipt: success)
        XCTAssertNil(book.draft(for: empty), "准确成功只清除第二版空卡草稿")
        XCTAssertEqual(book.draft(for: scope(machine: "b"))?.text, "必须保留", "另一台电脑必须保留")
        let disk = try await ChatDraftStore(url: dir.appendingPathComponent("drafts.json")).load()
        XCTAssertFalse(disk.contains { $0.scope == empty }, "成功清除必须落盘，不能重启后复活")
        XCTAssertTrue(disk.contains { $0.scope == scope(machine: "b") && $0.text == "必须保留" }, "另一电脑正文必须仍在磁盘")
    }

    /// 文件损坏和超过草稿上限不能静默覆盖旧正文，失败的新编辑仍保留在内存。
    ///
    /// 数据构造：64KiB=65536字节草稿上限，65537=上限+1；损坏文件只有一个左方括号。
    /// 执行过程：保存合法正文 → 超限编辑/flush失败 → 重读旧正文 → 写损坏文件 → 新客户端加载/编辑。
    /// 预期结果：旧文件未被超限正文替换；未保存编辑仍可复制；损坏库不进入可编辑状态且原始数据不变。
    func testFailedStoragePreservesDiskAndUnsubmittedText() async throws {
        let dir = try directory(); defer { try? FileManager.default.removeItem(at: dir) }
        let url = dir.appendingPathComponent("drafts.json")
        let book = ChatDraftBook(store: ChatDraftStore(url: url)); await book.load()
        book.update("已保存", for: scope()); try await book.flush()
        book.update(String(repeating: "a", count: 65537), for: scope())
        do { try await book.flush(); XCTFail("超限保存必须失败") } catch {}
        XCTAssertEqual(book.draft(for: scope())?.text.utf8.count, 65537, "未保存的新内容不能被截断或丢弃")
        let disk = try await ChatDraftStore(url: url).load()
        XCTAssertEqual(disk.first?.text, "已保存", "失败不能覆盖上一次落盘内容")
        XCTAssertNotNil(book.failure, "界面必须知道草稿尚未保存")
        try Data("[".utf8).write(to: url)
        let broken = ChatDraftBook(store: ChatDraftStore(url: url)); await broken.load()
        broken.update("不得覆盖", for: scope())
        XCTAssertFalse(broken.loaded, "损坏库不能假装已加载")
        XCTAssertNil(broken.draft(for: scope()), "加载失败不能创建覆盖性新草稿")
        XCTAssertEqual(try Data(contentsOf: url), Data("[".utf8), "损坏的原始记录必须保留")
    }

    #if os(iOS)
    /// 真机上的草稿文件必须采用完整数据保护；模拟器无保护属性，明确跳过而不冒充已验证。
    ///
    /// 数据构造：临时文件中一份短草稿，期望NSFileProtectionComplete，不接受UntilFirstAuthentication。
    /// 执行过程：真机保存并flush → 读取文件属性；模拟器直接说明本项需要真机。
    /// 预期结果：真实设备明确返回complete；nil或较弱保护均失败，模拟器结果不作为锁屏保护证据。
    func testDraftFileProtectionOnPhysicalDevice() async throws {
        #if targetEnvironment(simulator)
        throw XCTSkip("iOS模拟器未返回文件保护属性；本项必须在物理iPhone验证")
        #else
        let dir = try directory(); defer { try? FileManager.default.removeItem(at: dir) }
        let url = dir.appendingPathComponent("drafts.json")
        let book = ChatDraftBook(store: ChatDraftStore(url: url)); await book.load()
        book.update("保护此草稿", for: scope()); try await book.flush()
        let attributes = try FileManager.default.attributesOfItem(atPath: url.path)
        let protection = (attributes[.protectionKey] as? FileProtectionType)?.rawValue ?? (attributes[.protectionKey] as? String)
        XCTAssertEqual(protection, FileProtectionType.complete.rawValue, "物理iPhone必须确认完整文件保护")
        XCTAssertNotEqual(protection, FileProtectionType.completeUntilFirstUserAuthentication.rawValue, "不能降低为首次解锁后可读取")
        #endif
    }
    #endif
}
