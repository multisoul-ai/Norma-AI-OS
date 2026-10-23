import E2EKit

@MainActor
public enum BrainModule {
    public static let manifest = DesktopModule(
        name: "brain",
        scenarioNames: [BrainReport.name, BrainSettings.name],
        run: { scenario, session in
            switch scenario {
            case BrainReport.name:
                try await BrainReport.run(session)
                return true
            case BrainSettings.name:
                try await BrainSettings.run(session)
                return true
            default:
                return false
            }
        }
    )
}
