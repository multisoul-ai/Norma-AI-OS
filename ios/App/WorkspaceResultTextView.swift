import SwiftUI
import NormaCore

struct WorkspaceResultTextView: View {
    let text: String
    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            ForEach(Array(workspaceResultBlocks(text).enumerated()), id: \.offset) { _, block in
                switch block {
                case .text(let body):
                    Text((try? AttributedString(markdown: body, options: .init(interpretedSyntax: .inlineOnlyPreservingWhitespace))) ?? AttributedString(body))
                        .font(.body).lineSpacing(4).textSelection(.enabled)
                case .code(let body):
                    ScrollView(.horizontal) {
                        Text(body).font(.callout.monospaced()).textSelection(.enabled).padding(14)
                    }.background(IdeaPalette.softGray, in: RoundedRectangle(cornerRadius: 16))
                }
            }
        }.frame(maxWidth: .infinity, alignment: .leading)
    }
}
