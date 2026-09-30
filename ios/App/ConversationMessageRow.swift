import SwiftUI
import NormaCore

struct ConversationMessageRow: View {
    let item: ConversationItem
    let profile: String
    let imageSource: (String) async throws -> Data
    @Environment(\.dynamicTypeSize) private var typeSize
    @ScaledMetric(relativeTo: .subheadline) private var textSize = 15.0
    private var isUser: Bool { item.role == "user" }
    private var attachments: (text: String, paths: [String])? { isUser ? ChatImageMessage.parse(item.text) : nil }
    private var visibleText: String { attachments?.text ?? item.text }

    private var message: AttributedString {
        guard !isUser else { return AttributedString(visibleText) }
        var value = (try? AttributedString(markdown: item.text, options: .init(
            interpretedSyntax: .inlineOnlyPreservingWhitespace,
            failurePolicy: .returnPartiallyParsedIfPossible))) ?? AttributedString(item.text)
        for run in value.runs where run.inlinePresentationIntent?.contains(.code) == true {
            value[run.range].font = .system(size: textSize - 1, design: .monospaced)
            value[run.range].backgroundColor = WorkspaceStyle.raised
        }
        return value
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            if let attachments {
                ScrollView(.horizontal) {
                    HStack(spacing: 8) {
                        ForEach(Array(attachments.paths.enumerated()), id: \.offset) { index, path in
                            if let id = ChatImageMessage.imageID(path: path) {
                                ChatPhotoTile(id: id, label: "图片 \(index + 1)", load: { try await imageSource(id) })
                            }
                        }
                    }
                }.scrollIndicators(.hidden)
            }
            if !visibleText.isEmpty {
                if !isUser {
                    HStack(spacing: 7) {
                        ProviderMark(profile: profile, size: 18)
                        Text(item.role == "assistant" ? profile : "工具")
                            .font(.caption.weight(.semibold)).foregroundStyle(WorkspaceStyle.muted)
                    }
                }
                Text(message).font(.system(size: textSize)).lineSpacing(5)
                    .foregroundStyle(.white.opacity(0.9)).tint(.white)
                    .textSelection(.enabled)
                    .fixedSize(horizontal: false, vertical: true)
            }
            ForEach(item.detailCards) { card in
                ConversationToolCard(card: card, identity: "\(item.id)-\(card.id)")
            }
        }
        .padding(isUser ? 14 : 0)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(isUser ? WorkspaceStyle.raised : .clear, in: RoundedRectangle(cornerRadius: 18))
        .padding(.leading, isUser && !typeSize.isAccessibilitySize ? 28 : 0)
        .id(item.id)
    }
}
