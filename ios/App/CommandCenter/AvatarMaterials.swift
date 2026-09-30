import SceneKit

enum AvatarMaterials {
    /// Retain the author's painted textures and primary lilToon shadow palette.
    /// This is a small native toon shader, not a port of the full lilToon renderer.
    static func apply(to material: SCNMaterial) {
        let name = material.name ?? ""
        let color: [Float], strength: Float, border: Float, blur: Float
        switch name {
        case "Milltina_face_alpha":
            // Author's Face_Alpha has UseShadow=0; eyelashes/eye details stay unlit.
            material.lightingModel = .constant; material.shaderModifiers = nil
            return
        case "Milltina_face":
            color = [1, 0.9103774, 0.9189129]; strength = 1; border = 0.5; blur = 0.17
        case "Milltina_body":
            color = [0.91372555, 0.8705883, 0.8509805]; strength = 0.669; border = 0.5; blur = 0.17
        case "Milltina_hair", "Milltina_hair_alpha":
            color = [0.753717, 0.6799572, 0.75471693]; strength = 1; border = 0.5; blur = 0.17
        case "Milltina_cloth_white":
            color = [0.8745099, 0.87843144, 0.9294118]; strength = 1; border = 0.423; blur = 0.444
        case "Milltina_cloth_black":
            color = [0.89, 0.80545, 0.89]; strength = 1; border = 0.5; blur = 0.444
        default: return
        }
        // Unity color parameters are sRGB; SceneKit's surface colors are linear.
        let linear = color.map { $0 <= 0.04045 ? $0/12.92 : pow(($0+0.055)/1.055, 2.4) }
        material.lightingModel = .constant
        material.setValue(NSValue(scnVector3: SCNVector3(linear[0], linear[1], linear[2])), forKey: "shadeColor")
        material.setValue(strength, forKey: "shadeStrength")
        material.setValue(border, forKey: "shadeBorder")
        material.setValue(blur, forKey: "shadeBlur")
        // ponytail: one camera-relative portrait key. Use a world-space light uniform if the camera becomes movable.
        // No additional texture sample, draw pass, or gray Lambert shading over the painted highlights.
        material.shaderModifiers = [.surface: """
        #pragma arguments
        float3 shadeColor;
        float shadeStrength;
        float shadeBorder;
        float shadeBlur;
        #pragma body
        float3 n = normalize(_surface.normal);
        float halfLambert = dot(n, normalize(float3(-0.35, 0.65, 1.0))) * 0.5 + 0.5;
        float lit = smoothstep(shadeBorder - shadeBlur * 0.5, shadeBorder + shadeBlur * 0.5, halfLambert);
        _surface.diffuse.rgb *= mix(float3(1.0), shadeColor, (1.0 - lit) * shadeStrength);
        """]
    }
}
