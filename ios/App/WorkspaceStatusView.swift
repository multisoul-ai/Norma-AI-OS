import SwiftUI

/// One layout for root-page and inline notices, with recovery actions beside their explanation.
struct WorkspaceStatusView<Actions: View>: View {
    let title: String
    let icon: String
    let message: String
    var inline = false
    @ViewBuilder var actions: () -> Actions

    var body: some View {
        Group {
            if inline { content }
            else {
                GeometryReader { geometry in
                    ScrollView {
                        content.frame(minHeight: geometry.size.height)
                    }.scrollBounceBehavior(.basedOnSize)
                }
            }
        }.listRowBackground(Color.clear).listRowSeparator(.hidden)
    }

    private var content: some View {
        VStack(spacing: 20) {
            Image(systemName: icon).font(.system(size: 36, weight: .regular))
                .foregroundStyle(.secondary).accessibilityHidden(true)
            VStack(spacing: 8) {
                Text(title).font(.headline).fixedSize(horizontal: false, vertical: true)
                Text(message).font(.subheadline).foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                    .accessibilityIdentifier("workspace-status-message")
            }
            VStack(spacing: 8, content: actions)
                .frame(width: 240).buttonStyle(.bordered).controlSize(.large)
                .font(.subheadline.weight(.medium))
        }.multilineTextAlignment(.center).padding(24).frame(maxWidth: .infinity)
    }
}

struct WorkspaceStatusButton: View {
    let title: String
    var primary = true
    let action: () -> Void

    var body: some View {
        let button = Button(action: action) {
            Text(title).fixedSize(horizontal: false, vertical: true)
                .foregroundStyle(primary ? .black : .white).frame(maxWidth: .infinity, minHeight: 28)
        }
        if primary { button.buttonStyle(.borderedProminent) }
        else { button.buttonStyle(.bordered) }
    }
}
