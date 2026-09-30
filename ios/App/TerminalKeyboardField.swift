import SwiftUI
import UIKit

/// Keep native composition, selection and dictation; Return sends an explicit PTY key without dismissing iOS input.
struct TerminalKeyboardField: UIViewRepresentable {
    @Binding var text: String
    @Binding var focused: Bool
    var enabled: Bool
    var submit: () -> Void

    func makeCoordinator() -> Coordinator { Coordinator(self) }
    func makeUIView(context: Context) -> UITextField {
        let field = UITextField()
        field.delegate = context.coordinator
        field.addTarget(context.coordinator, action: #selector(Coordinator.edit), for: .editingChanged)
        field.textColor = .white; field.tintColor = .white
        field.attributedPlaceholder = NSAttributedString(string: "输入命令 · 回车执行", attributes: [.foregroundColor: UIColor.white.withAlphaComponent(0.65)])
        field.autocorrectionType = .no; field.autocapitalizationType = .none
        field.smartDashesType = .no; field.smartQuotesType = .no; field.smartInsertDeleteType = .no
        field.keyboardAppearance = .light; field.returnKeyType = .default
        field.adjustsFontForContentSizeCategory = true
        field.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        field.accessibilityIdentifier = "thread-terminal-input"
        field.accessibilityLabel = "终端输入，回车提交"
        return field
    }
    func updateUIView(_ field: UITextField, context: Context) {
        context.coordinator.parent = self
        let base = UIFont.preferredFont(forTextStyle: .body)
        field.font = UIFont(descriptor: base.fontDescriptor.withDesign(.monospaced) ?? base.fontDescriptor, size: 0)
        field.isEnabled = enabled
        // Never overwrite an in-progress Chinese marked-text composition with a rendered binding value.
        if field.markedTextRange == nil && field.text != text { field.text = text }
        if focused && enabled && !field.isFirstResponder { field.becomeFirstResponder() }
        if !focused && field.isFirstResponder { field.resignFirstResponder() }
    }
    func sizeThatFits(_ proposal: ProposedViewSize, uiView: UITextField, context: Context) -> CGSize? {
        CGSize(width: proposal.width ?? 200, height: max(44, uiView.font?.lineHeight ?? 0))
    }
    @MainActor final class Coordinator: NSObject, UITextFieldDelegate {
        var parent: TerminalKeyboardField
        init(_ parent: TerminalKeyboardField) { self.parent = parent }
        @objc func edit(_ field: UITextField) { parent.text = field.text ?? "" }
        func textFieldDidBeginEditing(_ field: UITextField) { if !parent.focused { parent.focused = true } }
        func textFieldDidEndEditing(_ field: UITextField) { if parent.focused { parent.focused = false } }
        func textFieldShouldReturn(_ field: UITextField) -> Bool {
            guard field.markedTextRange == nil else { return false }
            parent.text = field.text ?? ""; parent.submit()
            return false
        }
    }
}
