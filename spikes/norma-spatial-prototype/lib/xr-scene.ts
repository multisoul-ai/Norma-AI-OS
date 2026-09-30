import * as THREE from 'three';
import { decisionSnapshot, decisionWindow } from './decisions';
import { createGlassMaterial } from './xr-glass';
import type { SpatialMenu, SpatialVoice } from './spatial-workflow';
import { describeSpatialScene } from './spatial-activity';
import {
  appendStroke,
  undoStroke,
  annotationCommand,
  strokeHits,
  type CanvasDocument,
  type CanvasTool,
  type Stroke,
  type CanvasNode,
  type Point,
} from './free-canvas';
import { spatialCanvasPoint, spatialCanvasViewport } from './xr-canvas';
import {
  createDrawing,
  drawPanel,
  hitRegion,
  type PanelDrawing,
} from './xr-panels';
import { type State, type Panel, defaultPanel, adjustPanel } from './model';
export type SceneBridge = {
  read: () => State;
  send: (text: string) => void;
  view: (view: State['view']) => void;
  canvas: (doc: CanvasDocument) => void;
  voice: () => void;
  listening: () => boolean;
  voiceState: () => SpatialVoice;
  toggleVoiceOutput: () => void;
  notice: (text: string) => void;
  leave: () => void;
};
type SpatialPanel = {
  mesh: THREE.Mesh<THREE.PlaneGeometry, THREE.ShaderMaterial>;
  drawing: PanelDrawing;
  texture: THREE.CanvasTexture;
  glassTexture: THREE.CanvasTexture;
  transform: Panel;
  initial: THREE.Vector3;
  hover?: string;
};
type Grab = {
  panel?: SpatialPanel;
  action: string;
  source?: XRInputSource;
  plane: THREE.Plane;
  offset: THREE.Vector3;
  initialPoint: THREE.Vector3;
  initialPosition: THREE.Vector3;
  gripStart?: THREE.Vector3;
  moved: boolean;
  lastX: number;
  lastY: number;
  stroke?: Stroke;
  node?: CanvasNode;
  canvasStart?: Point;
  canvasView?: ReturnType<typeof spatialCanvasViewport>;
};
export function createSpatialScene(host: HTMLElement, bridge: SceneBridge) {
  const renderer = new THREE.WebGLRenderer({ antialias: true, alpha: false });
  renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
  renderer.xr.enabled = true;
  renderer.xr.setReferenceSpaceType('local');
  renderer.outputColorSpace = THREE.SRGBColorSpace;
  host.appendChild(renderer.domElement);
  renderer.domElement.setAttribute('role', 'img');
  renderer.domElement.setAttribute(
    'aria-label',
    'Norma 三维空间：可用指针选择内容，拖动标题移动窗口',
  );
  const scene = new THREE.Scene();
  scene.background = new THREE.Color('#1b3041');
  const root = new THREE.Group();
  scene.add(root);
  const camera = new THREE.PerspectiveCamera(63, 1, 0.05, 100);
  camera.position.set(0, 1.6, 0);
  const raycaster = new THREE.Raycaster();
  const pointer = new THREE.Vector2();
  const matrices = new THREE.Matrix4();
  const quaternion = new THREE.Quaternion();
  const point = new THREE.Vector3();
  const panels: SpatialPanel[] = [];
  let disposed = false;
  let focus = true;
  let menu: SpatialMenu = null;
  let selectedNode: string | undefined;
  let selectedDecision: string | undefined;
  let decisionPage = 0;
  let dirty = true;
  let lastRevision = -1;
  let notice = '';
  let environment: HTMLImageElement | undefined;
  let grab: Grab | undefined;
  let desktopDown = false;
  let yaw = 0;
  let pitch = 0;
  let previousX = 0;
  let previousY = 0;
  let immersiveSession: XRSession | null = null;
  let centered = false;
  let canvasTool: CanvasTool = 'select';
  const marker = new THREE.Mesh(
    new THREE.RingGeometry(0.008, 0.012, 28),
    new THREE.MeshBasicMaterial({
      color: '#e5f5ff',
      transparent: true,
      opacity: 0.9,
      depthTest: false,
      side: THREE.DoubleSide,
    }),
  );
  marker.visible = false;
  marker.renderOrder = 9;
  scene.add(marker);
  const backgroundTexture = new THREE.TextureLoader().load(
    '/environment.png',
    (texture) => {
      if (disposed) {
        texture.dispose();
        return;
      }
      texture.colorSpace = THREE.SRGBColorSpace;
      texture.wrapS = THREE.RepeatWrapping;
      const bg = new THREE.Mesh(
        new THREE.SphereGeometry(30, 64, 32),
        new THREE.MeshBasicMaterial({
          map: texture,
          side: THREE.BackSide,
          color: '#8fa4b5',
        }),
      );
      bg.position.y = 5;
      scene.add(bg);
      environment = texture.image;
      dirty = true;
    },
  );
  // World-locked panels stay at a comfortable initial viewing height, with no artificial locomotion.
  for (const [kind, x, y, z, width] of [
    ['main', 0, 1.6, -2.1, 1.6],
    ['team', 0, 1.6, -1.6, 0.82],
    ['control', 0, 1.6, -1.62, 0.78],
    ['activity', -1.21, 1.63, -2.2, 0.58],
    ['decisions', 1.21, 1.62, -2.2, 0.6],
    ['decision-detail', 0, 1.6, -1.75, 0.9],
  ] as const) {
    const drawing = createDrawing(kind);
    const texture = new THREE.CanvasTexture(drawing.canvas);
    texture.colorSpace = THREE.SRGBColorSpace;
    texture.anisotropy = Math.min(renderer.capabilities.getMaxAnisotropy(), 4);
    const glassTexture = new THREE.CanvasTexture(drawing.glass);
    const mesh = new THREE.Mesh(
      new THREE.PlaneGeometry(
        width,
        (width * drawing.canvas.height) / drawing.canvas.width,
      ),
      createGlassMaterial(
        texture,
        glassTexture,
        backgroundTexture,
        drawing.canvas.width,
        drawing.canvas.height,
      ),
    );
    mesh.position.set(x, y, z);
    if (kind === 'activity') mesh.rotation.y = 0.12;
    if (kind === 'decisions') mesh.rotation.y = -0.12;
    mesh.visible =
      kind === 'main' || kind === 'activity' || kind === 'decisions';
    root.add(mesh);
    panels.push({
      mesh,
      drawing,
      texture,
      glassTexture,
      transform: { ...defaultPanel },
      initial: mesh.position.clone(),
    });
  }
  const transparencyPreference = window.matchMedia(
    '(prefers-reduced-transparency: reduce)',
  );
  const contrastPreference = window.matchMedia('(prefers-contrast: more)');
  const updateGlassPreference = () => {
    const solid =
      transparencyPreference.matches || contrastPreference.matches ? 1 : 0;
    panels.forEach((panel) => {
      panel.mesh.material.uniforms.solid.value = solid;
    });
  };
  transparencyPreference.addEventListener('change', updateGlassPreference);
  contrastPreference.addEventListener('change', updateGlassPreference);
  updateGlassPreference();
  const model = new THREE.Group();
  model.position.set(0, 1.5, -1.77);
  root.add(model);
  const modelMaterial = new THREE.MeshStandardMaterial({
    color: '#b4d6e7',
    metalness: 0.5,
    roughness: 0.25,
  });
  const object = new THREE.Mesh(
    new THREE.TorusKnotGeometry(0.17, 0.047, 160, 24),
    modelMaterial,
  );
  model.add(object);
  const halo = new THREE.Mesh(
    new THREE.TorusGeometry(0.28, 0.004, 8, 80),
    new THREE.MeshBasicMaterial({ color: '#7195ad' }),
  );
  halo.rotation.x = Math.PI / 2;
  model.add(halo);
  scene.add(new THREE.HemisphereLight('#e0f0ff', '#31404e', 3));
  const light = new THREE.DirectionalLight('#ffffff', 3);
  light.position.set(2, 4, 1);
  scene.add(light);
  function setNotice(text: string) {
    notice = text;
    dirty = true;
    bridge.notice(text);
  }
  function recenter() {
    const current = renderer.xr.isPresenting ? renderer.xr.getCamera() : camera;
    const p = current.getWorldPosition(new THREE.Vector3());
    const q = current.getWorldQuaternion(new THREE.Quaternion());
    const e = new THREE.Euler().setFromQuaternion(q, 'YXZ');
    root.position.set(p.x, p.y - 1.6, p.z);
    root.rotation.set(0, e.y, 0);
    panels.forEach((panel) => {
      panel.mesh.position.copy(panel.initial);
      panel.mesh.scale.setScalar(1);
      panel.transform = { ...defaultPanel };
    });
    marker.visible = false;
    dirty = true;
    setNotice('空间已回到你面前。');
  }
  function resize() {
    const width = host.clientWidth || window.innerWidth;
    const height = host.clientHeight || window.innerHeight;
    renderer.setSize(width, height);
    camera.aspect = width / height;
    // The desktop preview fits the available width; immersive projection remains owned by the headset.
    camera.fov = THREE.MathUtils.clamp(
      THREE.MathUtils.radToDeg(
        2 * Math.atan(Math.tan(THREE.MathUtils.degToRad(36)) / camera.aspect),
      ),
      40,
      75,
    );
    camera.updateProjectionMatrix();
  }
  const resizeObserver = new ResizeObserver(resize);
  resizeObserver.observe(host);
  resize();
  function fromSource(source: XRInputSource, frame: XRFrame) {
    const reference = renderer.xr.getReferenceSpace();
    if (!reference) return false;
    const pose = frame.getPose(source.targetRaySpace, reference);
    if (!pose) return false;
    matrices.fromArray(pose.transform.matrix);
    raycaster.ray.origin.setFromMatrixPosition(matrices);
    raycaster.ray.direction
      .set(0, 0, -1)
      .applyQuaternion(quaternion.setFromRotationMatrix(matrices))
      .normalize();
    return true;
  }
  function hit() {
    scene.updateMatrixWorld(true);
    const hits = raycaster.intersectObjects(
      [
        ...panels.filter((p) => p.mesh.visible).map((p) => p.mesh),
        ...(model.visible ? [object] : []),
      ],
      false,
    );
    const h = hits[0];
    if (!h) return;
    const panel = panels.find((p) => p.mesh === h.object);
    if (!panel) return { h, action: 'model' };
    const r =
      h.uv &&
      hitRegion(
        panel.drawing.regions,
        h.uv.x * panel.drawing.canvas.width,
        (1 - h.uv.y) * panel.drawing.canvas.height,
      );
    return { h, panel, action: r?.action ?? '' };
  }
  function feedback(result: ReturnType<typeof hit>) {
    panels.forEach((panel) => {
      const hover = result?.panel === panel ? result.action : undefined;
      if (panel.hover !== hover) {
        panel.hover = hover;
        dirty = true;
      }
    });
    marker.visible = !!result;
    if (result) {
      marker.position.copy(result.h.point);
      marker.lookAt(raycaster.ray.origin);
      marker.position.addScaledVector(raycaster.ray.direction, -0.003);
    }
    renderer.domElement.style.cursor =
      result?.action === 'drag'
        ? 'grab'
        : result?.action
          ? 'pointer'
          : 'default';
  }
  function action(action: string, panel?: SpatialPanel) {
    const detailPanel = panels.find(
      (p) => p.drawing.kind === 'decision-detail',
    )!;
    if (action.startsWith('decision:open:')) {
      const id = action.slice('decision:open:'.length);
      if (!decisionSnapshot(bridge.read()).items.some((item) => item.id === id))
        return;
      selectedDecision = id;
      detailPanel.mesh.visible = true;
      panels[1].mesh.visible = false;
      panels[2].mesh.visible = false;
      menu = null;
      dirty = true;
      return;
    }
    if (action === 'decision:close') {
      detailPanel.mesh.visible = false;
      dirty = true;
      return;
    }
    if (action.startsWith('decision:page:')) {
      decisionPage = decisionWindow(
        bridge.read(),
        decisionPage + (action.endsWith('next') ? 1 : -1),
      ).page;
      dirty = true;
      return;
    }
    if (
      ['dialogue', 'voice', 'team', 'more'].includes(action) ||
      action.startsWith('command:') ||
      action.startsWith('view:')
    ) {
      detailPanel.mesh.visible = false;
    }
    if (action === 'more') {
      menu = menu ? null : 'space';
      dirty = true;
      return;
    }
    if (action.startsWith('menu:')) {
      const next = action.slice(5);
      menu = next === 'close' ? null : (next as SpatialMenu);
      dirty = true;
      return;
    }
    if (action === 'dialogue' || action === 'voice') {
      panels[2].mesh.visible = !panels[2].mesh.visible;
      panels[1].mesh.visible = false;
      menu = null;
      dirty = true;
      return;
    }
    if (action === 'voice:toggle') {
      bridge.voice();
      dirty = true;
      return;
    }
    if (action === 'voice:output') {
      bridge.toggleVoiceOutput();
      dirty = true;
      return;
    }
    if (action.startsWith('say:')) {
      notice = '';
      bridge.send(action.slice(4));
      dirty = true;
      return;
    }
    if (action === 'team') {
      panels[1].mesh.visible = !panels[1].mesh.visible;
      panels[2].mesh.visible = false;
      menu = null;
      dirty = true;
      return;
    }
    if (action.startsWith('tool:')) {
      canvasTool = action.slice(5) as CanvasTool;
      if (canvasTool === 'pen' && bridge.read().view !== 'canvas')
        bridge.view('canvas');
      menu = null;
      dirty = true;
      return;
    }
    if (action === 'canvas:undo') {
      bridge.canvas(undoStroke(bridge.read().canvas));
      dirty = true;
      return;
    }
    if (action === 'canvas:send') {
      if (bridge.read().canvas.strokes.length) {
        bridge.send(annotationCommand(bridge.read().canvas));
        canvasTool = 'select';
        notice = '';
      } else setNotice('先选择画笔，在内容旁画出你的想法。');
      dirty = true;
      return;
    }
    if (action.startsWith('command:')) {
      notice = '';
      bridge.send(action.slice(8));
      menu = null;
      dirty = true;
      return;
    }
    if (action.startsWith('view:')) {
      bridge.view(action.slice(5) as State['view']);
      canvasTool = 'select';
      menu = null;
      notice = '';
      dirty = true;
      return;
    }
    const main = panel ?? panels[0];
    const t = main.transform;
    if (action === 'pin') {
      main.transform.pinned = !t.pinned;
      setNotice(
        main.transform.pinned
          ? '窗口已固定，再次点击可解除。'
          : '窗口已解锁，可以拖动。',
      );
    }
    if (action === 'smaller' || action === 'larger') {
      main.transform = adjustPanel(t, {
        scale: t.scale + (action === 'larger' ? 0.1 : -0.1),
      });
      main.mesh.scale.setScalar(main.transform.scale);
      dirty = true;
    }
    if (action === 'nearer' || action === 'farther') {
      main.transform = adjustPanel(t, {
        depth: t.depth + (action === 'farther' ? 0.2 : -0.2),
      });
      main.mesh.position.z = -main.transform.depth;
      dirty = true;
    }
    if (action === 'focus') {
      focus = !focus;
      panels.slice(1, 3).forEach((p) => (p.mesh.visible = !focus));
      dirty = true;
    }
    if (action === 'recenter') recenter();
    if (action === 'exit') {
      if (immersiveSession) void immersiveSession.end();
      else bridge.leave();
    }
  }
  function begin(source?: XRInputSource, frame?: XRFrame) {
    const result = hit();
    feedback(result);
    if (!result?.action) return;
    const { h, panel, action: kind } = result;
    const normal =
      (kind.startsWith('canvas:') || kind.startsWith('node:')) && panel
        ? new THREE.Vector3(0, 0, 1).applyQuaternion(
            panel.mesh.getWorldQuaternion(new THREE.Quaternion()),
          )
        : raycaster.ray.direction.clone().negate();
    const plane = new THREE.Plane().setFromNormalAndCoplanarPoint(
      normal,
      h.point,
    );
    dirty = true;
    grab = {
      panel,
      action: kind,
      source,
      plane,
      offset: panel
        ? h.point.clone().sub(panel.mesh.getWorldPosition(new THREE.Vector3()))
        : new THREE.Vector3(),
      initialPoint: h.point.clone(),
      initialPosition:
        panel?.mesh.getWorldPosition(new THREE.Vector3()) ??
        new THREE.Vector3(),
      moved: false,
      lastX: 0,
      lastY: 0,
    };
    if (panel && (kind.startsWith('canvas:') || kind.startsWith('node:'))) {
      const pixel = {
        x: h.uv!.x * panel.drawing.canvas.width,
        y: (1 - h.uv!.y) * panel.drawing.canvas.height,
      };
      grab.canvasStart = spatialCanvasPoint(bridge.read().canvas, pixel);
      grab.canvasView = spatialCanvasViewport(bridge.read().canvas);
      if (kind === 'canvas:pen')
        grab.stroke = {
          id: crypto.randomUUID(),
          color: '#b5e7dc',
          width: 4,
          points: [grab.canvasStart],
        };
      if (kind.startsWith('node:'))
        grab.node = bridge
          .read()
          .canvas.nodes.find((n) => n.id === kind.slice(5));
    }
    if (source?.gripSpace && frame) {
      const ref = renderer.xr.getReferenceSpace();
      const grip = ref && frame.getPose(source.gripSpace, ref);
      if (grip)
        grab.gripStart = new THREE.Vector3(
          grip.transform.position.x,
          grip.transform.position.y,
          grip.transform.position.z,
        );
    }
  }
  function move(frame?: XRFrame) {
    if (!grab) return;
    const g = grab;
    if (
      (g.action.startsWith('canvas:') || g.action.startsWith('node:')) &&
      g.panel &&
      g.canvasStart &&
      g.canvasView
    ) {
      const ref = renderer.xr.getReferenceSpace();
      const pose =
        frame && g.source?.gripSpace && ref
          ? frame.getPose(g.source.gripSpace, ref)
          : null;
      let world: THREE.Vector3 | undefined;
      if (pose && g.gripStart) {
        world = g.initialPoint
          .clone()
          .add(
            new THREE.Vector3(
              pose.transform.position.x,
              pose.transform.position.y,
              pose.transform.position.z,
            ).sub(g.gripStart),
          );
        g.plane.projectPoint(world, world);
      } else if (raycaster.ray.intersectPlane(g.plane, point))
        world = point.clone();
      if (!world) return;
      const local = g.panel.mesh.worldToLocal(world);
      const geometry = g.panel.mesh.geometry.parameters;
      const px =
          (local.x / geometry.width + 0.5) * g.panel.drawing.canvas.width,
        py = (0.5 - local.y / geometry.height) * g.panel.drawing.canvas.height;
      const v = g.canvasView;
      const p = {
        x: (px - v.x) / v.zoom,
        y: (py - v.y) / v.zoom,
        pressure: 0.5,
      };
      if (
        g.stroke &&
        Math.hypot(
          p.x - g.stroke.points.at(-1)!.x,
          p.y - g.stroke.points.at(-1)!.y,
        ) > 2
      ) {
        g.stroke.points.push(p);
        g.moved = true;
        dirty = true;
      }
      if (g.action === 'canvas:eraser') {
        const doc = bridge.read().canvas;
        bridge.canvas({
          ...doc,
          strokes: doc.strokes.filter((s) => !strokeHits(s, p, 18)),
          undone: [],
        });
        g.moved = true;
        dirty = true;
      }
      if (g.node) {
        const node = g.node;
        const dx = p.x - g.canvasStart.x,
          dy = p.y - g.canvasStart.y;
        const doc = bridge.read().canvas;
        if (Math.hypot(dx, dy) > 3) {
          g.moved = true;
          bridge.canvas({
            ...doc,
            nodes: doc.nodes.map((n) =>
              n.id === node.id ? { ...n, x: node.x + dx, y: node.y + dy } : n,
            ),
          });
          dirty = true;
        }
      }
      return;
    }
    if (g.action === 'drag' && g.panel && !g.panel.transform.pinned) {
      let position: THREE.Vector3 | undefined;
      const ref = renderer.xr.getReferenceSpace();
      const pose =
        frame && g.source?.gripSpace && ref
          ? frame.getPose(g.source.gripSpace, ref)
          : null;
      if (pose && g.gripStart) {
        const now = new THREE.Vector3(
          pose.transform.position.x,
          pose.transform.position.y,
          pose.transform.position.z,
        );
        position = g.initialPosition.clone().add(now.sub(g.gripStart));
      } else if (raycaster.ray.intersectPlane(g.plane, point))
        position = point.clone().sub(g.offset);
      if (position) {
        const delta = position.distanceTo(g.initialPosition);
        if (delta > 0.009) g.moved = true;
        const local = root.worldToLocal(position);
        local.x = THREE.MathUtils.clamp(local.x, -2, 2);
        local.y = THREE.MathUtils.clamp(local.y, 0.7, 2.7);
        local.z = THREE.MathUtils.clamp(local.z, -3.5, -1.2);
        g.panel.mesh.position.copy(local);
      }
    } else if (
      g.action === 'model' &&
      raycaster.ray.intersectPlane(g.plane, point)
    ) {
      const delta = point.clone().sub(g.initialPoint);
      object.rotation.y += delta.x * 3;
      object.rotation.x -= delta.y * 3;
      g.initialPoint.copy(point);
      g.moved = true;
    }
  }
  function end(cancel = false) {
    const g = grab;
    if (!g) return;
    grab = undefined;
    dirty = true;
    if (cancel) return;
    if (g.stroke) {
      bridge.canvas(appendStroke(bridge.read().canvas, g.stroke));
      setNotice('标注已留在画布上，可交给 Norma 继续处理。');
      return;
    }
    if (g.action.startsWith('node:') || g.action === 'canvas:eraser') {
      if (g.node && !g.moved) {
        selectedNode = g.node.id;
        dirty = true;
      }
      if (g.moved) setNotice('内容已重新摆放，继续留在空间里工作。');
      return;
    }
    const result = hit();
    if (
      !g.moved &&
      g.action !== 'drag' &&
      g.action !== 'model' &&
      result?.panel === g.panel &&
      result?.action === g.action
    )
      action(g.action, g.panel);
    if (g.moved)
      setNotice(
        g.action === 'model' ? '三维构型已旋转。' : '窗口已放到新的位置。',
      );
    marker.visible = false;
  }
  const canvas = renderer.domElement;
  function desktopRay(event: PointerEvent) {
    const bounds = canvas.getBoundingClientRect();
    pointer.set(
      ((event.clientX - bounds.left) / bounds.width) * 2 - 1,
      (-(event.clientY - bounds.top) / bounds.height) * 2 + 1,
    );
    raycaster.setFromCamera(pointer, camera);
  }
  function down(event: PointerEvent) {
    if (desktopDown || renderer.xr.isPresenting) return;
    desktopDown = true;
    canvas.setPointerCapture(event.pointerId);
    desktopRay(event);
    previousX = event.clientX;
    previousY = event.clientY;
    begin();
  }
  function pointerMove(event: PointerEvent) {
    if (renderer.xr.isPresenting) return;
    desktopRay(event);
    if (desktopDown) {
      if (grab) move();
      else {
        yaw = THREE.MathUtils.clamp(
          yaw - (event.clientX - previousX) * 0.002,
          -0.65,
          0.65,
        );
        pitch = THREE.MathUtils.clamp(
          pitch - (event.clientY - previousY) * 0.002,
          -0.3,
          0.3,
        );
        camera.rotation.set(pitch, yaw, 0, 'YXZ');
      }
      previousX = event.clientX;
      previousY = event.clientY;
    } else feedback(hit());
  }
  function up(event: PointerEvent) {
    desktopRay(event);
    end();
    desktopDown = false;
    if (canvas.hasPointerCapture(event.pointerId))
      canvas.releasePointerCapture(event.pointerId);
  }
  function cancel() {
    end(true);
    desktopDown = false;
  }
  canvas.addEventListener('pointerdown', down);
  canvas.addEventListener('pointermove', pointerMove);
  canvas.addEventListener('pointerup', up);
  canvas.addEventListener('pointercancel', cancel);
  canvas.style.touchAction = 'none';
  function xrStart(event: XRInputSourceEvent) {
    if (grab) return;
    if (fromSource(event.inputSource, event.frame))
      begin(event.inputSource, event.frame);
  }
  function xrEnd(event: XRInputSourceEvent) {
    if (grab?.source !== event.inputSource) return;
    fromSource(event.inputSource, event.frame);
    end();
  }
  function xrSources(event: XRInputSourcesChangeEvent) {
    if (grab?.source && [...event.removed].includes(grab.source)) end(true);
  }
  function xrVisibility() {
    if (immersiveSession?.visibilityState !== 'visible') end(true);
  }
  function sessionEnd() {
    if (immersiveSession) {
      immersiveSession.removeEventListener('selectstart', xrStart);
      immersiveSession.removeEventListener('selectend', xrEnd);
      immersiveSession.removeEventListener('inputsourceschange', xrSources);
      immersiveSession.removeEventListener('visibilitychange', xrVisibility);
      immersiveSession.removeEventListener('end', sessionEnd);
    }
    immersiveSession = null;
    end(true);
    bridge.leave();
  }
  renderer.setAnimationLoop((_time, frame) => {
    if (disposed) return;
    if (renderer.xr.isPresenting && frame) {
      if (!centered && frame.getViewerPose(renderer.xr.getReferenceSpace()!)) {
        recenter();
        centered = true;
      }
      if (grab?.source) {
        if (fromSource(grab.source, frame)) move(frame);
      } else {
        let result: ReturnType<typeof hit>;
        for (const source of frame.session.inputSources) {
          if (
            source.targetRayMode === 'tracked-pointer' &&
            !source.hand &&
            fromSource(source, frame)
          ) {
            result = hit();
            if (result) break;
          }
        }
        feedback(result);
      }
    }
    const state = bridge.read();
    if (state.revision !== lastRevision) {
      lastRevision = state.revision;

      dirty = true;
    }
    const decisionDetail = panels.find(
      (p) => p.drawing.kind === 'decision-detail',
    )!;
    if (
      selectedDecision &&
      !decisionSnapshot(state).items.some(
        (item) => item.id === selectedDecision,
      )
    ) {
      selectedDecision = undefined;
      decisionDetail.mesh.visible = false;
      dirty = true;
    }
    decisionPage = decisionWindow(state, decisionPage).page;
    model.visible =
      state.view === 'model' &&
      !menu &&
      !panels[1].mesh.visible &&
      !panels[2].mesh.visible &&
      !decisionDetail.mesh.visible;
    model.position
      .copy(panels[0].mesh.position)
      .add(new THREE.Vector3(0, -0.02, 0.26));
    if (dirty) {
      const queue = decisionWindow(state, decisionPage);
      const detail = decisionDetail.mesh.visible
        ? decisionSnapshot(state).items.find(
            (item) => item.id === selectedDecision,
          )
        : undefined;
      canvas.setAttribute(
        'aria-label',
        describeSpatialScene(state) +
          `。待决策列表第 ${queue.page + 1}/${queue.pages} 页：${queue.items.map((item) => item.task).join('、')}。` +
          (detail
            ? `正在查看决策：${detail.task}，${detail.title}`
            : '决策详情已关闭。'),
      );
      panels.forEach((panel) => {
        drawPanel(panel.drawing, state, {
          hover: panel.hover,
          pinned: panel.transform.pinned,
          scale: panel.transform.scale,
          depth: panel.transform.depth,
          notice,
          environment,
          listening: bridge.listening(),
          focus,
          menu,
          voiceState: bridge.voiceState(),
          pressed: grab?.panel === panel ? grab.action : undefined,
          selectedNode,
          selectedDecision,
          decisionPage,
          canvasTool,
          draft: grab?.stroke,
        });
        panel.texture.needsUpdate = true;
        panel.glassTexture.needsUpdate = true;
      });
      dirty = false;
    }
    renderer.render(scene, camera);
  });
  return {
    refresh() {
      dirty = true;
    },
    recenter,
    async enter(session: XRSession) {
      immersiveSession = session;
      centered = false;
      session.addEventListener('selectstart', xrStart);
      session.addEventListener('selectend', xrEnd);
      session.addEventListener('inputsourceschange', xrSources);
      session.addEventListener('visibilitychange', xrVisibility);
      session.addEventListener('end', sessionEnd);
      await renderer.xr.setSession(session);
    },
    dispose() {
      disposed = true;
      resizeObserver.disconnect();
      renderer.setAnimationLoop(null);
      canvas.removeEventListener('pointerdown', down);
      canvas.removeEventListener('pointermove', pointerMove);
      canvas.removeEventListener('pointerup', up);
      canvas.removeEventListener('pointercancel', cancel);
      if (immersiveSession) {
        const session = immersiveSession;
        sessionEnd();
        void session.end().catch(() => {});
      }
      scene.traverse((node) => {
        if (node instanceof THREE.Mesh) {
          node.geometry.dispose();
          const materials = Array.isArray(node.material)
            ? node.material
            : [node.material];
          materials.forEach((m) => m.dispose());
        }
      });
      transparencyPreference.removeEventListener(
        'change',
        updateGlassPreference,
      );
      contrastPreference.removeEventListener('change', updateGlassPreference);
      panels.forEach((p) => {
        p.texture.dispose();
        p.glassTexture.dispose();
      });
      backgroundTexture.dispose();
      renderer.dispose();
      canvas.remove();
    },
  };
}
