import SwiftUI
import NormaCore

struct ConversationMessageRow: View {
    let item: ConversationItem
    let profile: String
    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            if !item.text.isEmpty {
                Text(item.role == "user" ? "你" : item.role == "assistant" ? profile : "工具")
                    .font(.caption.weight(.semibold)).foregroundStyle(.secondary)
                Text(item.text).font(.subheadline).textSelection(.enabled)
                    .fixedSize(horizontal: false, vertical: true)
            }
            ForEach(item.detailCards) { card in
                ConversationToolCard(card: card, identity: "\(item.id)-\(card.id)")
            }
        }
        .padding(item.role == "user" ? 14 : 0)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(item.role == "user" ? Color.white.opacity(0.06) : .clear, in: RoundedRectangle(cornerRadius: 16))
        .id(item.id)
    }
}
