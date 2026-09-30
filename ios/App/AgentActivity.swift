import SwiftUI

/// Decorative motion stays in Core Animation; no per-frame SwiftUI state or layout work.
struct AgentActivityDecoration: View {
    let kind: AgentActivityView.Kind
    let active: Bool
    @Environment(\.scenePhase) private var phase
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var visible = false

    var body: some View {
        AgentActivityLayer(kind: kind, active: active && visible && phase == .active && !reduceMotion)
            .id(kind)
            .onAppear { visible = true }.onDisappear { visible = false }
            .allowsHitTesting(false).accessibilityHidden(true)
    }
}

private struct AgentActivityLayer: UIViewRepresentable {
    let kind: AgentActivityView.Kind
    let active: Bool
    func makeUIView(context: Context) -> AgentActivityView { AgentActivityView(kind: kind) }
    func updateUIView(_ view: AgentActivityView, context: Context) { view.setActive(active) }
    static func dismantleUIView(_ view: AgentActivityView, coordinator: ()) { view.setActive(false) }
}

final class AgentActivityView: UIView {
    enum Kind: Hashable { case border, provider(String) }
    let motionLayer: CALayer
    private let kind: Kind
    private let picture = UIImageView()
    private let borderMask = CAShapeLayer()
    private let fallback = CAShapeLayer()
    private var active = false
    private var laidOut = CGRect.null

    init(kind: Kind) {
        self.kind = kind
        switch kind {
        case .border: motionLayer = CAGradientLayer()
        case .provider: motionLayer = picture.layer
        }
        super.init(frame: .zero)
        isUserInteractionEnabled = false; isAccessibilityElement = false
        motionLayer.name = "agent.activity.motion"
        switch kind {
        case .border:
            let gradient = motionLayer as! CAGradientLayer
            gradient.type = .conic
            gradient.colors = [UIColor.clear.cgColor, UIColor.clear.cgColor, UIColor.white.withAlphaComponent(0.15).cgColor, UIColor.white.cgColor]
            gradient.startPoint = CGPoint(x: 0.5, y: 0.5)
            gradient.endPoint = CGPoint(x: 0.5, y: 0)
            borderMask.fillColor = UIColor.clear.cgColor
            borderMask.strokeColor = UIColor.white.cgColor; borderMask.lineWidth = 1
            layer.mask = borderMask; layer.addSublayer(gradient)
        case .provider(let profile):
            picture.image = UIImage(named: profile)?.withRenderingMode(.alwaysTemplate)
            picture.contentMode = .scaleAspectFit
            picture.tintColor = profile == "Claude" ? UIColor(WorkspaceStyle.claude) : .white
            if picture.image == nil {
                fallback.fillColor = UIColor.white.cgColor
                picture.layer.addSublayer(fallback)
            }
            addSubview(picture)
        }
    }
    required init?(coder: NSCoder) { nil }

    override func layoutSubviews() {
        super.layoutSubviews()
        guard bounds != laidOut else { return }
        laidOut = bounds
        CATransaction.begin(); CATransaction.setDisableActions(true)
        switch kind {
        case .border:
            borderMask.frame = bounds
            borderMask.path = UIBezierPath(roundedRect: bounds.insetBy(dx: 0.5, dy: 0.5), cornerRadius: 22.5).cgPath
            // The rotating square covers the full card diagonal at every angle; only its fixed 1pt outline is visible.
            let side = hypot(bounds.width, bounds.height) + 2
            motionLayer.bounds = CGRect(x: 0, y: 0, width: side, height: side)
            motionLayer.position = CGPoint(x: bounds.midX, y: bounds.midY)
        case .provider:
            picture.bounds = CGRect(origin: .zero, size: bounds.size)
            picture.center = CGPoint(x: bounds.midX, y: bounds.midY)
            fallback.frame = picture.bounds
            fallback.path = NormaMark().path(in: picture.bounds).cgPath
        }
        CATransaction.commit()
    }

    func setActive(_ value: Bool) { active = value; updateMotion() }
    override func didMoveToWindow() { super.didMoveToWindow(); updateMotion() }

    private func updateMotion() {
        CATransaction.begin(); CATransaction.setDisableActions(true)
        defer { CATransaction.commit() }
        let playing = active && window != nil
        if kind == .border { motionLayer.isHidden = !playing }
        guard playing else { motionLayer.removeAnimation(forKey: "activity"); return }
        // Content/status refreshes must not restart an already running animation.
        guard motionLayer.animation(forKey: "activity") == nil else { return }
        let animation: CAAnimation
        switch kind {
        case .border, .provider("Claude"):
            let rotation = CABasicAnimation(keyPath: "transform.rotation.z")
            rotation.fromValue = 0; rotation.toValue = Double.pi * 2
            // Preserve the original 3s border and 8s Claude cycles; Core Animation follows display refresh.
            rotation.duration = kind == .border ? 3 : 8
            rotation.timingFunction = CAMediaTimingFunction(name: .linear)
            animation = rotation
        case .provider:
            let scale = CABasicAnimation(keyPath: "transform.scale")
            scale.fromValue = 1; scale.toValue = 1.07
            let tilt = CABasicAnimation(keyPath: "transform.rotation.z")
            tilt.fromValue = -Double.pi / 45; tilt.toValue = Double.pi / 45 // Original ±4°.
            let group = CAAnimationGroup()
            group.animations = [scale, tilt]; group.duration = 1.4; group.autoreverses = true
            scale.duration = group.duration; tilt.duration = group.duration
            group.timingFunction = CAMediaTimingFunction(name: .easeInEaseOut)
            animation = group
        }
        animation.repeatCount = .infinity
        animation.beginTime = motionLayer.convertTime(CACurrentMediaTime(), from: nil)
        motionLayer.add(animation, forKey: "activity")
    }
}
