import AVFoundation
import SwiftUI

struct ChatVoiceInput<Keyboard: View>: View {
    @Binding var text: String
    @Binding var recording: Bool
    let disabled: Bool
    let focus: (Bool) -> Void
    @ViewBuilder let keyboard: () -> Keyboard
    @EnvironmentObject private var call: VoiceCall
    @Environment(\.scenePhase) private var scenePhase
    @StateObject private var voice = ChatDictation()
    @State private var voiceMode = false
    @State private var holding = false
    @State private var cancelOnRelease = false
    @GestureState private var pressing = false

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            if let error = voice.error {
                Text(error).foregroundStyle(.orange).accessibilityIdentifier("chat-voice-error")
            }
            if voiceMode && call.isCalling { Text("请先结束 Norma 通话，再使用语音输入").foregroundStyle(.secondary) }
            if voice.active {
                Text(voice.partial.isEmpty ? "松开转文字，上滑取消" : voice.partial)
                    .lineLimit(2).foregroundStyle(.secondary).accessibilityIdentifier("chat-voice-status")
            }
            HStack(alignment: .bottom, spacing: 8) {
                Button {
                    voice.cancel(); voiceMode.toggle(); focus(!voiceMode)
                } label: {
                    Image(systemName: voiceMode ? "keyboard" : "waveform")
                        .font(.system(size: 20)).frame(width: 44, height: 44)
                        .foregroundStyle(IdeaPalette.ink)
                        .background(IdeaPalette.softGray, in: Circle())
                }
                .buttonStyle(WorkspacePressStyle()).hoverEffect(.highlight)
                .disabled(disabled)
                .accessibilityLabel(voiceMode ? "切换到键盘输入" : "切换到语音输入")
                .accessibilityIdentifier("chat-input-mode")
                if voiceMode { holdButton } else { keyboard() }
            }
        }
        .task(id: voiceMode) { if voiceMode && !call.isCalling { await voice.preparePermissions() } }
        .onChange(of: voice.active) { _, active in recording = active }
        .onChange(of: disabled) { _, value in if value { voice.cancel() } }
        .onChange(of: call.isCalling) { _, calling in if calling { voice.interrupt() } }
        .onChange(of: scenePhase) { _, phase in
            if phase == .background || (phase == .inactive && voice.state.phase == .recording) { voice.interrupt() }
        }
        .onReceive(NotificationCenter.default.publisher(for: AVAudioSession.interruptionNotification)) { _ in voice.interrupt() }
        .onReceive(NotificationCenter.default.publisher(for: AVAudioSession.routeChangeNotification)) { note in
            if (note.userInfo?[AVAudioSessionRouteChangeReasonKey] as? UInt) == AVAudioSession.RouteChangeReason.oldDeviceUnavailable.rawValue {
                voice.interrupt()
            }
        }
        .onChange(of: pressing) { _, pressed in
            if !pressed && holding { holding = false; voice.cancel() }
        }
        .onDisappear {
            voice.cancel(); recording = false
        }
    }

    private var label: String {
        if voice.authorizing { return "请允许语音输入…" }
        if cancelOnRelease && holding { return "松开取消" }
        switch voice.state.phase {
        case .idle: return "按住说话"
        case .preparing: return "正在准备…"
        case .recording: return voice.readyToRelease ? "已录好，松开转文字" : "松开转文字"
        case .transcribing: return "正在识别…"
        }
    }

    private var holdButton: some View {
        let unavailable = disabled || call.isCalling || voice.authorizing || voice.state.phase == .transcribing
        return Text(label)
            .font(.subheadline.weight(.medium)).multilineTextAlignment(.center)
            .foregroundStyle(IdeaPalette.ink)
            .frame(maxWidth: .infinity, minHeight: 44).padding(.horizontal, 8)
            .background(holding ? (cancelOnRelease ? Color.orange.opacity(0.2) : .white) : IdeaPalette.softGray,
                        in: RoundedRectangle(cornerRadius: 22))
            .overlay(RoundedRectangle(cornerRadius: 22).strokeBorder(IdeaPalette.secondary.opacity(0.2), lineWidth: 1))
            .contentShape(RoundedRectangle(cornerRadius: 22))
            .opacity(unavailable ? 0.4 : 1).hoverEffect(.highlight)
            .disabled(unavailable)
            .gesture(DragGesture(minimumDistance: 0)
                .updating($pressing) { _, pressing, _ in pressing = true }
                .onChanged { value in
                    guard !unavailable else { return }
                    if !holding { holding = true; start() }
                    // 60pt = 44pt minimum target + 16pt hysteresis, avoiding accidental cancellation from a small tremor.
                    cancelOnRelease = value.translation.height < -60
                }
                .onEnded { _ in
                    guard holding else { return }
                    holding = false
                    if cancelOnRelease { voice.cancel() } else { voice.release() }
                    cancelOnRelease = false
                })
            .accessibilityAddTraits(.isButton)
            .accessibilityLabel(label)
            .accessibilityHint("按住录音，松开转文字，上滑取消。使用辅助功能时，激活开始，再次激活结束。")
            .accessibilityIdentifier("chat-voice-hold")
            .accessibilityAction {
                guard !unavailable else { return }
                if voice.active { voice.release() } else { start() }
            }
            .accessibilityAction(named: "取消录音") { voice.cancel() }
    }

    private func start() {
        focus(false)
        voice.begin(draft: text) { result in
            text = result; voiceMode = false; focus(true)
        }
    }
}
