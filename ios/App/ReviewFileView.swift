import SwiftUI
import NormaCore
import UIKit

struct ReviewFileView: View {
    let file: ReviewFileChange
    var baselineName: String = "HEAD"
    var resultName: String = "工作副本"
    @State private var mode = "diff"
    init(file: ReviewFileChange, baselineName: String = "HEAD", resultName: String = "工作副本") {
        self.file = file; self.baselineName = baselineName; self.resultName = resultName
        // Binary files have no text diff; open the existing version, including deleted images.
        let existing = file.after ?? file.before
        _mode = State(initialValue: existing?.kind == "binary" ? (file.after == nil ? "before" : "after") : "diff")
    }
    private var version: ReviewFileVersion? {
        switch mode { case "before": file.before; case "index": file.index; default: file.after }
    }
    private var text: String? {
        switch mode { case "diff": file.diff; case "staged": file.stagedDiff; default: version?.text }
    }
    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            VStack(alignment: .leading, spacing: 8) {
                Text(file.path).font(.caption).foregroundStyle(.secondary).textSelection(.enabled)
                    .fixedSize(horizontal: false, vertical: true)
                Picker("查看版本", selection: $mode) {
                    Text("变更 · \(baselineName) → \(resultName)").tag("diff")
                    Text("变更 · \(baselineName) → 暂存区").tag("staged")
                    Text("原文 · \(baselineName)").tag("before")
                    Text(resultName).tag("after")
                    Text("暂存区").tag("index")
                }.pickerStyle(.menu).font(.subheadline).frame(minHeight: 44)
                    .accessibilityIdentifier("review-version")
            }.padding(.horizontal, 16)
            Divider()
            if !["diff", "staged"].contains(mode), let version, version.kind == "binary" {
                ReviewImagePreview(version: version, path: file.path).id(mode)
            } else if let text {
                if text.isEmpty {
                    ContentUnavailableView(mode == "diff" || mode == "staged" ? "正文没有差异" : "空文件", systemImage: "doc", description: Text("文件的存在状态或执行权限仍可能不同。"))
                } else { ReviewText(text: text).accessibilityIdentifier("review-text") }
            } else {
                ContentUnavailableView("此版本暂无文本预览", systemImage: "doc", description: Text(explanation))
            }
            if !["diff", "staged"].contains(mode), let version {
                Text("\(version.bytes.count) 字节 · \(version.kind == "symlink" ? "符号链接文字，未读取目标" : version.executable ? "可执行文件" : "普通文件")")
                    .font(.caption).foregroundStyle(.secondary).padding(16)
            }
        }.frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .background(.black).navigationTitle("文件内容").navigationBarTitleDisplayMode(.inline)
    }
    private var explanation: String {
        if file.isConflicted && ["index", "staged"].contains(mode) { return "存在合并冲突，暂存区没有唯一版本。" }
        if ["diff", "staged"].contains(mode) { return "二进制或符号链接不生成文本 diff，可切换到具体版本查看信息。" }
        if version == nil { return "文件在这个版本中不存在。新增文件没有原文，删除文件没有工作副本。" }
        return "此二进制文件请在电脑上查看。"
    }
}

/// Native TextKit handles full selectable, wrapping text without building a SwiftUI row per line.
struct ReviewText: UIViewRepresentable {
    let text: String
    func makeUIView(context: Context) -> UITextView {
        let view = UITextView()
        view.isEditable = false; view.isSelectable = true; view.backgroundColor = .black
        view.textColor = .label; view.alwaysBounceVertical = true
        view.adjustsFontForContentSizeCategory = true
        view.textContainerInset = UIEdgeInsets(top: 14, left: 12, bottom: 20, right: 12)
        view.font = UIFontMetrics(forTextStyle: .footnote).scaledFont(for: .monospacedSystemFont(ofSize: 13, weight: .regular))
        return view
    }
    func updateUIView(_ view: UITextView, context: Context) {
        if view.text != text { view.text = text; view.setContentOffset(.zero, animated: false) }
    }
}
