import XCTest
@testable import Norma
@testable import NormaCore

@MainActor final class WorkspaceTerminalLocalTests: XCTestCase {
    /// 场景：显式隔离验收使用固定短草稿，避免UI测试的删除/恢复操作不断累积旧文本。
    /// 数据构造：已配对回环Mac、显式terminalAcceptanceTask开关；默认25字符草稿。
    ///   显式长草稿探针为883字符=25+107×8+2，复现之前验收积累的实际长度；不生成待发送消息。
    /// 执行过程：校验隔离fixture → 仅保存该模拟器该空间草稿 → 从正式store读回。
    /// 预期结果：草稿完整保存且处于讨论模式；没有指定Agent，也不执行任务。
    func testPrepareTerminalReadingDraft() async throws {
        guard let path = ProcessInfo.processInfo.environment["NORMA_WORKSPACE_FIXTURE"], path.hasPrefix("/") else { throw XCTSkip("需要显式隔离fixture") }
        struct Fixture: Decodable { let machineID: String; let workspaceID: String; let terminalAcceptanceTask: Bool?; let terminalAcceptanceLongDraft: Bool? }
        let fixture = try JSONDecoder().decode(Fixture.self, from: Data(contentsOf: URL(fileURLWithPath: path)))
        guard fixture.terminalAcceptanceTask == true else { throw XCTSkip("仅显式验收开关允许准备短草稿") }
        let computers = ComputerModel(); await computers.load()
        let machine = try XCTUnwrap(computers.machines.first { $0.id == fixture.machineID }, "应使用隔离Mac")
        XCTAssertTrue(machine.endpoints.allSatisfy { $0.host == "127.0.0.1" }, "不得修改生产配对空间的草稿")
        let store = WorkspaceChatModel().store
        let old = try await store.draftState(macID: fixture.machineID, workspaceID: fixture.workspaceID)
        print("Isolated terminal fixture previous draft characters: \(old.text.count)")
        let text = "Terminal keeps this draft" + (fixture.terminalAcceptanceLongDraft == true ? String(repeating: " · draft", count: 107) + "xx" : "")
        try await store.saveDraft(.init(text: text), macID: fixture.machineID, workspaceID: fixture.workspaceID)
        let draft = try await store.draftState(macID: fixture.machineID, workspaceID: fixture.workspaceID)
        XCTAssertEqual(draft.text, text, "固定验收草稿必须完整保存")
        XCTAssertEqual(draft.intent, .discuss, "准备草稿不能派工")
        XCTAssertNil(draft.target, "准备草稿不能接管Agent")
        print("Prepared isolated terminal draft characters: \(draft.text.count)")
    }

    /// 场景：真实 Mac 的长终端历史通过只读连接分页，不混入当前屏幕或替代运行。
    /// 数据构造：隔离 Bella 的主屏已有受控 PTY 输出的 180 行日志；页预算 120 行，因此至少需要两页。
    /// 执行过程：从目录读取原卡 → 获取最新页 → 用该页边界读取前一页 → 再取最新页。
    /// 预期结果：旧页在明确边界前结束；中文日志保留；分页不移动 Mac 屏幕或改变列宽。
    func testLiveHistoryPagingPreservesOriginalSession() async throws {
        guard let path = ProcessInfo.processInfo.environment["NORMA_WORKSPACE_FIXTURE"], path.hasPrefix("/") else { throw XCTSkip("需要显式隔离fixture") }
        struct Fixture: Decodable { let machineID: String; let workspaceID: String; let areaID: String; let nodeID: UInt64 }
        let fixture = try JSONDecoder().decode(Fixture.self, from: Data(contentsOf: URL(fileURLWithPath: path)))
        let computers = ComputerModel(); await computers.load()
        let machine = try XCTUnwrap(computers.machines.first { $0.id == fixture.machineID }, "必须使用隔离Mac")
        XCTAssertTrue(machine.endpoints.allSatisfy { $0.host == "127.0.0.1" }, "只读历史验收不能访问生产Mac")
        let chat = DirectWorkspaceChatAPI(machine: machine, client: computers.client)
        let catalog = try await chat.catalog()
        let target = try XCTUnwrap(catalog.workspaces.first { $0.id == fixture.workspaceID }?.agents.first {
            $0.target.areaID == fixture.areaID && $0.target.nodeID == fixture.nodeID
        }?.target, "应选择fixture原卡")
        let client = WorkspaceTerminalClient(machine: machine, mobile: computers.client) { throw MobileError.unavailable }
        let request = WorkspaceTerminalRequest(workspaceID: fixture.workspaceID, target: target)
        let latestCapture = try await client.frame(request)
        let latest = try XCTUnwrap(latestCapture, "最新画面应完整返回")
        XCTAssertGreaterThan(latest.startLine, 0, "真实长历史必须超过120行页预算，不能用短画面冒充分页验收")
        var olderRequest = request; olderRequest.before = latest.startLine; olderRequest.historyVersion = latest.version
        let olderCapture = try await client.frame(olderRequest)
        let older = try XCTUnwrap(olderCapture, "历史页应完整返回")
        XCTAssertEqual(older.startLine + older.rows.count, latest.startLine, "旧页必须恰好结束在最新页之前")
        XCTAssertLessThan(older.startLine, latest.startLine, "分页必须真正前进到更早输出")
        XCTAssertTrue(older.text().contains("实时终端验收"), "历史必须保留真实中文日志")
        XCTAssertFalse(older.text().contains("日志 180"), "旧页不能重复最新页尾部日志")
        let returnedCapture = try await client.frame(request)
        let returned = try XCTUnwrap(returnedCapture, "分页后仍应能读取最新画面")
        XCTAssertEqual(returned.version, latest.version, "手机分页不能改变Mac画面")
        XCTAssertEqual(returned.cols, latest.cols, "分页不能改变Mac列宽")
        await client.close()
    }

    /// 场景：显式隔离验收任务生成真正进入Agent终端历史的长正文，供手机分页验收。
    /// 数据构造：已配对回环Mac的一张Bella卡；180行×一句中文；不调用工具、不修改文件。
    ///   等待120秒=CLI冷启动30秒+180行生成60秒+调度余量30秒；每秒读取正式事件。
    /// 执行过程：校验fixture开关及回环地址 → 选择原卡 → 正式派工 → 等同operation绑定及完成。
    /// 预期结果：原任务有run和session；完成正文包含首尾编号；无其它operation参与绑定。
    func testPrepareLongTerminalHistoryAcceptance() async throws {
        guard let path = ProcessInfo.processInfo.environment["NORMA_WORKSPACE_FIXTURE"], path.hasPrefix("/") else { throw XCTSkip("需要显式隔离fixture") }
        struct Fixture: Decodable { let machineID: String; let workspaceID: String; let areaID: String; let nodeID: UInt64; let terminalAcceptanceTask: Bool? }
        let fixture = try JSONDecoder().decode(Fixture.self, from: Data(contentsOf: URL(fileURLWithPath: path)))
        guard fixture.terminalAcceptanceTask == true else { throw XCTSkip("仅显式验收开关允许准备长输出") }
        let computers = ComputerModel(); await computers.load()
        let machine = try XCTUnwrap(computers.machines.first { $0.id == fixture.machineID }, "必须使用已配对的隔离Mac")
        XCTAssertTrue(machine.endpoints.allSatisfy { $0.host == "127.0.0.1" }, "长输出任务只能发送到回环Mac")
        let api = DirectWorkspaceChatAPI(machine: machine, client: computers.client)
        let catalog = try await api.catalog()
        let member = try XCTUnwrap(catalog.workspaces.first { $0.id == fixture.workspaceID }?.agents.first {
            $0.target.areaID == fixture.areaID && $0.target.nodeID == fixture.nodeID
        }, "必须选择fixture指定的原卡")
        if member.target.runID != nil {
            let viewer = WorkspaceTerminalClient(machine: machine, mobile: computers.client) { throw MobileError.unavailable }
            let current = try await viewer.frame(.init(workspaceID: fixture.workspaceID, target: member.target))
            await viewer.close()
            if current?.text().contains("Hooks need review") == true || current?.text().contains("SessionStart hooks") == true {
                throw XCTSkip("隔离CLI仍等待hooks启动检查；长历史真实验收尚未完成")
            }
        }
        let message = WorkspaceChatMessage(workspaceID: fixture.workspaceID, text: """
            终端历史验收：请直接回复180行纯文本，每行固定格式为「日志 001：实时终端历史验收，中文字符与固定列宽正常。」一直到「日志 180：实时终端历史验收，中文字符与固定列宽正常。」。每个编号独占一行，完整写出所有180行，不省略、不开代码块。不调用工具、不修改文件、不执行命令，只生成这段回复。
            """, intent: .execute, target: member.target)
        // Reuse the same completed preparation after a test-harness retry; never dispatch it twice.
        let existing = try await api.events(workspaceID: fixture.workspaceID, after: 0)
        let previous = existing.last { candidate in
            candidate.kind == "received" && candidate.payload.text == message.text && existing.contains {
                $0.operationID == candidate.operationID && $0.kind == "task_result" &&
                $0.payload.target?.runID == member.target.runID
            }
        }
        let operationID = previous?.operationID ?? message.operationID
        if previous == nil { _ = try await api.submit(message) }
        var completed: WorkspaceChatEvent?
        var bound: WorkspaceAgentTarget?
        for _ in 0..<120 {
            let own = try await api.events(workspaceID: fixture.workspaceID, after: 0).filter { $0.operationID == operationID }
            bound = own.compactMap(\.payload.target).first { $0.runID != nil && $0.sessionID != nil } ?? bound
            completed = own.first { $0.kind == "task_result" }
            if completed != nil { break }
            try await Task.sleep(for: .seconds(1))
        }
        XCTAssertNotNil(bound?.runID, "只有原operation补齐的运行可以作为终端验收目标")
        XCTAssertNotNil(bound?.sessionID, "真实CLI必须提供会话身份")
        let result = try XCTUnwrap(completed, "长正文必须由真实Agent完成")
        XCTAssertTrue(result.payload.text?.contains("日志 001") == true, "首编号必须完整生成")
        XCTAssertTrue(result.payload.text?.contains("日志 180") == true, "尾编号必须完整生成")
        XCTAssertEqual(result.operationID, operationID, "其它任务不能冒充原任务的完成事件")
    }

    /// 场景：LAN不可达时独立终端连接通过真实加密中继读取，关闭重连仍锁定原运行。
    /// 数据构造：隔离Mac、本地Gateway、已配对模拟器；失效LAN端口1触发连接失败；中继1秒预算。
    /// 执行过程：正式LAN查询当前卡 → 用同Mac身份且失效端口读取 → 走加密中继 → 重连 → 篡改run。
    /// 预期结果：完整富网格、固定列宽、1秒刷新；替代run拒绝；群聊LAN入口不被占用。
    func testEncryptedRelayTerminalFallbackAndReconnect() async throws {
        guard let path = ProcessInfo.processInfo.environment["NORMA_WORKSPACE_FIXTURE"], path.hasPrefix("/") else { throw XCTSkip("需要隔离的本地加密中继") }
        struct Fixture: Decodable {
            struct Cloud: Decodable { let deviceID: String; let token: String; enum CodingKeys: String, CodingKey { case deviceID = "device_id", token } }
            let machineID: String; let certificateSHA256: String; let endpoints: [URL]; let workspaceID: String
            let areaID: String; let nodeID: UInt64; let relayPort: UInt16; let cloud: Cloud
        }
        let fixture = try JSONDecoder().decode(Fixture.self, from: Data(contentsOf: URL(fileURLWithPath: path)))
        XCTAssertTrue(fixture.endpoints.allSatisfy { $0.host == "127.0.0.1" }, "测试只能连接隔离Mac")
        func machine(endpoints: [String]) throws -> PairedMachine {
            try JSONDecoder().decode(PairedMachine.self, from: JSONSerialization.data(withJSONObject: ["id":fixture.machineID,
                "name":"终端中继验收 Mac", "certificateSHA256":fixture.certificateSHA256, "endpoints":endpoints]))
        }
        let actual = try machine(endpoints: fixture.endpoints.map(\.absoluteString))
        let computers = ComputerModel()
        let chat = DirectWorkspaceChatAPI(machine: actual, client: computers.client)
        let catalog = try await chat.catalog()
        let target = try XCTUnwrap(catalog.workspaces.first { $0.id == fixture.workspaceID }?.agents.first {
            $0.target.areaID == fixture.areaID && $0.target.nodeID == fixture.nodeID
        }?.target, "应使用目录中的原卡身份")
        XCTAssertNotNil(target.runID, "中继验收需要已经启动的原卡；先运行隔离长输出准备测试")
        let unavailable = try machine(endpoints: ["wss://127.0.0.1:1/mobile"])
        let vault = computers.vault
        let client = WorkspaceTerminalClient(machine: unavailable, mobile: computers.client) {
            try await CollaborationRelayClient(localPort: fixture.relayPort).connect(actual,
                cloud: .init(deviceID: fixture.cloud.deviceID, token: fixture.cloud.token), vault: vault)
        }
        let request = WorkspaceTerminalRequest(workspaceID: fixture.workspaceID, target: target)
        let capture = try await client.frame(request)
        let first = try XCTUnwrap(capture, "加密中继必须返回完整画面")
        XCTAssertEqual(first.rows.first?.count, first.cols, "中继必须保留Mac列宽")
        let delay = await client.refreshDelay
        XCTAssertEqual(delay, .seconds(1), "中继必须遵守每秒刷新和共享请求预算")
        await client.close()
        let secondCapture = try await client.frame(request)
        let second = try XCTUnwrap(secondCapture, "关闭后应能重新建立加密连接")
        XCTAssertEqual(second.cols, first.cols, "重连不能resize Mac终端")
        let replaced = WorkspaceAgentTarget(areaID: target.areaID, definitionID: target.definitionID, nodeID: target.nodeID,
            runID: "replacement-run", sessionID: target.sessionID)
        do { _ = try await client.frame(.init(workspaceID: fixture.workspaceID, target: replaced)); XCTFail("替代run不得读取画面") }
        catch { XCTAssertEqual(error as? MobileError, .sessionChanged, "中继与LAN应共用身份隔离入口") }
        await client.close()
        let events = try await chat.events(workspaceID: fixture.workspaceID, after: 0)
        XCTAssertFalse(events.isEmpty, "终端重连不能占用群聊RPC")
    }

    /// 场景：正式TLS连接读取同一Agent运行，断开再连保持目标；替代会话被拒绝。
    /// 数据构造：显式启用的隔离fixture、已配对模拟器；180行日志+60×0.5秒原位进度。
    ///   绑定等待60秒=冷启动10秒+CLI启动20秒+调度余量30秒；每秒检查一次。
    /// 执行过程：向隔离Agent提交验收任务 → 等原事件绑定 → 读取富网格 → 关闭独立连接 → 重连 → 改run。
    /// 预期结果：固定Mac列宽和完整120行页；重连保持原会话；假run不得返回画面；群聊仍可查询。
    func testLiveTerminalReconnectAndOriginalSessionIsolation() async throws {
        guard let path = ProcessInfo.processInfo.environment["NORMA_WORKSPACE_FIXTURE"], path.hasPrefix("/") else { throw XCTSkip("需要显式隔离fixture") }
        struct Fixture: Decodable { let machineID: String; let workspaceID: String; let areaID: String; let nodeID: UInt64; let terminalAcceptanceTask: Bool? }
        let fixture = try JSONDecoder().decode(Fixture.self, from: Data(contentsOf: URL(fileURLWithPath: path)))
        guard fixture.terminalAcceptanceTask == true else { throw XCTSkip("仅显式开启的验收fixture允许执行测试任务") }
        let computers = ComputerModel(); await computers.load()
        let machine = try XCTUnwrap(computers.machines.first { $0.id == fixture.machineID })
        XCTAssertTrue(machine.endpoints.allSatisfy { $0.host == "127.0.0.1" }, "真实任务只允许发给隔离回环Mac")
        let api = DirectWorkspaceChatAPI(machine: machine, client: computers.client)
        let catalog = try await api.catalog()
        let member = try XCTUnwrap(catalog.workspaces.first { $0.id == fixture.workspaceID }?.agents.first { $0.target.areaID == fixture.areaID && $0.target.nodeID == fixture.nodeID })
        let message = WorkspaceChatMessage(workspaceID: fixture.workspaceID, text: """
            实时终端验收：只在当前隔离验收目录操作。请用 shell 执行 python3 -u 输出180行带编号的中文日志，然后在同一行刷新进度60次，每次sleep(0.5)，使用红/绿ANSI颜色，最后回复「终端验收完成」。不修改项目文件。
            """, intent: .execute, target: member.target)
        let receipt = try await api.submit(message)
        var after = receipt.cursor - 1
        var binding: WorkspaceAgentTarget?
        for _ in 0..<60 {
            let events = try await api.events(workspaceID: fixture.workspaceID, after: after)
            after = events.last?.cursor ?? after
            binding = events.first(where: { $0.operationID == message.operationID && $0.payload.target?.runID != nil })?.payload.target
            if binding != nil { break }
            try await Task.sleep(for: .seconds(1))
        }
        let target = try XCTUnwrap(binding, "原任务必须由正式事件补全run")
        let client = WorkspaceTerminalClient(machine: machine, mobile: computers.client) { throw MobileError.unavailable }
        let request = WorkspaceTerminalRequest(workspaceID: fixture.workspaceID, target: target)
        let firstCapture = try await client.frame(request)
        let first = try XCTUnwrap(firstCapture, "首查询应返回完整网格")
        XCTAssertEqual(first.rows.first?.count, first.cols, "每行应保持Mac完整列宽")
        XCTAssertGreaterThan(first.cols, 1, "终端不能退化为纯文本一列")
        XCTAssertLessThanOrEqual(first.rows.count, 120, "手机页遵守120行预算")
        await client.close()
        let nextCapture = try await client.frame(request)
        let reconnected = try XCTUnwrap(nextCapture, "重连应重新获取完整网格")
        XCTAssertEqual(reconnected.cols, first.cols, "断线重连不能改变Mac尺寸")
        let stale = WorkspaceAgentTarget(areaID: target.areaID, definitionID: target.definitionID, nodeID: target.nodeID,
            runID: "replacement-run", sessionID: target.sessionID)
        do { _ = try await client.frame(.init(workspaceID: fixture.workspaceID, target: stale)); XCTFail("替代run不得返回画面") }
        catch { XCTAssertEqual(error as? MobileError, .sessionChanged, "更换原运行应停止读取，不能自动换卡") }
        await client.close()
        let events = try await api.events(workspaceID: fixture.workspaceID, after: 0)
        XCTAssertFalse(events.isEmpty, "终端独立连接不能阻塞群聊RPC")
    }
}
