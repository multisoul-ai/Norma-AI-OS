import SwiftUI
import NormaCore

struct PairingCodeView: View {
    let onOffer: (PairingOffer) -> Void
    @Environment(\.dismiss) private var dismiss
    @Environment(\.scenePhase) private var scenePhase
    @AppStorage("pairing.lastAddress") private var lastAddress = ""
    @State private var address = ""
    @State private var code = ""
    @State private var error: String?
    @State private var operation: Task<Void, Never>?
    @State private var busy = false

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    Text("打开 Mac 的「手机连接」，输入其中显示的电脑地址和 6 位验证码。")
                        .font(.subheadline).foregroundStyle(.secondary)
                }
                Section {
                    TextField("例如 192.168.1.2", text: $address)
                        .keyboardType(.URL).textInputAutocapitalization(.never).autocorrectionDisabled()
                        .accessibilityLabel("电脑地址").accessibilityIdentifier("pairing-address-input")
                        .disabled(busy)
                } header: { Text("电脑地址") } footer: { Text("支持局域网 IP 或 Tailscale 地址，默认端口 9443。连接后会记住这台电脑。") }
                Section {
                    TextField("6 位数字", text: $code)
                        .font(.title2.monospacedDigit()).keyboardType(.numberPad).textContentType(.oneTimeCode)
                        .privacySensitive().accessibilityLabel("六位验证码")
                        .accessibilityIdentifier("pairing-code-input").disabled(busy)
                } header: { Text("验证码") } footer: { Text("2 分钟内有效，每组最多验证 3 次。手机访问权限仍需在 Mac 上批准。") }
                if let error {
                    Section { Text(error).foregroundStyle(.orange).font(.subheadline).accessibilityAddTraits(.updatesFrequently) }
                }
                Section {
                    Button(action: verify) {
                        HStack {
                            Text(busy ? "正在验证…" : "验证电脑")
                            Spacer()
                            if busy { ProgressView() }
                        }
                    }.disabled(busy || address.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || !PairingCode.validCode(code))
                        .accessibilityIdentifier("verify-pairing-code")
                }
            }
            .scrollContentBackground(.hidden).background(.black)
            .navigationTitle("输入验证码").navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("返回") { cancel(); dismiss() }.accessibilityIdentifier("pairing-code-close")
                }
            }
        }
        .onAppear { address = lastAddress }
        .onDisappear { cancel(); code = "" }
        .onChange(of: scenePhase) { _, phase in
            if phase == .background { cancel(); code = "" }
        }
    }

    private func cancel() { operation?.cancel(); operation = nil; busy = false }
    private func verify() {
        guard !busy else { return }
        error = nil
        do { _ = try PairingCode.endpoint(address) }
        catch { self.error = error.localizedDescription; return }
        busy = true
        operation = Task {
            do {
                let offer = try await PairingCode.verify(address: address, code: code)
                try Task.checkCancellation()
                lastAddress = address.trimmingCharacters(in: .whitespacesAndNewlines)
                busy = false; code = ""; onOffer(offer)
            } catch {
                guard !Task.isCancelled else { return }
                if (error as? MobileError) == .timedOut || (error as? URLError)?.code == .timedOut {
                    self.error = "连接电脑超时。请检查电脑地址，并确认两台设备处于同一局域网或已连接 Tailscale，再重试。"
                } else {
                    self.error = (error as? MobileError)?.localizedDescription ?? MobileError.connectionFailed.localizedDescription
                }
                busy = false
            }
        }
    }
}
