import E2EKit

@MainActor
public enum StaffModule {
    public static let manifest = DesktopModule(
        name: "staff",
        scenarioNames: [StaffWorkspaceStreaming.name, StaffWorkspaceStreaming.englishName, StaffWorkflow.name, StaffWorkflow.englishName],
        visualScenarioNames: [StaffWorkspaceStreaming.name, StaffWorkspaceStreaming.englishName, StaffWorkflow.name, StaffWorkflow.englishName],
        run: { scenario, session in
            switch scenario {
            case StaffWorkflow.name: try await StaffWorkflow.run(session); return true
            case StaffWorkflow.englishName: try await StaffWorkflow.run(session, english: true); return true
            case StaffWorkspaceStreaming.name: try await StaffWorkspaceStreaming.run(session); return true
            case StaffWorkspaceStreaming.englishName: try await StaffWorkspaceStreaming.run(session, english: true); return true
            default: return false
            }
        }
    )
}
