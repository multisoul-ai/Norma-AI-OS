import XCTest
import SceneKit
import SwiftUI
import Security
import NormaCore
@testable import Norma

@MainActor final class RealtimeCompanionTests: XCTestCase {
    /// 正式 App bundle 包含真实角色和校验通过的动作，背景适配陪伴原型的纯黑。
    /// 数据构造：实际 bundle 的 Milltina.scn 与 companion-motion.json，不使用测试替身。
    /// 执行过程：初始化角色 → 读取场景与动作 → 检查真实骨骼和材质背景。
    /// 预期结果：角色载入无错误、头部骨骼存在、动作可解码，场景不是空替代物。
    func testBundledAvatarAndMotionAreReal() {
        let avatar = AvatarScene()
        XCTAssertNil(avatar.loadError, "迁入的真实模型必须可从正式 bundle 读取")
        XCTAssertNotNil(avatar.scene.rootNode.childNode(withName: "Head", recursively: true), "真实头部骨骼必须存在")
        XCTAssertNotNil(avatar.motion, "动作资源必须满足已有完整性校验")
        XCTAssertEqual(avatar.scene.background.contents as? UIColor, .black, "角色背景必须与陪伴界面的纯黑一致")
        XCTAssertFalse(avatar.scene.rootNode.childNodes.isEmpty, "不能用空场景代替真实角色")
    }

    /// 首次使用默认服务缺少配对时提示连接Mac，不能要求用户填写Key或伪造通话。
    /// 数据构造：空自动授权、无Mac与项目；不调用外部网关。
    /// 执行过程：主动开聊 → 显示配对提示 → 切前后台 → 挂断。
    /// 预期结果：不弹Key设置，没有通话/媒体ID/工具连接，前后台不凭空发起通话。
    func testMissingCredentialDoesNotPretendConnected() {
        let call = VoiceCall()
        call.gateway = "https://soulvoiceai.com/norma/voice"
        call.accessToken = ""
        call.connect(machine: nil, project: nil)
        XCTAssertFalse(call.showSettings, "默认服务不能要求用户填写Key或访问口令")
        XCTAssertTrue(call.status.contains("配对"), "首次使用必须给出可操作的配对提示")
        XCTAssertFalse(call.isCalling, "未创建媒体不能声称正在通话")
        XCTAssertNil(call.callID, "未完成 SDP 不能伪造媒体会话 ID")
        XCTAssertNil(call.realtime, "凭据校验前不能占用麦克风或创建传输")
        call.setForeground(false); call.setForeground(true)
        XCTAssertFalse(call.isCalling, "切换前后台不能自动发起通话")
        call.hangup()
        XCTAssertEqual(call.state, .idle, "结束后应回到空闲状态")
    }

    /// 已配对原生客户端通过真实TLS自动领取授权；不配置Key，不触发麦克风。
    /// 数据构造：Rust回环fixture的第一台Mac、一个控制项目，手机钥匙串生成独立配对凭据。
    /// 执行过程：真实配对并授权 → 同一MainActor轮次connect/hangup保留目标并取消媒体 → 自动领取 → 缓存复用。
    /// 预期结果：授权绑定当前Mac且没有手动口令；默认界面不弹设置，不伪造媒体已接通。
    /// 显式production模式使用已登记Mac身份，继续验证生产HTTP换票及WSS建立/关闭；不上传现场音频。
    func testPairedMacAutomaticallyProvidesVoiceAuthorization() async throws {
        let env=ProcessInfo.processInfo.environment
        guard env["NORMA_TEST_MODE"] == "doubao-loopback", let path=env["NORMA_TEST_QR"] else {
            throw XCTSkip("需要显式Rust TLS fixture")
        }
        struct Target: Decodable { let offer:PairingOffer; let writeA:String }
        struct Config: Decodable { let targets:[Target]; let production:Bool? }
        let config=try JSONDecoder().decode(Config.self,from:Data(contentsOf:URL(fileURLWithPath:path)))
        let fixture=try XCTUnwrap(config.targets.first,"实际TLS目标")
        let machine=PairedMachine(offer:fixture.offer), call=VoiceCall(), vault=CredentialVault()
        if config.production == true {
            let query:[CFString:Any]=[kSecClass:kSecClassGenericPassword,
                kSecAttrService:"com.norma-ai.os.ios.device",kSecAttrAccount:machine.id]
            guard SecItemCopyMatching(query as CFDictionary,nil)==errSecItemNotFound else {
                throw XCTSkip("生产身份验收需要未配对此Mac的隔离模拟器，不能覆盖已有配对")
            }
        }
        addTeardownBlock { try await vault.remove(machineID:machine.id) }
        defer { call.hangup() }
        _=try await call.mobileClient.connect(fixture.offer,deviceName:"Norma Voice Target Test")
        // 50 × 100ms = 5s，覆盖fixture的20ms批准循环及真实TLS往返。
        for _ in 0..<50 {
            if (try? await call.mobileClient.authenticate(machine)) != nil { break }
            try await Task.sleep(for:.milliseconds(100))
        }
        let snapshot=try await call.mobileClient.snapshot(machine,workspaceID:fixture.writeA)
        let project=try XCTUnwrap(snapshot.projects.first { $0.id == fixture.writeA },"控制项目必须存在")
        call.gateway=VoiceAuthorization.serviceOrigin;call.accessToken=""
        call.connect(machine:machine,project:project);call.hangup()
        XCTAssertTrue(call.usesAutomaticAuthorization,"默认网关应该自动配置")
        XCTAssertFalse(call.showSettings,"不能要求用户填写Key")
        let token=try await call.preparedAccessToken()
        let grant=try XCTUnwrap(VoiceAuthorization.cached(token,machineID:machine.id),"真实签名授权应符合缓存协议")
        XCTAssertEqual(grant.machineID,machine.id,"授权不能绑定其他Mac")
        XCTAssertTrue(token.hasPrefix("nva1."),"必须是Mac签名，不能是共享Key")
        XCTAssertFalse(call.isCalling,"取消后领取授权不能重新开聊")
        XCTAssertNil(call.socket,"授权请求不能占用媒体连接")
        call.accessToken=token
        let reused=try await call.preparedAccessToken()
        XCTAssertEqual(reused,token,"未到刷新窗口应复用有效授权")
        if config.production == true {
            let endpoint=try XCTUnwrap(VoiceSessionTicket.endpoint(VoiceAuthorization.serviceOrigin))
            let session=URLSession(configuration:.ephemeral,delegate:VoiceGatewayRedirects(),delegateQueue:nil)
            defer { session.invalidateAndCancel() }
            // 12s matches the production ticket/handshake request budget; no provider key is supplied.
            var request=URLRequest(url:endpoint,timeoutInterval:12)
            request.httpMethod="POST";request.setValue("application/json",forHTTPHeaderField:"Content-Type")
            request.setValue("Bearer \(reused)",forHTTPHeaderField:"Authorization")
            request.httpBody=Data("{\"provider\":\"doubao\"}".utf8)
            let (data,response)=try await session.data(for:request)
            XCTAssertEqual((response as? HTTPURLResponse)?.statusCode,200,"正式网关必须接受原生自动领取的Mac签名")
            let ticket=try VoiceSessionTicket(data:data,endpoint:endpoint,provider:.doubao)
            XCTAssertNotEqual(ticket.token,reused,"公网媒体票据必须与Mac授权分离")
            request=URLRequest(url:ticket.url,timeoutInterval:12)
            request.setValue("Bearer \(ticket.token)",forHTTPHeaderField:"Authorization")
            let socket=session.webSocketTask(with:request)
            // 12s handshake + 12s ASR setup + 6s cloud cleanup = 30s upper bound; no audio capture.
            let deadline=Task { try? await Task.sleep(for:.seconds(30));if !Task.isCancelled { socket.cancel(with:.goingAway,reason:nil) } }
            defer { deadline.cancel();socket.cancel(with:.normalClosure,reason:nil) }
            socket.resume()
            func event() async throws -> [String:Any] {
                let message=try await socket.receive(),data:Data
                switch message { case .string(let text):data=Data(text.utf8);case .data(let bytes):data=bytes;@unknown default:throw URLError(.cannotParseResponse) }
                return try XCTUnwrap(JSONSerialization.jsonObject(with:data) as? [String:Any])
            }
            let started=try await event()
            XCTAssertEqual(started["type"] as? String,"session.created","生产豆包上游必须真正就绪")
            XCTAssertNil(started["error"],"授权失败不能伪装媒体连接成功")
            try await socket.send(.string("{\"type\":\"session.close\"}"))
            var ended=try await event()
            if ended["type"] as? String == "gateway.usage" { ended=try await event() }
            XCTAssertEqual(ended["type"] as? String,"session.closed","免Key连接必须收到生产挂断确认")
            XCTAssertFalse(call.showSettings,"生产鉴权成功后也不能弹Key设置")
            print("PASS: native automatic Mac signature -> production HTTP 200 -> Doubao WSS ready -> close ACK; no provider key or microphone")
        }
    }

    /// Mac 离线时显示最后一次真实概况时间；没有快照或开始新通话不能伪造时间。
    /// 数据构造：1000秒对应固定历史日期；任务工具状态为离线，媒体未接通。
    /// 执行过程：无时间 → 写入真实采样时间 → 切换离线文案 → 缺凭据接通失败 → 新接通立即挂断。
    /// 预期结果：状态与时间分别保留；日期不被状态变化刷新，未取得快照时没有假日期。
    func testOfflineStatusRetainsTheActualSnapshotTime() async throws {
        let call = VoiceCall()
        call.toolsStatus = "Mac 离线，可以继续聊天"
        XCTAssertFalse(call.toolsStatusText.contains("概况更新于"), "无快照不能显示设备当前时间")
        let observed = Date(timeIntervalSince1970: 1000)
        call.snapshotObservedAt = observed
        XCTAssertTrue(call.toolsStatusText.contains("Mac 离线"), "快照日期不能掩盖工具离线状态")
        XCTAssertTrue(call.toolsStatusText.contains(observed.formatted(date: .abbreviated, time: .standard)), "必须显示原采样时间")
        call.toolsStatus = "Mac 连接中断，正在核对绑定"
        XCTAssertEqual(call.snapshotObservedAt, observed, "重试状态不能刷新旧快照日期")
        call.accessToken = ""
        call.connect(machine: nil, project: nil)
        XCTAssertFalse(call.isCalling, "缺少凭据的重试不能伪装新通话")
        call.gateway = "https://voice.example"; call.accessToken = String(repeating: "x", count: 24)
        call.connect(machine: nil, project: nil)
        XCTAssertNil(call.snapshotObservedAt, "新通话不能携带前次通话的项目时间")
        // 同一 MainActor 轮次立即取消，establish 尚未运行，不采集音频、不连接网络。
        call.hangup(); call.snapshotObservedAt = observed; call.accessToken = ""
        guard ProcessInfo.processInfo.environment["NORMA_TEST_MODE"] == "realtime-snapshot-cua" else { return }
        // 与已有 LoadingLayoutTests 相同的原生宿主；只展示合成状态，无网络或配对修改。
        let scene = try XCTUnwrap(UIApplication.shared.connectedScenes.first as? UIWindowScene)
        let window = UIWindow(windowScene: scene)
        var clicked = false
        call.toolsStatus = "Mac 离线，可以继续聊天"
        window.rootViewController = UIHostingController(rootView: CallSettingsView(call: call, manageComputers: { clicked = true }))
        window.makeKeyAndVisible()
        defer { window.isHidden = true; window.rootViewController = nil }
        print("snapshot-cua ready: 点击切换或管理电脑完成本次合成状态验收")
        // 180 × 1s = 3min，留给 Computer Use 查看日期并点击真实按钮；超时不算通过。
        for _ in 0..<180 {
            if clicked { break }
            try await Task.sleep(for: .seconds(1))
        }
        XCTAssertTrue(clicked, "Computer Use 必须实际点击任务执行区域，不能只等超时")
    }
}
