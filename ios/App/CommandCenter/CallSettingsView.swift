import SwiftUI
import NormaCore

struct CallSettingsView: View {
    @ObservedObject var call: VoiceCall
    let manageComputers: () -> Void
    @Environment(\.dismiss) private var dismiss
    var body: some View {
        NavigationStack {
            Form {
                Section("通话") {
                    LabeledContent("状态", value: call.state.rawValue)
                    Toggle("显示字幕", isOn: $call.subtitles)
                    Toggle("暂缓任务汇报", isOn: $call.reportsPaused)
                    if call.isCalling {
                        LabeledContent("音频输出", value: call.outputName)
                        if !call.externalOutput {
                            Button(call.speaker ? "切换到听筒" : "切换到扬声器", action: call.toggleSpeaker)
                        }
                    }
                }
                Section {
                    Picker("对话服务", selection: $call.provider) {
                        ForEach(VoiceProvider.allCases) { Text($0.title).tag($0) }
                    }.accessibilityIdentifier("realtime-provider")
                    if call.usesAutomaticAuthorization {
                        LabeledContent("接入方式", value: "自动配置")
                            .accessibilityIdentifier("realtime-automatic-service")
                    } else {
                      TextField("语音网关 HTTPS 地址", text: $call.gateway)
                        .textInputAutocapitalization(.never).autocorrectionDisabled().keyboardType(.URL)
                        .accessibilityIdentifier("realtime-gateway")
                    SecureField("访问口令", text: $call.accessToken)
                        .textInputAutocapitalization(.never).autocorrectionDisabled()
                        .accessibilityIdentifier("realtime-access-token")
                    }
                } header: { Text("语音服务") } footer: {
                    Text(call.usesAutomaticAuthorization
                         ? "完成电脑配对后即可使用，无需填写 Key 或访问口令。授权自动保存在本机，电脑暂时离线时仍可聊天。"
                         : "沿用你的语音网关获取临时通话凭证。访问口令保存在本机钥匙串，手机无需保存模型服务的长期密钥。")
                }.disabled(call.isCalling)
                Section {
                    Button("切换或管理电脑") { dismiss(); manageComputers() }.accessibilityIdentifier("manage-computers")
                    Text(call.toolsStatusText).font(.caption).foregroundStyle(.secondary)
                } header: { Text("任务执行") } footer: {
                    Text("浏览其他电脑和项目不会更改本次通话的派工目标。挂断只结束音频，已安排的任务继续执行。")
                }
            }.scrollContentBackground(.hidden).background(.black)
                .navigationTitle("通话设置").navigationBarTitleDisplayMode(.inline)
                .toolbar { ToolbarItem(placement: .cancellationAction) {
                    Button("完成") { dismiss() }.accessibilityIdentifier("realtime-settings-close")
                } }
        }.preferredColorScheme(.dark).tint(.white)
    }
}
