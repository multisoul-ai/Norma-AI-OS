import SwiftUI

/// Keep the chat's native offset while a detail sheet changes size or orientation.
@MainActor final class WorkspaceChatReadingPosition {
    weak var scrollView: UIScrollView?
    private struct Saved { let offset: CGPoint; let anchorID: UInt64?; let anchorY: CGFloat? }
    private final class Anchor { weak var view: UIView?; init(_ view: UIView) { self.view = view } }
    private var anchors: [UInt64: Anchor] = [:]
    private var saved: Saved?
    private var restorationID = UUID()
    func bind(scrollView: UIScrollView, anchorID: UInt64?, view: UIView) {
        self.scrollView = scrollView
        if let anchorID { anchors[anchorID] = Anchor(view) }
    }
    func preserve(anchorID: UInt64? = nil) {
        guard let scrollView else { return }
        restorationID = UUID()
        let anchorY = anchorID.flatMap { anchors[$0]?.view }.map {
            $0.convert(.zero, to: scrollView).y - scrollView.contentOffset.y
        }
        saved = Saved(offset: scrollView.contentOffset, anchorID: anchorID, anchorY: anchorY)
    }
    func restore() {
        guard let saved else { return }
        self.saved = nil
        let ownRestoration = restorationID
        apply(saved)
        // SwiftUI may finish measuring lazy rows after the sheet's dismissal callback.
        DispatchQueue.main.async { [weak self] in
            guard let self, self.restorationID == ownRestoration else { return }
            self.apply(saved)
        }
    }
    private func apply(_ saved: Saved) {
        guard let scrollView else { return }
        scrollView.layoutIfNeeded()
        var offset = saved.offset
        if let id = saved.anchorID, let y = saved.anchorY, let anchor = anchors[id]?.view,
           anchor.isDescendant(of: scrollView) {
            offset.y = anchor.convert(.zero, to: scrollView).y - y
        }
        scrollView.setContentOffset(offset, animated: false)
    }
}

struct WorkspaceChatScrollProbe: UIViewRepresentable {
    let position: WorkspaceChatReadingPosition
    var anchorID: UInt64? = nil
    func makeUIView(context: Context) -> Probe { Probe(position: position, anchorID: anchorID) }
    func updateUIView(_ view: Probe, context: Context) { view.bind() }
    final class Probe: UIView {
        let position: WorkspaceChatReadingPosition
        let anchorID: UInt64?
        init(position: WorkspaceChatReadingPosition, anchorID: UInt64?) {
            self.position = position; self.anchorID = anchorID
            super.init(frame: .zero); isUserInteractionEnabled = false
        }
        required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }
        override func didMoveToWindow() { super.didMoveToWindow(); bind() }
        override func layoutSubviews() { super.layoutSubviews(); bind() }
        func bind() {
            var ancestor = superview
            while let view = ancestor {
                if let scroll = view as? UIScrollView {
                    position.bind(scrollView: scroll, anchorID: anchorID, view: self); return
                }
                ancestor = view.superview
            }
        }
    }
}
