import SwiftUI
import NormaCore

struct ConversationToolCard: View {
    let card: ConversationDetailCard
    let identity: String
    @Environment(\.dynamicTypeSize) private var typeSize
    @ScaledMetric(relativeTo: .subheadline) private var iconSize = 20.0
    @State private var expanded = false

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Button { expanded.toggle() } label: {
                VStack(alignment: .leading, spacing: 5) {
                    HStack(alignment: .top, spacing: 10) {
                        Image(systemName: card.symbol).font(.subheadline)
                            .frame(width: iconSize, height: iconSize).foregroundStyle(.secondary).accessibilityHidden(true)
                        VStack(alignment: .leading, spacing: 5) {
                            (typeSize.isAccessibilitySize ? AnyLayout(VStackLayout(alignment: .leading, spacing: 4)) : AnyLayout(HStackLayout(spacing: 8))) {
                                Text(card.title).font(.subheadline.weight(.medium))
                                    .fixedSize(horizontal: false, vertical: true)
                                if !typeSize.isAccessibilitySize { Spacer(minLength: 0) }
                                if let status = card.status {
                                    Text(status).font(.caption).foregroundStyle(card.isFailure ? .orange : .secondary)
                                        .fixedSize(horizontal: false, vertical: true)
                                }
                            }
                        }.frame(maxWidth: .infinity, alignment: .leading)
                        Image(systemName: expanded ? "chevron.up" : "chevron.down")
                            .font(.caption2.weight(.semibold)).foregroundStyle(.secondary).accessibilityHidden(true)
                    }
                    if let summary = card.summary {
                        Text(summary).font(.caption.monospaced()).foregroundStyle(.secondary)
                            .lineLimit(typeSize.isAccessibilitySize ? nil : 2)
                            .fixedSize(horizontal: false, vertical: true)
                            .padding(.leading, typeSize.isAccessibilitySize ? 0 : iconSize + 10)
                    }
                }
                .padding(12).frame(minHeight: 44).contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityIdentifier("tool-card-" + identity)
            .accessibilityValue(expanded ? "已展开" : "已收起")
            .accessibilityHint("查看工具的完整输入与输出")
            if expanded {
                Divider().padding(.horizontal, 12)
                VStack(alignment: .leading, spacing: 12) {
                    ForEach(Array(card.sections.enumerated()), id: \.offset) { index, section in
                        VStack(alignment: .leading, spacing: 6) {
                            Text(section.title).font(.caption.weight(.medium)).foregroundStyle(.secondary)
                            ReviewText(text: section.text)
                                .frame(height: section.text.contains("\n") ? 180 : 72)
                                .clipShape(RoundedRectangle(cornerRadius: 8))
                                .accessibilityIdentifier("tool-section-\(identity)-\(index)")
                        }
                    }
                    DisclosureGroup("原始记录") {
                        ReviewText(text: card.raw).frame(height: 220)
                            .clipShape(RoundedRectangle(cornerRadius: 8)).padding(.top, 8)
                    }.font(.caption).frame(minHeight: 44)
                }.padding(12)
            }
        }
        .background(Color.white.opacity(0.035), in: RoundedRectangle(cornerRadius: 12))
        .overlay(RoundedRectangle(cornerRadius: 12).strokeBorder(Color.white.opacity(0.12), lineWidth: 1))
    }
}
