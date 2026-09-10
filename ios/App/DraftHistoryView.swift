import SwiftUI
import NormaCore

struct DraftHistoryView: View {
    let book: ChatDraftBook
    let drafts: [ChatDraft]
    @Environment(\.dismiss) private var dismiss
    @State private var deleting: ChatDraft?
    @State private var copied: String?
    private var visibleDrafts: [ChatDraft] {
        drafts.filter { draft in book.drafts.contains { $0.id == draft.id } }.sorted { $0.editedAt > $1.editedAt }
    }
    var body: some View {
        NavigationStack {
            List {
                Text("草稿保存在此手机，不会自动发送。复制需要的内容，再粘贴到目标对话。")
                    .font(.footnote).foregroundStyle(.secondary)
                if !book.loaded && book.failure == nil { ProgressView("正在读取草稿…") }
                else if book.loaded && visibleDrafts.isEmpty { ContentUnavailableView("没有未发送的草稿", systemImage: "note.text") }
                ForEach(visibleDrafts) { draft in
                    Section {
                        Text(draft.text).font(.subheadline).textSelection(.enabled)
                        Button(copied == draft.id ? "已复制" : "复制正文") {
                            UIPasteboard.general.string = draft.text; copied = draft.id
                        }.frame(minHeight: 44)
                        Button("删除草稿", role: .destructive) { deleting = draft }.frame(minHeight: 44)
                    } header: {
                        VStack(alignment: .leading) {
                            Text(draft.label ?? "先前会话")
                            Text(draft.editedAt, format: .dateTime.month().day().hour().minute())
                        }
                    }
                }
                if let failure = book.failure {
                    Text(failure).foregroundStyle(.orange)
                    Button("重试草稿存储") { Task { await book.retry() } }.frame(minHeight: 44)
                }
            }
            .scrollContentBackground(.hidden).background(.black)
            .navigationTitle("本地草稿").navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("关闭") { dismiss() }.accessibilityIdentifier("drafts-close") } }
            .confirmationDialog("删除这份未发送的草稿？", isPresented: Binding(get: { deleting != nil }, set: { if !$0 { deleting = nil } }), titleVisibility: .visible) {
                if let deleting { Button("删除草稿", role: .destructive) { book.remove(id: deleting.id) } }
            }
        }
    }
}
