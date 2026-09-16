import * as THREE from 'three';

// Keep glass in the world-rendered scene. CSS backdrop-filter is invisible in WebXR.
export function drawGlassSurface(
  c: CanvasRenderingContext2D,
  mask: CanvasRenderingContext2D,
  x: number,
  y: number,
  w: number,
  h: number,
  radius = 30,
) {
  mask.save();
  mask.setTransform(c.getTransform());
  mask.fillStyle = '#fff';
  mask.beginPath();
  mask.roundRect(x, y, w, h, radius);
  mask.fill();
  mask.restore();

  c.save();
  c.beginPath();
  c.roundRect(x, y, w, h, radius);
  const sheen = c.createLinearGradient(x, y, x + w * 0.35, y + h);
  sheen.addColorStop(0, '#ffffff18');
  sheen.addColorStop(0.4, '#dceafa09');
  sheen.addColorStop(1, '#9caeb80e');
  c.fillStyle = sheen;
  c.fill();
  const rim = c.createLinearGradient(x, y, x + w * 0.72, y + h);
  rim.addColorStop(0, '#ffffffa8');
  rim.addColorStop(0.28, '#eef5ff40');
  rim.addColorStop(0.62, '#e2edfa16');
  rim.addColorStop(1, '#eef7ff60');
  c.strokeStyle = rim;
  c.lineWidth = 1.8;
  c.stroke();
  c.beginPath();
  c.roundRect(x + 3, y + 3, w - 6, h - 6, radius - 2);
  c.strokeStyle = '#eef6ff0d';
  c.lineWidth = 2;
  c.stroke();
  c.restore();
}

export function createGlassMaterial(
  content: THREE.Texture,
  mask: THREE.Texture,
  environment: THREE.Texture,
  width: number,
  height: number,
) {
  return new THREE.ShaderMaterial({
    transparent: true,
    side: THREE.DoubleSide,
    uniforms: {
      content: { value: content },
      glassMask: { value: mask },
      environment: { value: environment },
      texel: { value: new THREE.Vector2(1 / width, 1 / height) },
      solid: { value: 0 },
      backdropTint: { value: new THREE.Color('#8fa4b5') },
    },
    vertexShader: /* glsl */ `
      varying vec2 panelUv;
      varying vec3 worldPoint;
      varying vec3 worldNormal;
      void main() {
        panelUv = uv;
        worldPoint = (modelMatrix * vec4(position, 1.0)).xyz;
        worldNormal = normalize(mat3(modelMatrix) * normal);
        gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
      }
    `,
    fragmentShader: /* glsl */ `
      uniform sampler2D content;
      uniform sampler2D glassMask;
      uniform sampler2D environment;
      uniform vec2 texel;
      uniform float solid;
      uniform vec3 backdropTint;
      varying vec2 panelUv;
      varying vec3 worldPoint;
      varying vec3 worldNormal;

      vec3 frostedEnvironment(vec2 uv) {
        vec2 spread = vec2(0.006, 0.013);
        vec3 color = texture2D(environment, uv).rgb * 0.24;
        color += texture2D(environment, uv + vec2(spread.x, 0.0)).rgb * 0.12;
        color += texture2D(environment, uv - vec2(spread.x, 0.0)).rgb * 0.12;
        color += texture2D(environment, uv + vec2(0.0, spread.y)).rgb * 0.12;
        color += texture2D(environment, uv - vec2(0.0, spread.y)).rgb * 0.12;
        color += texture2D(environment, uv + spread).rgb * 0.07;
        color += texture2D(environment, uv - spread).rgb * 0.07;
        color += texture2D(environment, uv + spread * vec2(1.0, -1.0)).rgb * 0.07;
        color += texture2D(environment, uv + spread * vec2(-1.0, 1.0)).rgb * 0.07;
        return color * backdropTint;
      }

      void main() {
        vec4 ink = texture2D(content, panelUv);
        float coverage = texture2D(glassMask, panelUv).a;
        if (max(ink.a, coverage) < 0.003) discard;
        // Each eye uses its own camera position. Intersect the same environment
        // sphere as the scene so moving a window never drags a painted backdrop.
        vec3 ray = normalize(worldPoint - cameraPosition);
        vec3 origin = cameraPosition - vec3(0.0, 5.0, 0.0);
        float b = dot(origin, ray);
        float distance = -b + sqrt(max(0.0, b * b + 900.0 - dot(origin, origin)));
        vec3 direction = normalize(origin + distance * ray);
        vec2 envUv = vec2(atan(direction.z, -direction.x) / 6.2831853,
                          0.5 + asin(clamp(direction.y, -1.0, 1.0)) / 3.1415927);
        // A small edge deflection suggests thickness, without distorting text.
        vec2 edge = vec2(
          texture2D(glassMask, panelUv + vec2(texel.x * 4.0, 0.0)).a -
          texture2D(glassMask, panelUv - vec2(texel.x * 4.0, 0.0)).a,
          texture2D(glassMask, panelUv + vec2(0.0, texel.y * 4.0)).a -
          texture2D(glassMask, panelUv - vec2(0.0, texel.y * 4.0)).a);
        vec3 glass = frostedEnvironment(envUv + edge * 0.004);
        glass = mix(glass, vec3(0.035, 0.049, 0.063), 0.35);
        float fresnel = pow(1.0 - abs(dot(normalize(worldNormal), -ray)), 2.0);
        glass += vec3(0.035, 0.042, 0.05) * (0.35 + fresnel);
        glass = mix(glass, vec3(0.028, 0.043, 0.06), solid);
        float alpha = ink.a + coverage * (1.0 - ink.a);
        vec3 color = (ink.rgb * ink.a + glass * coverage * (1.0 - ink.a)) / max(alpha, 0.001);
        gl_FragColor = vec4(color, alpha);
        #include <colorspace_fragment>
      }
    `,
  });
}
