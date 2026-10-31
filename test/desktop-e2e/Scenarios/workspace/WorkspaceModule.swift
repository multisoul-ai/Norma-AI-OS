import E2EKit

@MainActor
public enum WorkspaceModule {
    public static let manifest = DesktopModule(
        name: "workspace",
        scenarioNames: [
            WorkspaceLifecycle.name,
            WorkspaceHomeDesign.name,
            WorkspaceHomePreview.name,
            WorkspaceDrawerTabs.name,
            WorkspaceRunOverview.name,
            WorkspaceStatusCapsule.name,
            WorkspaceRunOverviewAgents.name,
            WorkspaceRunOverviewTitles.name,
            WorkspaceOverviewFloat.name,
            WorkspaceTitlebarBrand.name,
            WorkspaceIdeas.name,
            WorkspaceIdeasShortcuts.name,
            WorkspaceIdeasProjectFocus.name,
            WorkspaceIdeasCompose.name,
            WorkspaceIdeasDeleteUndo.name,
            WorkspaceFeatureGuide.name,
            WorkspaceDispatchEffects.name,
            WorkspaceHandoffEffects.name,
            WorkspaceSwitchShortcuts.name,
        ],
        visualScenarioNames: [WorkspaceIdeasDeleteUndo.name, WorkspaceDrawerTabs.name],
        run: { scenario, session in
            switch scenario {
            case WorkspaceHomePreview.name:
                try await WorkspaceHomePreview.run(session)
                return true
            case WorkspaceHomeDesign.name:
                try await WorkspaceHomeDesign.run(session)
                return true
            case WorkspaceLifecycle.name:
                try await WorkspaceLifecycle.run(session)
                return true
            case WorkspaceDrawerTabs.name:
                try await WorkspaceDrawerTabs.run(session)
                return true
            case WorkspaceRunOverview.name:
                try await WorkspaceRunOverview.run(session)
                return true
            case WorkspaceRunOverviewAgents.name:
                try await WorkspaceRunOverviewAgents.run(session)
                return true
            case WorkspaceRunOverviewTitles.name:
                try await WorkspaceRunOverviewTitles.run(session)
                return true
            case WorkspaceStatusCapsule.name:
                try await WorkspaceStatusCapsule.run(session)
                return true
            case WorkspaceOverviewFloat.name:
                try await WorkspaceOverviewFloat.run(session)
                return true
            case WorkspaceTitlebarBrand.name:
                try await WorkspaceTitlebarBrand.run(session)
                return true
            case WorkspaceIdeas.name:
                try await WorkspaceIdeas.run(session)
                return true
            case WorkspaceIdeasShortcuts.name:
                try await WorkspaceIdeasShortcuts.run(session)
                return true
            case WorkspaceIdeasProjectFocus.name:
                try await WorkspaceIdeasProjectFocus.run(session)
                return true
            case WorkspaceIdeasCompose.name:
                try await WorkspaceIdeasCompose.run(session)
                return true
            case WorkspaceIdeasDeleteUndo.name:
                try await WorkspaceIdeasDeleteUndo.run(session)
                return true
            case WorkspaceDispatchEffects.name:
                try await WorkspaceDispatchEffects.run(session)
                return true
            case WorkspaceHandoffEffects.name:
                try await WorkspaceHandoffEffects.run(session)
                return true
            case WorkspaceSwitchShortcuts.name:
                try await WorkspaceSwitchShortcuts.run(session)
                return true
            case WorkspaceFeatureGuide.name:
                try await WorkspaceFeatureGuide.run(session)
                return true
            default:
                return false
            }
        }
    )
}
