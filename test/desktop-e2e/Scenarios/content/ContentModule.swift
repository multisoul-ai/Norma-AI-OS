import E2EKit

@MainActor
public enum ContentModule {
    public static let manifest = DesktopModule(
        name: "content", scenarioNames: [ContentWorkbench.name],
        run: { scenario, session in
            if scenario == ContentWorkbench.name { try await ContentWorkbench.run(session); return true }
            return false
        }
    )
}
