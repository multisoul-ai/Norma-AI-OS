import SwiftUI
import NormaCore

/// Frozen results and explicit user decisions. Opening files never dispatches or accepts a task.
struct WorkThreadReviewView: View {
    let operation: ThreadTaskOutboxOperation
    let agentName: String
    let projectName: String
    @Environment(ComputerModel.self) private var computers
    @Environment(CloudChatModel.self) private var cloud
    @Environment(WorkspaceLibraryModel.self) private var library
    @Environment(\.scenePhase) private var scenePhase
    @Environment(\.dynamicTypeSize) private var typeSize
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency
    @State private var model: WorkThreadReviewModel
    @State private var response: ProjectReviewResponse?
    @State private var error: String?
    @State private var loading = false
    @State private var retry = 0
    @State private var selectedFile: String?
    @State private var showingFeedback = false
    @State private var showingRevision = false

    init(operation: ThreadTaskOutboxOperation, agentName: String, projectName: String) {
        self.operation = operation; self.agentName = agentName; self.projectName = projectName
        _model = State(initialValue: WorkThreadReviewModel(operation: operation))
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                VStack(alignment: .leading, spacing: 12) {
                    Label(model.status, systemImage: model.operation.acceptance?.acceptance == nil ? "clock" : "checkmark.circle")
                        .font(.subheadline.weight(.medium)).foregroundStyle(model.operation.acceptance?.acceptance == nil ? IdeaPalette.secondary : IdeaPalette.positiveText)
                        .accessibilityIdentifier("thread-review-status")
                    Label("\(projectName) · \(agentName)", systemImage: "folder")
                        .font(.subheadline).foregroundStyle(IdeaPalette.secondary)
                }.frame(maxWidth: .infinity, alignment: .leading)
                if loading {
                    HStack(spacing: 12) {
                        ProgressView()
                        Text("正在从 Mac 读取完整成果…").font(.subheadline)
                    }.frame(maxWidth: .infinity, alignment: .leading).padding(20)
                        .accessibilityIdentifier("thread-review-loading")
                }
                if let error {
                    VStack(alignment: .leading, spacing: 12) {
                        Label("暂时无法读取成果", systemImage: "exclamationmark.circle").font(.headline)
                        Text(error).font(.subheadline).foregroundStyle(IdeaPalette.secondary)
                            .accessibilityIdentifier("thread-review-error")
                        Button("重新读取") { retry += 1 }.frame(minHeight: 44)
                            .disabled(loading || scenePhase != .active).accessibilityIdentifier("thread-review-retry")
                    }.threadCard()
                }
                if let response {
                    if let file = response.review.files.first {
                        Button { selectedFile = file.id } label: {
                            VStack(alignment: .leading, spacing: 14) {
                                Label("本轮文件预览", systemImage: "doc.text.magnifyingglass").font(.headline)
                                Text(file.path).font(.subheadline).foregroundStyle(IdeaPalette.secondary)
                                Text(String((file.diff ?? file.after?.text ?? "图片或二进制文件，请打开查看。").prefix(240)))
                                    .font(.system(.callout, design: .monospaced)).lineLimit(6)
                                    .frame(maxWidth: .infinity, alignment: .leading)
                                Text("部分预览 · 查看完整内容").font(.footnote).foregroundStyle(IdeaPalette.blue)
                            }.frame(maxWidth: .infinity, alignment: .leading).padding(20)
                                .background(IdeaPalette.blue.opacity(0.05), in: RoundedRectangle(cornerRadius: 24))
                        }.buttonStyle(IdeaPressStyle()).accessibilityIdentifier("thread-review-preview")
                    }
                    if let summary = operation.receipt?.execution?.execution.result?.text, !summary.isEmpty {
                        VStack(alignment: .leading, spacing: 12) {
                            Label("\(agentName) 的执行结果", systemImage: model.resultSymbol)
                                .font(.headline).foregroundStyle(model.resultColor)
                            Text(summary).font(.body).textSelection(.enabled)
                            if operation.receipt?.execution?.execution.result?.truncated == true {
                                Text("部分回复，完整内容保留在 Mac。").font(.footnote).foregroundStyle(IdeaPalette.secondary)
                            }
                        }.threadCard()
                    }
                    VStack(alignment: .leading, spacing: 16) {
                        Text("文件与变更").font(.headline)
                        Text("\(response.review.files.count) 个文件 · 任务起点 → 本次交付")
                            .font(.subheadline).foregroundStyle(IdeaPalette.secondary)
                            .accessibilityIdentifier("thread-review-file-count")
                        if response.review.conflicted {
                            Label("包含合并冲突，请在 Mac 处理。", systemImage: "exclamationmark.triangle")
                                .font(.subheadline).foregroundStyle(IdeaPalette.secondary)
                        }
                        if response.review.files.isEmpty {
                            Text("本轮没有文件变更。").font(.body)
                        }
                        ForEach(response.review.files) { file in
                            Button { selectedFile = file.id } label: {
                                HStack(spacing: 12) {
                                    Image(systemName: "doc.text").font(.system(size: 20)).foregroundStyle(IdeaPalette.blue)
                                        .frame(width: 44, height: 44)
                                        .background(IdeaPalette.blue.opacity(0.08), in: RoundedRectangle(cornerRadius: 12))
                                    VStack(alignment: .leading, spacing: 4) {
                                        Text(file.path).font(.body).foregroundStyle(IdeaPalette.ink)
                                            .fixedSize(horizontal: false, vertical: true)
                                        Text(file.changeLabel).font(.caption).foregroundStyle(IdeaPalette.secondary)
                                    }
                                    Spacer(minLength: 0)
                                    Image(systemName: "chevron.right").font(.caption).foregroundStyle(IdeaPalette.secondary)
                                }.frame(minHeight: 44).contentShape(Rectangle())
                            }.buttonStyle(.plain).accessibilityIdentifier("review-file-\(file.id)")
                        }
                    }.threadCard()
                    Label("这份成果已保存，后续任务不会改写。查看文件不会确认验收。", systemImage: "info.circle")
                        .font(.footnote).foregroundStyle(IdeaPalette.secondary)
                }
                DisclosureGroup {
                    VStack(alignment: .leading, spacing: 12) {
                        Text(operation.task.task).font(.body).textSelection(.enabled)
                            .accessibilityIdentifier("thread-review-instruction")
                        if let delivery = operation.receipt?.execution?.execution.delivery {
                            Text("保存于 \(Date(timeIntervalSince1970: TimeInterval(delivery.savedAtUnix)).formatted(date: .abbreviated, time: .shortened))")
                                .font(.footnote).foregroundStyle(IdeaPalette.secondary)
                        }
                    }.frame(maxWidth: .infinity, alignment: .leading).padding(.top, 12)
                } label: {
                    Text("本次任务").accessibilityIdentifier("thread-review-task-disclosure")
                }.threadCard()
            }.padding(20)
        }.background(IdeaPalette.background).foregroundStyle(IdeaPalette.ink)
            .navigationTitle("成果验收").navigationBarTitleDisplayMode(.inline)
            .safeAreaInset(edge: .bottom, spacing: 0) { if response != nil { decisions } }
            .sheet(isPresented: $showingFeedback) { feedbackSheet }
            .onChange(of: scenePhase) { _, phase in if phase != .active { model.finishDraftWrites() } }
            .onDisappear { model.finishDraftWrites() }
            .navigationDestination(isPresented: $showingRevision) {
                if let revision = model.revisionTask {
                    WorkThreadTaskDetail(operation: revision, workspace: library.workspaces.first { $0.id == operation.task.collaborationWorkspaceID },
                        deliveryError: library.taskDeliveryErrors[revision.id])
                }
            }
            .task(id: library.deliveryRevision) { await model.load(library.store) }
            .navigationDestination(item: $selectedFile) { id in
                if let file = response?.review.files.first(where: { $0.id == id }) {
                    ReviewFileView(file: file, baselineName: "任务起点", resultName: "本次交付", showsIndex: false)
                }
            }
            .task(id: "\(scenePhase)-\(retry)") {
                // Keep a complete snapshot while reading its files. A new visit authenticates again.
                guard scenePhase == .active, response == nil else { return }
                loading = true; error = nil
                var connection: CollaborationRelayConnection?
                do {
                    await cloud.load()
                    try Task.checkCancellation()
                    guard cloud.loaded else {
                        error = cloud.error ?? "暂时无法读取云端授权，请稍后重试。"
                        loading = false; return
                    }
                    guard let credential = cloud.credential else { throw CloudTextError.unauthorized }
                    guard let machine = computers.machines.first(where: { $0.id == operation.machineID }) else { throw MobileError.invalidCredential }
                    guard let receipt = operation.receipt else { throw MobileError.reviewUnavailable }
                    let opened = try await collaborationRelayClient().connect(machine, cloud: credential, vault: computers.vault)
                    connection = opened
                    let result = try await opened.threadTaskReview(task: operation.task, receipt: receipt)
                    await opened.close()
                    try Task.checkCancellation()
                    response = result; loading = false
                } catch {
                    await connection?.close()
                    guard !Task.isCancelled else { return }
                    self.error = error.localizedDescription; loading = false
                }
            }
    }

    private var decisions: some View {
        VStack(spacing: 10) {
            if let failure = model.error ?? library.taskDeliveryErrors[operation.id] {
                Text(failure).font(.footnote).foregroundStyle(.red)
                    .frame(maxWidth: .infinity, alignment: .leading).accessibilityIdentifier("thread-review-action-error")
            } else if let acceptance = model.operation.acceptance, acceptance.acceptance == nil {
                Text(acceptance.explanation).font(.footnote).foregroundStyle(IdeaPalette.secondary)
                    .frame(maxWidth: .infinity, alignment: .leading)
            } else if model.canDecide, let reason = model.acceptanceRestriction ?? (response?.review.conflicted == true ? "成果包含合并冲突，请先退回修改或在 Mac 处理。" : nil) {
                Text(reason).font(.footnote).foregroundStyle(IdeaPalette.secondary)
                    .frame(maxWidth: .infinity, alignment: .leading).accessibilityIdentifier("thread-review-restriction")
            }
            if model.operation.revisionOperationID != nil {
                Button { showingRevision = true } label: {
                    Label("查看修改任务", systemImage: "arrow.triangle.branch").frame(maxWidth: .infinity, minHeight: 44)
                }.buttonStyle(.borderedProminent).disabled(model.revisionTask == nil)
                    .accessibilityIdentifier("thread-review-revision-task")
            } else if model.canDecide {
                let layout = typeSize.isAccessibilitySize ? AnyLayout(VStackLayout(spacing: 12)) : AnyLayout(HStackLayout(spacing: 12))
                layout {
                    Button { showingFeedback = true } label: { Text("退回修改").frame(maxWidth: .infinity, minHeight: 44) }
                        .buttonStyle(.bordered).accessibilityIdentifier("thread-review-return")
                    Button { Task { await model.accept(library.store); library.deliveryRevision &+= 1 } } label: {
                        Text(model.operation.acceptance?.receipt?.state == "rejected" ? "重新验收" : "验收通过")
                            .frame(maxWidth: .infinity, minHeight: 44)
                    }.buttonStyle(.borderedProminent)
                        .disabled(response?.review.conflicted != false || model.acceptanceRestriction != nil)
                        .accessibilityIdentifier("thread-review-accept")
                }
            } else if model.operation.acceptance?.acceptance != nil {
                Label("已验收", systemImage: "checkmark.circle.fill").font(.headline).foregroundStyle(IdeaPalette.positiveText)
                    .frame(maxWidth: .infinity, minHeight: 44).accessibilityIdentifier("thread-review-accepted")
            } else {
                Label(model.status, systemImage: "clock").font(.headline).frame(maxWidth: .infinity, minHeight: 44)
                    .accessibilityIdentifier("thread-review-pending")
            }
        }.padding(.horizontal, 20).padding(.vertical, 12).tint(IdeaPalette.blue)
            .background {
                let shape = UnevenRoundedRectangle(topLeadingRadius: 24, topTrailingRadius: 24)
                if reduceTransparency { shape.fill(IdeaPalette.background) }
                else if #available(iOS 26.0, *) { Color.clear.glassEffect(.regular, in: shape) }
                else { shape.fill(.regularMaterial) }
            }
    }

    private var feedbackSheet: some View {
        NavigationStack {
            Form {
                Section {
                    Text(operation.task.task).font(.headline)
                    Text("修改意见将交给 \(agentName)，进展和新成果会回到原线程。")
                        .font(.subheadline).foregroundStyle(IdeaPalette.secondary)
                }.listRowBackground(Color.white)
                Section("修改意见") {
                    TextField("描述需要修改的地方", text: Binding(get: { model.draft?.text ?? "" }, set: { model.updateDraft($0, store: library.store) }), axis: .vertical)
                        // Two visible lines keep AX5 editing within the keyboard viewport; native input scrolls the rest.
                        .font(.body).lineLimit(1...2).disabled(model.saving)
                        .accessibilityLabel("修改意见").accessibilityIdentifier("thread-review-feedback-input")
                    if let error = model.error { Text(error).font(.subheadline).foregroundStyle(.red) }
                }.listRowBackground(Color.white)
            }.scrollContentBackground(.hidden).background(IdeaPalette.background).foregroundStyle(IdeaPalette.ink)
                .scrollDismissesKeyboard(.interactively).navigationTitle("退回修改").navigationBarTitleDisplayMode(.inline)
                .toolbar {
                    ToolbarItem(placement: .cancellationAction) {
                        Button("取消") { showingFeedback = false }.disabled(model.saving)
                    }
                    ToolbarItem(placement: .confirmationAction) {
                        Button(model.saving ? "正在保存" : "发送") {
                            Task {
                                if await model.revise(library.store) { showingFeedback = false; library.deliveryRevision &+= 1 }
                            }
                        }.disabled(model.saving || (model.draft?.text ?? "").trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                            .accessibilityIdentifier("thread-review-feedback-send")
                    }
                }
        }.presentationDetents([.large]).presentationDragIndicator(.visible).interactiveDismissDisabled(model.saving)
    }
}
