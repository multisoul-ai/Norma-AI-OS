import SwiftUI
import NormaCore

private struct ReturnToWorkThreadKey: EnvironmentKey {
    static let defaultValue: (@MainActor @Sendable (String?) async throws -> Void)? = nil
}
extension EnvironmentValues {
    var returnToWorkThread: (@MainActor @Sendable (String?) async throws -> Void)? {
        get { self[ReturnToWorkThreadKey.self] }
        set { self[ReturnToWorkThreadKey.self] = newValue }
    }
}

struct WorkThreadTaskActions: View {
    let operation: ThreadTaskOutboxOperation
    let archived: Bool
    @Environment(\.returnToWorkThread) private var returnToThread
    @Environment(\.dynamicTypeSize) private var typeSize
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency
    @State private var returning = false
    @State private var error: String?

    var body: some View {
        if returnToThread != nil {
            VStack(spacing: 8) {
                if let error { Text(error).font(.caption).foregroundStyle(.red) }
                let layout = typeSize.isAccessibilitySize ? AnyLayout(VStackLayout(spacing: 8)) : AnyLayout(HStackLayout(spacing: 10))
                layout {
                    Button { go(nil) } label: {
                        Text("回到线程").font(.headline).frame(maxWidth: .infinity, minHeight: 48)
                            .foregroundStyle(IdeaPalette.ink).background(IdeaPalette.softGray, in: RoundedRectangle(cornerRadius: 16))
                    }.accessibilityIdentifier("task-return-thread")
                    Button { go(operation.id) } label: {
                        HStack(spacing: 8) {
                            if returning { ProgressView().tint(.white) }
                            else { Image(systemName: "arrowshape.turn.up.left") }
                            Text("回线程追问")
                        }.font(.headline).frame(maxWidth: .infinity, minHeight: 48)
                            .foregroundStyle(.white).background(IdeaPalette.blue, in: RoundedRectangle(cornerRadius: 16))
                    }.accessibilityIdentifier("task-return-followup")
                        .disabled(archived || operation.receipt?.receipt.state != "succeeded" || operation.receipt?.receipt.result?.taskRef == nil)
                }.disabled(returning)
            }.buttonStyle(IdeaPressStyle()).padding(12)
                .background {
                    if reduceTransparency { Color.white }
                    else if #available(iOS 26.0, *) { Color.clear.glassEffect(.regular, in: RoundedRectangle(cornerRadius: 26)) }
                    else { RoundedRectangle(cornerRadius: 26).fill(.regularMaterial) }
                }.padding(.horizontal, 10).padding(.bottom, 4)
        }
    }

    private func go(_ id: String?) {
        guard !returning, let returnToThread else { return }
        returning = true; error = nil
        Task {
            do { try await returnToThread(id) }
            catch { self.error = error.localizedDescription }
            returning = false
        }
    }
}

struct WorkThreadFollowupTarget: View {
    let operation: ThreadTaskOutboxOperation?
    let cancel: () -> Void

    var body: some View {
        HStack(spacing: 10) {
            Image(systemName: "arrowshape.turn.up.left").foregroundStyle(IdeaPalette.blue).accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 3) {
                Text("追问 · \(operation?.target?.name ?? "Agent")").font(.caption.weight(.semibold))
                Text(operation?.task.task ?? "原任务").font(.caption2).foregroundStyle(IdeaPalette.secondary).lineLimit(1)
            }.frame(maxWidth: .infinity, alignment: .leading)
                .accessibilityElement(children: .combine)
                .accessibilityLabel("追问 \(operation?.target?.name ?? "Agent")，原任务：\(operation?.task.task ?? "正在读取")")
                .accessibilityIdentifier("thread-followup-target")
            Button(action: cancel) {
                Image(systemName: "xmark.circle.fill").font(.system(size: 20)).foregroundStyle(IdeaPalette.secondary)
                    .frame(width: 44, height: 44)
            }.accessibilityLabel("取消追问对象，保留正文").accessibilityIdentifier("thread-followup-cancel")
        }
    }
}

struct WorkThreadFollowupStatus: View {
    let operation: ThreadFollowupOutboxOperation
    let agent: String
    let deliveryError: String?
    let openTask: () -> Void

    private var status: String {
        guard let receipt = operation.receipt else { return "追问待发送" }
        if let outcome = receipt.outcome {
            return switch outcome.state {
            case .completed: operation.replyMessageID == nil ? "追问完成 · 无文字回复" : "已回复"
            case .failed: "追问失败"
            case .interrupted: "追问已停止"
            }
        }
        return switch receipt.queuedMessage.state {
        case "queued", "dispatching": "Mac 已接收 · 等待 Agent"
        case "sent": "Agent 已接收 · 等待回复"
        case "cancelled": "追问已取消"
        default: "追问结果待核对"
        }
    }

    var body: some View {
        VStack(alignment: .trailing, spacing: 6) {
            Button(action: openTask) {
                Label("追问 \(agent) · \(status)", systemImage: "arrowshape.turn.up.left")
                    .font(.caption).multilineTextAlignment(.trailing)
                    .frame(minHeight: 44)
                    .accessibilityIdentifier("thread-followup-status-\(operation.id)")
            }.foregroundStyle(IdeaPalette.blue)
                .accessibilityHint("查看原任务")
            if operation.awaitingReceipt, let deliveryError {
                Text(deliveryError).font(.footnote).foregroundStyle(IdeaPalette.secondary)
            }
        }.frame(maxWidth: .infinity, alignment: .trailing)
    }
}
