import Foundation

public struct ChatDraft: Codable, Sendable, Equatable, Identifiable {
    public struct Scope: Codable, Sendable, Hashable {
        public let machineID: String
        public let workspaceID: String
        public let routeID: String
        public let nodeID: UInt64
        public let runID: String?
        public let sessionID: String?
        public var feedbackFingerprint: String? = nil
        public init(machineID: String, workspaceID: String, routeID: String, nodeID: UInt64, runID: String?, sessionID: String?, feedbackFingerprint: String? = nil) {
            self.machineID = machineID; self.workspaceID = workspaceID; self.routeID = routeID
            self.nodeID = nodeID; self.runID = runID; self.sessionID = sessionID
            self.feedbackFingerprint = feedbackFingerprint
        }
        func validate() throws {
            if let feedbackFingerprint { guard validHex(feedbackFingerprint), runID != nil, sessionID != nil else { throw MobileError.invalidResponse } }
            // A restored original card retains its session before a new PTY/run starts.
            // Keep both optional identities in the scope; do not migrate drafts when the run changes.
            guard machineID.hasPrefix("mac-"), validHex(String(machineID.dropFirst(4))),
                  [workspaceID, routeID].allSatisfy(validID), nodeID > 0,
                  [runID, sessionID].compactMap({ $0 }).allSatisfy(validID) else { throw MobileError.invalidResponse }
        }
    }
    public let id: String
    public let scope: Scope
    public let text: String
    public let editedAt: Date
    public let label: String?
    public var images: [ChatImage]? = nil
    public var imageAttachments: [ChatImage] { images ?? [] }

    func validate() throws {
        try scope.validate()
        // 64 KiB = twice the send limit, allowing an oversized message to be edited/split without loss.
        try imageAttachments.forEach { try $0.validate() }
        guard imageAttachments.count <= 4, Set(imageAttachments.map(\.id)).count == imageAttachments.count,
              validID(id), !text.isEmpty || !imageAttachments.isEmpty, text.utf8.count <= 64 * 1024,
              editedAt.timeIntervalSince1970.isFinite, (label?.utf8.count ?? 0) <= 1024 else { throw MobileError.invalidResponse }
    }
}

public actor ChatDraftStore {
    let url: URL
    public init(url: URL) { self.url = url }
    public func load() throws -> [ChatDraft] {
        guard FileManager.default.fileExists(atPath: url.path) else { return [] }
        let data = try Data(contentsOf: url)
        // 4 MiB = 128 typical maximum-send bodies × 32 KiB; encoded metadata also counts toward this bound.
        guard data.count <= 4 * 1024 * 1024 else { throw MobileError.invalidResponse }
        let drafts = try JSONDecoder().decode([ChatDraft].self, from: data)
        try validate(drafts)
        return drafts
    }
    func replace(_ drafts: [ChatDraft]) throws {
        let previous = try load() // Refuse to overwrite a damaged or protected file, even after an earlier successful load.
        try validate(drafts)
        let data = try JSONEncoder().encode(drafts)
        guard data.count <= 4 * 1024 * 1024 else { throw MobileError.invalidResponse }
        try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        #if os(iOS)
        try data.write(to: url, options: [.atomic, .completeFileProtection])
        #else
        try data.write(to: url, options: .atomic)
        try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: url.path)
        #endif
        removeUnusedImages(previous: previous, next: drafts)
    }
    private func validate(_ drafts: [ChatDraft]) throws {
        guard drafts.count <= 128, Set(drafts.map(\.scope)).count == drafts.count,
              Set(drafts.map(\.id)).count == drafts.count else { throw MobileError.invalidResponse }
        try drafts.forEach { try $0.validate() }
    }
}
