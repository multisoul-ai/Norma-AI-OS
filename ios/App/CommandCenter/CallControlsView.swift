import SwiftUI
import NormaCore

struct CallControlsView: View {
    @ObservedObject var call: VoiceCall
    let machine: PairedMachine?
    let project: ProjectSnapshot?
    var body: some View {
        VStack(spacing: 10) {
            if call.isCalling {
                HStack(spacing: 16) {
                    control(call.muted ? "mic.slash" : "mic", label: call.muted ? "取消静音" : "静音", selected: call.muted,
                            id: "realtime-call-mute", action: call.toggleMute)
                        .disabled(call.state != .connected).opacity(call.state == .connected ? 1 : 0.45)
                    control("hand.raised", label: "打断回复", id: "realtime-call-interrupt", action: call.interrupt)
                        .disabled(call.state != .connected).opacity(call.state == .connected ? 1 : 0.45)
                    control(call.reportsPaused ? "bell.slash" : "bell", label: call.reportsPaused ? "恢复汇报" : "先别汇报",
                            selected: call.reportsPaused, id: "realtime-reports") { call.setReportsPaused(!call.reportsPaused) }
                    control("phone.down.fill", label: "结束通话", destructive: true, id: "realtime-call-end", action: call.hangup)
                }
            } else {
                Button { call.connect(machine: machine, project: project) } label: {
                    Label("和 Norma 聊聊", systemImage: "headphones")
                        .font(.subheadline.weight(.semibold)).frame(maxWidth: .infinity, minHeight: 48)
                        .foregroundStyle(.black).background(Color(white: 0.94), in: Capsule())
                }.accessibilityLabel("开始通话").accessibilityIdentifier("realtime-call-start")
            }
            Text(call.isCalling ? call.outputName + " · " + (call.reportsPaused ? "稍后汇报" : "随时可以插话") : "实时语音 · 由你主动接通")
                .font(.caption2).foregroundStyle(.secondary)
        }.buttonStyle(WorkspacePressStyle()).padding(.horizontal, 20).padding(.vertical, 14)
            .frame(maxWidth: .infinity).background(Color(white: 0.025))
    }
    private func control(_ symbol: String, label: String, selected: Bool = false, destructive: Bool = false,
                         id: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Image(systemName: symbol).font(.body).frame(width: 48, height: 48)
                .foregroundStyle(selected ? .black : .white)
                .background(destructive ? Color(red: 0.58, green: 0.24, blue: 0.21) : selected ? Color(white: 0.9) : WorkspaceStyle.raised, in: Circle())
        }.accessibilityLabel(label).accessibilityAddTraits(selected ? .isSelected : []).accessibilityIdentifier(id)
    }
}
