import Foundation
import Security

public struct CloudDeviceCredential: Codable, Sendable, CustomStringConvertible {
    public let deviceID: String
    public let token: String
    public var description: String { "CloudDeviceCredential(redacted)" }
}

public actor CloudCredentialVault {
    private let service: String
    private let account: String
    public init(service: String = "com.norma-ai.os.ios.cloud", account: String = "invited-device") {
        self.service = service; self.account = account
    }
    public nonisolated var pendingRegistration: CloudCredentialVault {
        CloudCredentialVault(service: service, account: "pending-registration")
    }

    private var query: [CFString: Any] {
        [kSecClass: kSecClassGenericPassword, kSecAttrService: service,
         kSecAttrAccount: account, kSecAttrSynchronizable: false]
    }

    public func load() throws -> CloudDeviceCredential? {
        var request = query
        request[kSecReturnData] = true
        request[kSecMatchLimit] = kSecMatchLimitOne
        var result: CFTypeRef?
        let status = SecItemCopyMatching(request as CFDictionary, &result)
        if status == errSecItemNotFound { return nil }
        guard status == errSecSuccess else { throw CloudTextError.credentialUnavailable }
        guard let data = result as? Data, data.count <= 512,
              let credential = try? JSONDecoder().decode(CloudDeviceCredential.self, from: data),
              UUID(uuidString: credential.deviceID) != nil,
              credential.token.range(of: "^[A-Za-z0-9_-]{43}$", options: .regularExpression) != nil else {
            throw CloudTextError.credentialUnavailable
        }
        return credential
    }

    public func save(_ credential: CloudDeviceCredential) throws {
        guard UUID(uuidString: credential.deviceID) != nil,
              credential.token.range(of: "^[A-Za-z0-9_-]{43}$", options: .regularExpression) != nil else {
            throw CloudTextError.credentialUnavailable
        }
        let attributes: [CFString: Any] = [kSecValueData: try JSONEncoder().encode(credential),
                                            kSecAttrAccessible: kSecAttrAccessibleWhenUnlockedThisDeviceOnly]
        var status = SecItemUpdate(query as CFDictionary, attributes as CFDictionary)
        if status == errSecItemNotFound {
            status = SecItemAdd(query.merging(attributes) { _, new in new } as CFDictionary, nil)
        }
        guard status == errSecSuccess else { throw CloudTextError.credentialUnavailable }
    }

    public func remove() throws {
        let status = SecItemDelete(query as CFDictionary)
        guard status == errSecSuccess || status == errSecItemNotFound else {
            throw CloudTextError.credentialUnavailable
        }
    }
}

public enum CloudTextError: LocalizedError, Sendable {
    case invitationInvalid, unauthorized, revoked, quotaExceeded, busy, duplicateTurn
    case providerUnavailable, providerTimeout, invalidResponse, interrupted, credentialUnavailable
    case invalidRequest

    public var errorDescription: String? {
        switch self {
        case .invitationInvalid: "邀请已失效，请获取新的邀请。"
        case .unauthorized: "手机注册尚未完成，请在 Norma 内扫描电脑二维码完成连接。"
        case .revoked: "这部 iPhone 的云端访问权限已撤销。"
        case .quotaExceeded: "今天的对话额度已用完，请明天再试。"
        case .busy: "Norma 正在回复上一条消息，请稍等。"
        case .duplicateTurn: "这条消息已经提交过；请查看原回合状态。"
        case .providerUnavailable: "云端 Norma 暂时不可用，请稍后重试。"
        case .providerTimeout: "云端 Norma 回复超时，原消息已保留。"
        case .invalidResponse: "云端回复格式异常，原消息已保留。"
        case .interrupted: "云端回复中断，原消息已保留，可再次发送。"
        case .credentialUnavailable: "无法读取此手机的云端授权。"
        case .invalidRequest: "消息无法发送，请检查内容后重试。"
        }
    }

    static func server(_ code: String) -> Self {
        switch code {
        case "invitation_invalid": .invitationInvalid
        case "unauthorized": .unauthorized
        case "revoked": .revoked
        case "quota_exceeded": .quotaExceeded
        case "busy": .busy
        case "duplicate_turn": .duplicateTurn
        case "provider_timeout": .providerTimeout
        case "provider_invalid_plan": .invalidResponse
        case "provider_unavailable", "device_state_unavailable", "not_found": .providerUnavailable
        default: .invalidRequest
        }
    }
}

struct CloudTextStream {
    let turnID: String
    private(set) var text = ""
    private(set) var started = false
    private(set) var completed = false

    mutating func consume(_ line: String) throws {
        guard line.utf8.count <= 20_000,
              let data = line.data(using: .utf8),
              let event = try? JSONDecoder().decode(Event.self, from: data) else {
            throw CloudTextError.invalidResponse
        }
        switch event.type {
        case "start":
            guard !started, !completed, event.turnID == turnID else { throw CloudTextError.invalidResponse }
            started = true
        case "delta":
            guard started, !completed, let chunk = event.text,
                  text.utf8.count + chunk.utf8.count <= 65_536 else { throw CloudTextError.invalidResponse }
            text += chunk
        case "done":
            guard started, !completed, event.turnID == turnID, !text.isEmpty else {
                throw CloudTextError.invalidResponse
            }
            completed = true
        case "error": throw CloudTextError.server(event.code ?? "provider_unavailable")
        default: throw CloudTextError.invalidResponse
        }
    }

    private struct Event: Decodable {
        let type: String
        let turnID: String?
        let text: String?
        let code: String?
        enum CodingKeys: String, CodingKey { case type, turnID = "turn_id", text, code }
    }
}

public struct CloudTextRequestMessage: Encodable, Sendable {
    public let role: String
    public let content: String
    public init(role: String, content: String) { self.role = role; self.content = content }
}

final class CloudNoRedirects: NSObject, URLSessionTaskDelegate, @unchecked Sendable {
    func urlSession(_ session: URLSession, task: URLSessionTask, willPerformHTTPRedirection response: HTTPURLResponse,
                    newRequest request: URLRequest, completionHandler: @escaping (URLRequest?) -> Void) {
        completionHandler(nil)
    }
}

public struct CloudTextClient: Sendable {
    public static let serviceOrigin = URL(string: "https://soulvoiceai.com/norma/voice/")!
    private let origin: URL
    private let session: URLSession

    public init() {
        origin = Self.serviceOrigin
        session = URLSession(configuration: .ephemeral, delegate: CloudNoRedirects(), delegateQueue: nil)
    }

    #if DEBUG
    public static func developmentSettings(in environment: [String: String]) throws -> (port: UInt16, namespace: String)? {
        guard let value = environment["NORMA_CLOUD_LOCAL_PORT"] else { return nil }
        guard let port = UInt16(value), port > 0,
              let namespace = environment["NORMA_CLOUD_TEST_NAMESPACE"], UUID(uuidString: namespace) != nil else {
            throw CloudTextError.invalidRequest
        }
        return (port, namespace)
    }

    /// Simulator-only development traffic stays on this computer and never follows redirects.
    public init(localPort: UInt16) throws {
        guard localPort > 0 else { throw CloudTextError.invalidRequest }
        origin = URL(string: "http://127.0.0.1:\(localPort)/")!
        session = URLSession(configuration: .ephemeral, delegate: CloudNoRedirects(), delegateQueue: nil)
    }

    #endif

    public init(local: LocalCloudConfiguration) {
        origin = local.origin
        let configuration = URLSessionConfiguration.ephemeral
        configuration.tlsMinimumSupportedProtocolVersion = .TLSv13
        configuration.timeoutIntervalForRequest = 15
        session = URLSession(configuration: configuration, delegate: PinDelegate(pin: local.certificateSHA256), delegateQueue: nil)
    }

    init(origin: URL, session: URLSession) {
        self.origin = origin
        self.session = session
    }

    public func claim(code: String, name: String) async throws -> CloudDeviceCredential {
        guard code.range(of: "^[A-Za-z0-9_-]{43}$", options: .regularExpression) != nil,
              !name.isEmpty, name.count <= 80 else { throw CloudTextError.invitationInvalid }
        let payload = try JSONEncoder().encode(ClaimRequest(code: code, name: name))
        let (data, response) = try await session.data(for: request("v1/devices/claim", body: payload))
        guard data.count <= 4096, let http = response as? HTTPURLResponse else {
            throw CloudTextError.invalidResponse
        }
        guard http.statusCode == 200 else { throw try error(from: data) }
        let result = try JSONDecoder().decode(ClaimResponse.self, from: data)
        return CloudDeviceCredential(deviceID: result.deviceID, token: result.token)
    }

    public func register(_ credential: CloudDeviceCredential, proof: String, name: String) async throws {
        let payload = try JSONSerialization.data(withJSONObject: ["device_id": credential.deviceID,
            "token": credential.token, "proof": proof, "name": name])
        guard payload.count <= 4096 else { throw CloudTextError.invalidRequest }
        let (data, response) = try await session.data(for: request("v1/devices/register", body: payload))
        guard data.count <= 4096, let http = response as? HTTPURLResponse else { throw CloudTextError.invalidResponse }
        guard http.statusCode == 200 else { throw try error(from: data) }
        struct Registered: Decodable { let device_id: String }
        guard try JSONDecoder().decode(Registered.self, from: data).device_id == credential.deviceID else {
            throw CloudTextError.invalidResponse
        }
    }

    public func reply(turnID: String, token: String, messages: [CloudTextRequestMessage],
                      onDelta: @escaping @Sendable (String) async -> Void) async throws -> String {
        guard UUID(uuidString: turnID) != nil, token.count == 43, !messages.isEmpty,
              messages.count <= 24, messages.last?.role == "user",
              messages.allSatisfy({ ["user", "assistant"].contains($0.role) && !$0.content.isEmpty && $0.content.unicodeScalars.count <= 4096 }) else {
            throw CloudTextError.invalidRequest
        }
        let body = try JSONEncoder().encode(TurnRequest(turnID: turnID, messages: messages))
        guard body.count <= 32_768 else { throw CloudTextError.invalidRequest }
        var req = request("v1/text/turn", body: body)
        req.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        req.timeoutInterval = 40
        let (bytes, response) = try await session.bytes(for: req)
        guard let http = response as? HTTPURLResponse else { throw CloudTextError.invalidResponse }
        guard http.statusCode == 200 else {
            var data = Data()
            for try await byte in bytes {
                data.append(byte)
                if data.count > 4096 { throw CloudTextError.invalidResponse }
            }
            throw try error(from: data)
        }
        guard http.value(forHTTPHeaderField: "Content-Type")?.hasPrefix("application/x-ndjson") == true else {
            throw CloudTextError.invalidResponse
        }
        var stream = CloudTextStream(turnID: turnID)
        for try await line in bytes.lines {
            let previous = stream.text
            try stream.consume(line)
            if stream.text != previous { await onDelta(stream.text) }
            if stream.completed { break }
        }
        guard stream.completed else { throw CloudTextError.interrupted }
        return stream.text
    }


    private func request(_ path: String, body: Data) -> URLRequest {
        var request = URLRequest(url: origin.appending(path: path), timeoutInterval: 15)
        request.httpMethod = "POST"
        request.httpBody = body
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        return request
    }

    private func error(from data: Data) throws -> CloudTextError {
        struct ServerError: Decodable { let error: String }
        guard let result = try? JSONDecoder().decode(ServerError.self, from: data) else {
            throw CloudTextError.invalidResponse
        }
        return .server(result.error)
    }

    private struct ClaimRequest: Encodable { let code: String; let name: String }
    private struct ClaimResponse: Decodable {
        let deviceID: String
        let token: String
        enum CodingKeys: String, CodingKey { case deviceID = "device_id", token }
    }
    private struct TurnRequest: Encodable {
        let turnID: String
        let messages: [CloudTextRequestMessage]
        enum CodingKeys: String, CodingKey { case turnID = "turn_id", messages }
    }
}
