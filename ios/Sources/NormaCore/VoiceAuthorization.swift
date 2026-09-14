import Foundation

public struct VoiceAuthorization: Decodable, Sendable {
    public static let serviceOrigin = "https://norma-voice.vercel.app"
    public let origin: String
    public let token: String
    public let expiresAt: TimeInterval
    public let deviceID: String
    public let machineID: String
    enum CodingKeys: String, CodingKey {
        case origin, token, expiresAt="expires_at", deviceID="device_id", machineID="machine_id"
    }

    /// Cache validation only. The gateway verifies the Mac signature; fresh values arrive over pinned TLS.
    public static func cached(_ token: String, machineID: String? = nil,
                              now: TimeInterval = Date().timeIntervalSince1970) -> Self? {
        let parts=token.split(separator:".",omittingEmptySubsequences:false)
        guard token.utf8.count<=4096, parts.count==3, parts[0]=="nva1",
              parts[1].count.isMultiple(of:2), (128...160).contains(parts[2].count),
              parts[2].count.isMultiple(of:2), parts[1].allSatisfy({$0.isHexDigit && !$0.isUppercase}),
              parts[2].allSatisfy({$0.isHexDigit && !$0.isUppercase}) else { return nil }
        let hex=Array(parts[1].utf8)
        var bytes=Data()
        for offset in stride(from:0,to:hex.count,by:2) {
            guard let byte=UInt8(String(decoding:hex[offset..<(offset+2)],as:UTF8.self),radix:16) else { return nil }
            bytes.append(byte)
        }
        struct Claims: Decodable { let v:Int; let aud:String; let iss:String; let sub:String; let iat:Int64; let exp:Int64 }
        guard let claims=try? JSONDecoder().decode(Claims.self,from:bytes), claims.v==1,
              claims.aud==serviceOrigin, validID(claims.sub), claims.iss.hasPrefix("mac-"),
              validHex(String(claims.iss.dropFirst(4))), machineID==nil || machineID==claims.iss,
              claims.iat>=0, claims.exp>=claims.iat, claims.exp-claims.iat==86400,
              TimeInterval(claims.iat)<=now+30, TimeInterval(claims.exp)>now else { return nil }
        return Self(origin:claims.aud,token:token,expiresAt:TimeInterval(claims.exp),deviceID:claims.sub,machineID:claims.iss)
    }

    func validate(machine: PairedMachine, deviceID: String) throws {
        guard let cached=Self.cached(token,machineID:machine.id), origin==Self.serviceOrigin,
              self.deviceID==deviceID, self.machineID==machine.id,
              cached.deviceID==deviceID, cached.expiresAt==expiresAt else { throw MobileError.invalidResponse }
    }
}

extension MobileClient {
    public func voiceAuthorization(_ machine: PairedMachine) async throws -> VoiceAuthorization {
        try machine.validate()
        guard let credential=try await vault.load(machineID:machine.id) else { throw MobileError.invalidCredential }
        let reply=try await exchange(machine,action:["type":"voice_authorization",
            "device_id":credential.deviceID,"token":credential.token])
        guard reply.result?.status=="voice_authorized",let authorization=reply.result?.voiceAuthorization else { throw MobileError.invalidResponse }
        try authorization.validate(machine:machine,deviceID:credential.deviceID)
        return authorization
    }
}
