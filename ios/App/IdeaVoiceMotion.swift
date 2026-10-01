import SwiftUI

struct IdeaVoiceWaveform: View {
    let recording: Bool
    let level: Float
    let reduceMotion: Bool

    var body: some View {
        HStack(alignment: .center, spacing: 3) {
            ForEach(0..<47, id: \.self) { index in
                let distance = abs(index - 23)
                let pattern = abs(sin(Double(index) * 0.71))
                let base = distance > 18 ? 5.0 : 9.0 + pattern * Double(max(0, 75 - distance * 2))
                let amplitude = recording && !reduceMotion ? 0.22 + Double(level) * 0.95 : 0.66
                let restingHeight = max(4, base * 0.66)
                Capsule()
                    .fill(IdeaPalette.green.opacity(index.isMultiple(of: 4) ? 0.72 : 0.5))
                    .frame(maxWidth: 5).frame(height: restingHeight)
                    // Keep layout fixed; the sampled microphone envelope only transforms each bar.
                    .scaleEffect(x: 1, y: max(4, base * amplitude) / restingHeight)
            }
        }
        .frame(maxWidth: .infinity).frame(height: 90)
        .animation(reduceMotion ? nil : .easeOut(duration: 0.12), value: level)
        .animation(reduceMotion ? nil : .easeOut(duration: 0.2), value: recording)
        .accessibilityElement(children: .ignore)
    }
}

struct IdeaVoiceRecordStyle: ButtonStyle {
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.isEnabled) private var enabled

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .opacity(enabled ? (configuration.isPressed ? 0.83 : 1) : 0.55)
            .scaleEffect(configuration.isPressed && !reduceMotion ? 0.94 : 1)
            .animation(reduceMotion ? .easeOut(duration: 0.1) :
                configuration.isPressed ? .easeOut(duration: 0.1) : .spring(response: 0.25, dampingFraction: 0.85),
                value: configuration.isPressed)
    }
}
