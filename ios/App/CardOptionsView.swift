import SwiftUI
import NormaCore

struct CardOptionsView: View {
    let model: ComputerModel
    let machine: PairedMachine
    let project: ProjectSnapshot
    let card: CardSnapshot
    let revision: UInt64
    let canControl: Bool
    let openConversation: () -> Void
    @Environment(\.dismiss) private var dismiss
    @Environment(\.dynamicTypeSize) private var typeSize
    @State private var showingCommand = false
    @State private var confirmingDeletion = false
    @State private var deleting = false
    @State private var deletionNotice: String?
    @State private var deletionWork: Task<Void, Never>?
    @Environment(\.scenePhase) private var scenePhase

    private var pending: PendingMobileCommand? {
        model.pendingCommands.first {
            guard $0.machineID == machine.id, $0.request.workspaceID == project.id,
                  $0.request.routeID == project.routeID else { return false }
            switch $0.request.command {
            case let .deleteCard(expected, _), let .sendMessage(expected, _),
                 let .feedbackDelivery(expected, _), let .interruptTurn(expected),
                 let .terminalInput(expected, _, _, _): return expected.id == card.id
            case let .acceptDelivery(key, _): return key.nodeID == card.id
            default: return false
            }
        }
    }
    private var deletionUnavailable: String? {
        if deleting { return "正在删除卡片…" }
        if !canControl { return "当前项目为只读，请在电脑上允许控制后删除。" }
        if pending != nil { return "这张卡片有待确认的操作，请先核对结果。" }
        if !MobileAgentProfile.supports(card.profile) { return "请在电脑上管理这类卡片。" }
        if ["preparing", "unknown", "collecting", "interrupting"].contains(card.state) {
            return "卡片状态正在同步，确认后可以删除。"
        }
        return nil
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                HStack {
                    Text("卡片选项").font(.title3.weight(.semibold)).accessibilityAddTraits(.isHeader)
                    Spacer(minLength: 8)
                    Button { dismiss() } label: {
                        Image(systemName: "xmark").font(.system(size: 14)).frame(width: 44, height: 44)
                            .background(WorkspaceStyle.line.opacity(0.5), in: Circle())
                            .overlay(Circle().strokeBorder(WorkspaceStyle.line, lineWidth: 1))
                    }.disabled(deleting).accessibilityLabel("关闭").accessibilityIdentifier("card-options-close")
                }
                HStack(spacing: 14) {
                    ProviderMark(profile: card.profile, size: 30, working: card.isBusy)
                    VStack(alignment: .leading, spacing: 5) {
                        Text(card.title).font(.footnote.weight(.semibold))
                        Text("\(project.displayName) · \(AgentSnapshotCard.status(card.state))")
                            .font(.caption).foregroundStyle(WorkspaceStyle.muted)
                    }.fixedSize(horizontal: false, vertical: true)
                }
                VStack(spacing: 0) {
                    Button(action: openConversation) {
                        HStack(spacing: 14) {
                            Image(systemName: card.taskTitle == nil ? "plus" : "bubble.left.and.bubble.right").frame(width: 20)
                            VStack(alignment: .leading, spacing: 5) {
                                Text(card.taskTitle == nil ? "安排任务" : "查看任务").font(.footnote)
                                if card.taskTitle == nil {
                                    Text("给这张卡片一个目标后再开始执行").font(.caption).foregroundStyle(WorkspaceStyle.muted)
                                }
                            }.fixedSize(horizontal: false, vertical: true)
                            Spacer(minLength: 0)
                            Image(systemName: "chevron.right").font(.caption).foregroundStyle(WorkspaceStyle.muted)
                        }.padding(18).frame(maxWidth: .infinity, minHeight: 60, alignment: .leading)
                    }.disabled(deleting).accessibilityIdentifier("card-open-task")
                    Rectangle().fill(WorkspaceStyle.line).frame(height: 1)
                    Button(role: .destructive) { confirmingDeletion = true } label: {
                        Label(deleting ? "正在删除…" : "删除卡片", systemImage: "trash").font(.footnote)
                            .frame(maxWidth: .infinity, minHeight: 60, alignment: .leading).padding(.horizontal, 18)
                    }.foregroundStyle(Color(red: 0.94, green: 0.59, blue: 0.54))
                        .disabled(deletionUnavailable != nil).opacity(deletionUnavailable == nil ? 1 : 0.45)
                        .accessibilityIdentifier("card-delete")
                }.background(WorkspaceStyle.surface, in: RoundedRectangle(cornerRadius: 24))
                    .overlay(RoundedRectangle(cornerRadius: 24).strokeBorder(WorkspaceStyle.line, lineWidth: 1))
                if let reason = deletionUnavailable {
                    Text(reason).font(.caption).foregroundStyle(WorkspaceStyle.muted).fixedSize(horizontal: false, vertical: true)
                }
                if let deletionNotice {
                    Text(deletionNotice).font(.caption).foregroundStyle(.orange)
                }
                if pending != nil && !deleting {
                    Button("查询操作结果") { showingCommand = true }.font(.footnote).frame(minHeight: 44)
                        .accessibilityIdentifier("card-check-result")
                }
            }.padding(20)
        }.background(WorkspaceStyle.raised).foregroundStyle(.white).buttonStyle(WorkspacePressStyle())
            .presentationDetents(typeSize.isAccessibilitySize ? [.large] : [.height(pending != nil ? 420 : deletionUnavailable == nil ? 310 : 360), .large])
            .presentationDragIndicator(.visible).presentationBackground(WorkspaceStyle.raised)
            .interactiveDismissDisabled(deleting)
            .confirmationDialog("删除“\(card.title)”？", isPresented: $confirmingDeletion, titleVisibility: .visible) {
                Button(card.isBusy ? "停止并删除卡片" : "删除卡片", role: .destructive) {
                    deletionWork = Task { await deleteCard() }
                }.accessibilityIdentifier("confirm-delete-card")
                Button("取消", role: .cancel) {}
            } message: {
                Text(card.isBusy ? "会停止当前任务。已有任务记录和对话保留。" : "已有任务记录和对话保留。")
            }
            .onChange(of: scenePhase) { _, phase in if phase != .active { deletionWork?.cancel() } }
            .sheet(isPresented: $showingCommand) {
                CardCommandSheet(model: model, machine: machine, project: project, revision: revision,
                                 pending: pending)
            }
    }

    @MainActor private func deleteCard() async {
        guard deletionUnavailable == nil, scenePhase == .active, let route = project.routeID else { return }
        deleting = true; deletionNotice = nil
        defer { deleting = false }
        do {
            let command = try await model.client.prepareCardDeletion(machine, workspaceID: project.id,
                routeID: route, revision: revision, card: card, confirmBusy: card.isBusy)
            try Task.checkCancellation()
            try await model.commandStore.save(command)
            try await model.reloadCommands()
            try Task.checkCancellation()
            var receipt = try await model.client.submit(command, to: machine)
            // 与现有命令查询一致：10 × 500ms = 5s，覆盖通常360ms的持久化等待。
            for _ in 0..<10 where receipt.state == "accepted" {
                try await Task.sleep(for: .milliseconds(500))
                receipt = try await model.client.receipt(for: command, on: machine)
            }
            if receipt.state == "succeeded" {
                // 列表可能先收到卡片移除事件；仍需完成已知成功请求的本地清理。
                try await model.commandStore.remove(id: command.id)
                try await model.reloadCommands()
                dismiss()
            } else {
                deletionNotice = receipt.state == "rejected"
                    ? receipt.rejectionMessage(actionName: "删除") : "删除结果待确认，请查询操作结果。"
            }
        } catch { deletionNotice = error.localizedDescription }
    }
}
