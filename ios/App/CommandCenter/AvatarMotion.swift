import SceneKit

/// Baked licensed world-space rotations. Sources and terms ship in Resources/Motions.
struct AvatarMotion: Decodable {
    struct Clip: Decodable {
        let name: String
        let duration: Double
        let frames: [[Float]]
        let fingerCurls: [[Float]]?
        let fingerSpreads: [[Float]]?
    }
    let fps: Double
    let bones: [String]
    let clips: [Clip]

    static func load() -> AvatarMotion? {
        guard let url = Bundle.main.url(forResource: "companion-motion", withExtension: "json", subdirectory: "Motions"),
              let data = try? Data(contentsOf: url), data.count < 512*1024,
              let motion = try? JSONDecoder().decode(Self.self, from: data),
              motion.fps == 30, motion.bones.count == 19, Set(motion.bones).count == 19,
              motion.clips.map(\.name) == ["idle", "talk", "reply"],
              motion.clips.allSatisfy({ clip in
                  clip.duration.isFinite && clip.duration > 0 && clip.duration <= 15 &&
                  clip.frames.count == Int(ceil(clip.duration*motion.fps))+1 &&
                  clip.frames.allSatisfy { $0.count == motion.bones.count*4 && $0.allSatisfy { $0.isFinite && abs($0) <= 1.001 } } &&
                  (clip.fingerCurls?.count == clip.frames.count &&
                    clip.fingerCurls!.allSatisfy { $0.count == 30 && $0.allSatisfy { $0.isFinite && $0 >= 0 && $0 <= 1.3 } } &&
                    clip.fingerSpreads?.count == clip.frames.count &&
                    clip.fingerSpreads!.allSatisfy { $0.count == 10 && $0.allSatisfy { $0.isFinite && abs($0) <= 0.22 } })
              }) else { return nil }
        return motion
    }

    func rotation(clip: Int, bone: Int, time: Double) -> simd_quatf {
        let c = clips[clip]
        let position = (clip == 0 ? max(0, time).truncatingRemainder(dividingBy: c.duration) : min(c.duration, max(0, time)))*fps
        let first = min(Int(position), c.frames.count-2), next = first+1, offset = bone*4
        func quat(_ frame: Int) -> simd_quatf {
            let v = c.frames[frame]
            return simd_normalize(simd_quatf(ix: v[offset], iy: v[offset+1], iz: v[offset+2], r: v[offset+3]))
        }
        return simd_slerp(quat(first), quat(next), Float(position-Double(first)))
    }

    func fingerPose(clip: Int, finger: Int, time: Double) -> SIMD2<Float>? {
        guard let curls = clips[clip].fingerCurls, let spreads = clips[clip].fingerSpreads else { return nil }
        let c = clips[clip]
        let position = (clip == 0 ? max(0, time).truncatingRemainder(dividingBy: c.duration) : min(c.duration, max(0, time)))*fps
        let first = min(Int(position), curls.count-2), blend = Float(position-Double(first))
        let curl = curls[first][finger]*(1-blend)+curls[first+1][finger]*blend
        let spread = finger % 3 == 0 ? spreads[first][finger/3]*(1-blend)+spreads[first+1][finger/3]*blend : 0
        return [curl, spread]
    }
}
