import Foundation
import CryptoKit

public struct ProjectAccess: Codable, Sendable, Identifiable {
    public let workspaceID: String
    public let access: String
    public var id: String { workspaceID }
    enum CodingKeys: String, CodingKey { case workspaceID = "workspace_id", access }
}
public struct DeviceGrant: Codable, Sendable {
    public let deviceID: String
    public let deviceName: String
    public let projects: [ProjectAccess]
    public let revision: UInt64
    public let revoked: Bool
    enum CodingKeys: String, CodingKey { case deviceID = "device_id", deviceName = "device_name", projects, revision, revoked }
    func validate(deviceID: String) throws {
        guard self.deviceID == deviceID, !revoked, revision > 0, projects.count <= 256,
              Set(projects.map(\.id)).count == projects.count,
              projects.allSatisfy({ validID($0.workspaceID) && ["read", "control"].contains($0.access) }) else {
            throw MobileError.unauthorized
        }
    }
}
public struct PairingPending: Decodable, Sendable {
    public let requestID: String
    public let deviceID: String
    public let deviceName: String
    public let verificationCode: String
    enum CodingKeys: String, CodingKey { case requestID = "request_id", deviceID = "device_id", deviceName = "device_name", verificationCode = "verification_code" }
}
public struct PairedMachine: Codable, Sendable, Identifiable, Equatable {
    public let id: String
    public let name: String
    public let certificateSHA256: String
    public let endpoints: [URL]
    public init(offer: PairingOffer) {
        id = offer.machineID; name = offer.machineName
        certificateSHA256 = offer.certificateSHA256; endpoints = offer.endpoints
    }
    func validate() throws {
        guard validHex(certificateSHA256), id == "mac-" + certificateSHA256, validName(name),
              !endpoints.isEmpty, endpoints.count <= 4, endpoints.allSatisfy(validEndpoint) else { throw MobileError.invalidOffer }
    }
}
struct WireReply: Decodable, Sendable {
    let version: Int
    let requestID: String
    let error: String?
    let result: ResultBody?
    struct ResultBody: Decodable, Sendable {
        let voiceAuthorization: VoiceAuthorization?
        let status: String
        let image: UploadedChatImage?
        let imageOffset: Int?
        let subscription: PushSubscriptionState?
        let workspaceID: String?
        let plan: ProjectPlan?
        let decisions: [ProjectTodoDecision]?
        let plans: ProjectPlanPage?
        let pending: PairingPending?
        let grant: DeviceGrant?
        let projects: [ProjectSnapshot]?
        let agents: [ProjectAgentDefinition]?
        let receipt: MobileCommandReceipt?
        let review: ProjectReviewResponse?
        let deliveries: TaskDeliveryPage?
        let terminal: TerminalResponse?
        let terminalEvent: TerminalEvent?
        let conversation: ConversationResponse?
        let archive: SessionArchivePage?
        let queuedMessage: QueuedMessageStatus?
        let approvals: ApprovalListResponse?
        let event: ProjectEvent?
        let fragment: ConversationFragment?
        let conversationEvent: ConversationEvent?
        let approvalEvent: ApprovalFeedEvent?
        let planEvent: PlanFeedEvent?
        let deliveryEvent: DeliveryFeedEvent?
        enum CodingKeys: String, CodingKey { case voiceAuthorization = "voice_authorization", image, imageOffset = "image_offset", terminal, terminalEvent = "terminal_event", status, subscription, review, deliveries, plans, plan, decisions, workspaceID = "workspace_id", pending, grant, projects, agents, receipt, conversation, archive, approvals, event, fragment, conversationEvent = "conversation_event", approvalEvent = "approval_event", planEvent = "plan_event", deliveryEvent = "delivery_event", queuedMessage = "queued_message" }
    }
    enum CodingKeys: String, CodingKey { case version, requestID = "request_id", error, result }
    // Older servers reject unknown action fields before extracting request_id. This is used only
    // on a newly opened read socket; fallback reads still validate credentials, scope and responses.
    static func rejectsNewRead(_ data: Data) -> Bool {
        guard data.count <= 1024, let reply = try? JSONDecoder().decode(Self.self, from: data) else { return false }
        return reply.version == 1 && reply.requestID.isEmpty && reply.error == "invalid_request" && reply.result == nil
    }
    static func decode(_ data: Data, expectedID: String) throws -> Self {
        guard data.count <= 512 * 1024, let reply = try? JSONDecoder().decode(Self.self, from: data),
              reply.version == 1, reply.requestID == expectedID,
              (reply.error == nil) != (reply.result == nil) else { throw MobileError.invalidResponse }
        if let error = reply.error {
            switch error {
            case "invalid_image": throw ChatImageError.invalid
            case "image_storage_full": throw ChatImageError.storageFull
            case "image_storage_unavailable", "image_not_found", "image_timeout": throw ChatImageError.unavailable
            case "terminal_changed": throw MobileError.terminalChanged
            case "terminal_unavailable": throw MobileError.terminalUnavailable
            case "push_subscription_changed": throw MobileError.pushSubscriptionChanged
            case "invalid_push_registration": throw MobileError.invalidPushRegistration
            case "unauthorized": throw MobileError.unauthorized
            case "pairing_unavailable": throw MobileError.pairingUnavailable
            case "unsupported_version": throw MobileError.unsupportedVersion
            case "unavailable": throw MobileError.unavailable
            case "snapshot_too_large": throw MobileError.snapshotTooLarge
            case "grant_changed": throw MobileError.grantChanged
            case "idempotency_conflict": throw MobileError.commandConflict
            case "not_found": throw MobileError.commandNotFound
            case "busy": throw MobileError.busy
            case "approval_changed": throw MobileError.approvalChanged
            case "approval_unavailable": throw MobileError.approvalUnavailable
            case "approval_too_large": throw MobileError.approvalTooLarge
            case "decision_unavailable": throw MobileError.decisionUnavailable
            case "todo_decided": throw MobileError.todoDecided
            case "invalid_todo": throw MobileError.invalidTodo
            case "queue_pending": throw MobileError.queuePending
            case "queue_not_cancellable": throw MobileError.queueNotCancellable
            case "queue_unavailable": throw MobileError.queueUnavailable
            case "history_unavailable", "invalid_cursor": throw MobileError.historyUnavailable
            case "review_too_large": throw MobileError.reviewTooLarge
            case "review_expired": throw MobileError.timedOut
            case "delivery_unavailable", "review_unavailable": throw MobileError.reviewUnavailable
            case "review_changed": throw MobileError.reviewChanged
            case "review_unsupported": throw MobileError.reviewUnsupported
            case "history_too_large": throw MobileError.historyTooLarge
            case "session_changed", "route_expired": throw MobileError.sessionChanged
            default: throw MobileError.invalidResponse
            }
        }
        return reply
    }
}
func verificationCode(offerID: String, secret: String, credential: DeviceCredential) throws -> String {
    // All four validated fields are ASCII hex/UUID, so JSONEncoder and Rust serde_json
    // encode this array identically without Unicode or slash-escaping differences.
    let input = try JSONEncoder().encode([offerID, secret, credential.deviceID, credential.token])
    let bytes = Array(SHA256.hash(data: input))
    let number = bytes.prefix(4).reduce(UInt32(0)) { ($0 << 8) | UInt32($1) }
    return String(format: "%06u", number % 1_000_000)
}
func validID(_ value: String) -> Bool {
    !value.isEmpty && value.utf8.count <= 128 && value.utf8.allSatisfy {
        (48...57).contains($0) || (65...90).contains($0) || (97...122).contains($0) || $0 == 45 || $0 == 95
    }
}
