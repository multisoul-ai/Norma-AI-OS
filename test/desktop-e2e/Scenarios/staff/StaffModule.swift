import E2EKit

@MainActor
public enum StaffModule {
    public static let manifest = DesktopModule(
        name: "staff",
        scenarioNames: [StaffTaskListLayout.name, StaffTaskListLayout.englishName, StaffComposerImages.name, StaffComposerImages.englishName, StaffProposalLayout.name, StaffProposalLayout.englishName, StaffWorkspaceStreaming.name, StaffWorkspaceStreaming.englishName, StaffWorkflow.name, StaffWorkflow.englishName, StaffOpenCodeDispatch.name, StaffArchive.name, StaffArchive.englishName, StaffProtocolRecovery.name, StaffProtocolRecovery.englishName, StaffAutoDispatch.name, StaffAutoDispatch.englishName, StaffRichLink.name, StaffToolStepPaths.name, StaffLongReply.name, StaffWithdrawNewCard.name, StaffWithdrawReuse.name],
        visualScenarioNames: [StaffTaskListLayout.name, StaffTaskListLayout.englishName, StaffComposerImages.name, StaffComposerImages.englishName, StaffProposalLayout.name, StaffProposalLayout.englishName, StaffWorkspaceStreaming.name, StaffWorkspaceStreaming.englishName, StaffWorkflow.name, StaffWorkflow.englishName, StaffOpenCodeDispatch.name, StaffArchive.name, StaffArchive.englishName, StaffProtocolRecovery.name, StaffProtocolRecovery.englishName, StaffAutoDispatch.name, StaffAutoDispatch.englishName, StaffRichLink.name, StaffToolStepPaths.name, StaffLongReply.name, StaffWithdrawNewCard.name, StaffWithdrawReuse.name],
        run: { scenario, session in
            switch scenario {
            case StaffTaskListLayout.name: try await StaffTaskListLayout.run(session); return true
            case StaffTaskListLayout.englishName: try await StaffTaskListLayout.run(session, english: true); return true
            case StaffComposerImages.name: try await StaffComposerImages.run(session); return true
            case StaffComposerImages.englishName: try await StaffComposerImages.run(session, english: true); return true
            case StaffProposalLayout.name: try await StaffProposalLayout.run(session); return true
            case StaffProposalLayout.englishName: try await StaffProposalLayout.run(session, english: true); return true
            case StaffWorkflow.name: try await StaffWorkflow.run(session); return true
            case StaffWorkflow.englishName: try await StaffWorkflow.run(session, english: true); return true
            case StaffWorkspaceStreaming.name: try await StaffWorkspaceStreaming.run(session); return true
            case StaffWorkspaceStreaming.englishName: try await StaffWorkspaceStreaming.run(session, english: true); return true
            case StaffOpenCodeDispatch.name: try await StaffOpenCodeDispatch.run(session); return true
            case StaffProtocolRecovery.name: try await StaffProtocolRecovery.run(session); return true
            case StaffProtocolRecovery.englishName: try await StaffProtocolRecovery.run(session, english: true); return true
            case StaffArchive.name: try await StaffArchive.run(session); return true
            case StaffArchive.englishName: try await StaffArchive.run(session, english: true); return true
            case StaffAutoDispatch.name: try await StaffAutoDispatch.run(session); return true
            case StaffAutoDispatch.englishName: try await StaffAutoDispatch.run(session, english: true); return true
            case StaffRichLink.name: try await StaffRichLink.run(session); return true
            case StaffToolStepPaths.name: try await StaffToolStepPaths.run(session); return true
            case StaffLongReply.name: try await StaffLongReply.run(session); return true
            case StaffWithdrawNewCard.name: try await StaffWithdrawNewCard.run(session); return true
            case StaffWithdrawReuse.name: try await StaffWithdrawReuse.run(session); return true
            default: return false
            }
        }
    )
}
