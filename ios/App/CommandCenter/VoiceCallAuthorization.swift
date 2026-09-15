import Foundation
import NormaCore

extension VoiceCall {
    static func initialGateway() -> String {
        VoiceAuthorization.initialGateway(UserDefaults.standard.string(forKey:"realtime-gateway"))
    }

    var usesAutomaticAuthorization: Bool { gateway == Self.serviceOrigin }

    func preparedAccessToken() async throws -> String {
        guard usesAutomaticAuthorization else { return accessToken }
        let cached=VoiceAuthorization.cached(accessToken,machineID:target?.machine.id)
        // Refresh during the last hour when Mac is reachable; an unexpired cached grant still works offline.
        if let cached, cached.expiresAt-Date().timeIntervalSince1970>60*60 { return cached.token }
        if let machine=target?.machine {
            do { return try await mobileClient.voiceAuthorization(machine).token }
            catch {
                if let cached { return cached.token }
                throw CallError.message("未能自动获取语音授权，请确认已配对的 Mac 在线并已更新到最新版。")
            }
        }
        if let cached { return cached.token }
        throw CallError.message("请先配对并选择 Mac 项目，语音服务会自动配置。")
    }
}
