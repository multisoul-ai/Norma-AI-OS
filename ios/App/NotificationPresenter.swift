import SwiftUI
import NormaCore

/// A notification opens above the current presentation, preserving sheets, navigation and drafts.
/// SwiftUI's root fullScreenCover cannot present while a descendant already owns a modal.
struct NotificationPresenter: UIViewControllerRepresentable {
    let model: ComputerModel
    @ObservedObject var notifications: PushNotifications
    @Environment(\.scenePhase) private var phase

    func makeUIViewController(context: Context) -> Controller { Controller() }

    func updateUIViewController(_ controller: Controller, context: Context) {
        controller.model = model
        controller.notifications = notifications
        controller.phase = phase
        controller.showPending()
    }

    final class Controller: UIViewController {
        var model: ComputerModel?
        var notifications: PushNotifications?
        var phase: ScenePhase = .inactive
        private var destination: UIHostingController<AnyView>?

        override func viewDidAppear(_ animated: Bool) {
            super.viewDidAppear(animated)
            showPending()
        }

        func showPending() {
            guard let model, let target = notifications?.target else { return }
            let content = AnyView(NotificationDestinationView(target: target, model: model, close: { [weak self] in self?.close() })
                .id(target).environment(\.scenePhase, phase))
            if let destination {
                if !destination.isBeingDismissed { destination.rootView = content }
                return
            }
            guard phase == .active else { return }
            // Use this scene's own presentation chain, including nested sheets and covers.
            var presenter: UIViewController = self
            while let parent = presenter.parent { presenter = parent }
            while let presented = presenter.presentedViewController { presenter = presented }
            guard presenter.viewIfLoaded?.window != nil else { return }
            if let transition = presenter.transitionCoordinator {
                transition.animate(alongsideTransition: nil) { [weak self] _ in
                    Task { @MainActor in self?.showPending() }
                }
                return
            }
            let host = UIHostingController(rootView: content)
            host.modalPresentationStyle = .overFullScreen
            host.overrideUserInterfaceStyle = .dark
            host.view.accessibilityViewIsModal = true
            destination = host
            presenter.present(host, animated: true)
        }

        private func close() {
            notifications?.target = nil
            destination?.dismiss(animated: true) { [weak self] in
                self?.destination = nil
                self?.showPending()
            }
        }
    }
}
