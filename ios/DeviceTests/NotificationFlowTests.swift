import XCTest
import UIKit
@testable import NormaCore
@testable import Norma

final class NotificationFlowTests: XCTestCase {
    /// 图片UI验收通过正式草稿入口准备附件，不依赖手动选图或改写生产存储。
    /// 数据构造：已配对的唯一“通知验收Mac”、fixture card7、一份显式photo-fixture.jpg；不接触其他电脑。
    /// 执行过程：核对独立配对与卡片→正式编码→保存纯图片草稿→UI测试重启后发送。
    /// 预期结果：只有card7有一张可读图片，无文本或发送操作；缺少显式fixture则跳过。
    @MainActor func testSeedChatPhoto() async throws {
        let env = ProcessInfo.processInfo.environment
        guard env["NORMA_TEST_MODE"] == "notification-ui", let path = env["NORMA_TEST_QR"] else { throw XCTSkip("仅隔离图片fixture") }
        let imageURL = URL(fileURLWithPath: path).deletingLastPathComponent().appendingPathComponent("photo-fixture.jpg")
        guard FileManager.default.fileExists(atPath: imageURL.path) else { throw XCTSkip("未显式提供测试图") }
        let model = ComputerModel()
        let machines = try await model.store.load()
        let offer = try PairingOffer.decode(Data(contentsOf: URL(fileURLWithPath: path)))
        XCTAssertEqual(machines.count, 1, "只能使用专用验收模拟器")
        let machine = try XCTUnwrap(machines.first)
        XCTAssertEqual(machine.id, PairedMachine(offer: offer).id, "不能使用用户真实配对")
        XCTAssertEqual(machine.name, "通知验收 Mac", "必须是已识别fixture")
        guard machines.count == 1, machine.id == PairedMachine(offer: offer).id, machine.name == "通知验收 Mac" else { return }
        let catalog = try await model.client.snapshot(machine)
        let workspace = try XCTUnwrap(catalog.projects.first?.id)
        let snapshot = try await model.client.snapshot(machine, workspaceID: workspace)
        let project = try XCTUnwrap(snapshot.projects.first)
        let card = try XCTUnwrap(project.cards?.first { $0.id == 7 })
        let scope = ChatDraft.Scope(machineID: machine.id, workspaceID: workspace, routeID: try XCTUnwrap(project.routeID), nodeID: card.id, runID: card.runID, sessionID: card.sessionID)
        await model.drafts.load()
        XCTAssertNil(model.drafts.draft(for: scope), "不能覆盖已有输入")
        guard model.drafts.draft(for: scope) == nil else { return }
        let data = try Data(contentsOf: imageURL)
        let prepared = try await Task.detached { try ChatPhotoEncoding.prepare(data) }.value
        try await model.drafts.addImage(prepared, for: scope, label: "图片验收")
        XCTAssertEqual(model.drafts.draft(for: scope)?.imageAttachments.count, 1, "仅保存指定测试图")
        XCTAssertEqual(model.drafts.draft(for: scope)?.text, "", "必须验证纯图片发送")
    }

    /// 超时隔离fixture只清理已明确识别的测试配对，不依赖已退出的WSS服务。
    ///
    /// 数据构造：显式cleanup模式和验收模拟器导出的唯一电脑元数据；名称与完整身份必须一致。
    /// 执行过程：核对磁盘记录 → 仅移除该机器续登记意图/已送达通知 → 正式forget清理Keychain与元数据。
    /// 预期结果：测试电脑消失，其他数据不清空；身份不符立即停止，不能误删用户电脑。
    @MainActor func testRecoverExpiredNotificationFixture() async throws {
        let env = ProcessInfo.processInfo.environment
        guard env["NORMA_TEST_MODE"] == "notification-cleanup", let path = env["NORMA_TEST_QR"], path.hasPrefix("/") else {
            throw XCTSkip("仅清理显式识别的过期验收fixture")
        }
        let expected = try JSONDecoder().decode([PairedMachine].self, from: Data(contentsOf: URL(fileURLWithPath: path)))
        let model = ComputerModel()
        let saved = try await model.store.load()
        XCTAssertEqual(saved, expected, "当前电脑记录必须与本次待清理fixture完全一致")
        XCTAssertEqual(saved.count, 1, "恢复路径只接受一台明确识别的验收电脑")
        XCTAssertEqual(saved.first?.name, "通知验收 Mac", "不能将用户电脑当作测试残留")
        guard saved == expected, saved.count == 1, let machine = saved.first, machine.name == "通知验收 Mac" else { return }
        let enabled = UserDefaults.standard.stringArray(forKey: "notification-machines") ?? []
        UserDefaults.standard.set(enabled.filter { $0 != machine.id }, forKey: "notification-machines")
        await PushNotifications().clearDelivered(machine.id)
        for command in try await model.commandStore.load() where command.machineID == machine.id {
            try await model.commandStore.remove(id: command.id)
        }
        await model.forget(machine)
        let remaining = try await model.store.load()
        XCTAssertTrue(remaining.isEmpty, "过期测试电脑必须从正式存储删除")
        XCTAssertFalse((UserDefaults.standard.stringArray(forKey: "notification-machines") ?? []).contains(machine.id), "不得续登记已回收fixture")
        XCTAssertNil(model.error, "正式Keychain与元数据清理不能失败")
    }

    /// 已配对电脑的提醒通过真实WSS开启/续token/关闭，关闭后不能因回前台自动复活。
    ///
    /// 数据构造：空模拟器的真实Keychain、Rust隔离只读项目；32字节假APNs token编码为64位hex。
    /// 执行过程：正式配对 → Mac批准 → 真实订阅读取 → 系统回调替身 → 显式开启 → 续登记 → 关闭 → 撤销与忘记。
    /// 预期结果：记录绑定准确机器，续登记保留启用，关闭不再续登记；撤销不影响安全清理；不向Apple发通知。
    /// UI模式仅在独立空模拟器保留配对供Computer Use验收，仍先完成同一组关闭回归。
    /// ui-forget-online标记另验收：开启提醒 → 本地移除与Keychain删除 → 独立回读Mac提醒关闭。
    /// 创建验收标记额外保存1条未决消息（不发送），用于确认它不会阻塞创建；清理仅移除准确测试电脑的记录。
    @MainActor func testPairedNotificationLifecycle() async throws {
        let env = ProcessInfo.processInfo.environment
        guard let path = env["NORMA_TEST_QR"], path.hasPrefix("/"),
              let mode = env["NORMA_TEST_MODE"], ["notifications", "notification-ui"].contains(mode) else {
            throw XCTSkip("需要独立Rust通知fixture和显式模拟器ID")
        }
        let model = ComputerModel()
        let previous = try await model.store.load()
        XCTAssertTrue(previous.isEmpty, "测试只能在空模拟器中建立自己的配对，不能覆盖用户电脑")
        guard previous.isEmpty else { return }
        let offer = try PairingOffer.decode(Data(contentsOf: URL(fileURLWithPath: path)))
        let machine = PairedMachine(offer: offer)
        guard case .pending = try await model.client.connect(offer, deviceName: "Notification Acceptance iPhone") else {
            XCTFail("新测试手机必须等待Mac明确批准"); return
        }
        // 30s = 20ms Rust fixture轮询与原生TLS调度的宽裕上限，不改变生产配对有效期。
        let deadline = ContinuousClock.now.advanced(by: .seconds(30))
        var grant: DeviceGrant?
        while ContinuousClock.now < deadline {
            do { grant = try await model.client.authenticate(machine); break }
            catch MobileError.unauthorized { try await Task.sleep(for: .milliseconds(100)) }
        }
        XCTAssertEqual(grant?.projects.count, 1, "只能读取隔离项目")
        XCTAssertEqual(grant?.projects.first?.access, "read", "通知订阅不能提升控制权限")
        guard grant != nil else { throw MobileError.timedOut }
        try await model.remember(machine)
        let notifications = PushNotifications()
        await notifications.load([machine], client: model.client)
        XCTAssertNil(notifications.subscriptions[machine.id]?.registration, "配对不能自动订阅通知")
        XCTAssertNil(notifications.errors[machine.id], "已配对机器的订阅读取应成功")
        notifications.authorization = .authorized // Test substitute, not evidence of real system permission.
        notifications.application(UIApplication.shared, didRegisterForRemoteNotificationsWithDeviceToken: Data(repeating: 0xab, count: 32))
        let first = try XCTUnwrap(notifications.registration, "合法系统回调应给出可登记token")
        let enabled = await notifications.update(machine, to: first, client: model.client)
        XCTAssertTrue(enabled, "显式开启必须收到Mac确认")
        XCTAssertEqual(notifications.subscriptions[machine.id]?.registration, first, "状态应显示已确认的token")
        XCTAssertTrue((UserDefaults.standard.stringArray(forKey: "notification-machines") ?? []).contains(machine.id), "开启意图应绑定当前电脑")
        notifications.application(UIApplication.shared, didRegisterForRemoteNotificationsWithDeviceToken: Data(repeating: 0xcd, count: 32))
        let refreshed = try XCTUnwrap(notifications.registration, "新token")
        await notifications.synchronize([machine], client: model.client)
        let renewed = try await model.client.pushSubscription(machine)
        XCTAssertEqual(renewed.registration, refreshed, "已开启电脑必须收到新token")
        XCTAssertNotEqual(renewed.registration, first, "旧token不能继续显示为有效登记")
        let disabled = await notifications.update(machine, to: nil, client: model.client)
        XCTAssertTrue(disabled, "关闭必须经Mac确认")
        await notifications.synchronize([machine], client: model.client)
        let stopped = try await model.client.pushSubscription(machine)
        XCTAssertNil(stopped.registration, "关闭后回前台不能自动恢复订阅")
        XCTAssertFalse((UserDefaults.standard.stringArray(forKey: "notification-machines") ?? []).contains(machine.id), "停用后应删除本机自动续登记意图")
        let directory = URL(fileURLWithPath: path).deletingLastPathComponent()
        if mode == "notification-ui" {
            if FileManager.default.fileExists(atPath: directory.appendingPathComponent("ui-forget-online").path) {
                // Verify online best-effort cleanup after real Keychain deletion, using only a
                // test-local credential copy for the independent server readback.
                let savedCredential = try await model.vault.load(machineID: machine.id)
                let credential = try XCTUnwrap(savedCredential, "在线验收必须使用真实配对凭据")
                let reenabled = await notifications.update(machine, to: refreshed, client: model.client)
                XCTAssertTrue(reenabled, "在线移除前须有真实开启的提醒")
                await model.forget(machine, notifications: notifications)
                let storedCredential = try await model.vault.load(machineID: machine.id)
                XCTAssertNil(storedCredential, "本地移除必须先删除Keychain，不能等远端处理")
                XCTAssertFalse(model.machines.contains { $0.id == machine.id }, "在线电脑也应立即从列表移除")
                // 10s = 5s单次请求上限 + 5s独立回读/调度余量，不是本地删除预算。
                let cleanupDeadline = ContinuousClock.now.advanced(by: .seconds(10))
                var remaining: PushSubscriptionState?
                repeat {
                    remaining = try await model.client.exchange(machine, action: ["type": "push_subscription",
                        "device_id": credential.deviceID, "token": credential.token]).result?.subscription
                    if remaining?.registration == nil { break }
                    try await Task.sleep(for: .milliseconds(100))
                } while ContinuousClock.now < cleanupDeadline
                XCTAssertEqual(remaining?.machineID, machine.id, "需由准确Mac回读清理结果")
                XCTAssertNil(remaining?.registration, "Keychain删除后，后台仍应关闭在线Mac的提醒")
                XCTAssertNil(notifications.subscriptions[machine.id], "移除后不得恢复旧订阅状态")
                XCTAssertFalse((UserDefaults.standard.stringArray(forKey: "notification-machines") ?? []).contains(machine.id), "不得继续自动续登记")
                try Data().write(to: directory.appendingPathComponent("ui-ready"), options: .atomic)
                try Data().write(to: directory.appendingPathComponent("ui-finished"), options: .atomic)
                print("NATIVE_NOTIFICATION_UI_READY")
                print("NATIVE_FORGET_ONLINE_OK")
                return
            }
            if FileManager.default.fileExists(atPath: directory.appendingPathComponent("ui-card-creation").path) {
                let workspace = try XCTUnwrap(grant?.projects.first?.workspaceID)
                let snapshot = try await model.client.snapshot(machine, workspaceID: workspace)
                let project = try XCTUnwrap(snapshot.projects.first)
                let card = try XCTUnwrap(project.cards?.first { $0.id == 1 })
                let command = try await model.client.prepareMessage(machine, workspaceID: project.id,
                    routeID: try XCTUnwrap(project.routeID), revision: snapshot.grant.revision, card: card,
                    text: "未确认消息验收")
                // Only save an unrelated pending message; never submit it or launch a Provider.
                try await model.commandStore.save(command)
            }
            try Data().write(to: directory.appendingPathComponent("ui-ready"), options: .atomic)
            print("NATIVE_NOTIFICATION_UI_READY")
            return
        }
        try Data().write(to: directory.appendingPathComponent("native-done"), options: .atomic)
        var revoked = false
        while ContinuousClock.now < deadline {
            do { _ = try await model.client.authenticate(machine) }
            catch MobileError.unauthorized { revoked = true; break }
            try await Task.sleep(for: .milliseconds(100))
        }
        XCTAssertTrue(revoked, "Mac必须真正撤销授权")
        let canForget = await notifications.update(machine, to: nil, client: model.client)
        XCTAssertTrue(canForget, "可信Mac返回无授权后允许清理电脑")
        XCTAssertNil(notifications.subscriptions[machine.id], "撤销后不能显示旧订阅成功状态")
        await model.forget(machine)
        let remaining = try await model.store.load()
        XCTAssertTrue(remaining.isEmpty, "测试结束不得残留电脑或继续自动连接")
        XCTAssertNil(model.error, "删除自己的配对记录应成功")
        print("NATIVE_NOTIFICATION_LIFECYCLE_OK")
    }
}


extension NotificationFlowTests {
    /// 离线移除使用两台隔离电脑与真实Keychain，不需要可连接的Mac。
    ///
    /// 数据构造：仅显式forget-ui-setup模式；两台验收机器使用不同64位hex身份，回环端口1无服务。
    /// 执行过程：确认列表为空 → 创建两份Keychain凭据和电脑元数据 → 保存两台提醒续登记意图。
    /// 预期结果：只创建验收A/B且未发起网络请求；拒绝覆盖任何已有配对。
    @MainActor func testPrepareOfflineForgetUI() async throws {
        guard ProcessInfo.processInfo.environment["NORMA_TEST_MODE"] == "forget-ui-setup" else {
            throw XCTSkip("仅用于明确指定的空验收模拟器")
        }
        let model = ComputerModel()
        let saved = try await model.store.load()
        XCTAssertTrue(saved.isEmpty, "不得覆盖已有电脑配对")
        guard saved.isEmpty else { return }
        for (hex, name) in [("a", "离线验收 Mac A"), ("b", "离线验收 Mac B")] {
            let identity = String(repeating: hex, count: 64)
            let data = try JSONSerialization.data(withJSONObject: ["version": 1, "machine_id": "mac-" + identity,
                "machine_name": name, "certificate_sha256": identity, "offer_id": identity, "secret": identity,
                "endpoints": ["wss://127.0.0.1:1/mobile"], "expires_in_seconds": 120])
            let machine = PairedMachine(offer: try PairingOffer.decode(data))
            _ = try await model.vault.loadOrCreate(machineID: machine.id)
            try await model.remember(machine)
        }
        UserDefaults.standard.set(model.machines.map(\.id), forKey: "notification-machines")
        UserDefaults.standard.set(model.machines[0].id, forKey: "selected-machine")
        XCTAssertEqual(model.machines.map(\.name), ["离线验收 Mac A", "离线验收 Mac B"], "应仅保存两台明确的验收电脑")
        XCTAssertNil(model.grant, "离线数据不能伪造成功授权")
    }

    /// UI移除后持久化、Keychain与通知续登记必须同时清理。
    ///
    /// 数据构造：显式forget-ui-check模式；A/B身份与准备步骤一致，不访问其他机器。
    /// 执行过程：UI已移除两台电脑 → 重读磁盘、Keychain和UserDefaults。
    /// 预期结果：A/B凭据与电脑记录均不存在，续登记关闭；本机存储保持可正常读取。
    @MainActor func testOfflineForgetPersistence() async throws {
        guard ProcessInfo.processInfo.environment["NORMA_TEST_MODE"] == "forget-ui-check" else {
            throw XCTSkip("需要先完成离线移除UI流程")
        }
        let model = ComputerModel()
        let machines = try await model.store.load()
        let a = "mac-" + String(repeating: "a", count: 64), b = "mac-" + String(repeating: "b", count: 64)
        let credentialA = try await model.vault.load(machineID: a)
        let credentialB = try await model.vault.load(machineID: b)
        let enabled = UserDefaults.standard.stringArray(forKey: "notification-machines") ?? []
        XCTAssertEqual(machines.count, 0, "全部移除后仍能读取有效的空电脑列表")
        XCTAssertNil(credentialA, "A的Keychain凭据必须删除")
        XCTAssertNil(credentialB, "B的Keychain凭据必须删除")
        XCTAssertFalse(enabled.contains(a), "A不能继续自动续登记提醒")
        XCTAssertFalse(enabled.contains(b), "B不能继续自动续登记提醒")
    }
}
