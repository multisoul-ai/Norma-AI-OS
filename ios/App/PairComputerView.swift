import SwiftUI
import NormaCore

struct PairComputerView: View {
    let model: ComputerModel
    @Environment(\.dismiss) private var dismiss
    @Environment(\.scenePhase) private var scenePhase
    @State private var scanning = false
    @State private var enteringCode = false
    @State private var offer: PairingOffer?
    @State private var pending: PairingPending?
    @State private var busy = false
    @State private var error: String?
    @State private var operation: Task<Void, Never>?

    var body: some View {
        NavigationStack {
            List {
                if let pending {
                    Section {
                        VStack(alignment: .leading, spacing: 16) {
                            Label("等待电脑确认", systemImage: "lock.shield").font(.headline)
                            Text(offer?.machineName ?? "电脑").font(.subheadline).foregroundStyle(.secondary)
                            Text(pending.verificationCode).font(.largeTitle.monospacedDigit().weight(.medium))
                                .accessibilityLabel("核对码 \(pending.verificationCode.map(String.init).joined(separator: " "))")
                            Text("确认电脑显示相同核对码，再在电脑上选择允许此手机访问的项目和权限。")
                                .font(.subheadline).foregroundStyle(.secondary)
                            if busy { ProgressView("等待授权…").font(.subheadline) }
                        }.padding(.vertical, 12)
                    }
                } else if let offer {
                    Section("确认要连接的电脑") {
                        Label(offer.machineName, systemImage: "laptopcomputer").font(.headline).padding(.vertical, 8)
                        Text("确认这是你要连接的电脑。已配对的电脑会恢复连接；首次连接需在电脑上批准。")
                            .font(.subheadline).foregroundStyle(.secondary)
                        LabeledContent("设备指纹", value: String(offer.certificateSHA256.prefix(12)))
                            .font(.caption.monospaced())
                    }
                    Section {
                        Button("连接电脑", action: pair).disabled(busy)
                        Button("重新扫描") { self.offer = nil; error = nil; scanning = true }.disabled(busy)
                        Button("输入新的验证码") { enteringCode = true }.disabled(busy)
                        if busy { ProgressView("正在连接电脑…").font(.subheadline) }
                    }
                } else {
                    Section {
                        VStack(alignment: .leading, spacing: 18) {
                            Image(systemName: "qrcode.viewfinder").font(.largeTitle).foregroundStyle(.secondary)
                            Text("连接你的电脑").font(.title3.weight(.semibold))
                            step("1", "在 Mac 上打开 Norma", "保持窗口打开，Agent 才能继续工作。")
                            step("2", "打开手机连接", "在电脑上生成 6 位验证码或二维码。")
                            step("3", "输入验证码或扫码", "在电脑上确认核对码，选择访问权限。")
                        }.padding(.vertical, 12)
                    }
                    Section {
                        Button { scanning = true } label: { Label("扫描二维码", systemImage: "camera").frame(minHeight: 30) }
                            .accessibilityIdentifier("scan-qr")
                        Button { enteringCode = true } label: { Label("输入验证码", systemImage: "keyboard").frame(minHeight: 30) }
                            .accessibilityIdentifier("enter-pairing-code")
                    } footer: { Text("手机和电脑连接同一局域网，或在两端登录 Tailscale 并开启连接。") }
                }
                if let error {
                    Section {
                        Text(error).font(.subheadline).foregroundStyle(.orange)
                        if offer != nil {
                            Button("查询授权结果") { checkAuthorization() }.disabled(busy)
                            Button("重新扫描") { cancel(); offer = nil; pending = nil; self.error = nil; scanning = true }
                            if pending != nil { Button("输入新的验证码") { enteringCode = true }.disabled(busy) }
                        }
                    }
                }
            }
            .scrollContentBackground(.hidden).background(.black)
            .navigationTitle("添加电脑").navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("取消") { cancel(); dismiss() }.accessibilityIdentifier("pairing-cancel")
                }
            }
            .fullScreenCover(isPresented: $scanning) {
                ScannerScreen { text in
                    scanning = false
                    do { offer = try PairingOffer.decode(Data(text.utf8)); error = nil }
                    catch { self.error = "二维码无效或版本不兼容，请在电脑上重新生成。" }
                }
            }
            .sheet(isPresented: $enteringCode) {
                PairingCodeView { value in
                    cancel(); offer = value; pending = nil; error = nil; enteringCode = false
                }
            }
        }
        .onDisappear { cancel() }
        .onChange(of: scenePhase) { _, phase in
            if phase == .background {
                cancel()
                if offer != nil { error = "配对已暂停。返回后可查询授权结果，或重新扫描。" }
            }
        }
    }

    private func step(_ number: String, _ title: String, _ subtitle: String) -> some View {
        HStack(alignment: .top, spacing: 12) {
            Text(number).font(.caption.weight(.semibold)).frame(width: 24, height: 24).background(.white.opacity(0.08), in: Circle())
            VStack(alignment: .leading, spacing: 4) {
                Text(title).font(.subheadline.weight(.medium))
                Text(subtitle).font(.caption).foregroundStyle(.secondary)
            }
        }
    }
    private func cancel() { operation?.cancel(); operation = nil; busy = false }
    private func pair() {
        guard let offer, !busy else { return }
        busy = true; error = nil
        operation = Task {
            do {
                let result = try await model.client.connect(offer, deviceName: UIDevice.current.name)
                try Task.checkCancellation()
                // Only trust new route metadata after a verified host response. The in-memory
                // offer and Keychain credential still allow querying a lost pairing response.
                try await model.remember(PairedMachine(offer: offer))
                try Task.checkCancellation()
                switch result {
                case .authorized(let grant):
                    model.select(offer.machineID, verified: grant)
                    model.refresh()
                    busy = false; dismiss(); return
                case .pending(let request): pending = request
                }
                // Host owns QR expiry. Local monotonic deadline only bounds UI polling.
                let deadline = ContinuousClock.now.advanced(by: .seconds(120))
                while ContinuousClock.now < deadline {
                    do { try await authenticate(offer); return }
                    catch MobileError.unauthorized { try await Task.sleep(for: .seconds(2)) }
                }
                throw MobileError.timedOut
            } catch {
                guard !Task.isCancelled else { return }
                self.error = error.localizedDescription; busy = false
            }
        }
    }
    private func checkAuthorization() {
        guard let offer, !busy else { return }
        busy = true; error = nil
        operation = Task {
            do { try await authenticate(offer) }
            catch {
                guard !Task.isCancelled else { return }
                self.error = error.localizedDescription; busy = false
            }
        }
    }
    private func authenticate(_ offer: PairingOffer) async throws {
        let machine = PairedMachine(offer: offer)
        let grant = try await model.client.authenticate(machine)
        try Task.checkCancellation()
        try await model.remember(machine)
        try Task.checkCancellation()
        model.select(machine.id, verified: grant)
        model.refresh()
        busy = false; dismiss()
    }
}
