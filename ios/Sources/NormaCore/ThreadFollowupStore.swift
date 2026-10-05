import Foundation
import SQLite3

public struct ThreadFollowupOutboxOperation: Codable, Equatable, Identifiable, Sendable {
    public var id: String { followup.operationID }
    public let machineID: String
    public let followup: ThreadFollowupOperation
    public internal(set) var receipt: ThreadFollowupReceipt?
    public internal(set) var replyMessageID: String?
    public var awaitingReceipt: Bool {
        guard let receipt else { return true }
        if !receipt.queuedMessage.isTerminal { return true }
        return receipt.queuedMessage.state == "sent" && (receipt.outcome == nil || receipt.delivery == nil)
    }
}
