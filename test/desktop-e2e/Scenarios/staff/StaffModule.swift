import E2EKit

@MainActor
public enum StaffModule {
    public static let manifest = DesktopModule(
        name: "staff",
        scenarioNames: [StaffWorkspaceStreaming.name, StaffWorkspaceStreaming.englishName, StaffWorkflow.name, StaffWorkflow.englishName, StaffOpenCodeDispatch.name, StaffArchive.name, StaffArchive.englishName, StaffProtocolRecovery.name, StaffProtocolRecovery.englishName],
        visualScenarioNames: [StaffWorkspaceStreaming.name, StaffWorkspaceStreaming.englishName, StaffWorkflow.name, StaffWorkflow.englishName, StaffOpenCodeDispatch.name, StaffArchive.name, StaffArchive.englishName, StaffProtocolRecovery.name, StaffProtocolRecovery.englishName],
        run: { scenario, session in
            switch scenario {
            case StaffWorkflow.name: try await StaffWorkflow.run(session); return true
            case StaffWorkflow.englishName: try await StaffWorkflow.run(session, english: true); return true
            case StaffWorkspaceStreaming.name: try await StaffWorkspaceStreaming.run(session); return true
            case StaffWorkspaceStreaming.englishName: try await StaffWorkspaceStreaming.run(session, english: true); return true
            case StaffOpenCodeDispatch.name: try await StaffOpenCodeDispatch.run(session); return true
            case StaffProtocolRecovery.name: try await StaffProtocolRecovery.run(session); return true
            case StaffProtocolRecovery.englishName: try await StaffProtocolRecovery.run(session, english: true); return true
            case StaffArchive.name: try await StaffArchive.run(session); return true
            case StaffArchive.englishName: try await StaffArchive.run(session, english: true); return true
            default: return false
            }
        }
    )
}
