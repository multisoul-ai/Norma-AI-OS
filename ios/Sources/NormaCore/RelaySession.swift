import CryptoKit
import Foundation
import Security

public struct RelayHello: Codable, Sendable {
    public let version: Int
    public let clientKey: String
    public let nonce: String
    enum CodingKeys: String, CodingKey { case version, clientKey = "client_key", nonce }
}

public struct RelayHandshake: Codable, Sendable {
    public let version: Int
    public let serverKey: String
    public let certificate: String
    public let signature: String
    enum CodingKeys: String, CodingKey { case version, serverKey = "server_key", certificate, signature }
}

public enum RelaySessionError: LocalizedError {
    case invalidIdentity, invalidHandshake, consumed, invalidFrame, oversized
    public var errorDescription: String? {
        switch self {
        case .invalidIdentity: "无法验证这台 Mac 的配对身份。"
        case .invalidHandshake: "安全连接验证失败，请重新连接。"
        case .consumed: "这次连接已结束，请重新连接。"
        case .invalidFrame: "收到的消息未通过安全验证，请重新连接。"
        case .oversized: "消息超过本次传输上限，原内容已保留。"
        }
    }
}

/// A single-use offer. Its nonce/key cannot be copied into two sessions with repeated GCM counters.
public actor RelaySessionClient {
    public nonisolated let hello: RelayHello
    private let streamID: String
    private let machineID: String
    private var privateKey: Curve25519.KeyAgreement.PrivateKey?

    public init(streamID: String, machineID: String) throws {
        guard UUID(uuidString: streamID) != nil, machineID.hasPrefix("mac-"),
              (try? relayHex(String(machineID.dropFirst(4)), bytes: 32)) != nil else {
            throw RelaySessionError.invalidIdentity
        }
        let key = Curve25519.KeyAgreement.PrivateKey()
        var nonce = [UInt8](repeating: 0, count: 32)
        let status = nonce.withUnsafeMutableBytes { SecRandomCopyBytes(kSecRandomDefault, $0.count, $0.baseAddress!) }
        guard status == errSecSuccess else { throw RelaySessionError.invalidHandshake }
        self.streamID = streamID; self.machineID = machineID; self.privateKey = key
        hello = RelayHello(version: 1, clientKey: relayHex(key.publicKey.rawRepresentation), nonce: relayHex(Data(nonce)))
    }

    public func finish(_ reply: RelayHandshake) throws -> RelayEncryptedSession {
        guard let privateKey else { throw RelaySessionError.consumed }
        self.privateKey = nil
        guard reply.version == 1 else { throw RelaySessionError.invalidHandshake }
        let certificate = try relayHex(reply.certificate, maximum: 16 * 1024)
        guard "mac-" + relayHex(Data(SHA256.hash(data: certificate))) == machineID,
              let cert = SecCertificateCreateWithData(nil, certificate as CFData),
              let key = SecCertificateCopyKey(cert),
              let publicBytes = SecKeyCopyExternalRepresentation(key, nil) else { throw RelaySessionError.invalidIdentity }
        let serverBytes = try relayHex(reply.serverKey, bytes: 32)
        let transcript = Data("Norma collaboration session v1\n\(streamID)\n\(hello.nonce)\n\(hello.clientKey)\n\(reply.serverKey)\n\(machineID)".utf8)
        do {
            let signingKey = try P256.Signing.PublicKey(x963Representation: publicBytes as Data)
            let signature = try P256.Signing.ECDSASignature(derRepresentation: relayHex(reply.signature, maximum: 80))
            guard signingKey.isValidSignature(signature, for: transcript) else { throw RelaySessionError.invalidHandshake }
            let remote = try Curve25519.KeyAgreement.PublicKey(rawRepresentation: serverBytes)
            let secret = try privateKey.sharedSecretFromKeyAgreement(with: remote)
            let digest = Data(SHA256.hash(data: transcript))
            let material = secret.hkdfDerivedSymmetricKey(using: SHA256.self, salt: digest,
                sharedInfo: Data("Norma collaboration keys v1".utf8), outputByteCount: 64)
            let keys = material.withUnsafeBytes { Data($0) }
            return RelayEncryptedSession(outgoing: SymmetricKey(data: keys.prefix(32)),
                incoming: SymmetricKey(data: keys.suffix(32)), transcriptHash: digest)
        } catch { throw RelaySessionError.invalidHandshake }
    }
}

/// Mutable counters are serialized by the actor. Persist operations, never this ephemeral cipher.
public actor RelayEncryptedSession {
    // 256 KiB outer frame = 8-byte sequence + plaintext + 16-byte authentication tag.
    private static let maximumPlaintext = 256 * 1024 - 24
    private let outgoing: SymmetricKey
    private let incoming: SymmetricKey
    private let transcriptHash: Data
    private var sent: UInt64 = 0
    private var received: UInt64 = 0

    fileprivate init(outgoing: SymmetricKey, incoming: SymmetricKey, transcriptHash: Data) {
        self.outgoing = outgoing; self.incoming = incoming; self.transcriptHash = transcriptHash
    }

    public func seal(_ plaintext: Data) throws -> Data {
        guard plaintext.count <= Self.maximumPlaintext else { throw RelaySessionError.oversized }
        guard sent < UInt64.max else { throw RelaySessionError.consumed }
        let sequence = relaySequence(sent)
        do {
            let sealed = try AES.GCM.seal(plaintext, using: outgoing, nonce: nonce(sequence),
                                         authenticating: aad(0, sequence))
            sent += 1
            return sequence + sealed.ciphertext + sealed.tag
        } catch { throw RelaySessionError.invalidFrame }
    }

    public func open(_ frame: Data) throws -> Data {
        guard (24...Self.maximumPlaintext + 24).contains(frame.count), received < UInt64.max else {
            throw RelaySessionError.invalidFrame
        }
        let sequence = Data(frame.prefix(8))
        guard sequence == relaySequence(received) else { throw RelaySessionError.invalidFrame }
        do {
            let box = try AES.GCM.SealedBox(nonce: nonce(sequence), ciphertext: frame.dropFirst(8).dropLast(16), tag: frame.suffix(16))
            let plaintext = try AES.GCM.open(box, using: incoming, authenticating: aad(1, sequence))
            received += 1
            return plaintext
        } catch { throw RelaySessionError.invalidFrame }
    }

    private func nonce(_ sequence: Data) throws -> AES.GCM.Nonce { try AES.GCM.Nonce(data: Data(repeating: 0, count: 4) + sequence) }
    private func aad(_ direction: UInt8, _ sequence: Data) -> Data { transcriptHash + Data([direction]) + sequence }
}

private func relaySequence(_ value: UInt64) -> Data { Data((0..<8).reversed().map { UInt8(truncatingIfNeeded: value >> ($0 * 8)) }) }
private func relayHex(_ data: Data) -> String { data.map { String(format: "%02x", $0) }.joined() }
private func relayHex(_ value: String, bytes: Int? = nil, maximum: Int = 32) throws -> Data {
    guard value.utf8.count <= (bytes ?? maximum) * 2 else { throw RelaySessionError.invalidHandshake }
    let input = Array(value.utf8)
    guard !input.isEmpty, input.count % 2 == 0, input.count <= (bytes ?? maximum) * 2,
          bytes.map({ input.count == $0 * 2 }) ?? true,
          input.allSatisfy({ (48...57).contains($0) || (97...102).contains($0) }) else { throw RelaySessionError.invalidHandshake }
    let nibble: (UInt8) -> UInt8 = { $0 < 58 ? $0 - 48 : $0 - 87 }
    return Data(stride(from: 0, to: input.count, by: 2).map { nibble(input[$0]) * 16 + nibble(input[$0 + 1]) })
}
