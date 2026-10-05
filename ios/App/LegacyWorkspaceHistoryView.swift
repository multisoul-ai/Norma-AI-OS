import SwiftUI
import NormaCore

/// Legacy identities are displayed verbatim; they are never matched by name or sent again.
struct LegacyWorkspaceHistoryView: View {
    @Environment(ComputerModel.self) private var computers
    @Environment(CloudChatModel.self) private var cloud
    @Environment(\.dismiss) private var dismiss
    @State private var spaces: [CollaborationWorkspace] = []
    @State private var histories: [String: [WorkThread]] = [:]
    @State private var messages: [String: [WorkThreadMessage]] = [:]
    @State private var outbox: [String: [ThreadOutboxOperation]] = [:]
    @State private var tasks: [String: [ThreadTaskOutboxOperation]] = [:]
    @State private var drafts: [String: WorkThreadDraft] = [:]
    @State private var statuses: [String: String] = [:]
    @State private var error: String?
    private let store = LegacyWorkspaceHistoryStore(databaseURL: URL.applicationSupportDirectory.appendingPathComponent("Norma/Workspaces/workspaces.sqlite"))
    var body: some View {
        NavigationStack {
            List {
                Section { Text("旧工作线程仅供查看。未发送操作已暂停，不会自动关联或派发到同名桌面空间。").font(.subheadline) }
                ForEach(spaces) { space in
                    Section(space.name) {
                        ForEach(histories[space.id] ?? []) { thread in
                            NavigationLink {
                                List {
                                    Section("历史来源") { Text("旧空间：\(space.id)\n旧线程：\(thread.id)").font(.footnote).textSelection(.enabled) }
                                    if let excerpt = thread.sourceExcerpt { Section("来源想法") { Text(excerpt).textSelection(.enabled) } }
                                    if let draft = drafts[thread.id] { Section("旧草稿 · 已暂停") { Text(draft.text).textSelection(.enabled) } }
                                    ForEach(messages[thread.id] ?? []) { message in
                                        VStack(alignment: .leading, spacing: 8) {
                                            Text(message.author).font(.caption).foregroundStyle(.secondary)
                                            Text(message.text).textSelection(.enabled)
                                            if let operation = outbox[thread.id]?.first(where: { $0.messageID == message.id }) {
                                                Text(operation.receipt == nil ? "旧操作已暂停" : "旧 Mac 接收回执已保存").font(.caption)
                                                if operation.message != nil && operation.machineID != nil {
                                                    Button("仅查询旧回执") { Task { await check(operation) } }.frame(minHeight: 44)
                                                    if let status = statuses[operation.id] { Text(status).font(.footnote) }
                                                }
                                            }
                                        }
                                    }
                                    Section("旧任务 · 仅核查") {
                                        ForEach(tasks[thread.id] ?? []) { task in
                                            VStack(alignment: .leading, spacing: 8) {
                                                Text(task.task.task).textSelection(.enabled)
                                                Text(task.receipt?.execution?.execution.result?.text ?? task.receipt?.rejectionDescription ?? "原任务已暂停自动派发").font(.subheadline)
                                                Button("仅查询原任务回执") { Task { await checkTask(task) } }.frame(minHeight: 44)
                                                if let status = statuses[task.id] { Text(status).font(.footnote) }
                                            }
                                        }
                                    }
                                }.navigationTitle(thread.title).navigationBarTitleDisplayMode(.inline)
                            } label: { VStack(alignment: .leading, spacing: 6) { Text(thread.title); Text("只读历史").font(.caption).foregroundStyle(.secondary) } }
                        }
                    }
                }
                if spaces.isEmpty { Text(error ?? "没有旧版历史记录。") }
            }.navigationTitle("旧版只读历史").navigationBarTitleDisplayMode(.inline)
                .toolbar { ToolbarItem(placement: .confirmationAction) { Button("完成") { dismiss() } } }
                .task {
                    do {
                        spaces = try await store.all()
                        for space in spaces {
                            let threads = try await store.threads(workspaceID: space.id)
                            histories[space.id] = threads
                            for thread in threads {
                                messages[thread.id] = try await store.messages(threadID: thread.id)
                                outbox[thread.id] = try await store.legacyThreadDeliveries(threadID: thread.id)
                                tasks[thread.id] = try await store.legacyThreadTasks(threadID: thread.id)
                                drafts[thread.id] = try await store.messageDraft(threadID: thread.id)
                            }
                        }
                    } catch { self.error = error.localizedDescription }
                }
        }.preferredColorScheme(.light)
    }
    private func check(_ operation: ThreadOutboxOperation) async {
        guard let message = operation.message, let machine = computers.machines.first(where: { $0.id == operation.machineID }) else { statuses[operation.id] = "原 Mac 不在已配对列表中；原文仍保留。"; return }
        do {
            let receipt: ThreadMessageReceipt?
            do { receipt = try await computers.client.legacyThreadReceipt(machine, message: message) }
            catch {
                guard error as? MobileError == .connectionFailed || error as? MobileError == .timedOut else { throw error }
                let connection = try await relay(machine)
                defer { Task { await connection.close() } }
                receipt = try await connection.threadReceipt(for: message)
            }
            statuses[operation.id] = receipt == nil ? "未查到旧接收回执；未重发。" : "原 Mac 已接收该操作；未重新执行。"
        } catch { statuses[operation.id] = error.localizedDescription }
    }
    private func relay(_ machine: PairedMachine) async throws -> CollaborationRelayConnection {
        await cloud.load()
        guard let credential = cloud.credential else { throw WorkspaceChatError.unavailable }
        return try await collaborationRelayClient().connect(machine, cloud: credential, vault: computers.vault)
    }
    private func checkTask(_ operation: ThreadTaskOutboxOperation) async {
        guard let machine = computers.machines.first(where: { $0.id == operation.machineID }) else { statuses[operation.id] = "原 Mac 未配对；原任务仍保留。"; return }
        do {
            let receipt: ThreadTaskReceipt?
            do { receipt = try await computers.client.legacyTaskReceipt(machine, task: operation.task) }
            catch {
                guard error as? MobileError == .connectionFailed || error as? MobileError == .timedOut else { throw error }
                let connection = try await relay(machine)
                defer { Task { await connection.close() } }
                receipt = try await connection.threadTaskReceipt(for: operation.task)
            }
            statuses[operation.id] = receipt?.execution?.execution.result?.text ?? receipt?.rejectionDescription ?? (receipt == nil ? "未查到原任务；未重发。" : "Mac 记录：\(receipt!.receipt.state)；未重新执行。")
        } catch { statuses[operation.id] = error.localizedDescription }
    }
}
