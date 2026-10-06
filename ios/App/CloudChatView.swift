import SwiftUI
import NormaCore

struct CloudChatView: View {
    @Bindable var model: CloudChatModel
    @Environment(ComputerModel.self) private var computers
    @State private var searching = false
    @State private var query = ""
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency

    private var visibleMessages: [CloudChatMessage] {
        let term = query.trimmingCharacters(in: .whitespacesAndNewlines)
        return term.isEmpty ? model.messages : model.messages.filter { $0.text.localizedStandardContains(term) }
    }

    var body: some View {
        VStack(spacing: 0) {
            header
            if searching {
                HStack(spacing: 9) {
                    Image(systemName: "magnifyingglass").foregroundStyle(IdeaPalette.secondary)
                    TextField("搜索本机对话", text: $query)
                        .accessibilityIdentifier("norma-text-search-field")
                    if !query.isEmpty {
                        Button { query = "" } label: { Image(systemName: "xmark.circle.fill") }
                            .accessibilityLabel("清除搜索").frame(width: 44, height: 44)
                    }
                }
                .padding(.leading, 15).padding(.trailing, 3)
                .background(IdeaPalette.softGray, in: Capsule())
                .padding(.horizontal, 20).padding(.bottom, 12)
            }
            ScrollViewReader { reader in
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 16) {
                        if model.credential == nil { invitationCard }
                        else if model.messages.isEmpty { welcomeCard }
                        ForEach(visibleMessages) { message in
                            messageRow(message).id(message.id)
                        }
                        if model.sending && !model.partialReply.isEmpty {
                            normaBubble(model.partialReply, streaming: true).id("streaming")
                                .transition(reduceMotion ? .opacity : .move(edge: .bottom).combined(with: .opacity))
                        }
                        if let error = model.error {
                            Label(error, systemImage: "exclamationmark.circle")
                                .font(.subheadline).foregroundStyle(Color(red: 0.62, green: 0.25, blue: 0.24))
                                .padding(15).frame(maxWidth: .infinity, alignment: .leading)
                                .background(Color(red: 1, green: 0.94, blue: 0.93), in: RoundedRectangle(cornerRadius: 16))
                                .accessibilityIdentifier("norma-text-error")
                                .transition(.opacity)
                        }
                        Color.clear.frame(height: 1).id("bottom")
                    }
                    .padding(.horizontal, 20).padding(.top, 22).padding(.bottom, 20)
                }
                .scrollDismissesKeyboard(.interactively)
                .onChange(of: model.messages.count) { _, _ in reader.scrollTo("bottom", anchor: .bottom) }
                .onChange(of: model.partialReply) { _, _ in reader.scrollTo("bottom", anchor: .bottom) }
            }
            composer
        }
        .animation(reduceMotion ? .easeOut(duration: 0.12) : .snappy(duration: 0.25),
                   value: model.sending && !model.partialReply.isEmpty)
        .animation(.easeOut(duration: 0.18), value: model.error != nil)
        .background(IdeaPalette.background.ignoresSafeArea())
        .foregroundStyle(IdeaPalette.ink)
        .tint(IdeaPalette.blue)
    }

    private var header: some View {
        HStack {
            Text("对话记录").font(.subheadline.weight(.semibold))
            Spacer()
            Button {
                withAnimation(reduceMotion ? .easeOut(duration: 0.12) : .snappy(duration: 0.25)) {
                    searching.toggle()
                    if !searching { query = "" }
                }
            } label: { Image(systemName: "magnifyingglass").frame(width: 44, height: 44) }
                .accessibilityLabel("搜索对话").accessibilityIdentifier("norma-text-search")
        }.padding(.horizontal, 20)
    }

    private var invitationCard: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text(model.enrolling ? "正在启用云端聊天…" : computers.machines.isEmpty ? "连接你的电脑" : "启用云端聊天")
                .font(.title2.bold()).accessibilityAddTraits(.isHeader)
            Text(computers.machines.isEmpty
                 ? "先在「已连接电脑」中与 Mac 配对，再回到这里启用云端聊天。无需设置模型或密钥。"
                 : "电脑配对已完成。保持 Mac 在线，启用云端聊天后即可在这里与 Norma 对话。")
                .font(.body).foregroundStyle(IdeaPalette.secondary)
            if model.enrolling { ProgressView().accessibilityLabel("正在启用云端聊天") }
            else if let machine = computers.selected ?? computers.machines.first {
                Button("启用云端聊天") {
                    Task {
                        do { try await model.register(on: machine, using: computers.client) }
                        catch { model.error = error.localizedDescription }
                    }
                }.frame(minHeight: 44).accessibilityIdentifier("norma-registration-retry")
            }
        }
        .padding(22).frame(maxWidth: .infinity, alignment: .leading)
        .background(.white, in: RoundedRectangle(cornerRadius: 22))
        .overlay(RoundedRectangle(cornerRadius: 22).strokeBorder(IdeaPalette.softGray, lineWidth: 1))
    }

    private var welcomeCard: some View {
        VStack(alignment: .leading, spacing: 9) {
            Text("和 Norma 聊聊").font(.title3.bold())
            Text("和我说说你的想法。私聊记录保存在这部 iPhone。")
                .font(.subheadline).foregroundStyle(IdeaPalette.secondary)
        }
        .padding(20).frame(maxWidth: .infinity, alignment: .leading)
        .background(.white, in: RoundedRectangle(cornerRadius: 20))
    }

    @ViewBuilder private func messageRow(_ message: CloudChatMessage) -> some View {
        if message.role == .norma {
            normaBubble(message.text, streaming: false)
        } else {
            VStack(alignment: .trailing, spacing: 6) {
                Text(message.text)
                    .font(.body).foregroundStyle(.white)
                    .padding(.horizontal, 16).padding(.vertical, 12)
                    .background(IdeaPalette.blue, in: RoundedRectangle(cornerRadius: 19))
                    .textSelection(.enabled)
                if message.state == .failed {
                    Button("回复中断 · 再次发送") { model.prepareRetry(message) }
                        .font(.caption.weight(.medium)).frame(minHeight: 44)
                        .accessibilityIdentifier("norma-text-retry-\(message.id)")
                } else if message.state == .pending {
                    Text("正在等待回复").font(.caption).foregroundStyle(IdeaPalette.secondary)
                }
            }.frame(maxWidth: .infinity, alignment: .trailing)
        }
    }

    private func normaBubble(_ text: String, streaming: Bool) -> some View {
        HStack(alignment: .top, spacing: 9) {
            Image("NormaPortrait").resizable().scaledToFill()
                .frame(width: 29, height: 29).clipShape(Circle()).accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 7) {
                Text(text).font(.body).textSelection(.enabled)
                if streaming { Label("正在回复", systemImage: "ellipsis").font(.caption).foregroundStyle(IdeaPalette.secondary) }
            }
            .padding(.horizontal, 16).padding(.vertical, 12)
            .background(.white, in: RoundedRectangle(cornerRadius: 19))
            Spacer(minLength: 25)
        }
    }

    private var composer: some View {
        HStack(alignment: .bottom, spacing: 10) {
            TextField("问 Norma…", text: $model.draft, axis: .vertical)
                .lineLimit(1...5)
                .font(.body).padding(.horizontal, 15).padding(.vertical, 11)
                .background(.white, in: RoundedRectangle(cornerRadius: 22))
                .disabled(model.credential == nil)
                .accessibilityIdentifier("norma-text-input")
            Button { Task { await model.send() } } label: {
                Image(systemName: "arrow.up")
                    .font(.system(size: 18, weight: .semibold))
                    .foregroundStyle(.white)
                    .frame(width: 46, height: 46)
                    .background(canSend ? IdeaPalette.blue : IdeaPalette.faint, in: Circle())
            }
            .disabled(!canSend)
            .accessibilityLabel("发送")
            .accessibilityIdentifier("norma-text-send")
        }
        .padding(.horizontal, 16).padding(.vertical, 11)
        .background { chromeMaterial }
    }

    private var canSend: Bool {
        model.loaded && model.credential != nil && !model.enrolling && !model.sending &&
            !model.draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    @ViewBuilder private var chromeMaterial: some View {
        if reduceTransparency { Color.white }
        else if #available(iOS 26.0, *) {
            Color.clear.glassEffect(.regular)
        } else { Color.white.opacity(0.9).background(.regularMaterial) }
    }
}

enum CloudInvitationLink {
    static func code(from url: URL) -> String? {
        guard let parts = URLComponents(url: url, resolvingAgainstBaseURL: false),
              parts.scheme?.lowercased() == "norma", parts.host?.lowercased() == "invite",
              parts.path.isEmpty || parts.path == "/",
              let code = parts.queryItems?.first(where: { $0.name == "code" })?.value,
              code.range(of: "^[A-Za-z0-9_-]{43}$", options: .regularExpression) != nil else { return nil }
        return code
    }
}
