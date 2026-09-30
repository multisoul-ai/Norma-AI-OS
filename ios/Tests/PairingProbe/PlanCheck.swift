import Foundation
import NormaCore

enum PlanCheck {
    static func run(client: MobileClient, machine: PairedMachine, directory: URL) async throws {
        let catalog = try await client.snapshot(machine)
        guard let project = catalog.projects.first, let route = project.routeID else { throw MobileError.invalidResponse }
        let denied = try await client.preparePlan(machine, workspaceID: project.id, routeID: route, revision: 1, direction: "不可生成")
        do { _ = try await client.submit(denied, to: machine); throw MobileError.invalidResponse }
        catch MobileError.unauthorized { }
        let empty = try await client.projectPlans(machine, workspaceID: project.id)
        guard empty.items.isEmpty, empty.nextCursor == nil else { throw MobileError.invalidResponse }
        try Data("control".utf8).write(to: directory.appendingPathComponent("enable-control"), options: .atomic)
        // 30s covers Mac approval and one bounded20s model generation, with10s transport margin.
        let deadline = ContinuousClock.now.advanced(by: .seconds(30))
        while try await client.authenticate(machine).revision != 2 {
            guard ContinuousClock.now < deadline else { throw MobileError.timedOut }
            try await Task.sleep(for: .milliseconds(100))
        }
        let command = try await client.preparePlan(machine, workspaceID: project.id, routeID: route, revision: 2, direction: "优先验证登录")
        let url = directory.appendingPathComponent("plan-command.json")
        try await PendingCommandStore(url: url).save(command)
        _ = try await client.submit(command, to: machine)
        let reloaded = try await PendingCommandStore(url: url).load()
        guard reloaded == [command] else { throw MobileError.invalidResponse }
        var receipt = try await client.receipt(for: reloaded[0], on: machine)
        while receipt.state == "accepted" {
            guard ContinuousClock.now < deadline else { throw MobileError.timedOut }
            try await Task.sleep(for: .milliseconds(100))
            receipt = try await client.receipt(for: reloaded[0], on: machine)
        }
        guard receipt.state == "succeeded", let planID = receipt.result?.planID,
              receipt.result?.status == "proposed", receipt.result?.nodeID == nil else { throw MobileError.invalidResponse }
        let plans = try await client.projectPlans(machine, workspaceID: project.id)
        let detail = try await client.snapshot(machine, workspaceID: project.id)
        guard plans.items.count == 1, plans.nextCursor == nil, let plan = plans.items.first,
              plan.planID == planID, plan.direction == "优先验证登录", plan.draft.summary == "下一步验证登录",
              plan.draft.todos.first?.target.nodeID == detail.projects.first?.cards?.first?.nodeID,
              plan.draft.todos.first?.instruction == "检查登录边界并补测试。",
              detail.projects.first?.cards?.first?.state == "idle" else { throw MobileError.invalidResponse }
        let replay = try await client.submit(command, to: machine)
        guard replay.result?.planID == planID else { throw MobileError.invalidResponse }
        let previous = try await client.projectPlans(machine, workspaceID: project.id, before: plan.sequence)
        guard previous.items.isEmpty else { throw MobileError.invalidResponse }
        do { _ = try await client.projectPlans(machine, workspaceID: "not-granted"); throw MobileError.invalidResponse }
        catch MobileError.unauthorized { }
        try await PendingCommandStore(url: url).remove(id: command.id)
        print("NATIVE_PLAN_PROPOSAL_OK")
    }
}
