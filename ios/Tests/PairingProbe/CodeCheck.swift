import Foundation
import NormaCore

enum CodeCheck {
    static func run(_ file: URL) async throws {
        struct Input: Decodable { let address: String; let code: String; let machine: String }
        let input = try JSONDecoder().decode(Input.self, from: Data(contentsOf: file))
        let directory = file.deletingLastPathComponent()
        let wrong = (input.code.first == "0" ? "1" : "0") + input.code.dropFirst()
        do {
            _ = try await PairingCode.verify(address: input.address, code: wrong)
            throw MobileError.invalidResponse
        } catch MobileError.invalidPairingCode { }
        try Data().write(to: directory.appendingPathComponent("wrong-code-rejected"))
        let offer = try await PairingCode.verify(address: input.address, code: input.code)
        guard offer.machineID == input.machine else { throw MobileError.wrongComputer }
        let third = try await PairingCode.verify(address: input.address, code: input.code)
        guard third.machineID == offer.machineID else { throw MobileError.wrongComputer }
        do {
            _ = try await PairingCode.verify(address: input.address, code: input.code)
            throw MobileError.invalidResponse
        } catch MobileError.invalidPairingCode { }
        let machine = PairedMachine(offer: offer)
        let vault = CredentialVault(service: "com.norma.code-probe." + UUID().uuidString)
        let client = MobileClient(vault: vault)
        do {
            guard case .pending = try await client.connect(offer, deviceName: "Code probe") else { throw MobileError.invalidResponse }
            // 30s allows the local Rust fixture to approve; it does not extend the desktop's 120s TTL.
            let deadline = ContinuousClock.now.advanced(by: .seconds(30))
            var authorized = false
            while ContinuousClock.now < deadline {
                do {
                    let grant = try await client.authenticate(machine)
                    guard grant.projects.count == 1, grant.projects[0].access == "read" else { throw MobileError.invalidResponse }
                    authorized = true; break
                } catch MobileError.unauthorized { try await Task.sleep(for: .milliseconds(100)) }
            }
            guard authorized else { throw MobileError.timedOut }
            let snapshot = try await client.snapshot(machine)
            guard snapshot.projects.count == 1, snapshot.projects[0].displayName == "Code project" else { throw MobileError.invalidResponse }
            try await subscriptions(client, machine: machine)
            try Data().write(to: directory.appendingPathComponent("authorized"))
            var revoked = false
            while ContinuousClock.now < deadline {
                do { _ = try await client.authenticate(machine) }
                catch MobileError.unauthorized { revoked = true; break }
                try await Task.sleep(for: .milliseconds(100))
            }
            guard revoked else { throw MobileError.timedOut }
            do { _ = try await client.pushSubscription(machine); throw MobileError.invalidResponse }
            catch MobileError.unauthorized { }
            try await vault.remove(machineID: machine.id)
            print("NATIVE_CODE_PAIR_AUTH_REVOKE_OK")
        } catch {
            try await vault.remove(machineID: machine.id)
            throw error
        }
    }

    /// 原生Swift经真实WSS管理只读手机订阅，旧更新不覆盖新设置；不发送真实APNs。
    /// 数据构造：隔离token为32字节hex；版本0→1注册、1→2关闭、2→3重新开启。
    /// 执行过程：读取 → 注册 → 拒绝旧版本关闭 → 关闭 → 拒绝旧版本开启 → 重开并回查。
    /// 预期结果：每次只改变本机订阅，响应与回查一致，配对项目仍为read。
    private static func subscriptions(_ client: MobileClient, machine: PairedMachine) async throws {
        let initial = try await client.pushSubscription(machine)
        guard initial.revision == 0, initial.registration == nil, initial.machineID == machine.id else { throw MobileError.invalidResponse }
        let wrongDevice = try JSONDecoder().decode(PushSubscriptionState.self, from: JSONSerialization.data(withJSONObject: [
            "machine_id": machine.id, "device_id": "other-phone", "revision": 0,
            "registration": NSNull(), "registered_at_ms": NSNull()
        ]))
        do { _ = try await client.updatePushSubscription(machine, expected: wrongDevice, registration: nil); throw MobileError.invalidResponse }
        catch MobileError.invalidCredential { }
        guard try await client.pushSubscription(machine) == initial else { throw MobileError.invalidResponse }
        let registration = APNsRegistration(token: String(repeating: "d", count: 64), environment: .sandbox)
        let enabled = try await client.updatePushSubscription(machine, expected: initial, registration: registration)
        guard enabled.revision == 1, enabled.registration == registration else { throw MobileError.invalidResponse }
        do { _ = try await client.updatePushSubscription(machine, expected: initial, registration: nil); throw MobileError.invalidResponse }
        catch MobileError.pushSubscriptionChanged { }
        let disabled = try await client.updatePushSubscription(machine, expected: enabled, registration: nil)
        guard disabled.revision == 2, disabled.registration == nil, disabled.registeredAtMilliseconds == nil else { throw MobileError.invalidResponse }
        do { _ = try await client.updatePushSubscription(machine, expected: enabled, registration: registration); throw MobileError.invalidResponse }
        catch MobileError.pushSubscriptionChanged { }
        let current = try await client.updatePushSubscription(machine, expected: disabled, registration: registration)
        let read = try await client.pushSubscription(machine)
        guard read == current, current.revision == 3, current.registration == registration else { throw MobileError.invalidResponse }
        let grant = try await client.authenticate(machine)
        guard grant.projects.count == 1, grant.projects[0].access == "read" else { throw MobileError.invalidResponse }
        print("NATIVE_PUSH_REGISTER_DISABLE_REVOKE_READY")
    }
}
