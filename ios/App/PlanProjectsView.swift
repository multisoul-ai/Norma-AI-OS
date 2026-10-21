import SwiftUI
import NormaCore

/// Native navigation uses each machine's authenticated catalog, with no invented Todo data.
struct PlanProjectsView: View {
    let model: ComputerModel
    @Environment(\.scenePhase) private var scenePhase
    @State private var machineID: String?
    @State private var projects: [ProjectSnapshot] = []
    @State private var error: String?
    @State private var loading = false
    private var machine: PairedMachine? { model.machines.first { $0.id == (machineID ?? model.selectedID) } ?? model.machines.first }
    var body: some View {
        List {
            if let machine {
                Section {
                    ForEach(projects) { project in
                        NavigationLink {
                            ProjectPlansView(model: model, machine: machine, workspaceID: project.id, name: project.displayName)
                        } label: {
                            Label(project.displayName, systemImage: "folder").font(.subheadline).padding(.vertical, 6)
                        }
                        NavigationLink {
                            WorkspaceIdeasView(model: model, machine: machine, workspaceID: project.id, name: project.displayName)
                        } label: {
                            Label("\(project.displayName) 的想法清单", systemImage: "lightbulb").font(.subheadline).padding(.vertical, 6)
                        }
                    }
                    if let error { Text(error).font(.caption).foregroundStyle(.orange) }
                    else if !loading && projects.isEmpty { Text("这台电脑还没有授权给手机的项目").font(.subheadline).foregroundStyle(.secondary) }
                } header: { Text(machine.name) } footer: { Text("选择项目，查看建议或调整下一步方向。") }
            } else {
                ContentUnavailableView("连接电脑后梳理下一步", systemImage: "laptopcomputer.and.iphone", description: Text("在电脑页通过验证码或扫码连接 Norma，再选择项目。"))
            }
        }
        .scrollContentBackground(.hidden).background(.black)
        .workspaceLoading(loading && projects.isEmpty, "正在读取项目…")
        .navigationTitle("下一步").navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                if model.machines.count > 1 {
                    Menu {
                        ForEach(model.machines) { computer in Button(computer.name) { machineID = computer.id } }
                    } label: { Label("切换电脑", systemImage: "laptopcomputer").font(.caption) }
                }
            }
        }
        .task(id: "\(machine?.id ?? "")-\(scenePhase)") {
            projects = []; error = nil
            guard scenePhase == .active, let machine else { loading = false; return }
            loading = true
            defer { loading = false }
            do {
                let catalog = try await model.client.snapshot(machine)
                try Task.checkCancellation()
                projects = catalog.projects
            } catch { if !Task.isCancelled { self.error = error.localizedDescription } }
        }
    }
}
