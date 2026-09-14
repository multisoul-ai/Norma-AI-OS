import SceneKit
import SwiftUI

final class AvatarScene: NSObject, SCNSceneRendererDelegate, @unchecked Sendable {
    enum Phase: String { case idle, listening, thinking, speaking }
    let scene: SCNScene
    let camera: SCNNode
    let loadError: String?
    private let lock = NSLock()
    private var phase: Phase = .idle
    private var level: Float = 0
    private var emotion = "neutral"
    private var gesture = "none"
    private var gestureStart: TimeInterval = 0
    private var lastCloudGesture: TimeInterval = -.infinity
    private var reducedMotion = false
    private var degraded = false
    private var latestTime: TimeInterval = 0
    private var frames: [Double] = []
    let motion = AvatarMotion.load()
    private var motionBones: [(node: SCNNode, worldRest: simd_quatf)] = []
    private var userSpeaking = false
    private var lastTouch = -10.0
    private var talkBlend: Float = 0
    private var waveBlend: Float = 0
    private var talkStart: TimeInterval = -.infinity
    private var talkClip = 1
    private var talkEnabled = false
    private var emotionUntil: TimeInterval = 0
    private var expression = SIMD3<Float>(repeating: 0)
    private var bones: [String: SCNNode] = [:]
    private var rest: [String: simd_quatf] = [:]
    private var armAxes: [String: SIMD3<Float>] = [:]
    private var relaxedFingers: [(node: SCNNode, rest: simd_quatf, axis: SIMD3<Float>, spreadAxis: SIMD3<Float>, curl: Float)] = []
    private var morphs: [(SCNMorpher, [String: Int])] = []
    private var lastTime = 0.0, nextBlink = 2.5, blinkStart = -10.0
    private var mouth: Float = 0

    override init() {
        if let url = Bundle.main.url(forResource: "Milltina", withExtension: "scn", subdirectory: "Avatar") {
            do { scene = try SCNScene(url: url, options: [.checkConsistency: true]); loadError = nil }
            catch { scene = SCNScene(); loadError = "角色资源读取失败，请重新导入模型" }
        } else { scene = SCNScene(); loadError = "角色资源缺失，请重新安装 Norma" }
        camera = SCNNode(); camera.camera = SCNCamera()
        super.init()
        scene.background.contents = UIColor.black
        scene.rootNode.enumerateChildNodes { node, _ in
            for material in node.geometry?.materials ?? [] { AvatarMaterials.apply(to: material) }
            if let name = node.name, node.skinner == nil { self.bones[name] = node; self.rest[name] = node.simdOrientation }
            if let morpher = node.morpher {
                var map: [String: Int] = [:]
                for (index, shape) in morpher.targets.enumerated() { if let name = shape.name { map[name] = index } }
                self.morphs.append((morpher, map))
            }
        }
        for name in ["Upper_arm_L", "Upper_arm_R"] {
            if let node = bones[name] { armAxes[name] = simd_normalize(node.simdConvertVector([0,0,1], from: nil)) }
        }
        for side in ["L", "R"] {
            for finger in ["Index", "Middle", "Ring", "Little", "Thumb"] {
                for (joint, curl) in [("Proximal", Float(0.18)), ("Intermediate", Float(0.32)), ("Distal", Float(0.18))] {
                    if let node = bones["\(finger) \(joint)_\(side)"] {
                        // The imported T-pose palms face down; derive each local bend axis instead of
                        // guessing a mirrored Euler sign. Total curl = 0.68 rad, approximately 39 degrees.
                        let palm = node.simdConvertVector([0,-1,0], from: nil)
                        let axis = simd_normalize(simd_cross(SIMD3<Float>(0,1,0), palm))
                        let resting: Float = finger == "Thumb" ? (joint == "Intermediate" ? 0.12 : 0.08) : curl
                        relaxedFingers.append((node, node.simdOrientation, axis, simd_normalize(palm), resting))
                    }
                }
            }
        }
        if let motion {
            motionBones = motion.bones.compactMap { name in
                guard let node = bones[name] else { return nil }
                return (node, node.simdWorldOrientation)
            }
        }
        let head = bones["Head"]?.worldPosition ?? SCNVector3(0, 1.35, 0)
        // 1.65 / 1.30 ≈ 1.27: enlarge the character without pushing subtitles or controls down.
        camera.position = SCNVector3(head.x, head.y - 0.13, head.z + 1.30)
        camera.look(at: SCNVector3(head.x, head.y - 0.14, head.z))
        camera.camera?.fieldOfView = 36
        camera.camera?.zNear = 0.05; camera.camera?.zFar = 20
        scene.rootNode.addChildNode(camera)
        let key = SCNNode(); key.light = SCNLight(); key.light?.type = .omni
        key.light?.intensity = 750; key.position = SCNVector3(-1, 2.8, 3)
        scene.rootNode.addChildNode(key)
        let fill = SCNNode(); fill.light = SCNLight(); fill.light?.type = .ambient
        fill.light?.intensity = 650; fill.light?.color = UIColor(red: 0.91, green: 0.96, blue: 1, alpha: 1)
        scene.rootNode.addChildNode(fill)
    }

    func update(phase: Phase, level: Float, reduceMotion: Bool, degraded: Bool, userSpeaking: Bool = false) {
        lock.lock(); defer { lock.unlock() }
        if phase == .speaking && self.phase != .speaking {
            talkEnabled = latestTime-talkStart >= 8
            if talkEnabled { talkStart = latestTime; talkClip = talkClip == 1 ? 2 : 1 }
        }
        if self.phase == .speaking && phase != .speaking { emotionUntil = latestTime+1.2 }
        if phase == .idle && self.phase != .idle { emotion = "neutral"; gesture = "none" }
        self.phase = phase; self.level = level; self.reducedMotion = reduceMotion; self.degraded = degraded
        if userSpeaking && !self.userSpeaking { gesture = "none"; emotion = "neutral"; talkEnabled = false }
        self.userSpeaking = userSpeaking
    }

    @discardableResult func behave(emotion: String, gesture: String) -> Bool {
        guard ["neutral", "happy", "curious", "concerned"].contains(emotion), ["none", "nod", "wave"].contains(gesture) else { return false }
        lock.lock(); defer { lock.unlock() }
        // 2.8 s wave + at least 5.2 s rest = 8 s between cloud gestures; touch retains its own cooldown.
        guard !userSpeaking else { return false }
        let allowed = gesture == "none" || latestTime-lastCloudGesture >= 8
        if allowed && gesture != "none" { lastCloudGesture = latestTime }
        self.emotion = emotion; self.gesture = allowed ? gesture : "none"; gestureStart = latestTime
        emotionUntil = latestTime+8
        return allowed
    }

    @discardableResult func touch(head: Bool) -> Bool {
        lock.lock(); defer { lock.unlock() }
        guard !userSpeaking, latestTime-lastTouch >= 1.2 else { return false }
        lastTouch = latestTime
        emotion = "happy"; gesture = head ? "pat" : "wave"; gestureStart = latestTime
        emotionUntil = latestTime+3
        return true
    }

    func fpsSamples() -> [Double] { lock.lock(); defer { lock.unlock() }; return frames }

    func renderer(_ renderer: SCNSceneRenderer, updateAtTime time: TimeInterval) {
        lock.lock()
        latestTime = time
        let phase = self.phase, level = self.level, gesture = self.gesture
        let emotion = phase == .speaking || time < emotionUntil ? self.emotion : "neutral"
        let talkAge = time-talkStart, talkClip = self.talkClip, talkEnabled = self.talkEnabled
        let start = gestureStart, reduce = reducedMotion, degraded = self.degraded, userSpeaking = self.userSpeaking
        if lastTime > 0, time > lastTime {
            frames.append(1 / (time-lastTime))
            // Ten minutes at 60 fps is unnecessary here; retain 600 recent frame intervals.
            if frames.count > 600 { frames.removeFirst(frames.count-600) }
        }
        lock.unlock()
        let dt = Float(min(0.1, lastTime > 0 ? time-lastTime : 1/60)); lastTime = time
        // One captured phrase, then rest; never cycle a game-style talking loop.
        let duration = motion?.clips[talkClip].duration ?? 3
        let enter = min(1, max(0, talkAge/0.7)), leave = min(1, max(0, (duration-talkAge)/0.8))
        let touchActive = (gesture == "wave" && time-start < 2.8) || (gesture == "pat" && time-start < 2.4)
        let talkTarget = talkEnabled && phase == .speaking && !userSpeaking && !reduce && !touchActive
            ? Float(enter*enter*(3-2*enter)*leave*leave*(3-2*leave)) : 0
        talkBlend += (talkTarget-talkBlend)*min(1, dt*10)
        if let motion, motionBones.count == motion.bones.count {
            // Apply parents first. World deltas retarget a different rest-axis and spine hierarchy.
            for (index, entry) in motionBones.enumerated() {
                let idle = motion.rotation(clip: 0, bone: index, time: reduce ? 0 : time)
                let talk = motion.rotation(clip: talkClip, bone: index, time: max(0, talkAge))
                // Let one hand lead each phrase; the other keeps the captured resting pose.
                let armWeight: Float = (5...8).contains(index) ? (talkClip == 1 ? 1 : 0.15)
                    : (9...12).contains(index) ? (talkClip == 2 ? 1 : 0.15) : 1
                let world = simd_slerp(idle, talk, talkBlend*armWeight)*entry.worldRest
                entry.node.simdOrientation = (entry.node.parent?.simdWorldOrientation.inverse ?? simd_quatf())*world
            }
        }
        if time >= nextBlink { blinkStart = time; nextBlink = time + Double.random(in: 3...6) }
        let blinkAge = time-blinkStart
        // Fast close, slower opening, with zero velocity at both ends.
        let blinkProgress = blinkAge < 0.065 ? blinkAge/0.065 : 1-(blinkAge-0.065)/0.13
        let blinkT = max(0, min(1, blinkProgress))
        let blink = blinkT*blinkT*(3-2*blinkT)
        mouth += ((phase == .speaking ? level : 0) - mouth) * min(1, dt*24)
        let expressive = !userSpeaking
        let gestureAge = time-start
        func envelope(_ length: Double) -> Float {
            let enter = min(1, max(0, gestureAge/0.3)), leave = min(1, max(0, (length-gestureAge)/0.4))
            return expressive ? Float(enter*enter*(3-2*enter)*leave*leave*(3-2*leave)) : 0
        }
        let pat = gesture == "pat" ? envelope(2.4) : 0
        // Facial meaning is independent of gesture cooldown; hold it through actual playback.
        // Neutral speech still has a small engaged face; concern never receives the smile fallback.
        let target = SIMD3<Float>(emotion == "happy" ? 1 : (emotion == "neutral" && phase == .speaking ? 0.25 : 0),
                                 emotion == "curious" ? 1 : (phase == .thinking && emotion == "neutral" ? 0.35 : 0),
                                 emotion == "concerned" ? 1 : 0) * (expressive ? 1 : 0)
        expression += (target-expression)*min(1, dt*8)
        let happy = Double(expression.x), curious = Double(expression.y), concern = Double(expression.z)
        // An emotion arrives, settles, then lightly follows speech emphasis. Do not pin a
        // maximal smile/raised brow for the entire reply or run a periodic facial oscillator.
        let onset = exp(-max(0, gestureAge-0.5)/1.4)
        let emphasis = phase == .speaking ? Double(mouth) : 0
        for (morpher, indices) in morphs {
            func set(_ name: String, _ value: Double) { if let i = indices[name] { morpher.setWeight(CGFloat(value), forTargetAt: i) } }
            set("eye_close", max(blink, Double(pat)*0.65))
            set("vrc.v_aa", Double(mouth)*0.82)
            set("vrc.v_oh", Double(mouth)*0.12)
            set("vrc.v_ih", 0)
            set("eye_joy", max(happy*(0.17+0.17*onset+0.12*emphasis), Double(pat)*0.65)*(1-blink))
            set("mouth_happy_1", max(0.04+happy*(0.25+0.17*onset), Double(pat)*0.6)*(1-Double(mouth)*0.3))
            set("brow_joy", happy*(0.15+0.15*onset+0.1*emphasis))
            set("eye_sad_1", concern*(0.22+0.14*onset)*(1-blink))
            set("brow_sad_1", concern*(0.3+0.16*onset+0.08*emphasis))
            set("eye_bikkuri", curious*(0.22+0.2*onset)*(1-blink))
            set("brow_surprised", curious*(0.2+0.2*onset+0.1*emphasis) + (userSpeaking ? 0.1 : 0))
        }
        let idle: Float = reduce ? 0 : 1
        var pitch: Float = 0
        if gesture == "nod", gestureAge >= 0 && gestureAge < 1.6 {
            // A single acknowledgement, not repeated metronomic nodding.
            pitch += Float(pow(sin(gestureAge/1.6 * .pi), 2))*0.10*idle
        }
        pitch += pat*0.07*idle
        if let node = bones["Head"] {
            let base = motion == nil ? rest["Head"]! : node.simdOrientation
            node.simdOrientation = base * simd_quatf(angle: pitch, axis: [1,0,0]) * simd_quatf(angle: pat*0.14*idle, axis: [0,0,1])
        }
        if motion == nil, let node = bones["Chest"] {
            let base = rest["Chest"]!
            node.simdOrientation = base * simd_quatf(angle: Float(sin(time*1.5))*0.008*idle, axis: [1,0,0])
        }
        // Small gaze shifts return toward the caller while listening; no camera tracking.
        let gaze: Float = userSpeaking ? 0 : Float(sin(time*0.23))*0.035*idle
        for name in ["LeftEye", "RightEye"] {
            if let node = bones[name], let base = rest[name] {
                node.simdOrientation = base * simd_quatf(angle: gaze, axis: [0,1,0])
            }
        }
        if motion == nil {
            for (name, angle) in [("Upper_arm_L", Float(-0.95)), ("Upper_arm_R", Float(0.95))] {
                if let node = bones[name], let base = rest[name] {
                    node.simdOrientation = base * simd_quatf(angle: angle, axis: armAxes[name] ?? [0,0,1])
                }
            }
            for name in ["Lower_arm_R", "Hand_R"] { if let base = rest[name] { bones[name]?.simdOrientation = base } }
        }
        let waveTarget = gesture == "wave" ? envelope(2.8)*idle : 0
        waveBlend = reduce ? 0 : waveBlend+(waveTarget-waveBlend)*min(1, dt*10)
        let wave = waveBlend
        let waveSweep = Float(sin(min(1, max(0, gestureAge/2.8)) * .pi*3))
        for (index, finger) in relaxedFingers.enumerated() {
            // Fingers retain their own capture strength even when the other arm leads.
            // Only the waving hand opens; the other hand keeps its resting motion.
            let quiet = motion?.fingerPose(clip: 0, finger: index, time: reduce ? 0 : time) ?? SIMD2<Float>(finger.curl, 0)
            let captured = motion?.fingerPose(clip: talkClip, finger: index, time: max(0, talkAge)) ?? quiet
            var pose = quiet+(captured-quiet)*talkBlend
            if index >= 15 {
                let opening: Float = index >= 27 ? 0.45 : 0.88
                pose.x *= 1-wave*opening
                // Preserve a little captured individual motion while opening for a greeting.
                pose += (captured-quiet)*wave*0.18
            }
            let target = finger.rest * simd_quatf(angle: pose.y, axis: finger.spreadAxis) * simd_quatf(angle: max(0, pose.x), axis: finger.axis)
            finger.node.simdOrientation = simd_slerp(finger.node.simdOrientation, target, reduce ? 1 : min(1, dt*14))
        }
        for side in ["L", "R"] {
            guard let upper = bones["Upper_arm_\(side)"], let lower = bones["Lower_arm_\(side)"],
                  let hand = bones["Hand_\(side)"], let hips = bones["Hips"], let head = bones["Head"] else { continue }
            let shoulder = upper.simdWorldPosition, captured = hand.simdWorldPosition
            let handOrientation = hand.simdOrientation
            // Milltina's flared skirt occupies the source capture's low hand workspace.
            // Rebase the captured trajectory above/in front of it; retain its timing and asymmetry.
            // ponytail: calibrated to this 1.3 m avatar and outfit; re-fit paths if the model or clothing changes.
            let lead: Float = side == "L" ? (talkClip == 1 ? 1 : 0.15) : (talkClip == 2 ? 1 : 0.15)
            // Rest down beside the apron; raise only the leading hand into the conversational workspace.
            let resting = SIMD3<Float>(captured.x*0.8, captured.y+0.09, hips.simdWorldPosition.z+0.17+captured.z*0.3)
            let conversing = SIMD3<Float>(captured.x*0.8, captured.y+0.14, hips.simdWorldPosition.z+0.215+captured.z*0.3)
            var target = resting+(conversing-resting)*talkBlend*lead
            let greeting = side == "R" ? wave : 0
            let waveTarget = head.simdWorldPosition + SIMD3<Float>(-0.14+waveSweep*0.006, -0.10, 0.25)
            target += (waveTarget-target)*greeting
            let a = simd_distance(shoulder, lower.simdWorldPosition), b = simd_distance(lower.simdWorldPosition, captured)
            let direction = simd_normalize(target-shoulder)
            let distance = min(a+b-0.008, max(abs(a-b)+0.008, simd_distance(shoulder, target)))
            let along = (a*a-b*b+distance*distance)/(2*distance)
            let height = sqrt(max(0, a*a-along*along))
            let pole = SIMD3<Float>(side == "R" ? -0.5 : 0.5, -1, 0.25)
            let bend = simd_normalize(pole-direction*simd_dot(pole, direction))
            let elbow = shoulder+direction*along+bend*height
            func aim(_ node: SCNNode, child: SCNNode, at position: SIMD3<Float>) {
                let from = simd_normalize(child.simdWorldPosition-node.simdWorldPosition)
                let to = simd_normalize(position-node.simdWorldPosition)
                node.simdWorldOrientation = simd_quatf(from: from, to: to)*node.simdWorldOrientation
            }
            aim(upper, child: lower, at: elbow)
            aim(lower, child: hand, at: shoulder+direction*distance)
            // Preserve captured wrist flexion relative to the corrected forearm, not the old world direction.
            hand.simdOrientation = handOrientation
            if greeting > 0 { hand.simdOrientation *= simd_quatf(angle: waveSweep*0.14*greeting, axis: [0,0,1]) }
        }
        for name in ["twin tail1_L.002", "twin tail1_R.002"] {
            if let node = bones[name], let base = rest[name] {
                node.simdOrientation = base * simd_quatf(angle: degraded ? 0 : Float(sin(time*1.1))*0.018*idle, axis: [0,0,1])
            }
        }
    }
}

struct AvatarView: UIViewRepresentable {
    let avatar: AvatarScene
    var degraded: Bool
    var active: Bool
    var onTouch: (Bool) -> Void
    @MainActor final class Coordinator: NSObject {
        let avatar: AvatarScene
        var onTouch: (Bool) -> Void
        init(_ avatar: AvatarScene, onTouch: @escaping (Bool) -> Void) { self.avatar = avatar; self.onTouch = onTouch }
        @objc func tapped(_ sender: UITapGestureRecognizer) {
            guard let view = sender.view as? SCNView else { return }
            let point = sender.location(in: view)
            guard !view.hitTest(point, options: [.searchMode: SCNHitTestSearchMode.closest.rawValue]).isEmpty else { return }
            let neck = avatar.scene.rootNode.childNode(withName: "Neck", recursively: true)
            let isHead = neck.map { Float(point.y) < view.projectPoint($0.worldPosition).y } ?? false
            onTouch(isHead)
        }
    }
    func makeCoordinator() -> Coordinator { Coordinator(avatar, onTouch: onTouch) }
    func makeUIView(context: Context) -> SCNView {
        let view = SCNView(frame: .zero, options: [SCNView.Option.preferredRenderingAPI.rawValue: SCNRenderingAPI.metal.rawValue])
        view.scene = avatar.scene; view.pointOfView = avatar.camera
        view.delegate = avatar; view.isPlaying = true; view.rendersContinuously = true
        view.accessibilityIdentifier = "realtime-avatar"; view.accessibilityLabel = "Norma 虚拟陪伴角色"; view.isAccessibilityElement = true
        view.addGestureRecognizer(UITapGestureRecognizer(target: context.coordinator, action: #selector(Coordinator.tapped(_:))))
        return view
    }
    func updateUIView(_ view: SCNView, context: Context) {
        context.coordinator.onTouch = onTouch
        view.preferredFramesPerSecond = degraded ? 30 : 60
        view.isPlaying = active; view.rendersContinuously = active
        view.antialiasingMode = degraded ? .none : .multisampling2X
        view.contentScaleFactor = min(UIScreen.main.scale, degraded ? 1.5 : 2)
    }
}
