import E2EKit

@MainActor
public enum BootModule {
    public static let manifest = DesktopModule(
        name: "boot",
        scenarioNames: [BootSyncAnimation.name],
        run: { scenario, session in
            switch scenario {
            case BootSyncAnimation.name:
                try await BootSyncAnimation.run(session)
                return true
            default:
                return false
            }
        }
    )
}
