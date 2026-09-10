import SwiftUI
import NormaCore

struct ApprovalContext: View {
    let snapshot: ApprovalSnapshot
    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            ApprovalSummary(snapshot: snapshot)
            ApprovalInput(snapshot: snapshot)
        }
    }
}

struct ApprovalSummary: View {
    let snapshot: ApprovalSnapshot
    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            HStack(spacing: 12) {
                Image(snapshot.target.profile).resizable().scaledToFit().frame(width: 26, height: 26).accessibilityHidden(true)
                VStack(alignment: .leading, spacing: 3) {
                    Text(snapshot.cardTitle).font(.subheadline.weight(.semibold))
                    Text(snapshot.target.profile).font(.caption).foregroundStyle(.secondary)
                }
            }
            Text(snapshot.requiresDesktop ? "需要在电脑上处理" : snapshot.tool == "Bash" || snapshot.tool == "item/commandExecution/requestApproval" ? "请求运行命令" : snapshot.tool)
                .font(.headline)
        }
    }
}

struct ApprovalInput: View {
    let snapshot: ApprovalSnapshot
    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            if let content = try? snapshot.details.formatted() {
                Text(content).font(.caption.monospaced()).textSelection(.enabled)
                    .accessibilityIdentifier("approval-tool-input")
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(12).background(.black.opacity(0.5), in: RoundedRectangle(cornerRadius: 12))
            } else { Text("无法完整显示工具输入，请在电脑上处理。").foregroundStyle(.orange) }
            DisclosureGroup {
                VStack(alignment: .leading, spacing: 6) {
                    Text("工具：" + snapshot.tool)
                    Text("会话：" + snapshot.target.sessionID)
                    Text("回合：" + snapshot.target.turnID)
                    Text("请求：" + snapshot.target.requestID)
                }.font(.caption.monospaced()).textSelection(.enabled).frame(maxWidth: .infinity, alignment: .leading)
            } label: { Text("请求详情").frame(minHeight: 44) }
            .font(.caption).foregroundStyle(.secondary)
        }
    }
}

struct ApprovalRecordsView: View {
    let model: ComputerModel
    @Environment(\.dismiss) private var dismiss
    private var commands: [PendingMobileCommand] { model.pendingCommands.filter { $0.request.command.isApproval } }
    var body: some View {
        NavigationStack {
            List {
                if commands.isEmpty { ContentUnavailableView("没有决定记录", systemImage: "clock.arrow.circlepath") }
                ForEach(commands) { command in
                    NavigationLink { ApprovalRecordView(model: model, command: command) } label: {
                        VStack(alignment: .leading, spacing: 5) {
                            Text(command.approvalPreview?.cardTitle ?? "Agent 请求").font(.subheadline.weight(.medium))
                            Text(model.machines.first { $0.id == command.machineID }?.name ?? "已移除的电脑").font(.caption).foregroundStyle(.secondary)
                            if case .decideApproval(_, let decision) = command.request.command {
                                Text(decision == .allow ? "选择了批准 · 查看回执" : "选择了拒绝 · 查看回执").font(.caption).foregroundStyle(.secondary)
                            }
                        }.padding(.vertical, 6)
                    }
                }
            }
            .scrollContentBackground(.hidden).background(.black)
            .navigationTitle("决定记录").navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("关闭") { dismiss() }.accessibilityIdentifier("approval-records-close") } }
        }
    }
}

struct ApprovalRecordView: View {
    let model: ComputerModel
    let command: PendingMobileCommand
    @Environment(\.dismiss) private var dismiss
    @Environment(\.scenePhase) private var scenePhase
    @State private var receipt: MobileCommandReceipt?
    @State private var error: String?
    @State private var busy = false
    @State private var clearing = false
    private var machine: PairedMachine? { model.machines.first { $0.id == command.machineID } }

    var body: some View {
        Form {
            Section {
                LabeledContent("电脑", value: machine?.name ?? "已移除的电脑")
                if case .decideApproval(_, let decision) = command.request.command {
                    LabeledContent("我的选择", value: decision == .allow ? "批准本次" : "拒绝本次")
                }
                Text(status).fontWeight(.medium).accessibilityIdentifier("approval-status")
                if let error { Text(error).font(.caption).foregroundStyle(.orange) }
                if busy { ProgressView("正在查询回执…") }
                Button("查询决定结果") { Task { await check() } }.disabled(busy || machine == nil)
            } footer: { Text("重新连接只查询原操作。提交决定与工具执行完成是不同的状态。") }
            if let snapshot = command.approvalPreview {
                Section("当时的请求内容") { ApprovalContext(snapshot: snapshot) }
            }
            Section {
                DisclosureGroup("来源标识") {
                    Text("电脑：" + command.machineID)
                    Text("项目：" + command.request.workspaceID)
                    Text("窗口：" + command.request.routeID)
                    Text("操作：" + command.request.operationID)
                }.font(.caption.monospaced()).textSelection(.enabled)
                Button("清除本机记录", role: .destructive) { clearing = true }.disabled(busy)
            }
        }
        .font(.subheadline).scrollContentBackground(.hidden).background(.black)
        .navigationTitle("决定结果").navigationBarTitleDisplayMode(.inline)
        .confirmationDialog("清除这次决定的本机记录？", isPresented: $clearing, titleVisibility: .visible) {
            Button("已核对，清除记录", role: .destructive) { Task { await clear() } }
        } message: { Text("清除不会撤回电脑上的决定。结果未确认时，请先在电脑核对。") }
        .task(id: scenePhase) { if scenePhase == .active { await check() } }
    }

    private var status: String {
        switch receipt?.state {
        case "accepted": "电脑已接收，等待提交决定"
        case "succeeded": receipt?.result?.status == "submitted" ? "决定已提交，执行结果请查看任务" : "请求已关闭，无法确认本次决定是否被采用"
        case "rejected": rejection
        case "outcome_unknown": "决定结果未知，请在电脑核对"
        default: "已保存选择，等待查询决定结果"
        }
    }
    private var rejection: String {
        switch receipt?.error {
        case "approval_changed": "请求已变化或已被处理，本次未提交"
        case "route_expired": "项目窗口已关闭或变化，本次未提交"
        case "grant_changed", "unauthorized": "项目授权已变化，请重新核验电脑"
        case "decision_unavailable": "这项请求没有提供该选项，请刷新事项"
        default: "电脑未执行这次决定，请刷新事项并核对授权"
        }
    }
    @MainActor private func check() async {
        guard !busy, let machine else { return }
        busy = true; error = nil
        defer { busy = false }
        do {
            receipt = try await model.client.receipt(for: command, on: machine)
            // 30 × 1s matches the server's existing decision deadline. Only query, never submit.
            for _ in 0..<30 where receipt?.state == "accepted" {
                try await Task.sleep(for: .seconds(1))
                receipt = try await model.client.receipt(for: command, on: machine)
            }
        } catch { self.error = error.localizedDescription }
    }
    @MainActor private func clear() async {
        busy = true
        defer { busy = false }
        do {
            try await model.commandStore.remove(id: command.id)
            try await model.reloadCommands(); dismiss()
        } catch { self.error = error.localizedDescription }
    }
}
