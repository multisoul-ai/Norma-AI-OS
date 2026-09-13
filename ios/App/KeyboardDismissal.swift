import SwiftUI
import UIKit

/// One passive recognizer per app window also covers sheets and navigation chrome.
/// It does not replace SwiftUI focus, native scrolling, or text-selection gestures.
struct KeyboardDismissal: UIViewRepresentable {
    func makeUIView(context: Context) -> DismissalView { DismissalView() }
    func updateUIView(_ view: DismissalView, context: Context) {}
    static func dismantleUIView(_ view: DismissalView, coordinator: ()) { view.detach() }

    final class DismissalView: UIView, UIGestureRecognizerDelegate {
        private weak var attachedWindow: UIWindow?
        private lazy var tap: UITapGestureRecognizer = {
            let recognizer = UITapGestureRecognizer(target: self, action: #selector(dismissKeyboard))
            recognizer.cancelsTouchesInView = false
            recognizer.delaysTouchesBegan = false
            recognizer.delaysTouchesEnded = false
            recognizer.delegate = self
            return recognizer
        }()

        override func didMoveToWindow() {
            super.didMoveToWindow()
            guard attachedWindow !== window else { return }
            detach()
            attachedWindow = window
            window?.addGestureRecognizer(tap)
        }

        func detach() {
            attachedWindow?.removeGestureRecognizer(tap)
            attachedWindow = nil
        }

        @objc private func dismissKeyboard() { attachedWindow?.endEditing(false) }

        func gestureRecognizer(_ gestureRecognizer: UIGestureRecognizer, shouldReceive touch: UITouch) -> Bool {
            var view = touch.view
            while let current = view {
                // Switching fields, moving the caret and editing must keep the keyboard open.
                if current is UITextField || (current as? UITextView)?.isEditable == true { return false }
                view = current.superview
            }
            return true
        }

        func gestureRecognizer(_ gestureRecognizer: UIGestureRecognizer, shouldRecognizeSimultaneouslyWith other: UIGestureRecognizer) -> Bool { true }

        func gestureRecognizer(_ gestureRecognizer: UIGestureRecognizer, shouldRequireFailureOf other: UIGestureRecognizer) -> Bool {
            other is UILongPressGestureRecognizer
        }
    }
}
