import SwiftUI
import NormaCore

struct CardCommandSheet: View {
    let model: ComputerModel
    let machine: PairedMachine
    let project: ProjectSnapshot
    let revision: UInt64
    @State var pending: PendingMobileCommand?
    var accepting: TaskDeliveryReference? = nil
    var feedbackFor: TaskDeliveryReference? = nil
    private var isFeedback: Bool { pending?.request.command.feedback != nil || feedbackFor != nil }
    private var isAcceptance: Bool { pending?.request.command.isAcceptance ?? (accepting != nil) }
    private var isDeletion: Bool { pending?.request.command.isDeletion == true }
    private var isInterrupt: Bool { pending?.request.command.isInterrupt == true }
    private var isTerminal: Bool { pending?.request.command.isTerminal == true }
    private var actionName: String { isTerminal ? "输入" : isFeedback ? "修改" : isAcceptance ? "验收" : isInterrupt ? "停止" : pending?.request.command.isMessage == true ? "发送" : isDeletion ? "删除" : "创建" }
    @Environment(\.dismiss) private var dismiss
    @Environment(\.scenePhase) private var scenePhase
    @State private var receipt: MobileCommandReceipt?
    @State private var busy = false
    @State private var error: String?
    @State private var clearing = false
    @State private var work: Task<Void, Never>?
    private var feedbackCard: CardSnapshot? { project.cards?.first { $0.id == feedbackFor?.outcome.key.nodeID } }
    private var feedbackScope: ChatDraft.Scope? {
        guard let reference = feedbackFor, let route = project.routeID else { return nil }
        let key = reference.outcome.key
        return ChatDraft.Scope(machineID: machine.id, workspaceID: project.id, routeID: route, nodeID: key.nodeID,
                               runID: key.runID, sessionID: key.sessionID, feedbackFingerprint: reference.fingerprint)
    }
    private var feedbackText: String { feedbackScope.flatMap { model.drafts.draft(for: $0)?.text } ?? "" }

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    LabeledContent("电脑", value: machine.name)
                    LabeledContent("项目", value: project.displayName)
                }.font(.subheadline)
                if let pending {
                    Section {
                        Label(pending.request.command.displayName, systemImage: "square.stack")
                        Text(status).font(.subheadline.weight(.medium))
                        if let error { Text(error).font(.caption).foregroundStyle(.orange) }
                        if busy { WorkspaceLoadingView(message: "正在核对电脑…", inline: true) }
                        Button("查询结果") { work = Task { await check() } }.disabled(busy)
                    } footer: {
                        Text("请求已保存在这台 iPhone。重新连接只查询结果，不会再次执行操作。")
                    }
                    Section {
                        if ["succeeded", "rejected"].contains(receipt?.state ?? "") {
                            Button("完成") { Task { await clear() } }.disabled(busy)
                        } else {
                            Button("已在电脑上核实，清除记录", role: .destructive) { clearing = true }.disabled(busy)
                        }
                    }
                    if let feedback = pending.request.command.feedback {
                        Section("原始修改意见") {
                            Text(feedback.text).font(.subheadline).textSelection(.enabled)
                            Text("成果 \(feedback.fingerprint.prefix(12))").font(.caption.monospaced()).foregroundStyle(.secondary)
                        }
                    }
                } else if let feedbackFor {
                    Section {
                        Label("发回原 Agent 会话", systemImage: "bubble.left.and.text.bubble.right")
                        Text("意见会开启新的任务回合；这份成果和原来的验收记录保持不变。")
                            .font(.subheadline)
                        Text("成果 \(feedbackFor.fingerprint.prefix(12))").font(.caption.monospaced()).foregroundStyle(.secondary)
                    }
                    Section {
                        TextEditor(text: Binding(get: { feedbackText }, set: { text in
                            if let scope = feedbackScope { model.drafts.update(text, for: scope, label: "修改意见 · \(project.displayName)") }
                        })).font(.subheadline).frame(minHeight: 160).disabled(busy || !model.drafts.loaded)
                            .accessibilityLabel("修改意见").accessibilityIdentifier("delivery-feedback-text")
                        if let failure = model.drafts.failure { Text(failure).font(.caption).foregroundStyle(.orange) }
                    } header: { Text("修改意见") } footer: { Text("未发送的意见保存在这台 iPhone，关闭后可继续编辑。") }
                    Section {
                        Button("发送修改意见") { work = Task { await submit() } }
                            .disabled(busy || !model.drafts.loaded || feedbackText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || feedbackText.utf8.count > 30 * 1024)
                            .frame(minHeight: 44).accessibilityIdentifier("submit-delivery-feedback")
                        if feedbackText.utf8.count > 30 * 1024 { Text("修改意见过长，请缩短后发送。").font(.caption).foregroundStyle(.orange) }
                        if busy { WorkspaceLoadingView(message: "正在发送…", inline: true) }
                        if let error { Text(error).font(.caption).foregroundStyle(.orange) }
                    }
                } else if let accepting {
                    Section {
                        Label("验收本轮成果", systemImage: "checkmark.seal")
                        Text("确认已核对这份成果。验收只记录本轮决定，后续任务需要单独验收。").font(.subheadline)
                        LabeledContent("内容指纹", value: String(accepting.fingerprint.prefix(12))).font(.caption.monospaced())
                    }
                    Section {
                        Button("确认验收") { work = Task { await submit() } }
                            .disabled(busy).frame(minHeight: 44).accessibilityIdentifier("confirm-delivery-acceptance")
                        if busy { WorkspaceLoadingView(message: "正在保存验收…", inline: true) }
                        if let error { Text(error).font(.caption).foregroundStyle(.orange) }
                    }
                } else {
                    ContentUnavailableView("没有待查询的操作", systemImage: "checkmark.circle")
                }
            }
            .scrollContentBackground(.hidden).background(.black)
            .navigationTitle(pending == nil ? (isFeedback ? "提出修改" : isAcceptance ? "验收成果" : "\(actionName)卡片") : "\(actionName)结果").navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("关闭") { dismiss() } } }
            .task(id: scenePhase) { if scenePhase == .active, pending != nil { await check() } }
            .onChange(of: scenePhase) { _, phase in if phase != .active { work?.cancel() } }
            .onDisappear { work?.cancel() }
            .confirmationDialog("确定已核实这次操作？", isPresented: $clearing, titleVisibility: .visible) {
                Button("清除手机上的记录", role: .destructive) { Task { await clear() } }
            } message: { Text("清除记录不会取消电脑上的操作。请先确认电脑上的卡片状态。") }
        }
    }

    private var status: String {
        switch receipt?.state {
        case "succeeded": isTerminal ? "已写入终端，执行结果请查看终端内容" : isFeedback ? "原 Agent 已收到修改意见" : isAcceptance ? "本轮成果已验收并保存" : isInterrupt ? interruptStatus : pending?.request.command.isMessage == true ? "Agent 已收到消息" : "卡片已\(actionName)并保存"
        case "accepted": isTerminal ? "正在核对终端输入结果" : isInterrupt ? "已请求停止，等待 Agent 确认" : pending?.request.command.isMessage == true ? "电脑已接收，等待 Agent 确认" : "电脑已接收，等待保存结果"
        case "rejected": rejectionMessage
        case "outcome_unknown": "结果尚无法确认，请在电脑上核对"
        default: "等待查询\(actionName)结果"
        }
    }
    private var interruptStatus: String {
        switch receipt?.result?.status {
        case "interrupted": "任务已停止，可以继续对话"
        case "completed": "任务已自然完成"
        case "failed": "任务以失败结束"
        default: "停止结果尚无法确认"
        }
    }
    private var rejectionMessage: String { receipt?.rejectionMessage(actionName: actionName) ?? "电脑未执行这次操作" }
    @MainActor private func submit() async {
        guard !busy, pending == nil, let route = project.routeID else { return }
        busy = true; error = nil
        defer { busy = false }
        do {
            let command: PendingMobileCommand
            if let feedbackFor, let card = feedbackCard, let scope = feedbackScope {
                try await model.drafts.flush()
                command = try await model.client.prepareDeliveryFeedback(machine, workspaceID: project.id, routeID: route, revision: revision,
                                                                         delivery: feedbackFor, card: card, text: feedbackText,
                                                                         draftID: model.drafts.draft(for: scope)?.id)
            } else if feedbackFor != nil { throw MobileError.sessionChanged }
            else if let accepting {
                command = try await model.client.prepareDeliveryAcceptance(machine, workspaceID: project.id, routeID: route, revision: revision, delivery: accepting)
            } else { throw MobileError.invalidResponse }
            try await model.commandStore.save(command)
            pending = command
            try await model.reloadCommands()
            try Task.checkCancellation()
            receipt = try await model.client.submit(command, to: machine)
            try await poll(command)
        } catch { self.error = error.localizedDescription }
    }
    @MainActor private func check() async {
        guard !busy, let pending else { return }
        busy = true; error = nil
        defer { busy = false }
        do {
            receipt = try await model.client.receipt(for: pending, on: machine)
            try await poll(pending)
        } catch { self.error = error.localizedDescription }
    }
    @MainActor private func poll(_ command: PendingMobileCommand) async throws {
        // Ten half-second intervals cover ordinary 360ms saves without an endless background poll.
        for _ in 0..<10 where receipt?.state == "accepted" {
            try await Task.sleep(for: .milliseconds(500))
            receipt = try await model.client.receipt(for: command, on: machine)
        }
        if let receipt { try await model.drafts.confirm(command, receipt: receipt) }
    }
    @MainActor private func clear() async {
        guard let pending else { return }
        busy = true
        defer { busy = false }
        do {
            if let receipt { try await model.drafts.confirm(pending, receipt: receipt) }
            try await model.commandStore.remove(id: pending.id); try await model.reloadCommands(); dismiss()
        }
        catch { self.error = error.localizedDescription }
    }
}

extension MobileCommandReceipt {
    func rejectionMessage(actionName: String) -> String {
        switch error {
        case "terminal_changed": "终端已变化，本次输入未写入。请查看最新画面后重试。"
        case "paste_unavailable": "终端暂不支持多行粘贴，本次输入未写入。"
        case "feedback_not_saved": "修改意见未能保存，尚未发给 Agent，原文仍在手机上"
        case "session_changed": "原 Agent 会话已关闭或重开，本次操作没有转发到其他会话"
        case "delivery_accepted": "这份成果已经验收，请重新读取查看原决定"
        case "delivery_changed": "成果内容与所见不一致，未执行验收"
        case "delivery_missing", "delivery_not_ready": "这份成果暂不可验收，请在电脑上核对"
        case "route_expired": "项目窗口已变化，未执行操作"
        case "card_changed": "卡片或任务已变化，请刷新后重新确认"
        case "desktop_input_pending": "电脑终端中有待处理的输入，请先处理后再发送。"
        case "state_not_ready": "Agent 正在同步或有排队任务，请稍后重试"
        case "terminal_not_ready": "电脑终端尚未准备好多行输入，请稍后重试"
        case "confirmation_required": "需要确认停止当前任务"
        case "mobile_control_required": "请先在电脑上为这张卡片启用手机控制"
        case "not_found": "卡片已经移除"
        default: "电脑未执行这次\(actionName)"
        }
    }
}
