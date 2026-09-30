import SwiftUI
import NormaCore

struct OperationRecordsView: View {
    @ObservedObject var call: VoiceCall
    @Environment(\.dismiss) private var dismiss
    var body: some View {
        NavigationStack {
            List {
                Section {
                    Text("任务已接收后仍需执行；成果是否通过验收，以成果记录为准。")
                        .font(.subheadline).foregroundStyle(.secondary)
                    if let status = call.recoveryStatus { Text(status).foregroundStyle(.orange) }
                }
                Section("汇报状态") {
                    if let status = call.reportStatus ?? call.reports.failure { Text(status).foregroundStyle(.secondary) }
                    ForEach(call.reports.queue.entries.reversed()) { entry in
                        VStack(alignment: .leading, spacing: 6) {
                            Text(entry.name).font(.caption).foregroundStyle(.secondary)
                            Text(playbackLabel(entry.playback))
                            if entry.outcome == "completed" && entry.available {
                                Text(entry.accepted ? "已通过验收" : "待你验收").font(.caption).foregroundStyle(.secondary)
                            }
                        }
                    }
                }
                if call.operationRecords.isEmpty {
                    ContentUnavailableView("还没有任务操作", systemImage: "checklist",
                        description: Text("Mac 接收语音任务后，操作回执会保存在这里。"))
                }
                ForEach(call.operationRecords.reversed()) { record in
                    VStack(alignment: .leading, spacing: 8) {
                        Text(record.targetName ?? "过去通话").font(.caption).foregroundStyle(.secondary)
                        Label(record.label, systemImage: record.unresolved ? "clock" : "checkmark.circle")
                        if let message = record.operation.result?.message {
                            Text(message).font(.subheadline).foregroundStyle(.secondary).textSelection(.enabled)
                        }
                    }.padding(.vertical, 6)
                }
            }.navigationTitle("任务操作记录").navigationBarTitleDisplayMode(.inline)
                .task { await call.loadOperations(); try? await call.reports.load() }
                .toolbar { ToolbarItem(placement: .cancellationAction) {
                    Button("完成") { dismiss() }.accessibilityIdentifier("realtime-operations-close")
                } }
        }.preferredColorScheme(.dark).tint(.white)
    }
    private func playbackLabel(_ playback: NormaCore.RealtimeReportQueue.Playback) -> String {
        switch playback {
        case .waiting: "等待汇报"
        case .speaking: "正在汇报"
        case .played: "已播放汇报"
        case .unconfirmed: "汇报播放待核对"
        }
    }
}
