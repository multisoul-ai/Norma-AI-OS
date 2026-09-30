import SwiftUI
import NormaCore

struct ConversationToolCard: View {
    let card: ConversationDetailCard
    let identity: String
    @Environment(\.dynamicTypeSize) private var typeSize
    @ScaledMetric(relativeTo: .subheadline) private var iconSize = 28.0
    @State private var expanded = false

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Button { expanded.toggle() } label: {
                VStack(alignment: .leading, spacing: 5) {
                    HStack(alignment: .top, spacing: 10) {
                        Image(systemName: card.symbol).font(.footnote)
                            .frame(width: iconSize, height: iconSize).foregroundStyle(WorkspaceStyle.muted)
                            .background(WorkspaceStyle.line.opacity(0.45), in: RoundedRectangle(cornerRadius: 8))
                            .accessibilityHidden(true)
                        VStack(alignment: .leading, spacing: 5) {
                            (typeSize.isAccessibilitySize ? AnyLayout(VStackLayout(alignment: .leading, spacing: 4)) : AnyLayout(HStackLayout(spacing: 8))) {
                                Text(card.title).font(.footnote.weight(.medium))
                                    .fixedSize(horizontal: false, vertical: true)
                                if !typeSize.isAccessibilitySize { Spacer(minLength: 0) }
                                if let status = card.status {
                                    Text(status).font(.caption2.weight(.medium)).foregroundStyle(card.isFailure ? .orange : WorkspaceStyle.muted)
                                        .fixedSize(horizontal: false, vertical: true)
                                        .padding(.horizontal, 7).padding(.vertical, 4)
                                        .background(card.isFailure ? Color.orange.opacity(0.1) : WorkspaceStyle.line.opacity(0.45), in: Capsule())
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
            .buttonStyle(WorkspacePressStyle())
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
        .background(WorkspaceStyle.surface, in: RoundedRectangle(cornerRadius: 14))
        .overlay(RoundedRectangle(cornerRadius: 14).strokeBorder(WorkspaceStyle.line.opacity(0.8), lineWidth: 1))
    }
}
