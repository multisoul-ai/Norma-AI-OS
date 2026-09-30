import Foundation
import NormaCore
import Darwin

/// Isolated cross-language test client, never linked into the iOS app.
#if !NATIVE_TEST
@main struct Probe {
    static func main() async {
        do {
            guard CommandLine.arguments.count == 3 else { throw MobileError.invalidOffer }
            try await PairingCheck.run(qrURL: URL(fileURLWithPath: CommandLine.arguments[1]), mode: CommandLine.arguments[2])
        }
        catch { print("PROBE_FAILURE \((error as NSError).domain) \((error as NSError).code): \(error.localizedDescription)"); exit(1) }
    }
}
#endif

enum PairingCheck {
    static func run(qrURL: URL, mode: String) async throws {
        if mode == "code" { try await CodeCheck.run(qrURL); return }
        var data = try Data(contentsOf: qrURL)
        let wrongPin = mode == "wrong-pin"
        if wrongPin {
            guard var object = try JSONSerialization.jsonObject(with: data) as? [String: Any] else { throw MobileError.invalidOffer }
            let pin = String(repeating: "0", count: 64)
            object["certificate_sha256"] = pin; object["machine_id"] = "mac-" + pin
            data = try JSONSerialization.data(withJSONObject: object)
        }
        let offer = try PairingOffer.decode(data)
        let vault = CredentialVault(service: "com.norma.probe." + UUID().uuidString)
        let client = MobileClient(vault: vault)
        do {
            let endpointTiming = FileManager.default.fileExists(atPath: qrURL.deletingLastPathComponent().appendingPathComponent("endpoint-latency").path)
            let connectStarted = ContinuousClock.now
            guard case .pending(let pending) = try await client.connect(offer, deviceName: "Swift probe") else {
                throw MobileError.invalidResponse
            }
            if endpointTiming {
                let elapsed = connectStarted.duration(to: .now)
                print("NATIVE_ENDPOINT_CONNECT=\(elapsed)")
                //2s includes local TLS/Keychain/pairing; must not wait for the stalled route's5s timeout.
                guard elapsed < .seconds(2) else { throw MobileError.timedOut }
                print("NATIVE_ENDPOINT_LATENCY_OK")
            }
            guard !wrongPin, pending.verificationCode.count == 6 else { throw MobileError.invalidResponse }
            let machine = PairedMachine(offer: offer)
            let deadline = ContinuousClock.now.advanced(by: .seconds(30))
            var authorized = false
            while ContinuousClock.now < deadline {
                do {
                    let grant = try await client.authenticate(machine)
                    guard grant.projects.count == 1, grant.projects[0].access == (["images", "terminal", "feedback", "reviews", "todos", "native-card", "messages", "interrupts", "queues", "approvals", "bash-edges", "approval-stop", "write-approvals", "external", "events"].contains(mode) ? "control" : "read") else { throw MobileError.invalidResponse }
                    authorized = true; break
                } catch MobileError.unauthorized { try await Task.sleep(for: .milliseconds(100)) }
            }
            guard authorized else { throw MobileError.timedOut }
            let measuring = FileManager.default.fileExists(atPath: qrURL.deletingLastPathComponent().appendingPathComponent("measure-latency").path)
            if mode == "images" {
                try await ImageCheck.run(client: client, machine: machine)
                try await vault.remove(machineID: offer.machineID)
                return
            }
            if mode == "terminal" {
                try await TerminalCheck.run(client: client, machine: machine)
                return
            }
            if mode == "feedback" {
                try await FeedbackCheck.run(client: client, machine: machine, directory: qrURL.deletingLastPathComponent())
                try await vault.remove(machineID: offer.machineID)
                return
            }
            if mode == "archive" {
                try await ArchiveCheck.run(client: client, machine: machine)
                try await vault.remove(machineID: offer.machineID)
                return
            }
            if mode == "reviews" {
                try await ReviewCheck.run(client: client, machine: machine)
                try await vault.remove(machineID: offer.machineID)
                return
            }
            if mode == "todos" {
                try await TodoCheck.run(client: client, machine: machine)
                try await vault.remove(machineID: offer.machineID)
                return
            }
            if mode == "plans" {
                try await PlanCheck.run(client: client, machine: machine, directory: qrURL.deletingLastPathComponent())
                try await vault.remove(machineID: offer.machineID)
                return
            }
            if mode == "chat-events" {
                try await ConversationRecoveryCheck.run(client: client, machine: machine, directory: qrURL.deletingLastPathComponent())
                try await vault.remove(machineID: offer.machineID)
                return
            }
            if mode == "large" {
                try await ConversationTransferCheck.run(client: client, machine: machine, directory: qrURL.deletingLastPathComponent(), measuring: measuring)
                try await vault.remove(machineID: offer.machineID)
                return
            }
            if mode == "events" {
                try await ProjectStreamCheck.run(client: client, machine: machine, directory: qrURL.deletingLastPathComponent())
                try await vault.remove(machineID: offer.machineID)
                return
            }
            if ["approvals", "bash-edges", "approval-stop", "write-approvals", "external"].contains(mode) {
                try await ApprovalCheck.run(client: client, machine: machine, directory: qrURL.deletingLastPathComponent(), external: mode == "external", measuring: measuring, bashEdges: mode == "bash-edges", stopFirst: mode == "approval-stop", writeTool: mode == "write-approvals")
                try await vault.remove(machineID: offer.machineID)
                return
            }
            if mode == "queues" {
                try await QueueCheck.run(client: client, machine: machine)
                try await vault.remove(machineID: offer.machineID)
                return
            }
            if mode == "interrupts" {
                try await InterruptCheck.run(client: client, machine: machine)
                try await vault.remove(machineID: offer.machineID)
                return
            }
            if mode == "messages" || mode == "native-card" {
                try await MessageCheck.run(client: client, machine: machine, measuring: measuring, nativeCard: mode == "native-card")
                try await vault.remove(machineID: offer.machineID)
                return
            }
            if mode == "conversation" {
                try await ConversationCheck.run(client: client, machine: machine)
                try await vault.remove(machineID: offer.machineID)
                return
            }
            #if DEBUG
            if FileManager.default.fileExists(atPath: qrURL.deletingLastPathComponent().appendingPathComponent("watch-lifecycle").path) {
                try await NativeTransferTiming.watchLifecycle(client: client, machine: machine)
            }
            #endif
            let catalog = try await client.snapshot(machine)
            guard catalog.projects.count == 1, catalog.projects[0].displayName == "Swift",
                  catalog.projects[0].cards == nil else { throw MobileError.invalidResponse }
            let detail = try await client.snapshot(machine, workspaceID: catalog.projects[0].id)
            guard let cards = detail.projects[0].cards, cards.count == 2,
                  cards[0].profile == "Codex", cards[1].profile == "Claude",
                  cards[0].state == "idle", cards[0].sessionID == nil else { throw MobileError.invalidResponse }
            do {
                _ = try await client.snapshot(machine, workspaceID: "not-granted")
                throw MobileError.invalidResponse
            } catch MobileError.unauthorized {}
            print("SCOPED_PROJECT_CARD_SNAPSHOT_OK")
            // Re-scan the same computer through its other real listener. The original offer is
            // already consumed: successful reconnect must authenticate, never request approval again.
            guard var moved = try JSONSerialization.jsonObject(with: data) as? [String: Any],
                  offer.endpoints.count >= 2 else { throw MobileError.invalidOffer }
            moved["endpoints"] = [offer.endpoints.last!.absoluteString]
            let rescanned = try PairingOffer.decode(JSONSerialization.data(withJSONObject: moved))
            guard case .authorized(let repeated) = try await client.connect(rescanned, deviceName: "Swift probe"),
                  repeated.deviceID == pending.deviceID, repeated.revision == 1,
                  repeated.projects.count == 1, repeated.projects[0].access == "read" else {
                throw MobileError.invalidResponse
            }
            print("RESCAN_AUTHORIZED_WITHOUT_NEW_PAIRING")
            let creation = try await CommandCheck.run(client: client, machine: machine, alternate: PairedMachine(offer: rescanned), detail: detail, directory: qrURL.deletingLastPathComponent())
            try Data("authorized".utf8).write(to: qrURL.deletingLastPathComponent().appendingPathComponent("authorized"), options: .atomic)
            var revoked = false
            while ContinuousClock.now < deadline {
                do { _ = try await client.authenticate(machine) }
                catch MobileError.unauthorized { revoked = true; break }
                try await Task.sleep(for: .milliseconds(100))
            }
            guard revoked else { throw MobileError.timedOut }
            do {
                _ = try await client.snapshot(machine)
                throw MobileError.invalidResponse
            } catch MobileError.unauthorized {}
            do { _ = try await client.receipt(for: creation, on: machine); throw MobileError.invalidResponse }
            catch MobileError.unauthorized {}
            try await vault.remove(machineID: offer.machineID)
            print("PAIR_AUTH_REVOKE_OK")
        } catch {
            try await vault.remove(machineID: offer.machineID)
            if wrongPin, (error as? MobileError) == .wrongComputer { print("WRONG_PIN_REJECTED"); return }
            throw error
        }
    }
}
