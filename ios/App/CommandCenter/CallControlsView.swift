import SwiftUI
import NormaCore

struct CallControlsView: View {
    @ObservedObject var call: VoiceCall
    let machine: PairedMachine?
    let project: ProjectSnapshot?
    var typing = false
    let toggleText: () -> Void

    var body: some View {
        HStack(alignment: .top, spacing: 24) {
            control(call.muted ? "mic.slash" : "mic", label: call.muted ? "取消静音" : "静音",
                    id: "realtime-call-mute", action: call.toggleMute)
                .disabled(call.state != .connected)
                .contextMenu {
                    Button("打断回复", action: call.interrupt).accessibilityIdentifier("realtime-call-interrupt")
                    Button(call.reportsPaused ? "恢复汇报" : "先别汇报") { call.setReportsPaused(!call.reportsPaused) }
                        .accessibilityIdentifier("realtime-reports")
                }
            if call.isCalling {
                control("phone.down.fill", label: "结束对话", id: "realtime-call-end", destructive: true, action: call.hangup)
            } else {
                control("phone.fill", label: "开始对话", id: "realtime-call-start", primary: true) {
                    call.connect(machine: machine, project: project)
                }
            }
            control(typing ? "person.crop.rectangle" : "keyboard", label: typing ? "返回角色" : "输入文字",
                    id: "norma-text-toggle", action: toggleText)
        }.buttonStyle(IdeaPressStyle())
            .padding(.horizontal, 24).padding(.vertical, 12)
            .frame(maxWidth: .infinity).background(IdeaPalette.background)
    }

    private func control(_ symbol: String, label: String, id: String, destructive: Bool = false,
                         primary: Bool = false, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            VStack(spacing: 6) {
                Image(systemName: symbol).font(.system(size: 25, weight: .medium))
                    .frame(width: 64, height: 64)
                    .foregroundStyle(destructive || primary ? .white : IdeaPalette.secondary)
                    .background(destructive ? Color.red : primary ? IdeaPalette.blue : IdeaPalette.softGray, in: Circle())
                Text(label).font(.caption).foregroundStyle(IdeaPalette.secondary)
            }.frame(maxWidth: .infinity, minHeight: 84)
        }.accessibilityLabel(label).accessibilityIdentifier(id)
    }
}
