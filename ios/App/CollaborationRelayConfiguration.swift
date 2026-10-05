import Foundation
import NormaCore

func collaborationRelayClient() throws -> CollaborationRelayClient {
    if let local = try LocalCloudConfiguration.bundled() {
        #if DEBUG
        guard ProcessInfo.processInfo.environment["NORMA_CLOUD_LOCAL_PORT"] == nil else { throw CloudTextError.invalidRequest }
        #endif
        return CollaborationRelayClient(local: local)
    }
    #if DEBUG
    if let local = try CloudTextClient.developmentSettings(in: ProcessInfo.processInfo.environment) {
        return try CollaborationRelayClient(localPort: local.port)
    }
    #endif
    return CollaborationRelayClient()
}
