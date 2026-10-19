import E2EKit

@MainActor
public enum ContentModule {
    public static let manifest = DesktopModule(
        name: "content", scenarioNames: [ContentWorkbench.name, ContentHome.name],
        run: { scenario, session in
            if scenario == ContentWorkbench.name { try await ContentWorkbench.run(session); return true }
            if scenario == ContentHome.name { try await ContentHome.run(session); return true }
            return false
        }
    )
}
