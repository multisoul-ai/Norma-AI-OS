import SwiftUI
import NormaCore

struct NewCardView: View {
    let projectName: String
    let unavailable: String?
    let busy: Bool
    let notice: String?
    let pending: PendingMobileCommand?
    let model: ComputerModel
    let machine: PairedMachine
    let project: ProjectSnapshot?
    let revision: UInt64
    let create: (String, String?) -> Void
    @Environment(\.dismiss) private var dismiss
    @Environment(\.dynamicTypeSize) private var typeSize
    @FocusState private var editing: Bool
    @State private var profile = "Codex"
    @State private var title = ""
    @State private var showResult = false
    private var name: String { title.trimmingCharacters(in: .whitespacesAndNewlines) }
    private var invalidName: Bool {
        name.unicodeScalars.count > 40 || name.unicodeScalars.contains(where: CharacterSet.controlCharacters.contains)
    }

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 18) {
                    HStack {
                        Text("创建卡片").font(.title3.weight(.semibold)).accessibilityAddTraits(.isHeader)
                        Spacer(minLength: 8)
                        Button { editing = false; dismiss() } label: {
                            Image(systemName: "xmark").font(.system(size: 14))
                                .frame(width: 44, height: 44).background(Color(white: 30.0 / 255), in: Circle())
                                .overlay(Circle().strokeBorder(WorkspaceStyle.line, lineWidth: 1))
                        }.accessibilityLabel("关闭").accessibilityIdentifier("new-card-close")
                    }
                    Text("先添加一个 Agent，任务可以稍后再安排。")
                        .font(.footnote).foregroundStyle(WorkspaceStyle.muted)
                        .fixedSize(horizontal: false, vertical: true)
                    VStack(alignment: .leading, spacing: 8) {
                        Text("Agent").font(.caption).foregroundStyle(WorkspaceStyle.muted)
                        Menu {
                            ForEach(["Codex", "Claude"], id: \.self) { item in
                                Button { profile = item } label: { Label(item, image: item) }
                                    .accessibilityIdentifier("create-agent-" + item)
                            }
                        } label: {
                            HStack {
                                Text(profile); Spacer(minLength: 8)
                                Image(systemName: "chevron.down").font(.caption)
                            }.padding(.horizontal, 16).frame(minHeight: 48)
                                .background(Color(white: 30.0 / 255), in: RoundedRectangle(cornerRadius: 16))
                                .overlay(RoundedRectangle(cornerRadius: 16).strokeBorder(WorkspaceStyle.line, lineWidth: 1))
                        }.accessibilityLabel("Agent").accessibilityValue(profile).accessibilityIdentifier("new-card-agent")
                            .disabled(busy || pending != nil)
                    }
                    VStack(alignment: .leading, spacing: 8) {
                        Text("卡片名称（选填）").font(.caption).foregroundStyle(WorkspaceStyle.muted)
                        TextField("例如：界面助手", text: $title).focused($editing)
                            .textInputAutocapitalization(.never).autocorrectionDisabled().submitLabel(.done)
                            .onSubmit { editing = false }.padding(.horizontal, 16).frame(minHeight: 48)
                            .background(Color(white: 30.0 / 255), in: RoundedRectangle(cornerRadius: 16))
                            .overlay(RoundedRectangle(cornerRadius: 16).strokeBorder(WorkspaceStyle.line, lineWidth: 1))
                            .accessibilityLabel("卡片名称（选填）").accessibilityIdentifier("new-card-title")
                            .disabled(busy || pending != nil)
                    }
                    Text("项目：\(projectName) · 创建后为空闲状态")
                        .font(.caption).foregroundStyle(WorkspaceStyle.muted).fixedSize(horizontal: false, vertical: true)
                    if !busy, let reason = invalidName ? "名称最多 40 个字符，不能包含换行或控制字符。" : (notice ?? unavailable) {
                        Text(reason).font(.caption).foregroundStyle(WorkspaceStyle.muted).fixedSize(horizontal: false, vertical: true)
                    }
                    if pending != nil && !busy {
                        Button("查询创建结果") { editing = false; showResult = true }
                            .font(.footnote).frame(minHeight: 44).accessibilityIdentifier("new-card-check-result")
                    }
                    Button { editing = false; create(profile, name.isEmpty ? nil : name) } label: {
                        HStack { if busy { ProgressView().tint(.black) }; Text(busy ? "正在创建…" : "创建卡片") }
                            .font(.footnote.weight(.semibold)).foregroundStyle(.black)
                            .frame(maxWidth: .infinity, minHeight: 48).padding(.vertical, 2)
                            .background(.white, in: Capsule())
                    }.disabled(busy || unavailable != nil || invalidName)
                        .opacity(busy || unavailable != nil || invalidName ? 0.42 : 1)
                        .accessibilityIdentifier("create-empty-card")
                }.padding(20)
            }.scrollDismissesKeyboard(.interactively).background(WorkspaceStyle.raised)
                .toolbar(.hidden, for: .navigationBar)
                .toolbar {
                    ToolbarItemGroup(placement: .keyboard) {
                        Spacer()
                        Button("收起键盘") { editing = false }.accessibilityIdentifier("new-card-dismiss-keyboard")
                    }
                }
                .sheet(isPresented: $showResult) {
                    if let project {
                        CardCommandSheet(model: model, machine: machine, project: project, revision: revision, pending: pending)
                    }
                }
        }.buttonStyle(WorkspacePressStyle()).foregroundStyle(.white)
            .presentationDetents(typeSize.isAccessibilitySize ? [.large] : [.height(420), .large])
            .presentationDragIndicator(.visible).presentationBackground(WorkspaceStyle.raised)
    }
}
