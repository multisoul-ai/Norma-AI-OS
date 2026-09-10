import SwiftUI
import NormaCore

struct ProjectReviewView: View {
    let machine: PairedMachine
    let workspaceID: String
    let routeID: String
    let model: ComputerModel
    var delivery: TaskDeliverySummary? = nil
    var onDeliveryLoaded: ((TaskDeliveryReference) -> Void)? = nil
    @Environment(\.scenePhase) private var scenePhase
    @State private var response: ProjectReviewResponse?
    @State private var checkedAt: Date?
    @State private var error: String?
    @State private var loading = false
    @State private var retry = 0
    @State private var selectedFile: String?
    @State private var project: ProjectSnapshot?
    @State private var grantRevision: UInt64?
    @State private var control = false
    @State private var acceptanceSelection: AcceptanceSelection?
    private struct AcceptanceSelection: Identifiable {
        let project: ProjectSnapshot
        let revision: UInt64
        let delivery: TaskDeliveryReference
        let pending: PendingMobileCommand?
        var feedback = false
        var id: String { delivery.fingerprint }
    }
    private var canAccept: Bool {
        guard scenePhase == .active, control, let result = response, let delivery = result.delivery,
              delivery.outcome.state == .completed, delivery.acceptance == nil, !result.review.conflicted else { return false }
        return project?.cards?.first(where: { $0.id == delivery.outcome.key.nodeID })
            .map { ["idle", "ready", "done", "ended", "failed"].contains($0.state) } ?? true
    }
    private var feedbackTarget: CardSnapshot? {
        guard scenePhase == .active, control, let key = response?.delivery?.outcome.key else { return nil }
        return project?.cards?.first {
            $0.id == key.nodeID && $0.profile == key.profile && $0.runID == key.runID && $0.sessionID == key.sessionID
                && $0.mobileControl && ["ready", "done", "ended", "failed"].contains($0.state)
        }
    }
    private func pending(for reference: TaskDeliveryReference, feedback: Bool = false) -> PendingMobileCommand? {
        model.pendingCommands.first {
            guard $0.machineID == machine.id, $0.request.workspaceID == workspaceID else { return false }
            if feedback {
                return $0.request.command.feedback?.key == reference.outcome.key && $0.request.command.feedback?.fingerprint == reference.fingerprint
            }
            guard
                  case let .acceptDelivery(key, fingerprint) = $0.request.command else { return false }
            return key == reference.outcome.key && fingerprint == reference.fingerprint
        }
    }

    var body: some View {
        List {
            Section {
                Label(machine.name, systemImage: "laptopcomputer").font(.caption).foregroundStyle(.secondary)
                Text(delivery == nil ? "项目当前未提交变更" : "本轮任务成果").font(.subheadline.weight(.semibold))
                Text(delivery == nil ? "所有 Agent 共用此目录。这里对比 HEAD、暂存区与工作副本；已提交的内容不在此列表中。" : "对比任务起点与结束时保存的项目内容，包含期间已提交的变更。项目由多个 Agent 共用，文件变化不代表作者归属。")
                    .font(.caption).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                if let delivery {
                    Label(delivery.stateLabel, systemImage: "doc.text").font(.caption)
                    Text("保存于 \(Date(timeIntervalSince1970: TimeInterval(delivery.savedAtUnix)).formatted(date: .abbreviated, time: .shortened))")
                        .font(.caption).foregroundStyle(.secondary)
                }
                if delivery == nil, let checkedAt {
                    HStack { Text("本次预览"); Spacer(); Text(checkedAt, style: .time) }
                        .font(.caption).foregroundStyle(.secondary)
                }
            }
            if loading { ProgressView("正在读取完整变更…").font(.subheadline) }
            if let error {
                Section {
                    Text(error).font(.subheadline).foregroundStyle(.orange)
                    Button("重新读取") { retry += 1 }.frame(minHeight: 44)
                }
            }
            if let reference = response?.delivery {
                Section {
                    if let accepted = reference.acceptance {
                        Label("本轮成果已验收", systemImage: "checkmark.seal.fill").font(.subheadline)
                        Text(Date(timeIntervalSince1970: TimeInterval(accepted.acceptedAtUnix)), format: .dateTime)
                            .font(.caption).foregroundStyle(.secondary)
                    }
                    if reference.acceptance == nil || pending(for: reference) != nil {
                        Button(pending(for: reference) == nil ? "验收这份成果" : "查看验收结果") {
                            guard let project, let grantRevision else { return }
                            acceptanceSelection = AcceptanceSelection(project: project, revision: grantRevision, delivery: reference, pending: pending(for: reference))
                        }.frame(minHeight: 44)
                            .disabled(pending(for: reference) == nil && !canAccept)
                            .accessibilityIdentifier("accept-delivery")
                    }
                    if reference.acceptance == nil && !canAccept {
                        Text("需要控制权限、可用的已完成成果，且卡片当前没有执行中的任务。").font(.caption).foregroundStyle(.secondary)
                    }
                    Button(pending(for: reference, feedback: true) == nil ? "提出修改" : "查看修改结果") {
                        guard let project, let grantRevision else { return }
                        acceptanceSelection = AcceptanceSelection(project: project, revision: grantRevision, delivery: reference,
                                                                  pending: pending(for: reference, feedback: true), feedback: true)
                    }.frame(minHeight: 44)
                        .disabled(pending(for: reference, feedback: true) == nil && feedbackTarget == nil)
                        .accessibilityIdentifier("feedback-delivery")
                    if feedbackTarget == nil {
                        Text("原 Agent 会话空闲且具备控制权限时，可以提出修改。重开后的会话不会接收这份反馈。")
                            .font(.caption).foregroundStyle(.secondary)
                    }
                }
            }
            if let review = response?.review {
                if review.conflicted {
                    Label("存在合并冲突，请先让 Agent 处理。", systemImage: "exclamationmark.triangle")
                        .font(.footnote).foregroundStyle(.orange)
                }
                Section {
                    if review.files.isEmpty {
                        ContentUnavailableView(delivery == nil ? "没有未提交变更" : "本轮没有文件变更", systemImage: "doc.text.magnifyingglass", description: Text("这不代表任务已经验收。"))
                    }
                    ForEach(review.files) { file in
                        Button { selectedFile = file.id } label: {
                            HStack(spacing: 12) {
                                Image(systemName: file.isConflicted ? "exclamationmark.triangle" : "doc.text")
                                    .foregroundStyle(file.isConflicted ? .orange : .secondary)
                                Text(file.path).font(.subheadline).foregroundStyle(.primary)
                                    .fixedSize(horizontal: false, vertical: true)
                                Spacer(minLength: 4)
                                Text(file.changeLabel).font(.caption).foregroundStyle(.secondary)
                                Image(systemName: "chevron.right").font(.caption2).foregroundStyle(.tertiary)
                            }.frame(minHeight: 44)
                        }.accessibilityIdentifier("review-file-\(file.id)")
                    }
                } header: { Text("\(review.files.count) 个文件") }
                footer: { Text(delivery == nil ? "预览不会提交代码或执行验收。项目仍可能变化，可随时重新读取。" : "这份记录已固定。查看成果不会提交代码或执行验收。") }
            }
        }
        .scrollContentBackground(.hidden).background(.black)
        .navigationTitle(delivery == nil ? "项目变更" : "任务成果").navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                Button { retry += 1 } label: { Image(systemName: "arrow.clockwise") }
                    .disabled(loading || scenePhase != .active).accessibilityLabel("重新读取变更")
            }
        }
        .sheet(item: $acceptanceSelection, onDismiss: { retry += 1 }) { selection in
            CardCommandSheet(model: model, machine: machine, project: selection.project, revision: selection.revision,
                             pending: selection.pending, accepting: selection.feedback ? nil : selection.delivery,
                             feedbackFor: selection.feedback ? selection.delivery : nil)
        }
        .navigationDestination(item: $selectedFile) { id in
            if let file = response?.review.files.first(where: { $0.id == id }) { ReviewFileView(file: file, baselineName: delivery == nil ? "HEAD" : "任务起点", resultName: delivery == nil ? "工作副本" : "本次交付") }
        }
        .task(id: "\(scenePhase)-\(retry)") {
            response = nil; checkedAt = nil; selectedFile = nil; error = nil; loading = false
            control = false; project = nil; grantRevision = nil
            guard scenePhase == .active else { return }
            loading = true
            var attempted = false
            var revision: UInt64?
            do {
                for try await update in model.client.projectUpdates(machine, workspaceID: workspaceID) {
                    try Task.checkCancellation()
                    switch update {
                    case .recovering:
                        response = nil; selectedFile = nil; checkedAt = nil; control = false; project = nil; grantRevision = nil
                        loading = false; error = "连接已中断，重新读取后可继续核对。"
                    case .current(let snapshot):
                        guard snapshot.projects.first?.state == "open", snapshot.projects.first?.routeID == routeID else { throw MobileError.sessionChanged }
                        if let revision, snapshot.grant.revision != revision { throw MobileError.grantChanged }
                        revision = snapshot.grant.revision
                        grantRevision = snapshot.grant.revision; project = snapshot.projects.first
                        control = snapshot.grant.projects.contains { $0.id == workspaceID && $0.access == "control" }
                        guard !attempted else { continue }; attempted = true
                        let result = try await model.client.projectReview(machine, request: ProjectReviewRequest(workspaceID: workspaceID, routeID: routeID, deliveryKey: delivery?.outcome.key))
                        try Task.checkCancellation()
                        if let delivery, result.delivery?.fingerprint != delivery.fingerprint { throw MobileError.invalidResponse }
                        response = result; checkedAt = .now; loading = false; error = nil
                        if let reference = result.delivery { onDeliveryLoaded?(reference) }
                    }
                }
            } catch {
                guard !Task.isCancelled else { return }
                response = nil; selectedFile = nil; checkedAt = nil; loading = false
                control = false; project = nil; grantRevision = nil
                self.error = error.localizedDescription
            }
        }
    }
}
