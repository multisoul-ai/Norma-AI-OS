import E2EKit

@MainActor
public enum BrainModule {
    public static let manifest = DesktopModule(
        name: "brain",
        scenarioNames: [BrainReport.name, BrainReport.voiceName, BrainSettings.name, BrainAutonomy.name, BrainQuiet.name, BrainReturn.name, BrainPropose.name, BrainQuestions.name, BrainCodexPermission.name, BrainClaudeQuota.name, BrainActivity.name, BrainDockChat.name, BrainDockChat.englishName],
        run: { scenario, session in
            switch scenario {
            case BrainDockChat.name:
                try await BrainDockChat.run(session)
                return true
            case BrainDockChat.englishName:
                try await BrainDockChat.run(session, english: true)
                return true
            case BrainQuestions.name:
                try await BrainQuestions.run(session)
                return true
            case BrainClaudeQuota.name:
                try await BrainClaudeQuota.run(session)
                return true
            case BrainCodexPermission.name:
                try await BrainCodexPermission.run(session)
                return true
            case BrainActivity.name:
                try await BrainActivity.run(session)
                return true
            case BrainReport.name:
                try await BrainReport.run(session)
                return true
            case BrainReport.voiceName:
                try await BrainReport.run(session, speech: true)
                return true
            case BrainAutonomy.name:
                try await BrainAutonomy.run(session)
                return true
            case BrainPropose.name:
                try await BrainPropose.run(session)
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
