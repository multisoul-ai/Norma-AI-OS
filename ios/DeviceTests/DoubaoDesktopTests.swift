import XCTest
@testable import NormaCore
@testable import Norma

@MainActor final class DoubaoDesktopTests: XCTestCase {
    private struct Config: Decodable {
        let origin: String, control: String, passphrase: String
        let machine: PairedMachine
        let workspace: String
    }

    /// 实际Node网关→真实Mac App派工→真实Hook成果→原生PCM汇报，禁止用手工成果替代。
    /// 数据构造：独立身份和空Git项目；A写首行后等待，B追加第二行，C排队后取消。
    ///   每次PCM回复24000Hz×0.2s=4800采样；讨论1次+5个工具各2次Chat=11次。
    /// 执行过程：讨论不派工 → A执行时排队B/C → 取消C → 挂断重拨原项目 →
    ///   重建VoiceCall并读取原操作 → 释放A，原PTY/Hook完成A/B → 成果feed/PCM → 查询原任务。
    /// 预期结果：始终一张卡、两个独立成果；C无PTY输入；已播不代表验收。
    func testActualGatewayDesktopTaskAndReport() async throws {
        guard ProcessInfo.processInfo.environment["NORMA_TEST_MODE"] == "doubao-desktop",
              let path=ProcessInfo.processInfo.environment["NORMA_TEST_QR"] else {
            throw XCTSkip("requires isolated real desktop and production Node gateway")
        }
        let url=URL(fileURLWithPath:path), directory=url.deletingLastPathComponent()
        let config=try JSONDecoder().decode(Config.self,from:Data(contentsOf:url))
        func makeCall() -> VoiceCall {
            VoiceCall(discussionURL:directory.appendingPathComponent("drafts.json"),
                operationURL:directory.appendingPathComponent("operations.json"),reportURL:directory.appendingPathComponent("reports.json"))
        }
        var call=makeCall()
        let preferences=UserDefaults.standard
        let oldOrigin=preferences.string(forKey:"realtime-gateway")
        let oldProvider=preferences.string(forKey:"realtime-provider")
        let vault=CredentialVault()
        let credential=try await vault.loadOrCreate(machineID:config.machine.id)
        let auth=directory.appendingPathComponent("phone-auth.json")
        try JSONEncoder().encode(credential).write(to:auth,options:.atomic)
        try FileManager.default.setAttributes([.posixPermissions:0o600],ofItemAtPath:auth.path)
        addTeardownBlock {
            try await vault.remove(machineID:config.machine.id)
            try? FileManager.default.removeItem(at:auth)
        }
        defer {
            call.hangup()
            ServiceCredential.remove(for:config.origin)
            preferences.set(oldOrigin,forKey:"realtime-gateway")
            preferences.set(oldProvider,forKey:"realtime-provider")
        }
        try ServiceCredential.save(config.passphrase,for:config.origin)
        call.gateway=config.origin; call.accessToken=config.passphrase; call.provider = .doubao
        func wait(_ label: String, _ condition: @MainActor () async throws -> Bool) async throws {
            // 60s = CLI startup grace(30s) + artifact capture(10s) + report coalescing(5s) + 15s margin.
            for _ in 0..<600 {
                if try await condition() { return }
                try await Task.sleep(for:.milliseconds(100))
            }
            XCTFail("\(label)：\(call.state) / \(call.toolsStatus) / \(call.reportStatus ?? "no report error")")
            throw MobileError.timedOut
        }
        func control(_ path: String, post: Bool = false) async throws -> [String:Any] {
            var request=URLRequest(url:URL(string:config.control+path)!)
            request.setValue("Bearer "+config.passphrase,forHTTPHeaderField:"Authorization")
            if post { request.httpMethod="POST" }
            let (data,response)=try await URLSession.shared.data(for:request)
            XCTAssertEqual((response as? HTTPURLResponse)?.statusCode,200,"测试网关控制请求必须成功")
            return try JSONSerialization.jsonObject(with:data) as! [String:Any]
        }
        try await wait("隔离授权") { (try? await call.mobileClient.authenticate(config.machine)) != nil }
        let snapshot=try await call.mobileClient.snapshot(config.machine,workspaceID:config.workspace)
        let project=try XCTUnwrap(snapshot.projects.first { $0.id == config.workspace },"必须读取实际Mac项目")
        call.connect(machine:config.machine,project:project)
        try await wait("Mac工具绑定") { call.toolsStatus.hasPrefix("任务将安排到") }
        _ = try await control("/discuss",post:true)
        try await wait("讨论音频播放") { call.subtitle == "可以先讨论，尚未派工。" && call.activeResponse.isEmpty }
        let discussed=try await call.mobileClient.snapshot(config.machine,workspaceID:config.workspace)
        XCTAssertEqual(discussed.projects.first?.cardCount,0,"讨论不能创建Agent卡片")
        _ = try await control("/dispatch",post:true)
        try await wait("派工回执") { (try await control("/stats")["receipts"] as? [[String:Any]])?.count == 1 }
        let receipts=try await control("/stats")["receipts"] as! [[String:Any]]
        XCTAssertEqual(receipts.first?["status"] as? String,"accepted","工具接收不能冒充任务已完成")
        try await wait("A进入真实PTY") { (try await control("/stats")["turns"] as? [[String:Any]])?.count == 1 }
        for (path,count) in [("/queue",2),("/queue-cancel",3),("/cancel",4)] {
            _ = try await control(path,post:true)
            try await wait("\(path)持久回执") {
                (try await control("/stats")["receipts"] as? [[String:Any]])?.count == count && call.activeResponse.isEmpty
            }
        }
        let queued=try await control("/stats")
        let accepted=try XCTUnwrap(queued["receipts"] as? [[String:Any]])
        XCTAssertEqual(accepted[1]["status"] as? String,"accepted","B必须真实排队")
        XCTAssertEqual(accepted[2]["status"] as? String,"accepted","C必须先真实排队")
        XCTAssertEqual(accepted[3]["status"] as? String,"succeeded","取消必须得到执行层成功回执")
        XCTAssertEqual(accepted[3]["task_id"] as? Int,accepted[2]["task_id"] as? Int,"必须取消准确的C任务")
        XCTAssertEqual((queued["turns"] as? [[String:Any]])?.count,1,"A未完成时B/C不得提交到PTY")
        XCTAssertTrue(call.reports.queue.entries.isEmpty,"accepted和取消回执不能生成完成成果")
        try await wait("豆包原操作核对") { call.operationRecords.count == 4 && call.recoveryStatus == nil }
        let originalOperations=call.operationRecords
        XCTAssertTrue(originalOperations[0].operation.callID.hasPrefix("doubao_"),"必须恢复豆包原媒体身份")
        XCTAssertEqual(originalOperations[0].operation.operationID,accepted[0]["operation_id"] as? String,"操作记录必须属于A的原回执")
        XCTAssertEqual(originalOperations[0].label,"任务已接收","A仍在执行，不能从工具回执推断完成")
        XCTAssertFalse(originalOperations[0].unresolved,"A的真实接收回执应完成核对")
        call.hangup(); await call.closingTask?.value
        let hanging=try await call.mobileClient.snapshot(config.machine,workspaceID:config.workspace)
        XCTAssertEqual(hanging.projects.first?.cardCount,1,"挂断必须保留原运行卡片")
        call=makeCall()
        await call.loadOperations()
        XCTAssertEqual(call.operationRecords,originalOperations,"重建通话对象必须从磁盘读回准确原操作")
        call.gateway=config.origin; call.accessToken=config.passphrase; call.provider = .doubao
        call.connect(machine:config.machine,project:project)
        try await wait("重拨原Mac绑定") { call.toolsStatus.hasPrefix("任务将安排到") }
        _ = try await control("/complete-first",post:true)
        try await wait("两个真实成果自动汇报") {
            call.reports.queue.entries.count == 2 && call.reports.queue.entries.allSatisfy { $0.playback == .played }
        }
        XCTAssertEqual(call.reports.queue.entries.count,2,"A/B各一个成果，取消C无成果")
        XCTAssertTrue(call.reports.queue.entries[0].available,"实际文件差异必须可供验收，不能仅记录成果不可用")
        XCTAssertEqual(call.reports.queue.entries[0].outcome,"completed","必须来自真实Provider完成事件")
        XCTAssertFalse(call.reports.queue.entries[0].accepted,"语音已播不能替用户验收")
        XCTAssertTrue(call.reports.queue.entries[1].available,"排队B也必须包含真实差异")
        XCTAssertEqual(call.reports.queue.entries[1].outcome,"completed","B必须有独立Provider完成事件")
        XCTAssertFalse(call.reports.queue.entries[1].accepted,"B也不能因已播自动验收")
        _ = try await control("/tasks",post:true)
        try await wait("完成后查询") { (try await control("/stats")["receipts"] as? [[String:Any]])?.count == 5 }
        let stats=try await control("/stats")
        XCTAssertEqual(stats["chats"] as? Int,11,"讨论1次+5个工具及回执续答各2次，汇报不能调用LLM")
        let finalReceipts=try XCTUnwrap(stats["receipts"] as? [[String:Any]])
        let tasks=try XCTUnwrap(finalReceipts.last?["tasks"] as? [[String:Any]])
        let first=try XCTUnwrap(tasks.first { $0["task_id"] as? Int == accepted[0]["task_id"] as? Int })
        let second=try XCTUnwrap(tasks.first { $0["task_id"] as? Int == accepted[1]["task_id"] as? Int })
        let cancelled=try XCTUnwrap(tasks.first { $0["task_id"] as? Int == accepted[2]["task_id"] as? Int })
        XCTAssertEqual(first["state"] as? String,"succeeded","A完成不能因挂断被取消")
        XCTAssertEqual(second["state"] as? String,"succeeded","B重拨后仍应接续执行")
        XCTAssertEqual(cancelled["state"] as? String,"cancelled","C不能因队列推进重新执行")
        XCTAssertEqual(tasks.count,3,"重拨不能复制任何任务")
        XCTAssertEqual((stats["turns"] as? [[String:Any]])?.count,2,"C不能进入真实PTY")
        // ASR messages = 2 calls × 1 configuration packet + N microphone packets; require N > 0.
        XCTAssertGreaterThan(stats["frames"] as? Int ?? 0,2,"不能把两次ASR配置首包当成真实麦克风上行")
        call.hangup(); await call.closingTask?.value
        let ended=try await call.mobileClient.snapshot(config.machine,workspaceID:config.workspace)
        XCTAssertEqual(ended.projects.first?.cardCount,1,"挂断不能取消任务或复制卡片")
        try await call.reports.flush()
        try JSONSerialization.data(withJSONObject:stats,options:.prettyPrinted).write(to:directory.appendingPathComponent("native-verified.json"))
    }
}
