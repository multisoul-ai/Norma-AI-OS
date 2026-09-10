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
extension EnvironmentValues {
    var workspaceTab: Binding<Int>? {
        get { self[WorkspaceTabKey.self] } set { self[WorkspaceTabKey.self] = newValue }
    }
    var workspaceNotifications: @MainActor @Sendable () -> Void {
        get { self[WorkspaceNotificationsKey.self] } set { self[WorkspaceNotificationsKey.self] = newValue }
    }
}

/// The v28 brand header and floating tabs belong only to the three home pages, never their chat destinations.
struct WorkspaceChrome: ViewModifier {
    let model: ComputerModel
    let manageComputers: () -> Void
    var enabled = true
    @Environment(\.workspaceTab) private var tab
    func body(content: Content) -> some View {
        if enabled, let tab {
            content
                .safeAreaInset(edge: .top, spacing: 9) { WorkspaceTopBar(model: model, manageComputers: manageComputers) }
                .safeAreaInset(edge: .bottom, spacing: 0) { WorkspaceTabBar(selection: tab).padding(.horizontal, 17).padding(.top, 4).padding(.bottom, 8) }
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
                Button("设备与连接", action: manageComputers)
                Button("任务提醒", action: openNotifications)
                Button("刷新连接", action: model.refresh)
            } label: {
                Text("A").font(.system(size: 12, weight: .semibold))
                    .frame(width: 29, height: 29).background(WorkspaceStyle.line, in: Circle()).frame(width: 44, height: 44).contentShape(Rectangle())
            }.accessibilityLabel("偏好设置")
        }.buttonStyle(WorkspacePressStyle()).foregroundStyle(.white)
            .padding(.horizontal, 20).frame(height: 58).background(.black)
            .dynamicTypeSize(...DynamicTypeSize.large)
    }
}

private struct WorkspaceTabBar: View {
    @Binding var selection: Int
    var body: some View {
        HStack(spacing: 0) {
            item(0, title: "项目", symbol: "folder", id: "projects")
            item(1, title: "指挥台", symbol: nil, id: "command")
            item(2, title: "任务", symbol: "list.bullet", id: "tasks")
        }.padding(4).frame(height: 74)
            .background(WorkspaceStyle.raised.opacity(0.96), in: Capsule())
            .overlay(Capsule().strokeBorder(WorkspaceStyle.line, lineWidth: 1))
            .dynamicTypeSize(...DynamicTypeSize.large)
    }
    private func item(_ index: Int, title: String, symbol: String?, id: String) -> some View {
        Button { selection = index } label: {
            VStack(spacing: index == 1 ? 2 : 5) {
                if let symbol {
                    Image(systemName: symbol).font(.system(size: 20, weight: selection == index ? .medium : .regular)).frame(height: 23)
                } else {
                    NormaMark().fill(.black).frame(width: 24, height: 24)
                        .frame(width: 44, height: 44).background(Color(white: 0.98), in: Circle())
                        .overlay(Circle().stroke(.white.opacity(selection == index ? 0.15 : 0.06), lineWidth: 4).padding(-2))
                }
                Text(title).font(.system(size: 10))
            }.foregroundStyle(selection == index ? .white : WorkspaceStyle.muted)
                .frame(maxWidth: .infinity, minHeight: 58).offset(y: index == 1 ? -5 : 0).contentShape(Rectangle())
        }.buttonStyle(WorkspacePressStyle()).accessibilityLabel(title)
            .accessibilityIdentifier("workspace-tab-" + id).accessibilityAddTraits(selection == index ? .isSelected : [])
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

struct WorkspaceSearch: View {
    let placeholder: String
    @Binding var text: String
    var identifier = "workspace-search"
    @Environment(\.dynamicTypeSize) private var typeSize
    var body: some View {
        HStack(spacing: 10) {
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
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.scenePhase) private var phase
    private var mark: some View {
        Group {
            if ["Claude", "Codex"].contains(profile) {
                Image(profile).renderingMode(.template).resizable().scaledToFit()
                    .foregroundStyle(profile == "Claude" ? WorkspaceStyle.claude : .white)
            } else { NormaMark().fill(.white) }
        }.frame(width: size, height: size)
    }
    var body: some View {
        Group {
            if working && !reduceMotion && phase == .active {
                if profile == "Claude" {
                    TimelineView(.animation(minimumInterval: 1.0 / 30)) { context in
                        mark.rotationEffect(.degrees(context.date.timeIntervalSinceReferenceDate.truncatingRemainder(dividingBy: 8) / 8 * 360))
                    }
                } else {
                    mark.phaseAnimator([false, true]) { content, phase in content.scaleEffect(phase ? 1.07 : 1).rotationEffect(.degrees(phase ? 4 : -4)) }
                }
            } else { mark }
        }.accessibilityHidden(true)
    }
}
