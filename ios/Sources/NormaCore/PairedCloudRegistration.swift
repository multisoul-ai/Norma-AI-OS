import Foundation
import CryptoKit
import Security

extension CloudCredentialVault {
    /// Save before requesting registration so interruption retries the same identity and secret.
    public func prepareRegistration() throws -> CloudDeviceCredential {
        if let existing = try load() { return existing }
        var bytes = [UInt8](repeating: 0, count: 32)
        guard SecRandomCopyBytes(kSecRandomDefault, bytes.count, &bytes) == errSecSuccess else {
            throw CloudTextError.credentialUnavailable
        }
        let credential = CloudDeviceCredential(deviceID: UUID().uuidString.lowercased(), token:
            Data(bytes).base64EncodedString().replacingOccurrences(of: "+", with: "-")
                .replacingOccurrences(of: "/", with: "_").replacingOccurrences(of: "=", with: ""))
        try save(credential)
        return credential
    }
}

extension MobileClient {
    public func cloudRegistration(_ machine: PairedMachine, credential cloud: CloudDeviceCredential) async throws -> String {
        try machine.validate()
        guard let credential = try await vault.load(machineID: machine.id) else { throw MobileError.invalidCredential }
        let hash = SHA256.hash(data: Data(cloud.token.utf8)).map { String(format: "%02x", $0) }.joined()
        let reply = try await exchange(machine, action: ["type": "cloud_registration",
            "device_id": credential.deviceID, "token": credential.token,
            "cloud_device_id": cloud.deviceID, "token_hash": hash])
        guard reply.result?.status == "cloud_registration", let proof = reply.result?.registrationProof,
              proof.hasPrefix("ncr1."), proof.utf8.count <= 4096 else { throw MobileError.invalidResponse }
        return proof
    }
}
