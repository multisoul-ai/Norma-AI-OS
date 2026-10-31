import E2EKit

@MainActor
public enum AgentModule {
    public static let manifest = DesktopModule(
        name: "agent",
        scenarioNames: [
            AgentStartupWatchdog.name,
            AgentCodexUpdateMenu.name,
            AgentCodexResumeFork.name,
            AgentBackgroundCompletion.name,
            AgentAskReady.name,
            AgentCodexWaitCompletion.name,
        ],
        run: { scenario, session in
            switch scenario {
            case AgentStartupWatchdog.name:
                try await AgentStartupWatchdog.run(session)
                return true
            case AgentCodexUpdateMenu.name:
                try await AgentCodexUpdateMenu.run(session)
                return true
            case AgentCodexResumeFork.name:
                try await AgentCodexResumeFork.run(session)
                return true
            case AgentBackgroundCompletion.name:
                try await AgentBackgroundCompletion.run(session)
                return true
            case AgentAskReady.name:
                try await AgentAskReady.run(session)
                return true
            case AgentCodexWaitCompletion.name:
                try await AgentCodexWaitCompletion.run(session)
                return true
            default:
                return false
            }
        }
    )
}
