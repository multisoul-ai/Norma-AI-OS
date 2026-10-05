import Foundation
import Observation
import UIKit
import NormaCore

@MainActor @Observable final class CloudChatModel {
    private let store: CloudChatStore
    private let vault: CloudCredentialVault
    private let client: CloudTextClient
    private var configurationError: String?
    private var loadingTask: Task<Void, Never>?
    private(set) var messages: [CloudChatMessage] = []
    private(set) var credential: CloudDeviceCredential?
    private(set) var loaded = false
    private(set) var enrolling = false
    private(set) var sending = false
    var draft = ""
    var partialReply = ""
    var error: String?

    init() {
        var directory = URL.applicationSupportDirectory.appendingPathComponent("Norma/CloudChat", isDirectory: true)
        var service = "com.norma-ai.os.ios.cloud"
        var configuredClient = CloudTextClient()
        do {
            if let local = try LocalCloudConfiguration.bundled() {
                directory = URL.applicationSupportDirectory.appendingPathComponent("Norma/LocalCloudChat/\(local.namespace)", isDirectory: true)
                service += ".local.\(local.namespace).\(local.certificateSHA256)"
                configuredClient = CloudTextClient(local: local)
            }
        } catch {
            configurationError = "本地验收配置无效，请安装正确的测试版本。"
        }
        #if DEBUG
        if let namespace = ProcessInfo.processInfo.environment["NORMA_CLOUD_TEST_NAMESPACE"],
           UUID(uuidString: namespace) != nil {
            directory = URL.applicationSupportDirectory.appendingPathComponent("Norma/UITestCloudChat/\(namespace)", isDirectory: true)
            service += ".\(namespace)"
        }
        do {
            if let local = try CloudTextClient.developmentSettings(in: ProcessInfo.processInfo.environment) {
                guard try LocalCloudConfiguration.bundled() == nil else { throw CloudTextError.invalidRequest }
                directory = URL.applicationSupportDirectory.appendingPathComponent("Norma/LocalCloudChat/\(local.port)/\(local.namespace)", isDirectory: true)
                service = "com.norma-ai.os.ios.cloud.local.\(local.port).\(local.namespace)"
                configuredClient = try CloudTextClient(localPort: local.port)
            }
        } catch {
            configurationError = "本地联调配置无效：需要有效端口和隔离命名空间。"
        }
        #endif
        store = CloudChatStore(databaseURL: directory.appendingPathComponent("chat.sqlite"))
        vault = CloudCredentialVault(service: service)
        client = configuredClient
    }

    init(store: CloudChatStore, vault: CloudCredentialVault, client: CloudTextClient) {
        self.store = store; self.vault = vault; self.client = client
    }

    func load() async {
        guard configurationError == nil else { error = configurationError; return }
        guard !loaded else { return }
        if let loadingTask { await loadingTask.value; return }
        let operation = Task {
            do {
                let saved = try await store.all()
                for message in saved where message.role == .user && message.state == .pending {
                    try await store.fail(turnID: message.turnID, code: "interrupted")
                }
                messages = try await store.all()
                credential = try await vault.load()
                loaded = true; error = nil
            } catch { self.error = error.localizedDescription }
        }
        loadingTask = operation
        await operation.value
        loadingTask = nil
    }

    func claim(code: String) async {
        await load()
        guard loaded, !enrolling else { return }
        enrolling = true
        defer { enrolling = false }
        do {
            let accepted = try await client.claim(code: code, name: UIDevice.current.name)
            try await vault.save(accepted)
            credential = accepted
            error = nil
        } catch { self.error = error.localizedDescription }
    }

    func register(on machine: PairedMachine, using mobile: MobileClient) async throws {
        await load()
        guard loaded else { throw CloudTextError.credentialUnavailable }
        if credential != nil { return }
        guard !enrolling else { throw CloudTextError.busy }
        enrolling = true
        error = nil
        defer { enrolling = false }
        let pending = vault.pendingRegistration
        let identity = try await pending.prepareRegistration()
        try Task.checkCancellation()
        let proof = try await mobile.cloudRegistration(machine, credential: identity)
        try Task.checkCancellation()
        try await client.register(identity, proof: proof, name: UIDevice.current.name)
        try Task.checkCancellation()
        try await vault.save(identity)
        credential = identity
        error = nil
        try? await pending.remove()
    }

    func send() async {
        guard loaded, !enrolling, !sending, let credential else { return }
        let text = draft
        guard !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return }
        sending = true
        defer { sending = false }
        let turnID = UUID().uuidString
        do {
            _ = try await store.appendUser(turnID: turnID, text: text)
            messages = try await store.all()
        } catch { self.error = error.localizedDescription; return }
        if draft == text { draft = "" }
        partialReply = ""
        error = nil
        var context = messages.filter { $0.state == .complete }
            .suffix(23).map { CloudTextRequestMessage(role: $0.role == .user ? "user" : "assistant", content: $0.text) }
        context.append(CloudTextRequestMessage(role: "user", content: text))
        // Leave room for turn_id and JSON syntax inside the server's 32 KiB request ceiling.
        while context.count > 1 && ((try? JSONEncoder().encode(context).count) ?? 0) > 28_000 {
            context.removeFirst()
        }
        do {
            let reply = try await client.reply(turnID: turnID, token: credential.token, messages: context) { [weak self] current in
                await MainActor.run { self?.partialReply = current }
            }
            _ = try await store.complete(turnID: turnID, text: reply)
            partialReply = ""
            messages = try await store.all()
        } catch {
            try? await store.fail(turnID: turnID, code: (error as? CloudTextError).map { String(describing: $0) } ?? "interrupted")
            if let saved = try? await store.all() { messages = saved }
            partialReply = ""
            self.error = error.localizedDescription
        }
    }

    func prepareRetry(_ message: CloudChatMessage) {
        guard message.role == .user && message.state == .failed else { return }
        draft = message.text
    }
}
