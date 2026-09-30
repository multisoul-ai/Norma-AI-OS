import Foundation
import Observation

public struct RealtimeReportQueue: Codable, Sendable {
    public enum Playback: String, Codable, Sendable { case waiting, speaking, played, unconfirmed }
    public struct Entry: Codable, Sendable, Identifiable {
        public let id: String
        public let machineID: String
        public let workspaceID: String
        public let name: String
        public let sequence: UInt64
        public let outcome: String
        public let available: Bool
        public var accepted: Bool
        public var playback: Playback
        public let receivedAt: Date
    }
    public struct Batch: Sendable {
        public let id = UUID().uuidString
        public let ids: [String]
        public let text: String
    }
    public private(set) var entries: [Entry] = []
    public init() {}
    @discardableResult public mutating func observe(machineID: String, name: String, delivery: TaskDeliverySummary, now: Date) throws -> Bool {
        try delivery.validate(workspaceID: delivery.workspaceID, nodeID: nil)
        guard machineID.hasPrefix("mac-"), validHex(String(machineID.dropFirst(4))),
              !name.isEmpty, name.utf8.count <= 1024 else { throw MobileError.invalidResponse }
        let id = machineID + "/" + delivery.workspaceID + "/" + delivery.fingerprint
        if let index = entries.firstIndex(where: { $0.id == id }) {
            guard delivery.acceptance != nil, !entries[index].accepted else { return false }
            entries[index].accepted = true
            return true
        }
        if entries.count == 128, let index = entries.firstIndex(where: { $0.playback == .played }) { entries.remove(at: index) }
        guard entries.count < 128 else { throw MobileError.unavailable }
        entries.append(Entry(id: id, machineID: machineID, workspaceID: delivery.workspaceID, name: name, sequence: delivery.sequence,
            outcome: delivery.outcome.state.rawValue, available: delivery.available, accepted: delivery.acceptance != nil,
            playback: .waiting, receivedAt: now))
        return true
    }
    public mutating func begin(machineID: String, workspaceID: String, now: Date, speaking: Bool, paused: Bool) -> Batch? {
        guard !speaking, !paused, !entries.contains(where: { $0.playback == .speaking }) else { return nil }
        let waiting = entries.filter { $0.machineID == machineID && $0.workspaceID == workspaceID && $0.playback == .waiting }
        // Five seconds from the first result batches nearby completions without postponing indefinitely.
        guard let first = waiting.first, now.timeIntervalSince(first.receivedAt) >= 5 else { return nil }
        let selected = Array(waiting.prefix(6))
        var phrases: [String] = []
        let completed = selected.filter { $0.outcome == "completed" }.count
        let failed = selected.filter { $0.outcome == "failed" }.count
        let stopped = selected.filter { $0.outcome == "interrupted" }.count
        let review = selected.filter { $0.outcome == "completed" && $0.available && !$0.accepted }.count
        if completed > 0 { phrases.append("\(completed)项执行完成") }
        if failed > 0 { phrases.append("\(failed)项执行失败") }
        if stopped > 0 { phrases.append("\(stopped)项已停止") }
        if review > 0 { phrases.append("\(review)项待你验收") }
        let batch = Batch(ids: selected.map(\.id), text: phrases.joined(separator: "，") + "。详情在成果记录中。")
        for index in entries.indices where batch.ids.contains(entries[index].id) { entries[index].playback = .speaking }
        return batch
    }
    public mutating func interrupt(_ batch: Batch) { finish(batch, as: .waiting) }
    public mutating func played(_ batch: Batch) { finish(batch, as: .played) }
    public mutating func unconfirmed(_ batch: Batch) { finish(batch, as: .unconfirmed) }
    private mutating func finish(_ batch: Batch, as state: Playback) {
        for index in entries.indices where batch.ids.contains(entries[index].id) && entries[index].playback == .speaking {
            entries[index].playback = state
        }
    }
    public mutating func restoreInterruptedPlayback() {
        for index in entries.indices where entries[index].playback == .speaking { entries[index].playback = .unconfirmed }
    }
    public mutating func resumeUnconfirmed() {
        for index in entries.indices where entries[index].playback == .unconfirmed { entries[index].playback = .waiting }
    }
    func validate() throws {
        guard entries.count <= 128, Set(entries.map(\.id)).count == entries.count else { throw MobileError.invalidResponse }
        for entry in entries {
            guard entry.machineID.hasPrefix("mac-"), validHex(String(entry.machineID.dropFirst(4))),
                  validID(entry.workspaceID), entry.name.utf8.count <= 1024, entry.receivedAt.timeIntervalSince1970.isFinite,
                  entry.sequence > 0, entry.sequence <= Int64.max,
                  ["completed", "failed", "interrupted"].contains(entry.outcome),
                  entry.id.hasPrefix(entry.machineID + "/" + entry.workspaceID + "/"),
                  validHex(String(entry.id.split(separator: "/").last ?? "")) else { throw MobileError.invalidResponse }
        }
    }
}

public actor RealtimeReportStore {
    private let url: URL
    public init(url: URL) { self.url = url }
    public func load() throws -> RealtimeReportQueue {
        guard FileManager.default.fileExists(atPath: url.path) else { return RealtimeReportQueue() }
        let data = try Data(contentsOf: url)
        guard data.count <= 1024 * 1024 else { throw MobileError.invalidResponse }
        let queue = try JSONDecoder().decode(RealtimeReportQueue.self, from: data)
        try queue.validate()
        return queue
    }
    public func save(_ queue: RealtimeReportQueue) throws {
        _ = try load()
        try queue.validate()
        let data = try JSONEncoder().encode(queue)
        guard data.count <= 1024 * 1024 else { throw MobileError.invalidResponse }
        try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        #if os(iOS)
        try data.write(to: url, options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication])
        #else
        try data.write(to: url, options: .atomic)
        try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: url.path)
        #endif
    }
}

@MainActor @Observable public final class RealtimeReportBook {
    public private(set) var queue = RealtimeReportQueue()
    public private(set) var loaded = false
    public private(set) var failure: String?
    private let store: RealtimeReportStore
    private var writer: Task<Void, Never>?
    private var revision = UUID()
    public init(store: RealtimeReportStore) { self.store = store }
    public func load() async throws {
        guard !loaded else { return }
        do {
            let restored = try await store.load()
            guard !loaded else { return }
            queue = restored; queue.restoreInterruptedPlayback(); loaded = true
        } catch { failure = "汇报记录未能读取，暂不自动播报。"; throw error }
    }
    public func observe(machineID: String, name: String, delivery: TaskDeliverySummary, now: Date = .now) throws {
        guard loaded else { throw MobileError.unavailable }
        if try queue.observe(machineID: machineID, name: name, delivery: delivery, now: now) { persist() }
    }
    public func begin(machineID: String, workspaceID: String, now: Date = .now, speaking: Bool, paused: Bool) -> RealtimeReportQueue.Batch? {
        guard loaded, failure == nil,
              let batch = queue.begin(machineID: machineID, workspaceID: workspaceID, now: now, speaking: speaking, paused: paused) else { return nil }
        persist()
        return batch
    }
    public func interrupt(_ batch: RealtimeReportQueue.Batch) { queue.interrupt(batch); persist() }
    public func played(_ batch: RealtimeReportQueue.Batch) { queue.played(batch); persist() }
    public func unconfirmed(_ batch: RealtimeReportQueue.Batch) { queue.unconfirmed(batch); persist() }
    public func resume() { queue.resumeUnconfirmed(); if loaded { persist() } }
    public func flush() async throws {
        while let writer { await writer.value }
        if failure != nil { throw MobileError.unavailable }
    }
    private func persist() {
        revision = UUID()
        guard writer == nil else { return }
        writer = Task {
            defer { writer = nil }
            while true {
                let observed = revision, snapshot = queue
                do { try await store.save(snapshot); failure = nil }
                catch { failure = "汇报记录尚未保存，暂不自动播报。"; return }
                if observed == revision { return }
            }
        }
    }
}
