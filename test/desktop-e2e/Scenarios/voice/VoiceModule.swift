import E2EKit

@MainActor
public enum VoiceModule {
    public static let manifest = DesktopModule(
        name: "voice",
        scenarioNames: [VoiceLiveCall.name, VoiceLiveAcoustic.name, VoiceDoubaoDirect.name, VoiceCommandBudget.name],
        run: { name, session in
            switch name {
            case VoiceLiveCall.name: try await VoiceLiveCall.run(session)
            case VoiceLiveAcoustic.name: try await VoiceLiveAcoustic.run(session)
            case VoiceDoubaoDirect.name: try await VoiceDoubaoDirect.run(session)
            case VoiceCommandBudget.name: try await VoiceCommandBudget.run(session)
            default: return false
            }
            return true
        }
    )
}
