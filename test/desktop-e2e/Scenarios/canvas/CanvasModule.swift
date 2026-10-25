import E2EKit

@MainActor
public enum CanvasModule {
    public static let manifest = DesktopModule(
        name: "canvas",
        scenarioNames: [CanvasDrawers.name, CanvasProjectDrawers.name, CanvasContentTabs.name, CanvasDeleteReflow.name, CanvasCommandDock.name, CanvasRunEffects.name, CanvasPresentationLayout.name],
        run: { scenario, session in
            switch scenario {
            case CanvasDrawers.name:
                try await CanvasDrawers.run(session)
                return true
            case CanvasProjectDrawers.name:
                try await CanvasProjectDrawers.run(session)
                return true
            case CanvasContentTabs.name:
                try await CanvasContentTabs.run(session)
                return true
            case CanvasDeleteReflow.name:
                try await CanvasDeleteReflow.run(session)
                return true
            case CanvasCommandDock.name:
                try await CanvasCommandDock.run(session)
                return true
            case CanvasRunEffects.name:
                try await CanvasRunEffects.run(session)
                return true
            case CanvasPresentationLayout.name:
                try await CanvasPresentationLayout.run(session)
                return true
            default:
                return false
            }
        }
    )
}
