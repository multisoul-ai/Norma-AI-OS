import Foundation

public enum VoiceProvider: String, CaseIterable, Sendable, Identifiable {
    case doubao, openai
    public var id: String { rawValue }
    public var title: String { self == .doubao ? "豆包" : "GPT" }
}

/// Validates a short-lived gateway ticket before any media transport receives its secret.
public struct VoiceSessionTicket: Sendable {
    public let token: String
    public let url: URL

    public init(data: Data, endpoint: URL, provider: VoiceProvider, now: TimeInterval = Date().timeIntervalSince1970) throws {
        guard data.count <= 32 * 1024,
              let ticket = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              ticket["provider"] as? String == provider.rawValue,
              let token = ticket["token"] as? String, !token.isEmpty, token.utf8.count <= 8192,
              !token.unicodeScalars.contains(where: CharacterSet.controlCharacters.contains),
              let expires = ticket["expiresAt"] as? Double, expires.isFinite, expires > now,
              let address = ticket["url"] as? String, let url = URL(string: address),
              url.user == nil, url.password == nil, url.query == nil, url.fragment == nil else { throw InvalidTicket() }
        switch provider {
        case .openai:
            guard ticket["transport"] as? String == "webrtc",
                  address == "https://api.openai.com/v1/realtime/calls" else { throw InvalidTicket() }
        case .doubao:
            guard ticket["transport"] as? String == "websocket",
                  ticket["inputRate"] as? Int == 16000, ticket["outputRate"] as? Int == 24000,
                  Self.matchesGateway(url, endpoint: endpoint, path: "/v1/voice") else { throw InvalidTicket() }
        }
        self.token = token; self.url = url
    }

    public static func endpoint(_ origin: String) -> URL? {
        guard let url = URL(string: origin.trimmingCharacters(in: .whitespacesAndNewlines)), url.host != nil,
              url.user == nil, url.password == nil, url.query == nil, url.fragment == nil,
              url.path.isEmpty || url.path == "/",
              url.scheme == "https" || isLocalEndpoint(url) else { return nil }
        return url.appendingPathComponent("v1/session")
    }

    static func matchesGateway(_ url: URL, endpoint: URL, path: String) -> Bool {
        guard let host = endpoint.host, url.host == host, url.user == nil, url.password == nil,
              url.query == nil, url.fragment == nil, url.path == path,
              (url.port ?? (url.scheme == "wss" ? 443 : 80)) == (endpoint.port ?? (endpoint.scheme == "https" ? 443 : 80)) else { return false }
        return (endpoint.scheme == "https" && url.scheme == "wss") || (isLocalEndpoint(endpoint) && url.scheme == "ws")
    }

    private static func isLocalEndpoint(_ url: URL) -> Bool {
        #if DEBUG
        url.scheme == "http" && ["127.0.0.1", "localhost", "[::1]"].contains(url.host ?? "")
        #else
        false
        #endif
    }

    private struct InvalidTicket: LocalizedError {
        var errorDescription: String? { "语音网关返回的临时凭证无效或已过期" }
    }
}
