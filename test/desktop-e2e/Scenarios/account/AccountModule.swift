import E2EKit

@MainActor
public enum AccountModule {
    public static let manifest = DesktopModule(
        name: "account",
        scenarioNames: [
            AccountLoginGate.name,
            AccountLoginFlow.name,
        ],
        run: { scenario, session in
            switch scenario {
            case AccountLoginGate.name:
                try await AccountLoginGate.run(session)
            case AccountLoginFlow.name:
                try await AccountLoginFlow.run(session)
            default:
                return false
            }
            return true
        }
    )
}
