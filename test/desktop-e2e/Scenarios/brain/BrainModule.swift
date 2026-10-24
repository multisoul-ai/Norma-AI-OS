import E2EKit

@MainActor
public enum BrainModule {
    public static let manifest = DesktopModule(
        name: "brain",
        scenarioNames: [BrainReport.name, BrainSettings.name, BrainAutonomy.name, BrainQuiet.name, BrainReturn.name],
        run: { scenario, session in
            switch scenario {
            case BrainReport.name:
                try await BrainReport.run(session)
                return true
            case BrainAutonomy.name:
                try await BrainAutonomy.run(session)
                return true
            case BrainReturn.name:
                try await BrainReturn.run(session)
                return true
            case BrainQuiet.name:
                try await BrainQuiet.run(session)
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
