import SwiftUI
import NormaCore

/// Reuses the existing bounded delivery feed and exact-task review destination.
struct CommandDeliveryReminder: View {
    let model: ComputerModel
    let machine: PairedMachine
    let project: ProjectSnapshot
    var active: Bool
    @Environment(\.scenePhase) private var scenePhase
    @State private var count: UInt64?
    @State private var showing = false
    @State private var issue: String?
    var body: some View {
        Button { showing = true } label: {
            Label("待验收 · " + (count.map(String.init) ?? "待核验"), systemImage: "checkmark.circle")
                .font(.caption).frame(maxWidth: .infinity, minHeight: 44)
                .background(WorkspaceStyle.surface, in: Capsule())
        }.disabled(project.state != "open" || project.routeID == nil)
            .accessibilityLabel("待验收，当前项目").accessibilityValue(count.map(String.init) ?? "待核验")
            .accessibilityHint(issue ?? "查看成果并通过或退回原任务")
            .accessibilityIdentifier("command-delivery-inbox")
            .sheet(isPresented: $showing) {
                if let route = project.routeID {
                    NavigationStack {
                        TaskDeliveriesView(model: model, machine: machine, workspaceID: project.id, routeID: route, pendingOnly: true)
                            .toolbar { ToolbarItem(placement: .cancellationAction) {
                                Button("完成") { showing = false }.accessibilityIdentifier("delivery-inbox-close")
                            } }
                    }.preferredColorScheme(.dark).tint(.white)
                }
            }
            .task(id: "\(scenePhase)-\(active)-\(showing)-\(project.routeID ?? "")-\(model.grant?.revision ?? 0)") {
                guard !showing else { return }
                count = nil; issue = nil
                guard active, scenePhase == .active, project.state == "open", let route = project.routeID else { return }
                do {
                    for try await update in model.client.deliveryUpdates(machine, scope: ProjectReviewRequest(workspaceID: project.id, routeID: route), pendingOnly: true) {
                        try Task.checkCancellation()
                        switch update {
                        case .current(let page): count = page.pendingCount; issue = nil
                        case .legacy: count = nil; issue = "更新电脑端后可显示待验收总数"
                        case .recovering: count = nil; issue = "待验收数量正在重新核验"
                        }
                    }
                } catch { if !Task.isCancelled { count = nil; issue = error.localizedDescription } }
            }
    }
}
