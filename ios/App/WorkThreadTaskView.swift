import SwiftUI
import NormaCore

struct TaskPresentation {
    let operation: ThreadTaskOutboxOperation
    let workspace: CollaborationWorkspace?
    var agent: String {
        operation.target?.name ?? workspace?.agents.first {
            $0.machineID == operation.machineID && $0.projectID == operation.task.projectID && $0.definitionID == operation.task.agentDefinitionID
        }?.name ?? "Agent"
    }
    var project: String {
        operation.projectName ?? workspace?.projects.first {
            $0.machineID == operation.machineID && $0.projectID == operation.task.projectID
        }?.name ?? "原关联项目"
    }
    var status: String {
        if operation.revisionOperationID != nil { return "已退回修改" }
        if let acceptance = operation.acceptance { return acceptance.statusLabel }
        return Self.status(dispatch: operation.receipt?.receipt.state, execution: operation.receipt?.execution?.execution.state)
    }
    static func status(dispatch: String?, execution: String?) -> String {
        if let execution {
            return switch execution {
            case "done": "执行完成"
            case "failed": "执行失败"
            case "cancelled", "interrupted": "已停止"
            case "cancel_requested": "正在停止"
            case "queued": "排队中"
            case "submitted": "执行中"
            case "untracked": "待核对"
            default: "Agent 已接收"
            }
        }
        return switch dispatch {
        case "accepted": "Mac 已接收"
        case "succeeded": "Agent 已接收"
        case "rejected": "派工失败"
        case "outcome_unknown": "待核对"
        default: "待发送"
        }
    }
    var symbol: String {
        if operation.revisionOperationID != nil { return "arrow.triangle.branch" }
        if let acceptance = operation.acceptance {
            if acceptance.acceptance != nil { return "checkmark.circle" }
            return switch acceptance.receipt?.state {
            case "rejected": "exclamationmark.circle"
            case "outcome_unknown": "questionmark.circle"
            default: "clock"
            }
        }
        return Self.symbol(dispatch: operation.receipt?.receipt.state, execution: operation.receipt?.execution?.execution.state)
    }
    static func symbol(dispatch: String?, execution: String?) -> String {
        if let execution {
            return switch execution {
            case "done": "checkmark.circle"
            case "failed": "exclamationmark.circle"
            case "cancelled", "interrupted": "stop.circle"
            case "untracked": "questionmark.circle"
            default: "clock"
            }
        }
        return switch dispatch {
        case "accepted", "succeeded": "checkmark.circle"
        case "rejected": "exclamationmark.circle"
        case "outcome_unknown": "questionmark.circle"
        default: "clock"
        }
    }
    var color: Color {
        if operation.revisionOperationID != nil { return IdeaPalette.blue }
        if let acceptance = operation.acceptance {
            if acceptance.acceptance != nil { return IdeaPalette.positiveText }
            return acceptance.receipt?.state == "rejected" ? .red : IdeaPalette.secondary
        }
        return Self.color(dispatch: operation.receipt?.receipt.state, execution: operation.receipt?.execution?.execution.state)
    }
    static func color(dispatch: String?, execution: String?) -> Color {
        if let execution {
            return execution == "failed" ? .red : execution == "done" ? IdeaPalette.positiveText : IdeaPalette.blue
        }
        return switch dispatch {
        case "accepted", "succeeded": IdeaPalette.positiveText
        case "rejected": .red
        default: IdeaPalette.secondary
        }
    }
    var explanation: String {
        if operation.revisionOperationID != nil { return "原成果已保留，修改意见已交给后续任务，进展继续回到原线程。" }
        if let acceptance = operation.acceptance { return acceptance.explanation }
        if let execution = operation.receipt?.execution?.execution {
            return switch execution.state {
            case "done": "Agent 已完成本次执行，可在这里回看回复与成果记录。执行完成仍需你核对。"
            case "failed": "本次执行失败，原任务与已有回复仍保留。"
            case "cancelled", "interrupted": "本次执行已停止，原任务与已有回复仍保留。"
            case "cancel_requested": "Mac 正在处理停止请求。"
            case "queued": "任务正在等待 Agent 处理。"
            case "submitted": "Agent 正在处理这项任务，结果会回到原线程。"
            case "untracked": "Mac 无法自动确认这次执行的进展，请在电脑上核对。"
            default: "Agent 已接收任务，正在等待执行进展。"
            }
        }
        return switch operation.receipt?.receipt.state {
        case "accepted": "Mac 已保存派工请求，正在等待 Agent 接收。"
        case "succeeded": "Agent 已接收任务，尚未收到执行结果。"
        case "rejected": "Mac 未能派发这项任务。原消息和任务仍保留，没有重新派工。"
        case "outcome_unknown": "暂时无法确认派工结果。原记录已保留，不会重复派工。"
        default: "任务已保存在这部 iPhone，来源消息送达 Mac 后会继续派工。"
        }
    }
}

struct WorkThreadTaskCard: View {
    let operation: ThreadTaskOutboxOperation
    let workspace: CollaborationWorkspace?
    let deliveryError: String?
    private var display: TaskPresentation { .init(operation: operation, workspace: workspace) }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 12) {
                Image(systemName: "chevron.left.forwardslash.chevron.right").font(.system(size: 20))
                    .foregroundStyle(IdeaPalette.blue).frame(width: 44, height: 44)
                    .background(IdeaPalette.blue.opacity(0.08), in: RoundedRectangle(cornerRadius: 14))
                VStack(alignment: .leading, spacing: 6) {
                    Text(display.agent).font(.headline)
                    Label(display.status, systemImage: display.symbol).font(.caption).foregroundStyle(display.color)
                }
                Spacer(minLength: 0)
                Image(systemName: "chevron.right").foregroundStyle(IdeaPalette.secondary)
            }
            Text(operation.task.task).font(.body).lineLimit(3).multilineTextAlignment(.leading)
            Text(display.project).font(.caption.weight(.medium)).foregroundStyle(IdeaPalette.blue)
                .padding(.horizontal, 12).padding(.vertical, 7)
                .background(IdeaPalette.blue.opacity(0.08), in: Capsule())
            if let failure = operation.receipt?.execution?.execution.error ?? operation.receipt?.rejectionDescription ?? deliveryError {
                Text(failure).font(.footnote).foregroundStyle(IdeaPalette.secondary)
            }
        }.threadCard().accessibilityElement(children: .combine)
    }
}

struct WorkThreadDeliveryCard: View {
    let operation: ThreadTaskOutboxOperation
    let workspace: CollaborationWorkspace?
    let open: () -> Void
    @Environment(\.dynamicTypeSize) private var typeSize
    private var display: TaskPresentation { .init(operation: operation, workspace: workspace) }

    var body: some View {
        Button(action: open) {
            HStack(alignment: .top, spacing: 12) {
                Image(systemName: "doc.richtext").font(.system(size: 22))
                    .foregroundStyle(Color(red: 0.77, green: 0.17, blue: 0.38)).frame(width: 44, height: 44)
                    .background(Color.pink.opacity(0.08), in: RoundedRectangle(cornerRadius: 14))
                VStack(alignment: .leading, spacing: 8) {
                    Text("成果 · \(display.project)").font(.headline)
                    Text("\(display.agent) · \(operation.receipt?.execution?.execution.delivery?.fileCount ?? 0) 个文件")
                        .font(.subheadline).foregroundStyle(IdeaPalette.secondary)
                    let layout = typeSize.isAccessibilitySize ? AnyLayout(VStackLayout(alignment: .leading, spacing: 8))
                        : AnyLayout(HStackLayout(spacing: 12))
                    layout {
                        Text(WorkThreadReviewModel.status(for: operation)).font(.subheadline.weight(.medium))
                            .foregroundStyle(operation.acceptance?.acceptance == nil ? IdeaPalette.secondary : IdeaPalette.positiveText)
                        Text("查看成果").font(.subheadline.weight(.medium)).foregroundStyle(IdeaPalette.blue)
                            .padding(.horizontal, 12).padding(.vertical, 6)
                            .background(IdeaPalette.blue.opacity(0.08), in: Capsule())
                    }
                }.frame(maxWidth: .infinity, alignment: .leading)
                Image(systemName: "chevron.right").font(.caption).foregroundStyle(IdeaPalette.secondary).padding(.top, 14)
            }.multilineTextAlignment(.leading).threadCard()
                .accessibilityElement(children: .combine)
        }.accessibilityIdentifier("thread-delivery-\(operation.id)")
    }
}

struct WorkThreadTaskDetail: View {
    let operation: ThreadTaskOutboxOperation
    let workspace: CollaborationWorkspace?
    let deliveryError: String?
    @Environment(WorkspaceLibraryModel.self) private var library
    @Environment(\.dynamicTypeSize) private var typeSize
    @State private var current: ThreadTaskOutboxOperation?
    @State private var terminalMode = false
    @State private var terminalKeyboard = false
    private var display: TaskPresentation { .init(operation: current ?? operation, workspace: workspace) }

    var body: some View {
        let operation = current ?? self.operation
        ZStack {
                ScrollView {
                    VStack(alignment: .leading, spacing: 20) {
                        taskHeader
                        modePicker
                VStack(alignment: .leading, spacing: 12) {
                    Text(display.explanation).font(.subheadline).foregroundStyle(IdeaPalette.secondary)
                    if let failure = operation.receipt?.execution?.execution.error ?? operation.receipt?.rejectionDescription ?? deliveryError {
                        Text(failure).font(.subheadline).foregroundStyle(.red)
                    }
                }.threadCard()
                progressTimeline(operation.progress ?? [])
                if let execution = operation.receipt?.execution?.execution {
                    if let text = execution.result?.text, !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                        VStack(alignment: .leading, spacing: 12) {
                            Label("最新进展", systemImage: "text.bubble").font(.headline)
                                .accessibilityIdentifier("task-latest-progress")
                            Text(display.agent).font(.caption).foregroundStyle(IdeaPalette.secondary)
                            Text(text).font(.body).textSelection(.enabled).accessibilityIdentifier("thread-task-result-text")
                            if execution.result?.truncated == true {
                                Text("这里只展示部分回复，完整内容保留在 Mac。")
                                    .font(.footnote).foregroundStyle(IdeaPalette.secondary)
                            }
                        }.threadCard()
                    }
                    if let delivery = execution.delivery {
                        VStack(alignment: .leading, spacing: 12) {
                            Label("本轮成果", systemImage: "doc.text").font(.headline)
                            Text(delivery.available ? "已保存 · \(delivery.fileCount) 个文件" : "未保存文件对比")
                                .font(.subheadline).accessibilityIdentifier("thread-task-delivery-summary")
                            Text(delivery.explanation).font(.subheadline).foregroundStyle(IdeaPalette.secondary)
                            if delivery.available {
                                NavigationLink {
                                    WorkThreadReviewView(operation: operation, agentName: display.agent, projectName: display.project)
                                } label: {
                                    Label("查看文件与变更", systemImage: "doc.text.magnifyingglass")
                                        .frame(minHeight: 44)
                                }.accessibilityIdentifier("thread-task-open-review")
                            }
                        }.threadCard()
                    } else if execution.terminal {
                        Text("尚未收到本轮成果记录，正在等待同步。")
                            .font(.footnote).foregroundStyle(IdeaPalette.secondary)
                    }
                }
                Label("发送时的线程目标、共同事实和最近讨论随任务一起保存。后续编辑不会改写这次派工。", systemImage: "text.bubble")
                    .font(.footnote).foregroundStyle(IdeaPalette.secondary)
                DisclosureGroup("完整指令") {
                    Text(operation.task.task).font(.body).frame(maxWidth: .infinity, alignment: .leading)
                        .padding(.top, 12).textSelection(.enabled)
                }.threadCard().accessibilityIdentifier("task-full-instruction")
                DisclosureGroup("本次共享内容") {
                    VStack(alignment: .leading, spacing: 16) {
                        if let context = try? JSONDecoder().decode(ThreadTaskContext.self, from: Data(operation.task.context.utf8)) {
                            Text(context.title).font(.headline)
                            if let source = context.sourceIdea {
                                Text("来源想法").font(.subheadline.bold())
                                Text(source)
                            }
                            ForEach(context.facts) { fact in
                                VStack(alignment: .leading, spacing: 6) {
                                    Text(fact.kind == .goal ? "线程目标" : fact.kind == .decision ? "共同决定" : "共同事实")
                                        .font(.subheadline.bold())
                                    Text(fact.text)
                                }
                            }
                            if !context.recentMessages.isEmpty {
                                Text("最近讨论").font(.subheadline.bold())
                                ForEach(context.recentMessages) { message in Text(message.text) }
                            }
                            if context.facts.isEmpty && context.recentMessages.isEmpty && context.sourceIdea == nil {
                                Text("本次只有线程标题和你发出的指令。")
                            }
                            if context.omittedMessages > 0 {
                                Text("另有 \(context.omittedMessages) 条较早消息未随本次任务发送，仍保留在线程中。")
                                    .font(.footnote).foregroundStyle(IdeaPalette.secondary)
                            }
                        } else { Text(operation.task.context.isEmpty ? "本次未附加其他内容。" : operation.task.context) }
                    }.frame(maxWidth: .infinity, alignment: .leading).padding(.top, 12).textSelection(.enabled)
                }.threadCard().accessibilityIdentifier("thread-task-context")
                    }.padding(20)
                }.opacity(terminalMode ? 0 : 1).allowsHitTesting(!terminalMode).accessibilityHidden(terminalMode)
            if terminalMode {
                VStack(spacing: 0) {
                    if !typeSize.isAccessibilitySize && !terminalKeyboard { taskHeader.padding(.horizontal, 20).padding(.top, 8) }
                    if !(typeSize.isAccessibilitySize && terminalKeyboard) {
                        modePicker.padding(.horizontal, 16).padding(.vertical, 6)
                    }
                    WorkThreadTerminalView(operation: operation, agent: display.agent, project: display.project,
                        machine: workspace?.projects.first { $0.machineID == operation.machineID && $0.projectID == operation.task.projectID }?.machineName ?? "原 Mac",
                        keyboardActive: $terminalKeyboard)
                }
            }
        }.background(IdeaPalette.background).foregroundStyle(IdeaPalette.ink)
            .navigationTitle(terminalMode ? operation.task.task : "任务详情").navigationBarTitleDisplayMode(.inline)
            .toolbar {
                if terminalMode && typeSize.isAccessibilitySize && terminalKeyboard {
                    ToolbarItem(placement: .topBarTrailing) {
                        Button { terminalMode = false } label: { Image(systemName: "doc.text").frame(width: 44, height: 44) }
                            .accessibilityLabel("工作情况").accessibilityIdentifier("task-mode-progress")
                    }
                }
            }
            .safeAreaInset(edge: .bottom, spacing: 0) {
                if !terminalMode { WorkThreadTaskActions(operation: operation, archived: workspace?.archived != false) }
            }
            .task(id: library.deliveryRevision) { current = try? await library.store.threadTask(operationID: operation.id) }
    }

    private var modePicker: some View {
        HStack(spacing: 4) {
            modeButton("工作情况", symbol: "doc.text", terminal: false)
            modeButton("终端", symbol: "terminal", terminal: true)
        }.padding(4).background(IdeaPalette.faint.opacity(0.16), in: RoundedRectangle(cornerRadius: 16))
    }
    private func modeButton(_ title: String, symbol: String, terminal: Bool) -> some View {
        Button { terminalMode = terminal } label: {
            Group {
                if typeSize.isAccessibilitySize { Text(terminal ? "终端" : "进展") }
                else { Label(title, systemImage: symbol) }
            }.font(.subheadline.weight(.semibold)).accessibilityLabel(title)
                .frame(maxWidth: .infinity, minHeight: 44)
                .background(terminalMode == terminal ? Color.white : Color.clear, in: RoundedRectangle(cornerRadius: 12))
        }.foregroundStyle(terminalMode == terminal ? IdeaPalette.blue : IdeaPalette.secondary)
            .accessibilityAddTraits(terminalMode == terminal ? .isSelected : [])
            .accessibilityIdentifier(terminal ? "task-mode-terminal" : "task-mode-progress")
    }
    private var taskHeader: some View {
        let operation = current ?? self.operation
        return VStack(alignment: .leading, spacing: 12) {
                    Text(operation.task.task).font(.title2.bold()).lineLimit(3)
                        .accessibilityIdentifier("task-detail-heading")
                    HStack(alignment: .top, spacing: 12) {
                        Image(systemName: "chevron.left.forwardslash.chevron.right")
                            .font(.system(size: 20, weight: .semibold)).foregroundStyle(.white)
                            .frame(width: 44, height: 44).background(IdeaPalette.blue, in: RoundedRectangle(cornerRadius: 12))
                            .accessibilityHidden(true)
                        let layout = typeSize.isAccessibilitySize ? AnyLayout(VStackLayout(alignment: .leading, spacing: 8))
                            : AnyLayout(HStackLayout(spacing: 8))
                        layout {
                            Text(display.agent).font(.headline)
                            Label(display.status, systemImage: display.symbol)
                                .font(.subheadline.weight(.medium)).foregroundStyle(display.color)
                                .accessibilityIdentifier("thread-task-status")
                        }.frame(maxWidth: .infinity, alignment: .leading)
                    }
                    Label(display.project, systemImage: "folder")
                        .font(.subheadline.weight(.medium)).foregroundStyle(IdeaPalette.blue)
                        .padding(.horizontal, 12).padding(.vertical, 8)
                        .background(IdeaPalette.blue.opacity(0.08), in: RoundedRectangle(cornerRadius: 12))
                    Text(workspace?.projects.first {
                        $0.machineID == operation.machineID && $0.projectID == operation.task.projectID
                    }?.machineName ?? "原 Mac")
                        .font(.caption).foregroundStyle(IdeaPalette.secondary).accessibilityIdentifier("task-detail-machine")
                }
    }

    private func progressTimeline(_ progress: [ThreadTaskProgress]) -> some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("工作情况").font(.headline).accessibilityAddTraits(.isHeader)
            if progress.isEmpty {
                Text("尚无已保存的进展记录，请以上方当前状态为准。")
                    .font(.subheadline).foregroundStyle(IdeaPalette.secondary)
            } else {
                VStack(alignment: .leading, spacing: 0) {
                    ForEach(progress) { item in
                        let color = TaskPresentation.color(dispatch: item.dispatchState, execution: item.executionState)
                        HStack(alignment: .top, spacing: 14) {
                            VStack(spacing: 4) {
                                Image(systemName: TaskPresentation.symbol(dispatch: item.dispatchState, execution: item.executionState))
                                    .font(.system(size: 22, weight: .medium)).foregroundStyle(color)
                                    .frame(width: 28, height: 28)
                                if item.id != progress.last?.id {
                                    Rectangle().fill(IdeaPalette.faint.opacity(0.4)).frame(width: 2).frame(maxHeight: .infinity)
                                }
                            }.accessibilityHidden(true)
                            VStack(alignment: .leading, spacing: 6) {
                                Text(TaskPresentation.status(dispatch: item.dispatchState, execution: item.executionState))
                                    .font(.headline)
                                Text(item.observedAt, format: .dateTime.month().day().hour().minute())
                                    .font(.caption).foregroundStyle(IdeaPalette.secondary)
                                if let error = item.error, !error.isEmpty {
                                    Text(error).font(.subheadline).foregroundStyle(IdeaPalette.secondary)
                                }
                            }.frame(maxWidth: .infinity, alignment: .leading)
                                .padding(.bottom, item.id == progress.last?.id ? 0 : 24)
                        }.fixedSize(horizontal: false, vertical: true)
                            .accessibilityElement(children: .combine)
                            .accessibilityIdentifier("task-progress-event-\(item.id)")
                    }
                }
                Text("时间为这部 iPhone 收到进展的同步时间。")
                    .font(.footnote).foregroundStyle(IdeaPalette.secondary)
                    .accessibilityIdentifier("task-progress-time-note")
            }
        }.frame(maxWidth: .infinity, alignment: .leading)
    }
}
