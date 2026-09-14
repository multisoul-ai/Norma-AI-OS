import CryptoKit
import Foundation
import NormaCore

extension VoiceCall {
    struct DiscussionCapture {
        let target: Target?
        let recordedAt: Date
        let id: String
    }
    func beginDiscussion(_ event: [String: Any]) {
        let capture = DiscussionCapture(target: target, recordedAt: .now, id: UUID().uuidString)
        currentDiscussion = capture
        if let key = discussionKey(event) {
            // At most 64 overlapping/late utterances; already-recorded originals remain in the book.
            if discussionCaptures.count >= 64 {
                discussionCaptures = discussionCaptures.filter { key, _ in !discussions.entries.contains { $0.id == key } }
            }
            if discussionCaptures.count < 64 { discussionCaptures[key] = capture }
        }
    }
    func recordDiscussion(_ event: [String: Any]) {
        guard let text = event["transcript"] as? String, !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return }
        let intent = text.trimmingCharacters(in: .whitespacesAndNewlines.union(.punctuationCharacters))
        if ["先别汇报", "暂停汇报", "不用汇报了", "等会儿一起说"].contains(intent) { setReportsPaused(true) }
        if ["恢复汇报", "继续汇报", "现在可以汇报了"].contains(intent) { setReportsPaused(false) }
        let key = discussionKey(event)
        // Missing capture identity must not attribute late speech to a newly selected project.
        let capture = key.flatMap { discussionCaptures[$0] } ?? (key == nil ? currentDiscussion : nil)
        let target = capture?.target
        discussions.record(RealtimeDiscussion(id: key ?? capture?.id ?? UUID().uuidString,
            machineID: target?.machine.id, workspaceID: target?.project.id,
            targetName: target.map { $0.machine.name + " · " + $0.project.displayName } ?? "未绑定项目",
            text: text, recordedAt: capture?.recordedAt ?? .now))
        currentDiscussion = nil
    }
    private func discussionKey(_ event: [String: Any]) -> String? {
        guard let id = event["item_id"] as? String, !id.isEmpty, id.utf8.count <= 128 else { return nil }
        let identity = "\(discussionSession)/\(generation)/\(id)"
        return SHA256.hash(data: Data(identity.utf8)).map { String(format: "%02x", $0) }.joined()
    }
}
