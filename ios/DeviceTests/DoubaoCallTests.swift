import XCTest
import NormaCore
@testable import Norma

@MainActor final class DoubaoCallTests: XCTestCase {
    /// 只有网关明确声明能力且提供豆包媒体身份时才能启用任务与成果通道。
    /// 数据构造：同一会话依次收到旧协议、冒用 rtc_ 身份、合法 doubao_ 身份；无真实网络。
    /// 执行过程：逐次配置媒体能力 → 查看可用能力 → 挂断清理。
    /// 预期结果：前两者保持聊天模式；第三者允许工具和汇报；挂断后不保留能力或票据。
    func testGatewayCapabilitiesRequireDoubaoMediaIdentity() throws {
        let call = VoiceCall(); call.provider = .doubao
        call.configureDoubaoMedia(["type":"session.created"])
        XCTAssertNil(call.callID, "旧网关不能自动获得任务能力")
        XCTAssertFalse(call.doubaoReports, "旧网关不能开始成果汇报")
        call.configureDoubaoMedia(["call_id":"rtc_wrong", "capabilities":["task_tools":true,"reports":true]])
        XCTAssertNil(call.callID, "豆包不能接受其他供应商媒体身份")
        XCTAssertFalse(call.doubaoTools, "非法身份不能暴露任务通道")
        call.configureDoubaoMedia(["call_id":"doubao_media-a", "capabilities":["task_tools":true,"reports":true]])
        XCTAssertEqual(call.callID, "doubao_media-a", "合法身份应绑定本次豆包媒体")
        XCTAssertTrue(call.doubaoTools, "已声明的任务能力应可用")
        XCTAssertTrue(call.doubaoReports, "已声明的汇报能力应可用")
        call.hangup()
        XCTAssertNil(call.callID, "挂断必须清除媒体身份")
        XCTAssertFalse(call.doubaoTools, "挂断不能保留任务能力")
        XCTAssertFalse(call.doubaoReports, "挂断不能保留汇报能力")
    }

    private struct GatewayFixture: Decodable {
        let origin: String
        let targets: [TargetFixture]
    }
    private struct TargetFixture: Decodable, Sendable {
        let offer: PairingOffer
        let writeA: String
        let writeB: String
        let readC: String
    }

    /// 为Computer Use或系统后台UI测试准备两台真实TLS测试Mac及本机语音网关，正式App仍由原生入口主动开聊。
    /// 数据构造：显式ui-cua-requested或ui-native-background标记、空机器存储、两份临时QR；不覆盖真实配对。
    /// 执行过程：保存原偏好 → 正式配对并等授权 → 保存测试电脑和本机语音口令。
    /// 预期结果：两个隔离目标可浏览，生产App没有测试跳过开关，也未提前采集麦克风。
    func testPrepareInteractiveVoiceTargets() async throws {
        let (config, directory) = try interactiveFixture()
        let model = ComputerModel()
        let preferences = UserDefaults.standard
        let existing = try await model.store.load()
        XCTAssertTrue(existing.isEmpty, "Computer Use只能使用空的隔离模拟器")
        guard existing.isEmpty else { throw MobileError.busy }
        let keys = ["realtime-gateway", "realtime-provider", "selected-machine"]
        let previous = Dictionary(uniqueKeysWithValues: keys.compactMap { key in
            preferences.string(forKey: key).map { (key, $0) }
        })
        try JSONEncoder().encode(previous).write(to: directory.appendingPathComponent("previous-defaults.json"), options: .atomic)
        for target in config.targets {
            _ = try await model.client.connect(target.offer, deviceName: "Norma Voice Target Test")
            let machine = PairedMachine(offer: target.offer)
            // 50 × 100ms = 5s，覆盖本机fixture的20ms批准循环及TLS往返。
            for _ in 0..<50 {
                if (try? await model.client.authenticate(machine)) != nil { break }
                try await Task.sleep(for: .milliseconds(100))
            }
            let grant = try await model.client.authenticate(machine)
            XCTAssertFalse(grant.revoked, "正式配对必须得到真实未撤销授权")
            try await model.store.save(machine)
        }
        try ServiceCredential.save("local-doubao-test-passphrase-24chars", for: config.origin)
        preferences.set(config.origin, forKey: "realtime-gateway")
        preferences.set("doubao", forKey: "realtime-provider")
        preferences.set(config.targets.first?.offer.machineID, forKey: "selected-machine")
        preferences.synchronize()
    }

    /// Computer Use结束后只回收本次两台测试Mac和假口令，恢复原语音/选择偏好。
    /// 数据构造：同一fixture保存的完整机器ID和previous-defaults.json；不扫描其他机器或清空钥匙串。
    /// 执行过程：逐项删除匹配测试元数据与凭据 → 恢复偏好 → 读取存储核对。
    /// 预期结果：测试电脑不存在，原偏好恢复；不触碰其他身份。
    func testCleanupInteractiveVoiceTargets() async throws {
        let (config, directory) = try interactiveFixture()
        let previous = try JSONDecoder().decode([String: String].self, from: Data(contentsOf: directory.appendingPathComponent("previous-defaults.json")))
        let model = ComputerModel()
        let preferences = UserDefaults.standard
        for target in config.targets {
            try await model.store.remove(id: target.offer.machineID)
            try await model.vault.remove(machineID: target.offer.machineID)
        }
        ServiceCredential.remove(for: config.origin)
        for key in ["realtime-gateway", "realtime-provider", "selected-machine"] {
            preferences.set(previous[key], forKey: key)
        }
        preferences.synchronize()
        let remaining = try await model.store.load()
        XCTAssertFalse(remaining.contains { machine in config.targets.contains { $0.offer.machineID == machine.id } }, "本次临时配对必须全部回收")
    }

    private func interactiveFixture() throws -> (GatewayFixture, URL) {
        let env = ProcessInfo.processInfo.environment
        guard env["NORMA_TEST_MODE"] == "doubao-loopback", let path = env["NORMA_TEST_QR"] else { throw XCTSkip("仅显式豆包Computer Use") }
        let url = URL(fileURLWithPath: path), directory = url.deletingLastPathComponent()
        guard ["ui-cua-requested", "ui-native-background"].contains(where: {
            FileManager.default.fileExists(atPath: directory.appendingPathComponent($0).path)
        }) else { throw XCTSkip("原生UI验收未请求") }
        return (try JSONDecoder().decode(GatewayFixture.self, from: Data(contentsOf: url)), directory)
    }
    /// 短暂接通后立刻断流不能无限清零重试次数，让指挥台一直停在“正在尝试恢复”。
    /// 数据构造：真实回环网关每次收到首个640bytes/20ms麦克风帧便报告可重试断流。
    ///   最多4次连接=首次+3次重试；等待上限30s=退避(1+2+4)s+4×(3s音频启动+2s关闭)+3s调度余量。
    /// 执行过程：主动接通 → 真实采集触发断流 → 自动重试 → 等待明确失败 → 检查媒体释放。
    /// 预期结果：最终显示可重新接通的失败状态，不能继续恢复、不能遗留媒体或通话意图。
    func testRepeatedShortConnectionsStopRecovering() async throws {
        let env = ProcessInfo.processInfo.environment
        guard env["NORMA_TEST_MODE"] == "doubao-recovery", let path = env["NORMA_TEST_QR"] else {
            throw XCTSkip("需要显式反复断流回环fixture")
        }
        let config = try JSONDecoder().decode(GatewayFixture.self, from: Data(contentsOf: URL(fileURLWithPath: path)))
        let defaults = UserDefaults.standard
        let oldGateway = defaults.object(forKey: "realtime-gateway"), oldProvider = defaults.object(forKey: "realtime-provider")
        let temporary = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let call = VoiceCall(discussionURL: temporary.appendingPathComponent("discussions.json"),
            operationURL: temporary.appendingPathComponent("operations.json"), reportURL: temporary.appendingPathComponent("reports.json"))
        call.gateway = config.origin; call.provider = .doubao; call.accessToken = "local-doubao-test-passphrase-24chars"
        defer {
            call.hangup(); ServiceCredential.remove(for: config.origin)
            defaults.set(oldGateway, forKey: "realtime-gateway"); defaults.set(oldProvider, forKey: "realtime-provider")
            try? FileManager.default.removeItem(at: temporary)
        }
        call.connect(machine: nil, project: nil)
        // 300×100ms=30s；真实状态推进，不修改通话内部状态或重试计数。
        for _ in 0..<300 {
            if call.state == .failed { break }
            try await Task.sleep(for: .milliseconds(100))
        }
        XCTAssertEqual(call.state, .failed, "反复短暂接通应耗尽重试，实际状态：\(call.status)")
        XCTAssertEqual(call.attempts, 4, "必须是首次加三次重试耗尽，不能以其他失败冒充恢复完成")
        XCTAssertEqual(call.status, "网络未恢复，点接通重试", "必须明确提示恢复失败和重新接通入口")
        XCTAssertFalse(call.isCalling, "重试耗尽必须释放通话意图，允许用户重新接通")
        XCTAssertFalse(call.status.contains("正在尝试恢复"), "不能永远停留在恢复提示，即使提示包含重试次数")
        XCTAssertNil(call.socket, "重试耗尽后不能保留付费媒体连接")
        call.hangup(); await call.closingTask?.value
    }

    /// 只有稳定连接才补充重试预算，短暂接通仍应耗尽预算并释放通话。
    /// 数据构造：已有3次重试；5s短连接低于30s稳定窗口，31s稳定连接高于30s窗口。
    ///   30s=2×15s豆包传输/音频就绪预算；通过单调时钟采样时间构造状态，不实际等待。
    /// 执行过程：5s连接后断流 → 失败 → 手动新接通 → 31s连接后断流 → 新预算第1次重试。
    /// 预期结果：短连接不能清零；稳定连接允许恢复并显示1/3，不直接结束通话。
    func testStableReadinessResetsRetryBudget() {
        let call = VoiceCall()
        call.gateway = "http://127.0.0.1:1"; call.accessToken = "local-doubao-test-passphrase-24chars"
        defer { call.hangup() }
        // 同一个MainActor轮次构造状态并结束，连接Task尚未运行，不触发网络或麦克风。
        call.connect(machine: nil, project: nil)
        call.state = .connected; call.attempts = 3
        call.connectedAt = ProcessInfo.processInfo.systemUptime - 5
        call.lostConnection()
        XCTAssertEqual(call.state, .failed, "5s短暂恢复不能重新取得三次重试")
        XCTAssertFalse(call.isCalling, "短连接耗尽重试后应允许重新接通")
        call.connect(machine: nil, project: nil)
        call.state = .connected; call.attempts = 3
        call.connectedAt = ProcessInfo.processInfo.systemUptime - 31
        call.lostConnection()
        XCTAssertEqual(call.state, .reconnecting, "稳定连接后的新故障应有独立恢复预算")
        XCTAssertEqual(call.attempts, 1, "稳定连接后的恢复应从第一次重试开始")
        XCTAssertTrue(call.status.contains("1/3"), "指挥台应显示真实恢复进度")
        XCTAssertTrue(call.isCalling, "首次重试应保留用户的通话意图")
        XCTAssertNil(call.socket, "旧媒体应在开始退避前释放")
    }

    /// 真实模拟器音频经本机网关双向传输，插话同步播放位置，静音/后台/立即重拨保持正确生命周期。
    /// 数据构造：网关由 Rust L3 runner 在独立回环端口启动；上行 640bytes/20ms，首回复 2s，下一回复 100ms。
    /// 执行过程：接通并实际采集 → 播放中打断 → 等新回复真实播放完 → 两Mac项目选择/拒绝只读 → 静音重连/解除 → 后台保持 → 挂断立即重拨。
    /// 预期结果：收到有效上行和真实播放完成、旧字幕不复活；静音不继续上传；连接不重叠，无伪造 Mac 就绪。
    func testNativeDuplexGatewayLifecycle() async throws {
        let env = ProcessInfo.processInfo.environment
        guard env["NORMA_TEST_MODE"] == "doubao-loopback", let path = env["NORMA_TEST_QR"] else {
            throw XCTSkip("显式豆包回环网关尚未迁移到独立测试入口")
        }
        let config = try JSONDecoder().decode(GatewayFixture.self, from: Data(contentsOf: URL(fileURLWithPath: path)))
        let origin = config.origin
        let defaults = UserDefaults.standard
        let oldGateway = defaults.object(forKey: "realtime-gateway"), oldProvider = defaults.object(forKey: "realtime-provider")
        let temporary = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: temporary) }
        let call = VoiceCall(discussionURL: temporary.appendingPathComponent("discussions.json"),
            operationURL: temporary.appendingPathComponent("operations.json"),
            reportURL: temporary.appendingPathComponent("reports.json"))
        call.gateway = origin; call.provider = .doubao
        call.accessToken = "local-doubao-test-passphrase-24chars"
        defer {
            call.hangup(); ServiceCredential.remove(for: origin)
            defaults.set(oldGateway, forKey: "realtime-gateway"); defaults.set(oldProvider, forKey: "realtime-provider")
        }
        func stats() async throws -> [String: Double] {
            let (data, response) = try await URLSession.shared.data(from: URL(string: origin + "/stats")!)
            XCTAssertEqual((response as? HTTPURLResponse)?.statusCode, 200, "必须读取本机网关真实计数")
            return try JSONDecoder().decode([String: Double].self, from: data)
        }
        func wait(_ condition: @MainActor () async throws -> Bool) async throws {
            // 150 × 100ms = 15s，覆盖麦克风首次启动；这只是失败截止，不是性能指标。
            for _ in 0..<150 {
                if try await condition() { return }
                try await Task.sleep(for: .milliseconds(100))
            }
            throw NSError(domain: "DoubaoL3", code: 1, userInfo: [NSLocalizedDescriptionKey: "等待真实音频链路超时：\(call.state) \(call.status)"])
        }
        call.connect(machine: nil, project: nil)
        try await wait { call.state == .connected && call.subtitle == "本地豆包回复" }
        try await wait { await call.audio.snapshot().playedMS > 0 }
        call.interrupt()
        try await wait { try await stats()["playback_done", default: 0] > 0 }
        let afterReply = try await stats()
        XCTAssertGreaterThan(afterReply["frames", default: 0], 0, "必须有真正麦克风上行")
        XCTAssertEqual(afterReply["invalid_frames"], 0, "上行必须满足20ms/16kHz PCM帧格式")
        XCTAssertEqual(afterReply["interrupts"], 1, "一次主动插话只报告一次截断")
        XCTAssertGreaterThan(afterReply["played_ms", default: 0], 0, "必须报告实际已播采样位置")
        XCTAssertLessThan(afterReply["played_ms", default: 2000], 2000, "插话时未播放的后缀不能计入上下文")
        XCTAssertEqual(call.subtitle, "插话后的新回复", "迟到旧字幕不能污染下一回复")
        XCTAssertNil(call.callID, "豆包不能制造 OpenAI sideband ID")
        XCTAssertNil(call.macBindingTask, "聊天接通不代表 Mac 派工工具接通")
        let vault = CredentialVault(), targets = config.targets
        addTeardownBlock {
            for target in targets { try await vault.remove(machineID: target.offer.machineID) }
        }
        for target in targets {
            _ = try await call.mobileClient.connect(target.offer, deviceName: "Norma Voice Target Test")
            try await wait { (try? await call.mobileClient.authenticate(PairedMachine(offer: target.offer))) != nil }
        }
        let first = try XCTUnwrap(targets.first, "fixture必须提供第一台Mac")
        let second = try XCTUnwrap(targets.last, "fixture必须提供第二台Mac")
        let firstMachine = PairedMachine(offer: first.offer), secondMachine = PairedMachine(offer: second.offer)
        let media = call.socket
        await call.switchTarget(machine: firstMachine, workspaceID: first.writeA)
        XCTAssertEqual(call.target?.project.id, first.writeA, "明确选择后才绑定第一项目")
        try await wait { call.operationRecords.contains { $0.operation.operationID == "rt-pending-A" } }
        let pending = try XCTUnwrap(call.operationRecords.first { $0.operation.operationID == "rt-pending-A" })
        XCTAssertTrue(pending.unresolved, "Mac只有历史占位，不能伪造成功")
        XCTAssertEqual(pending.machineID, firstMachine.id, "手机必须保留原电脑身份")
        XCTAssertNil(pending.operation.result, "查询不存在的命令不能自动派工")
        let restored = try await RealtimeOperationStore(url: temporary.appendingPathComponent("operations.json")).load()
        XCTAssertTrue(restored.contains { $0.id == pending.id && $0.unresolved }, "未确认操作必须实际落盘")
        let model = ComputerModel()
        model.selectedID = secondMachine.id
        XCTAssertEqual(call.target?.machine.id, firstMachine.id, "浏览另一电脑不能自动改变通话目标")
        await call.switchTarget(machine: firstMachine, workspaceID: first.readC)
        XCTAssertEqual(call.target?.project.id, first.writeA, "只读目标被拒绝时原绑定必须保留")
        XCTAssertNotNil(call.targetError, "不能把只读拒绝静默显示为成功")
        await call.switchTarget(machine: firstMachine, workspaceID: first.writeB)
        XCTAssertEqual(call.target?.project.id, first.writeB, "同一电脑内可明确切换获控制权限的项目")
        await call.switchTarget(machine: secondMachine, workspaceID: second.writeA)
        XCTAssertEqual(call.target?.machine.id, secondMachine.id, "只有明确操作才能切到第二台Mac")
        XCTAssertNotEqual(call.target?.project.id, first.writeB, "换电脑不能保留第一台电脑项目身份")
        XCTAssertTrue(call.socket === media, "切换执行目标不能重建语音媒体")
        XCTAssertNil(call.macBindingTask, "豆包选中目标不能伪造任务工具接线")
        call.toggleMute()
        try await wait { try await stats()["mute"] == 1 }
        // 16 × 20ms 发送缓冲 + 80ms 调度余量 = 400ms，再观察同样长度证明持续静音。
        try await Task.sleep(for: .milliseconds(400))
        let quiet = try await stats()
        try await Task.sleep(for: .milliseconds(400))
        let stillQuiet = try await stats()
        XCTAssertEqual(stillQuiet["frames"], quiet["frames"], "静音后不能继续上传麦克风数据")
        call.lostConnection()
        try await wait {
            let snapshot = try await stats()
            return call.state == .connected && snapshot["sessions"] == 2
        }
        let resumedMuted = try await stats()
        XCTAssertTrue(call.muted, "网络恢复不能擅自解除静音")
        XCTAssertEqual(resumedMuted["frames"], quiet["frames"], "静音重连的首次硬件回调也不能泄露音频")
        call.toggleMute()
        try await wait { try await stats()["frames", default: 0] > quiet["frames", default: 0] }
        call.setForeground(false)
        let background = try await stats()
        try await wait { try await stats()["frames", default: 0] > background["frames", default: 0] }
        XCTAssertEqual(call.state, .connected, "暂停角色渲染不能挂断主动开始的通话")
        call.setForeground(true)
        call.hangup(); call.connect(machine: nil, project: nil)
        try await wait { call.state == .connected }
        call.hangup(); await call.closingTask?.value
        let ended = try await stats()
        XCTAssertEqual(ended["sessions"], 3, "两次手动接通加一次受控重连，共三个会话")
        XCTAssertEqual(ended["closed"], 3, "三个会话都必须释放服务器连接")
        XCTAssertEqual(ended["overlap"], 0, "立即重拨必须等待旧连接关闭确认")
        XCTAssertEqual(ended["active"], 0, "最终挂断不能留下活动连接")
        XCTAssertFalse(call.isCalling, "挂断必须清除通话意图")
    }

    /// 插话清除当前回复，迟到的字幕、音频和角色动作不能复活已取消回复。
    /// 数据构造：response-one 为当前回复；PCM 960bytes = 24k × 2bytes × 20ms。
    /// 执行过程：接收字幕 → 收到用户说话事件 → 注入旧字幕/音频 → 接收下一轮 ASR。
    /// 预期结果：旧回复保持清除；新用户字幕正确拼接；不伪造 GPT 媒体 ID 或 Mac 工具就绪。
    func testInterruptionRejectsLateResponse() async throws {
        let call = VoiceCall(); call.provider = .doubao; call.state = .connected
        try await call.handleDoubao(["type": "response.output_text.delta", "response_id": "response-one", "delta": "尚未说完"], current: call.generation)
        XCTAssertEqual(call.subtitle, "尚未说完", "当前回复应显示真实流式字幕")
        try await call.handleDoubao(["type": "conversation.item.input_audio_transcription.started"], current: call.generation)
        try await call.handleDoubao(["type": "response.output_text.delta", "response_id": "response-one", "delta": "迟到文字"], current: call.generation)
        try await call.handleDoubao(["type": "response.output_audio.delta", "response_id": "response-one", "delta": Data(count: 960).base64EncodedString()], current: call.generation)
        XCTAssertTrue(call.subtitle.isEmpty, "插话后旧字幕不能重新出现")
        XCTAssertTrue(call.activeResponse.isEmpty, "旧音频不能恢复已取消回复")
        XCTAssertNotEqual(call.phase, .speaking, "用户讲话期间旧音频不能进入播放状态")
        try await call.handleDoubao(["type": "conversation.item.input_audio_transcription.delta", "transcript": "新的"], current: call.generation)
        try await call.handleDoubao(["type": "conversation.item.input_audio_transcription.delta", "delta": "问题"], current: call.generation)
        XCTAssertEqual(call.userSubtitle, "新的问题", "应同时支持完整ASR结果与增量协议")
        XCTAssertNil(call.callID, "豆包不能伪造 GPT 的媒体会话标识")
        XCTAssertNil(call.macBindingTask, "豆包聊天不能假装接入 OpenAI sideband")
        call.hangup()
    }
}
