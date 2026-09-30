import SwiftUI

enum WorkspaceStyle {
    static let surface = Color(white: 17.0 / 255)
    static let raised = Color(white: 23.0 / 255)
    static let line = Color(white: 42.0 / 255)
    static let muted = Color(white: 153.0 / 255)
    static let claude = Color(red: 209.0 / 255, green: 140.0 / 255, blue: 101.0 / 255)
}

private struct WorkspaceTabKey: EnvironmentKey { static let defaultValue: Binding<Int>? = nil }
private struct WorkspaceNotificationsKey: EnvironmentKey { static let defaultValue: @MainActor @Sendable () -> Void = {} }
private struct WorkspaceIdeasKey: EnvironmentKey { static let defaultValue: @MainActor @Sendable () -> Void = {} }
extension EnvironmentValues {
    var workspaceTab: Binding<Int>? {
        get { self[WorkspaceTabKey.self] } set { self[WorkspaceTabKey.self] = newValue }
    }
    var workspaceNotifications: @MainActor @Sendable () -> Void {
        get { self[WorkspaceNotificationsKey.self] } set { self[WorkspaceNotificationsKey.self] = newValue }
    }
    var workspaceIdeas: @MainActor @Sendable () -> Void {
        get { self[WorkspaceIdeasKey.self] } set { self[WorkspaceIdeasKey.self] = newValue }
    }
}

/// The v28 brand header and floating tabs belong only to the three home pages, never their chat destinations.
struct WorkspaceChrome: ViewModifier {
    let model: ComputerModel
    let manageComputers: () -> Void
    var enabled = true
    @Environment(\.workspaceTab) private var tab
    func body(content: Content) -> some View {
        if enabled, tab != nil {
            content
                .safeAreaInset(edge: .top, spacing: 9) { WorkspaceTopBar(model: model, manageComputers: manageComputers) }
                .toolbar(.hidden, for: .navigationBar, .tabBar)
        } else { content }
    }
}

struct NormaMark: Shape {
    func path(in rect: CGRect) -> Path {
        // Exact Norma mark from the approved HTML's 36×36 viewBox.
        let points: [CGPoint] = [(5,27),(14,8),(18,8),(16,25),(26,8),(31,8),(20,29),(14,29),(16,12),(9,27)]
            .map { CGPoint(x: CGFloat($0.0) * rect.width / 36, y: CGFloat($0.1) * rect.height / 36) }
        return Path { path in path.addLines(points); path.closeSubpath() }
    }
}

private struct WorkspaceTopBar: View {
    let model: ComputerModel
    let manageComputers: () -> Void
    @Environment(\.workspaceNotifications) private var openNotifications
    @Environment(\.workspaceIdeas) private var openIdeas
    var body: some View {
        HStack(spacing: 4) {
            Button(action: manageComputers) {
                HStack(spacing: 7) {
                    NormaMark().frame(width: 22, height: 22)
                    VStack(alignment: .leading, spacing: 0) {
                        Text("norma").font(.system(size: 17, weight: .semibold)).tracking(-0.5)
                        HStack(spacing: 3) {
                            Text(model.selected?.name ?? "连接电脑").lineLimit(1).truncationMode(.middle)
                            Image(systemName: "chevron.down").font(.system(size: 8))
                        }.font(.system(size: 10)).foregroundStyle(WorkspaceStyle.muted)
                    }
                }.frame(maxWidth: .infinity, minHeight: 44, alignment: .leading).contentShape(Rectangle())
            }.accessibilityLabel("norma，切换电脑").accessibilityValue(model.selected?.name ?? "未连接")
                .accessibilityIdentifier("workspace-machine-switch")
            Button(action: openNotifications) { Image(systemName: "bell").font(.system(size: 21, weight: .light)).frame(width: 44, height: 44).contentShape(Rectangle()) }
                .accessibilityLabel("任务提醒").accessibilityIdentifier("workspace-notifications")
            Button(action: manageComputers) { WorkspaceCircleIcon(symbol: "laptopcomputer") }
                .accessibilityLabel("切换或管理电脑").accessibilityValue(model.selected?.name ?? "未连接")
                .accessibilityIdentifier("manage-computers")
            Menu {
                Button("想法库", systemImage: "lightbulb", action: openIdeas)
                    .accessibilityIdentifier("idea-library-entry")
                Button("设备与连接", action: manageComputers)
                Button("任务提醒", action: openNotifications)
                Button("刷新连接", action: model.refresh)
            } label: {
                Text("A").font(.system(size: 12, weight: .semibold))
                    .frame(width: 29, height: 29).background(WorkspaceStyle.line, in: Circle()).frame(width: 44, height: 44).contentShape(Rectangle())
            }.accessibilityLabel("偏好设置").accessibilityIdentifier("workspace-preferences")
        }.buttonStyle(WorkspacePressStyle()).foregroundStyle(.white)
            .padding(.horizontal, 20).frame(height: 58).background(.black)
            .dynamicTypeSize(...DynamicTypeSize.large)
    }
}

struct WorkspaceTabBar: View {
    @Binding var selection: Int
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency
    @Environment(\.dynamicTypeSize) private var typeSize
    var body: some View {
        HStack(alignment: typeSize.isAccessibilitySize ? .top : .center, spacing: typeSize.isAccessibilitySize ? 8 : 0) {
            item(0, title: "工作空间", symbol: "folder.fill", id: "projects")
            item(1, title: "Norma", symbol: nil, id: "command")
            item(2, title: "收件箱", symbol: "tray.fill", id: "tasks")
        }
        .frame(minHeight: typeSize.isAccessibilitySize ? 94 : 72)
        .padding(.horizontal, typeSize.isAccessibilitySize ? 8 : 16)
        .background {
            let shape = UnevenRoundedRectangle(topLeadingRadius: 30, topTrailingRadius: 30)
            Group {
                if reduceTransparency { shape.fill(.white) }
                else if #available(iOS 26.0, *) { Color.clear.glassEffect(.regular, in: shape) }
                else { shape.fill(.ultraThinMaterial) }
            }.ignoresSafeArea(edges: .bottom)
        }
        .overlay(alignment: .top) { Color.white.opacity(0.8).frame(height: 1) }
    }
    private func item(_ index: Int, title: String, symbol: String?, id: String) -> some View {
        let keepWord = typeSize.isAccessibilitySize && index == 1
        return Button { selection = index } label: {
            VStack(spacing: 5) {
                if let symbol {
                    Image(systemName: symbol).font(.system(size: 23, weight: .medium))
                        .frame(width: 38, height: 38)
                        .background(selection == index ? IdeaPalette.blue.opacity(0.08) : .clear, in: Circle())
                } else {
                    Image("NormaPortrait").resizable().scaledToFill()
                        .frame(width: 36, height: 36).clipShape(Circle())
                        .accessibilityHidden(true)
                }
                Text(title).font(.caption2).multilineTextAlignment(.center)
                    .fixedSize(horizontal: keepWord, vertical: true)
            }
            .foregroundStyle(selection == index ? IdeaPalette.blue : IdeaPalette.secondary)
            .padding(.horizontal, typeSize.isAccessibilitySize ? 6 : 0)
            .frame(maxWidth: keepWord ? nil : .infinity, minHeight: 60).contentShape(Rectangle())
        }.frame(maxWidth: keepWord ? nil : .infinity).buttonStyle(IdeaPressStyle()).accessibilityLabel(title)
            .accessibilityIdentifier("workspace-tab-" + id)
            .accessibilityAddTraits(selection == index ? .isSelected : [])
    }
}

struct WorkspaceGlassTrack<Content: View>: View {
    @ViewBuilder var content: Content
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency
    var body: some View {
        content.padding(4)
            .background {
                if reduceTransparency { Capsule().fill(WorkspaceStyle.raised) }
                else { Capsule().fill(.ultraThinMaterial) }
            }
            .overlay(Capsule().strokeBorder(.white.opacity(0.14), lineWidth: 0.5).allowsHitTesting(false))
    }
}

struct WorkspaceGlassSelection: View {
    let namespace: Namespace.ID
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency
    var body: some View {
        Group {
            if reduceMotion { lens.transition(.opacity) }
            else { lens.matchedGeometryEffect(id: "selection", in: namespace) }
        }.allowsHitTesting(false).accessibilityHidden(true)
    }
    @ViewBuilder private var lens: some View {
        if reduceTransparency {
            Capsule().fill(Color(white: 0.28))
        } else if #available(iOS 26.0, *) {
            Color.clear.glassEffect(.regular.tint(.white.opacity(0.12)).interactive(), in: Capsule())
        } else {
            Capsule().fill(.thinMaterial).overlay(Capsule().fill(.white.opacity(0.12)))
                .overlay(Capsule().strokeBorder(.white.opacity(0.22), lineWidth: 0.5))
        }
    }
}

struct WorkspaceCircleIcon: View {
    let symbol: String
    var bordered = true
    var body: some View {
        Image(systemName: symbol).font(.system(size: 21, weight: .light))
            .frame(width: 44, height: 44).contentShape(Circle())
            .overlay(Circle().strokeBorder(bordered ? WorkspaceStyle.line : .clear, lineWidth: 1))
    }
}

struct WorkspacePageTitle: View {
    let title: String
    @ScaledMetric(relativeTo: .title) private var size = 26.0
    var body: some View { Text(title).font(.system(size: size, weight: .semibold)).tracking(-0.7).accessibilityAddTraits(.isHeader) }
}

struct WorkspaceSearch<Scope: View>: View {
    let placeholder: String
    @Binding var text: String
    var identifier = "workspace-search"
    @ViewBuilder var scope: Scope
    @Environment(\.dynamicTypeSize) private var typeSize
    var body: some View {
        HStack(spacing: 10) {
            scope
            Image(systemName: "magnifyingglass").font(.system(size: 17)).foregroundStyle(WorkspaceStyle.muted)
            TextField(typeSize.isAccessibilitySize ? "搜索" : placeholder, text: $text).font(.footnote).textInputAutocapitalization(.never).autocorrectionDisabled()
                .accessibilityLabel(placeholder)
                .frame(minHeight: 44).accessibilityIdentifier(identifier)
            if !text.isEmpty {
                Button { text = "" } label: { Image(systemName: "xmark.circle.fill").foregroundStyle(WorkspaceStyle.muted) }
                    .frame(width: 44, height: 44).accessibilityLabel("清除搜索")
            }
        }.padding(.leading, 14).padding(.trailing, 6).frame(minHeight: 48)
            .background(WorkspaceStyle.surface, in: Capsule()).overlay(Capsule().strokeBorder(WorkspaceStyle.line, lineWidth: 1))
    }
}

extension WorkspaceSearch where Scope == EmptyView {
    init(placeholder: String, text: Binding<String>, identifier: String = "workspace-search") {
        self.init(placeholder: placeholder, text: text, identifier: identifier) { EmptyView() }
    }
}

struct WorkspacePressStyle: ButtonStyle {
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    func makeBody(configuration: Configuration) -> some View {
        configuration.label.contentShape(Rectangle()).opacity(configuration.isPressed ? 0.72 : 1)
            .scaleEffect(configuration.isPressed && !reduceMotion ? 0.975 : 1)
    }
}

struct ProviderMark: View {
    let profile: String
    var size: CGFloat = 32
    var working = false
    var body: some View {
        AgentActivityDecoration(kind: .provider(profile), active: working)
            .frame(width: size, height: size)
    }
}
