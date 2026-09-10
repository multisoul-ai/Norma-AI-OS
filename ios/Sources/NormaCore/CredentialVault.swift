import Foundation
import Security

struct DeviceCredential: Codable, Sendable, CustomStringConvertible {
    let deviceID: String
    let token: String
    var description: String { "DeviceCredential(redacted)" }
}

/// All Security framework IO runs on this actor, outside the SwiftUI main actor.
public actor CredentialVault {
    private let service: String
    public init(service: String = "com.norma-ai.os.ios.device") { self.service = service }

    func loadOrCreate(machineID: String) throws -> DeviceCredential {
        if let existing = try load(machineID: machineID) { return existing }
        var bytes = [UInt8](repeating: 0, count: 32)
        let status = bytes.withUnsafeMutableBytes { buffer in
            SecRandomCopyBytes(kSecRandomDefault, buffer.count, buffer.baseAddress!)
        }
        guard status == errSecSuccess else { throw MobileError.keychain(status) }
        let credential = DeviceCredential(deviceID: UUID().uuidString,
            token: bytes.map { String(format: "%02x", $0) }.joined())
        var request = query(machineID)
        request[kSecValueData] = try JSONEncoder().encode(credential)
        request[kSecAttrAccessible] = kSecAttrAccessibleWhenUnlockedThisDeviceOnly
        let added = SecItemAdd(request as CFDictionary, nil)
        if added == errSecDuplicateItem, let existing = try load(machineID: machineID) { return existing }
        guard added == errSecSuccess else { throw MobileError.keychain(added) }
        return credential
    }

    func load(machineID: String) throws -> DeviceCredential? {
        var request = query(machineID)
        request[kSecReturnData] = true
        request[kSecMatchLimit] = kSecMatchLimitOne
        var result: CFTypeRef?
        let status = SecItemCopyMatching(request as CFDictionary, &result)
        if status == errSecItemNotFound { return nil }
        guard status == errSecSuccess else { throw MobileError.keychain(status) }
        guard let data = result as? Data, data.count <= 1024,
              let credential = try? JSONDecoder().decode(DeviceCredential.self, from: data),
              UUID(uuidString: credential.deviceID) != nil, validHex(credential.token) else {
            throw MobileError.invalidCredential
        }
        return credential
    }

    public func remove(machineID: String) throws {
        let status = SecItemDelete(query(machineID) as CFDictionary)
        guard status == errSecSuccess || status == errSecItemNotFound else { throw MobileError.keychain(status) }
    }

    private func query(_ machineID: String) -> [CFString: Any] {
        [kSecClass: kSecClassGenericPassword, kSecAttrService: service,
         kSecAttrAccount: machineID, kSecAttrSynchronizable: false]
    }
}
