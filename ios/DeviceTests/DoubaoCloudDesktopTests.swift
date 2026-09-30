import XCTest
@testable import NormaCore
@testable import Norma

@MainActor final class DoubaoCloudDesktopTests: XCTestCase {
    private struct Config: Decodable {
        let origin: String, control: String, passphrase: String
        let machine: PairedMachine
        let workspace: String
        let realCLI: Bool?
    }

    /// 原生发送预生成语音，经真实云端ASR/Chat/工具、Mac PTY/Hook成果后由云端TTS实际播放。
    /// 数据构造：独立Mac/原生身份；本地Tingting合成一句明确派工，640bytes=16kHz×20ms×2。
    /// 执行过程：静音接通→移除现场麦克风回调→仅Sender发送合成PCM→真实回执与PTY→
    ///   替身释放完成门；真实CLI自然完成且暂停汇报至接收回复结束→正式成果feed→云端TTS/原生播放。
    /// 预期结果：没有现场音频上云；一个真实完成成果且未验收，准确操作可恢复；汇报不调用Chat。
    func testCloudTaskAndNativeReport() async throws {
        guard ProcessInfo.processInfo.environment["NORMA_TEST_MODE"] == "doubao-cloud-desktop",
              let path=ProcessInfo.processInfo.environment["NORMA_TEST_QR"] else {
            throw XCTSkip("requires explicitly authorized cloud L3 and isolated desktop")
        }
        let url=URL(fileURLWithPath:path), directory=url.deletingLastPathComponent()
        let config=try JSONDecoder().decode(Config.self,from:Data(contentsOf:url))
        let call=VoiceCall(discussionURL:directory.appendingPathComponent("drafts.json"),
            operationURL:directory.appendingPathComponent("operations.json"),reportURL:directory.appendingPathComponent("reports.json"))
        let preferences=UserDefaults.standard
        let oldOrigin=preferences.string(forKey:"realtime-gateway"), oldProvider=preferences.string(forKey:"realtime-provider")
        let vault=CredentialVault(), credential=try await vault.loadOrCreate(machineID:config.machine.id)
        let auth=directory.appendingPathComponent("phone-auth.json")
        try JSONEncoder().encode(credential).write(to:auth,options:.atomic)
        try FileManager.default.setAttributes([.posixPermissions:0o600],ofItemAtPath:auth.path)
        addTeardownBlock { try await vault.remove(machineID:config.machine.id); try? FileManager.default.removeItem(at:auth) }
        defer {
            call.hangup(); ServiceCredential.remove(for:config.origin)
            preferences.set(oldOrigin,forKey:"realtime-gateway");preferences.set(oldProvider,forKey:"realtime-provider")
        }
        try ServiceCredential.save(config.passphrase,for:config.origin)
        call.gateway=config.origin;call.accessToken=config.passphrase;call.provider = .doubao
        call.muted=true // Sender drops every ambient frame, including the first hardware callback.
        if config.realCLI == true { call.setReportsPaused(true) }
        func wait(_ label: String, _ condition: @MainActor () async throws -> Bool) async throws {
            // 90s = cloud30s + durable dispatch35s + coalescing5s + scheduling margin20s.
            for _ in 0..<900 {
                if try await condition() { return }
                if call.state == .paused || call.state == .failed { break }
                try await Task.sleep(for:.milliseconds(100))
            }
            XCTFail("\(label)：\(call.state) / \(call.status) / \(call.toolsStatus) / \(call.reportStatus ?? "no report error")")
            throw MobileError.timedOut
        }
        func control(_ path: String, post: Bool = false) async throws -> Data {
            var request=URLRequest(url:URL(string:config.control+path)!)
            request.setValue("Bearer "+config.passphrase,forHTTPHeaderField:"Authorization")
            if post { request.httpMethod="POST" }
            let (data,response)=try await URLSession.shared.data(for:request)
            XCTAssertEqual((response as? HTTPURLResponse)?.statusCode,200,"只能读取本次本机fixture")
            return data
        }
        func stats() async throws -> [String:Any] {
            try JSONSerialization.jsonObject(with:await control("/stats")) as! [String:Any]
        }
        try await wait("隔离授权") { (try? await call.mobileClient.authenticate(config.machine)) != nil }
        let snapshot=try await call.mobileClient.snapshot(config.machine,workspaceID:config.workspace)
        let project=try XCTUnwrap(snapshot.projects.first { $0.id == config.workspace },"准确目标项目")
        XCTAssertEqual(project.cardCount,0,"只能使用空测试项目")
        call.connect(machine:config.machine,project:project)
        try await wait("真实Mac工具绑定") { call.toolsStatus.hasPrefix("任务将安排到") }
        call.audio.onFrame=nil // Keep native output; no ambient capture can reach the test Sender.
        _=await call.audio.snapshot() // Drain any callback already running before unmuting the sender.
        let mutedStats=try await stats()
        XCTAssertEqual(mutedStats["audioPackets"] as? Int,0,"静音接通不能上传现场音频")
        let sender=try XCTUnwrap(call.doubaoSender,"使用生产串行发送器")
        sender.setMuted(false)
        let speech=try await control("/audio")
        XCTAssertTrue(speech.count.isMultiple(of:2),"合成语音必须是完整PCM16采样")
        XCTAssertGreaterThan(speech.count,32000,"语音应超过16kHz×2bytes×1秒，不是空帧")
        // Prefix1s allows ASR re-establishment; tail2s exceeds server500ms endpoint silence.
        var pcm=Data(count:32000);pcm.append(speech);pcm.append(Data(count:64000))
        while !pcm.count.isMultiple(of:640) { pcm.append(0) }
        for offset in stride(from:0,to:pcm.count,by:640) {
            sender.append(pcm.subdata(in:offset..<(offset+640)))
            try await Task.sleep(for:.milliseconds(20))
        }
        try await wait("真实云端派工与PTY") { (try await stats()["turns"] as? [[String:Any]])?.count == 1 }
        try await wait("真实派工回执到达云端") {
            (try await stats()["receipts"] as? [[String:Any]])?.contains {
                $0["status"] as? String == "accepted" && $0["task_id"] != nil
            } == true
        }
        let running=try await stats()
        let receipts=try XCTUnwrap(running["receipts"] as? [[String:Any]],"必须有真实Mac回执")
        let accepted=try XCTUnwrap(receipts.first { $0["status"] as? String == "accepted" && $0["task_id"] != nil },"已接收真实任务")
        try await wait("云端接收回复播完") { call.activeResponse.isEmpty }
        let beforeReport=try await stats()["chats"] as? Int
        if config.realCLI == true {
            XCTAssertTrue(call.reportsPaused,"真实任务自然完成前应暂停自动播报，不能伪造完成门")
            call.setReportsPaused(false)
        } else {
            _=try await control("/complete-first",post:true)
        }
        try await wait("云端成果汇报实际播完") { call.reports.queue.entries.first?.playback == .played }
        XCTAssertEqual(call.reports.queue.entries.count,1,"只能生成一份原任务成果")
        XCTAssertTrue(call.reports.queue.entries[0].available,"必须有真实文件差异")
        XCTAssertEqual(call.reports.queue.entries[0].outcome,"completed","不能用accepted代替执行完成")
        XCTAssertFalse(call.reports.queue.entries[0].accepted,"已播不代表用户验收")
        try await wait("豆包操作准确恢复") { call.operationRecords.contains { $0.operation.operationID == accepted["operation_id"] as? String && !$0.unresolved } }
        let final=try await stats()
        XCTAssertEqual(final["chats"] as? Int,beforeReport,"自动成果汇报不得再次调用模型")
        XCTAssertGreaterThan(final["audioPackets"] as? Int ?? 0,0,"合成音频必须真正进入云端ASR")
        let ended=try await call.mobileClient.snapshot(config.machine,workspaceID:config.workspace)
        XCTAssertEqual(ended.projects.first?.cardCount,1,"云端模型不能重复创建卡片")
        call.hangup();await call.closingTask?.value
        try await call.reports.flush()
        try JSONSerialization.data(withJSONObject:final,options:.prettyPrinted).write(to:directory.appendingPathComponent("native-verified.json"))
    }
}
