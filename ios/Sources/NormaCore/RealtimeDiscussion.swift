import Foundation
import Observation

/// Original speech, scoped at capture time. A discussion is never an executable command.
public struct RealtimeDiscussion: Codable, Sendable, Equatable, Identifiable {
    public let id: String
    public let machineID: String?
    public let workspaceID: String?
    public let targetName: String
    public let text: String
    public let recordedAt: Date
    public init(id: String, machineID: String?, workspaceID: String?, targetName: String, text: String, recordedAt: Date) {
        self.id = id; self.machineID = machineID; self.workspaceID = workspaceID
        self.targetName = targetName; self.text = text; self.recordedAt = recordedAt
    }
    func validate() throws {
        guard validID(id), (machineID == nil) == (workspaceID == nil),
              machineID.map({ $0.hasPrefix("mac-") && validHex(String($0.dropFirst(4))) }) ?? true,
              workspaceID.map(validID) ?? true, !targetName.isEmpty, targetName.utf8.count <= 1024,
              !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty, text.utf8.count <= 64 * 1024,
              recordedAt.timeIntervalSince1970.isFinite else { throw MobileError.invalidResponse }
    }
}

public actor RealtimeDiscussionStore {
    private let url: URL
    public init(url: URL) { self.url = url }
    public func load() throws -> [RealtimeDiscussion] {
        guard FileManager.default.fileExists(atPath: url.path) else { return [] }
        let data = try Data(contentsOf: url)
        // 4 MiB total, including JSON metadata; 512 short utterances or fewer long ones.
        guard data.count <= 4 * 1024 * 1024 else { throw MobileError.invalidResponse }
        let entries = try JSONDecoder().decode([RealtimeDiscussion].self, from: data)
        guard entries.count <= 512, Set(entries.map(\.id)).count == entries.count else { throw MobileError.invalidResponse }
        try entries.forEach { try $0.validate() }
        return entries
    }
    public func append(_ entry: RealtimeDiscussion) throws {
        try entry.validate()
        var entries = try load() // Never overwrite a damaged/protected journal as if it were empty.
        if let old = entries.first(where: { $0.id == entry.id }) {
            guard old == entry else { throw MobileError.invalidResponse }
            return
        }
        guard entries.count < 512 else { throw MobileError.invalidResponse }
        entries.append(entry)
        try write(entries)
    }
    public func remove(id: String) throws { try write(load().filter { $0.id != id }) }
    private func write(_ entries: [RealtimeDiscussion]) throws {
        let data = try JSONEncoder().encode(entries)
        guard data.count <= 4 * 1024 * 1024 else { throw MobileError.invalidResponse }
        try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        #if os(iOS)
        // An already-started call keeps saving while locked; unavailable before first device unlock.
        try data.write(to: url, options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication])
        #else
        try data.write(to: url, options: .atomic)
        try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: url.path)
        #endif
    }
}

@MainActor @Observable public final class RealtimeDiscussionBook {
    public private(set) var entries: [RealtimeDiscussion] = []
    public private(set) var loaded = false
    public private(set) var failure: String?
    public private(set) var saving = false
    private let store: RealtimeDiscussionStore
    private var pending: [RealtimeDiscussion] = []
    private var write: Task<Void, Never>?
    public init(store: RealtimeDiscussionStore) { self.store = store }
    public func load() async {
        do {
            let saved = try await store.load()
            entries = saved + entries.filter { local in !saved.contains { $0.id == local.id } }
            loaded = true
        } catch { failure = "讨论草稿未能读取，原文件已保留。新原话暂存在内存中。" }
    }
    public func record(_ entry: RealtimeDiscussion) {
        guard !entries.contains(where: { $0.id == entry.id }) else { return }
        entries.append(entry); pending.append(entry); persist()
    }
    public func retry() { persist() }
    public func flush() async throws {
        while let write { await write.value }
        if failure != nil || !pending.isEmpty { throw CocoaError(.fileWriteUnknown) }
    }
    public func remove(id: String) async {
        while let write { await write.value }
        do {
            try await store.remove(id: id)
            entries.removeAll { $0.id == id }; pending.removeAll { $0.id == id }
            persist()
        } catch { failure = "未能删除草稿，原记录已保留。" }
    }
    private func persist() {
        guard write == nil else { return }
        saving = true
        write = Task {
            defer { saving = false; write = nil }
            await load()
            do {
                // ponytail: rewrite one bounded JSON file per utterance; shard only if measured IO warrants it.
                while let entry = pending.first {
                    try await store.append(entry)
                    pending.removeAll { $0.id == entry.id }
                }
                if loaded { failure = nil }
            } catch { failure = "讨论草稿尚未保存，原话仍在此处。请释放空间或删除不需要的记录后重试。" }
        }
    }
}
