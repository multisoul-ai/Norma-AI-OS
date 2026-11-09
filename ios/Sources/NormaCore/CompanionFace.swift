import Foundation

public enum CompanionPhase: String, Sendable {
    case idle, listening, thinking, speaking
}

/// Face and idle look for the companion. Directed emotions, speech, and thinking
/// stay ahead of the slow idle cycle. Weights stay below a directed expression.
public struct CompanionFace: Equatable, Sendable {
    public var happy: Float
    public var curious: Float
    public var concern: Float
    public var headYaw: Float
    public var headPitch: Float

    public static let still = CompanionFace(happy: 0, curious: 0, concern: 0, headYaw: 0, headPitch: 0)

    public static func resolve(phase: CompanionPhase, emotion: String, directed: Bool, userSpeaking: Bool,
                               reduceMotion: Bool, ornament: Bool, time: TimeInterval) -> CompanionFace {
        if userSpeaking { return .still }
        var face = CompanionFace.still
        if directed {
            switch emotion {
            case "happy": face.happy = 1
            case "curious": face.curious = 1
            case "concerned": face.concern = 1
            default: break
            }
        }
        if phase == .speaking && emotion == "neutral" && face.happy == 0 { face.happy = 0.25 }
        if phase == .thinking && emotion == "neutral" && face.happy == 0 && face.curious == 0 && face.concern == 0 {
            face.curious = 0.35
        }
        // 20s: 4s calm, 4s curious at 0.7, 4s calm, 4s smile at 0.6, 4s calm.
        // The morph layer already scales these down, so 0.6 reads as a real smile
        // and still stays under a directed weight of 1. Holds, not a per-frame oscillator.
        if !directed && phase != .speaking && phase != .thinking && !reduceMotion {
            var local = time.truncatingRemainder(dividingBy: 20)
            if local < 0 { local += 20 }
            if local >= 4 && local < 8 { face.curious = 0.7 }
            else if local >= 12 && local < 16 { face.happy = 0.6 }
        }
        // ±0.05 rad ≈ 2.9° over ~20s, and ±0.018 rad of pitch. A look, not a bobble.
        if ornament && !reduceMotion && !directed && (phase == .idle || phase == .listening) {
            face.headYaw = Float(sin(time * 0.31) * 0.05)
            face.headPitch = Float(sin(time * 0.17) * 0.018)
        }
        return face
    }
}
