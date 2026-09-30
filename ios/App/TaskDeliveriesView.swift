import SwiftUI
import NormaCore

struct TaskDeliveriesView: View {
    let model: ComputerModel
    let machine: PairedMachine
    let workspaceID: String
    let routeID: String
    var card: CardSnapshot? = nil
    @Environment(\.scenePhase) private var scenePhase
    @State private var items: [TaskDeliverySummary] = []
    @State private var nextCursor: UInt64?
    @State private var cursor: UInt64?
    @State private var selectedID: String?
    @State private var selectedDelivery: TaskDeliverySummary?
    @State private var pendingOnly: Bool
    @State private var pendingCount: UInt64?
    @State private var retry = 0
    @State private var loading = false
    @State private var error: String?
    @State private var collecting = false

    init(model: ComputerModel, machine: PairedMachine, workspaceID: String, routeID: String,
         card: CardSnapshot? = nil, pendingOnly: Bool = false) {
        self.model = model; self.machine = machine; self.workspaceID = workspaceID; self.routeID = routeID; self.card = card
        _pendingOnly = State(initialValue: pendingOnly)
    }

    var body: some View {
        List {
            Section {
                Label(machine.name + " · " + (card?.title ?? "全部卡片"), systemImage: "laptopcomputer")
                    .font(.caption).foregroundStyle(.secondary)
                Picker("成果范围", selection: $pendingOnly) {
                    Text("待验收").tag(true)
                    Text("全部成果").tag(false)
                }.pickerStyle(.segmented).accessibilityIdentifier("delivery-filter")
                if let pendingCount { Text("\(pendingCount) 份成果待验收").font(.subheadline.weight(.medium)) }
                Text("这里只展示已保存的回合成果。失败、缺少完整变更或存在冲突的记录，可在全部成果中查看。")
                    .font(.caption).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                if collecting { WorkspaceLoadingView(message: "正在整理本轮成果…", inline: true) }
            }
            if loading && !items.isEmpty { WorkspaceLoadingView(message: "正在读取成果…", inline: true) }
            if let error {
                Text(error).font(.subheadline).foregroundStyle(.orange)
            }
            if !loading && error == nil && items.isEmpty {
                ContentUnavailableView(pendingOnly ? "没有待验收成果" : "还没有任务成果", systemImage: "tray", description: Text(cursor == nil ? "Agent 完成任务后，可在这里核对成果。" : "这一页已没有符合条件的成果，可返回最新成果。"))
            }
            ForEach(items) { item in
                Section {
                    Button { selectedDelivery = item; selectedID = item.id } label: {
                        HStack(spacing: 12) {
                            Image(systemName: item.available ? "doc.text" : "exclamationmark.circle")
                                .foregroundStyle(item.available ? Color.primary : .orange)
                            VStack(alignment: .leading, spacing: 5) {
                                if card == nil {
                                    Text("\(item.outcome.key.profile) · 卡片 \(item.outcome.key.nodeID)")
                                        .font(.caption).foregroundStyle(.secondary)
                                }
                                Text(item.stateLabel).font(.subheadline.weight(.medium)).foregroundStyle(.primary)
                                Text(Date(timeIntervalSince1970: TimeInterval(item.savedAtUnix)), format: .dateTime.month().day().hour().minute())
                                    .font(.caption).foregroundStyle(.secondary)
                                if item.acceptance != nil {
                                    Label("已验收", systemImage: "checkmark.seal.fill")
                                        .font(.caption).foregroundStyle(.green)
                                        .accessibilityIdentifier("delivery-accepted-\(item.outcome.key.turnID)")
                                }
                            }
                            Spacer(minLength: 4)
                            if item.available {
                                Text("\(item.fileCount) 个文件").font(.caption).foregroundStyle(.secondary)
                                Image(systemName: "chevron.right").font(.caption2).foregroundStyle(.tertiary)
                            }
                        }.frame(minHeight: 44)
                    }.disabled(!item.available).accessibilityIdentifier("task-delivery-\(item.outcome.key.turnID)")
                } footer: { if !item.available { Text(item.explanation) } }
            }
            if let nextCursor {
                Button("更早的成果") { cursor = nextCursor }.frame(minHeight: 44).disabled(loading)
            }
            if cursor != nil { Button("返回最新成果") { cursor = nil }.frame(minHeight: 44) }
        }
        .scrollContentBackground(.hidden).background(.black)
        .workspaceLoading(loading && items.isEmpty, "正在读取成果…")
        .navigationTitle(card == nil ? "项目成果" : "任务成果").navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                Button { cursor = nil; retry += 1 } label: { Image(systemName: "arrow.clockwise") }
                    .disabled(loading || scenePhase != .active).accessibilityLabel("刷新任务成果")
            }
        }
        .navigationDestination(item: $selectedID) { id in
            if let delivery = selectedDelivery, delivery.id == id {
                ProjectReviewView(machine: machine, workspaceID: workspaceID, routeID: routeID, model: model, delivery: delivery) { reference in
                    selectedDelivery?.acceptance = reference.acceptance
                }
            }
        }
        .onChange(of: pendingOnly) { _, _ in cursor = nil }
        .task(id: "\(scenePhase)-\(retry)-\(pendingOnly)-\(cursor.map(String.init) ?? "first")-\(selectedID ?? "list")") {
            // Detail retains its immutable identity while hidden list watches are cancelled.
            guard selectedID == nil else { return }
            items = []; nextCursor = nil; pendingCount = nil; collecting = false
            loading = false; error = nil
            guard scenePhase == .active else { return }
            loading = true
            do {
                for try await update in model.client.deliveryUpdates(machine, scope: ProjectReviewRequest(workspaceID: workspaceID, routeID: routeID), nodeID: card?.id, before: cursor, pendingOnly: pendingOnly) {
                    try Task.checkCancellation()
                    switch update {
                    case .legacy(let page):
                        items = pendingOnly ? [] : page.items; nextCursor = pendingOnly ? nil : page.nextCursor
                        pendingCount = nil; collecting = false; loading = false
                        error = pendingOnly ? "电脑版本较早，请切换全部成果。更新电脑端后可按待验收筛选。" : "电脑版本较早，成果更新后请手动刷新。"
                    case .recovering:
                        items = []; nextCursor = nil; pendingCount = nil; collecting = false; loading = true
                        error = "连接已中断，正在重新核验成果…"
                    case .current(let page):
                        items = page.items; pendingCount = page.pendingCount; collecting = page.collecting == true
                        nextCursor = page.nextCursor; loading = false; error = nil
                    }
                }
            } catch {
                guard !Task.isCancelled else { return }
                items = []; nextCursor = nil; pendingCount = nil; collecting = false; loading = false
                self.error = error.localizedDescription
            }
        }
    }
}
