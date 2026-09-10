import SwiftUI
import NormaCore

/// The card moves with the finger. Selectable input and controls remain ordinary native interactions.
struct ApprovalSwipeCard: View {
    let snapshot: ApprovalSnapshot
    let source: String
    let canDecide: Bool
    let enabled: Bool
    let busy: Bool
    let cancellationID: String
    let position: String
    let hasNext: Bool
    let decide: (ApprovalDecision) -> Void
    let skip: () -> Void
    let viewTask: () -> Void
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.colorSchemeContrast) private var contrast
    @GestureState private var touching = false
    @State private var swipe = ApprovalSwipe()
    @State private var interaction = 0
    @State private var origin: Int?
    @State private var excluded: [CGRect] = []
    private let space = "approval-swipe-card"
    private var choice: ApprovalDecision? {
        guard enabled, let decision = swipe.decision, snapshot.decisions.contains(decision) else { return nil }
        return decision
    }
    private var hint: String {
        if busy { return "正在提交决定…" }
        if !canDecide { return "此项目为只读" }
        if snapshot.requiresDesktop { return "请在电脑的 Agent 卡片中确认" }
        if let choice { return choice == .allow ? "松手允许一次" : "松手拒绝本次" }
        return [snapshot.decisions.contains(.deny) ? "左滑拒绝" : nil, snapshot.decisions.contains(.allow) ? "右滑允许一次" : nil].compactMap { $0 }.joined(separator: " · ")
    }

    var body: some View {
        GeometryReader { geometry in
            ZStack {
                if hasNext {
                    RoundedRectangle(cornerRadius: 22).fill(Color.white.opacity(0.045))
                        .padding(.horizontal, 10).offset(y: 9).accessibilityHidden(true)
                }
                ScrollView {
                    VStack(alignment: .leading, spacing: 18) {
                        VStack(alignment: .leading, spacing: 18) {
                            Label(source, systemImage: "laptopcomputer").font(.caption).foregroundStyle(.secondary)
                            ApprovalSummary(snapshot: snapshot)
                        }.frame(maxWidth: .infinity, alignment: .leading)
                            .accessibilityElement(children: .contain).accessibilityIdentifier("approval-swipe-surface")
                        ApprovalInput(snapshot: snapshot).background(exclusion)
                        Text(hint).font(.caption.weight(.medium)).foregroundStyle(choice == .deny ? Color.orange : Color.secondary)
                            .fixedSize(horizontal: false, vertical: true).accessibilityIdentifier("approval-swipe-hint")
                        VStack(spacing: 12) {
                            Button(action: viewTask) {
                                Label("查看任务详情", systemImage: "bubble.left.and.bubble.right")
                                    .font(.subheadline).frame(maxWidth: .infinity, minHeight: 44)
                            }.disabled(busy).accessibilityIdentifier("approval-task-details")
                            ViewThatFits(in: .horizontal) {
                                HStack(spacing: 12) { buttons }
                                VStack(spacing: 12) { buttons }
                            }
                            HStack {
                                Text(position).font(.caption).foregroundStyle(.secondary)
                                Spacer()
                                Button("暂时跳过", action: skip).font(.subheadline).frame(minHeight: 44)
                                    .disabled(!hasNext || busy).accessibilityIdentifier("approval-skip")
                            }
                        }.background(exclusion)
                    }.padding(20)
                }
                .background(Color(white: 0.075), in: RoundedRectangle(cornerRadius: 22))
                .overlay {
                    RoundedRectangle(cornerRadius: 22)
                        .strokeBorder(choice == .deny ? Color.orange : .white, lineWidth: choice == nil ? 1 : 2)
                        .opacity(choice == nil ? (contrast == .increased ? 0.5 : 0.1) : 0.85)
                        .allowsHitTesting(false)
                }
                .contentShape(Rectangle())
                .onPreferenceChange(ApprovalExcludedFrames.self) { excluded = $0 }
                .simultaneousGesture(drag(width: geometry.size.width))
                .offset(x: reduceMotion ? 0 : swipe.dx)
                .rotationEffect(.degrees(reduceMotion ? 0 : min(9, max(-9, swipe.dx / 20))))
            }
        }
        .coordinateSpace(name: space)
        .onChange(of: touching) { _, active in if !active { reset() } }
        .onChange(of: snapshot) { _, _ in cancel() }
        .onChange(of: enabled) { _, _ in cancel() }
        .onChange(of: cancellationID) { _, _ in cancel() }
        .onDisappear { cancel() }
        .sensoryFeedback(.selection, trigger: choice)
    }

    private var exclusion: some View {
        GeometryReader { geometry in
            Color.clear.preference(key: ApprovalExcludedFrames.self, value: [geometry.frame(in: .named(space))])
        }
    }
    @ViewBuilder private var buttons: some View {
        ForEach(snapshot.decisions, id: \.self) { decision in
            Button { decide(decision) } label: {
                HStack {
                    if busy { ProgressView() }
                    Text(decision == .allow ? "允许一次" : "拒绝本次")
                }.frame(maxWidth: .infinity, minHeight: 44)
            }.buttonStyle(.bordered).tint(decision == .allow ? .white : .orange)
                .disabled(!enabled)
                .accessibilityIdentifier(decision == .allow ? "approve-once" : "deny-once")
        }
    }
    private func drag(width: CGFloat) -> some Gesture {
        DragGesture(minimumDistance: 8, coordinateSpace: .named(space))
            .updating($touching) { _, active, _ in active = true }
            .onChanged { value in
                if origin == nil {
                    swipe = ApprovalSwipe()
                    origin = interaction
                    if !enabled || excluded.contains(where: { $0.contains(value.startLocation) }) { swipe.cancel() }
                }
                guard origin == interaction, enabled else { swipe.cancel(); return }
                swipe.update(dx: value.translation.width, dy: value.translation.height, width: width)
            }
            .onEnded { value in
                guard origin == interaction, enabled else { reset(); return }
                swipe.update(dx: value.translation.width, dy: value.translation.height, width: width)
                let decision = choice
                reset()
                // Submit immediately; the spring never delays a network operation or predicts consent.
                if let decision { decide(decision) }
            }
    }
    private func cancel() { interaction += 1; swipe.cancel() }
    private func reset() {
        withAnimation(reduceMotion ? nil : .spring(response: 0.2, dampingFraction: 1)) {
            swipe = ApprovalSwipe(); origin = nil
        }
    }
}

private struct ApprovalExcludedFrames: PreferenceKey {
    static let defaultValue: [CGRect] = []
    static func reduce(value: inout [CGRect], nextValue: () -> [CGRect]) { value += nextValue() }
}
