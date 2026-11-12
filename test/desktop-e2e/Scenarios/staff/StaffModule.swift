import E2EKit

@MainActor
public enum StaffModule {
    public static let manifest = DesktopModule(
        name: "staff",
        scenarioNames: [StaffWorkflow.name, StaffWorkflow.englishName],
        visualScenarioNames: [StaffWorkflow.name, StaffWorkflow.englishName],
        run: { scenario, session in
            switch scenario {
            case StaffWorkflow.name: try await StaffWorkflow.run(session); return true
            case StaffWorkflow.englishName: try await StaffWorkflow.run(session, english: true); return true
            default: return false
            }
        }
    )
}
