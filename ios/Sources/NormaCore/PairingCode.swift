import Foundation
import NormaPairing

public enum PairingCode {
    public static func validCode(_ text: String) -> Bool {
        text.utf8.count == 6 && text.utf8.allSatisfy { (48...57).contains($0) }
    }

    public static func endpoint(_ text: String) throws -> URL {
        let text = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty, text.utf8.count <= 256,
              !text.unicodeScalars.contains(where: { CharacterSet.whitespacesAndNewlines.contains($0) }),
              var parts = URLComponents(string: text.contains("://") ? text : "wss://" + text),
              parts.scheme == "wss", parts.host?.isEmpty == false,
              parts.user == nil, parts.password == nil, parts.query == nil, parts.fragment == nil,
              ["", "/", "/mobile"].contains(parts.path),
              parts.port == nil || (1...65535).contains(parts.port!) else { throw MobileError.invalidComputerAddress }
        if parts.port == nil { parts.port = 9443 }
        parts.path = "/mobile"
        guard let url = parts.url, validEndpoint(url) else { throw MobileError.invalidComputerAddress }
        return url
    }

    /// The PIN stays local. Only a fresh SPAKE2 public message crosses the unpaired transport.
    /// The decrypted offer is then used by MobileClient's existing strict certificate pinning.
    public static func verify(address: String, code: String) async throws -> PairingOffer {
        let url = try endpoint(address)
        guard validCode(code) else { throw MobileError.invalidPairingCode }
        var message = [UInt8](repeating: 0, count: 33)
        let bytes = Array(code.utf8)
        guard let handle = norma_pairing_start(bytes, &message) else { throw MobileError.invalidPairingCode }
        defer { norma_pairing_free(handle) }
        let id = UUID().uuidString
        let request = try JSONSerialization.data(withJSONObject: ["version": 1, "request_id": id, "machine_id": "",
            "action": ["type": "pairing_code", "message": message.map { String(format: "%02x", $0) }.joined()]])
        let data = try await exchangePairingCode(url, request)
        try Task.checkCancellation()
        let reply = try JSONDecoder().decode(CodeReply.self, from: data)
        guard reply.version == 1, reply.requestID == id else { throw MobileError.invalidResponse }
        guard reply.error == nil, let result = reply.result else { throw MobileError.invalidPairingCode }
        guard result.status == "code_offer", let remote = hexBytes(result.message, maximum: 33), remote.count == 33,
              let sealed = hexBytes(result.sealed, maximum: 4124), sealed.count >= 28 else { throw MobileError.invalidResponse }
        // 4096-byte QR maximum + 12-byte nonce + 16-byte AEAD tag bounds the bridge inputs.
        var output = [UInt8](repeating: 0, count: 4096)
        let count = norma_pairing_finish(handle, remote, sealed, sealed.count, &output)
        guard count > 0, count <= output.count else { throw MobileError.invalidPairingCode }
        return try PairingOffer.decode(Data(output.prefix(Int(count))))
    }
}

private struct CodeReply: Decodable {
    let version: Int
    let requestID: String
    let error: String?
    let result: Body?
    struct Body: Decodable { let status: String; let message: String; let sealed: String }
    enum CodingKeys: String, CodingKey { case version, requestID = "request_id", error, result }
}
private func hexBytes(_ text: String, maximum: Int) -> [UInt8]? {
    let ascii = Array(text.utf8)
    guard !ascii.isEmpty, ascii.count <= maximum * 2, ascii.count % 2 == 0 else { return nil }
    var bytes: [UInt8] = []
    func nibble(_ c: UInt8) -> UInt8? {
        switch c { case 48...57: c - 48; case 97...102: c - 87; default: nil }
    }
    for i in stride(from: 0, to: ascii.count, by: 2) {
        guard let high = nibble(ascii[i]), let low = nibble(ascii[i+1]) else { return nil }
        bytes.append(high * 16 + low)
    }
    return bytes
}
