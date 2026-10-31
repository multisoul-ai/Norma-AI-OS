import E2EKit

@MainActor
public enum AgentModule {
    public static let manifest = DesktopModule(
        name: "agent",
        scenarioNames: [
            AgentStartupWatchdog.name,
            AgentCodexUpdateMenu.name,
        ],
        run: { scenario, session in
            switch scenario {
            case AgentStartupWatchdog.name:
                try await AgentStartupWatchdog.run(session)
                return true
            case AgentCodexUpdateMenu.name:
                try await AgentCodexUpdateMenu.run(session)
                return true
            default:
                return false
            }
        }
    )
}
