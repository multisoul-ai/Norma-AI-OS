import SwiftUI
import NormaCore

struct DiscussionDraftsView: View {
    @Bindable var book: RealtimeDiscussionBook
    @Environment(\.dismiss) private var dismiss
    @State private var deleting: RealtimeDiscussion?
    var body: some View {
        NavigationStack {
            List {
                Section {
                    Text("讨论仅作记录，不会自动派工。").font(.subheadline).foregroundStyle(.secondary)
                    if let failure = book.failure {
                        Text(failure).foregroundStyle(.orange).accessibilityIdentifier("realtime-discussions-error")
                        Button("重试保存") { book.retry() }.disabled(book.saving)
                    } else if book.saving { Label("正在保存", systemImage: "arrow.trianglehead.2.clockwise") }
                }
                if book.entries.isEmpty {
                    ContentUnavailableView("还没有讨论草稿", systemImage: "text.bubble",
                        description: Text("通话中的原话会保存在这里，挂断后仍可查看。"))
                }
                ForEach(book.entries.reversed()) { entry in
                    Section {
                        Text(entry.text).textSelection(.enabled).fixedSize(horizontal: false, vertical: true)
                        HStack {
                            Text(entry.recordedAt, style: .date).font(.caption).foregroundStyle(.secondary)
                            Spacer()
                            Button(role: .destructive) { deleting = entry } label: {
                                Label("删除", systemImage: "trash").frame(minHeight: 44)
                            }.buttonStyle(.borderless).accessibilityLabel("删除这条讨论草稿")
                        }
                    } header: { Text(entry.targetName) }
                }
            }.navigationTitle("讨论草稿").navigationBarTitleDisplayMode(.inline)
                .toolbar { ToolbarItem(placement: .cancellationAction) {
                    Button("完成") { dismiss() }.accessibilityIdentifier("realtime-discussions-close")
                } }
                .task { await book.load() }
                .confirmationDialog("删除这条讨论草稿？", isPresented: Binding(get: { deleting != nil }, set: { if !$0 { deleting = nil } })) {
                    Button("删除草稿", role: .destructive) {
                        if let entry = deleting { Task { await book.remove(id: entry.id) } }
                        deleting = nil
                    }
                    Button("取消", role: .cancel) { deleting = nil }
                }
        }.preferredColorScheme(.dark).tint(.white)
    }
}
