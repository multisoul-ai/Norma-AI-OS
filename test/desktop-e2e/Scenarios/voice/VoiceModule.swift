import E2EKit

@MainActor
public enum VoiceModule {
    public static let manifest = DesktopModule(
        name: "voice",
        scenarioNames: [VoiceLiveCall.name, VoiceLiveAcoustic.name],
        run: { name, session in
            switch name {
            case VoiceLiveCall.name: try await VoiceLiveCall.run(session)
            case VoiceLiveAcoustic.name: try await VoiceLiveAcoustic.run(session)
            default: return false
            }
            return true
        }
    )
}
