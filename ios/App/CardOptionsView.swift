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
                    }.accessibilityLabel("关闭").accessibilityIdentifier("card-options-close")
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
                    }.accessibilityIdentifier("card-open-task")
                    Rectangle().fill(WorkspaceStyle.line).frame(height: 1)
                    Button(role: .destructive) { showingCommand = true } label: {
                        Label("删除卡片", systemImage: "trash").font(.footnote)
                            .frame(maxWidth: .infinity, minHeight: 60, alignment: .leading).padding(.horizontal, 18)
                    }.foregroundStyle(Color(red: 0.94, green: 0.59, blue: 0.54))
                        .disabled(deletionUnavailable != nil).opacity(deletionUnavailable == nil ? 1 : 0.45)
                        .accessibilityIdentifier("card-delete")
                }.background(WorkspaceStyle.surface, in: RoundedRectangle(cornerRadius: 24))
                    .overlay(RoundedRectangle(cornerRadius: 24).strokeBorder(WorkspaceStyle.line, lineWidth: 1))
                if let reason = deletionUnavailable {
                    Text(reason).font(.caption).foregroundStyle(WorkspaceStyle.muted).fixedSize(horizontal: false, vertical: true)
                }
                if pending != nil {
                    Button("查询操作结果") { showingCommand = true }.font(.footnote).frame(minHeight: 44)
                        .accessibilityIdentifier("card-check-result")
                }
            }.padding(20)
        }.background(WorkspaceStyle.raised).foregroundStyle(.white).buttonStyle(WorkspacePressStyle())
            .presentationDetents(typeSize.isAccessibilitySize ? [.large] : [.height(pending != nil ? 420 : deletionUnavailable == nil ? 310 : 360), .large])
            .presentationDragIndicator(.visible).presentationBackground(WorkspaceStyle.raised)
            .sheet(isPresented: $showingCommand) {
                CardCommandSheet(model: model, machine: machine, project: project, revision: revision,
                                 pending: pending, deleting: card)
            }
    }
}
