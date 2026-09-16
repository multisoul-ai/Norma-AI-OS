'use client';
import { useEffect, useRef, useState } from 'react';
import { X, Glasses } from 'lucide-react';
import { Button } from '@/components/ui/button';
import type { SceneBridge, createSpatialScene } from '@/lib/xr-scene';
export function SpatialScene({
  bridge,
  session,
  enterVR,
  pending = false,
}: {
  bridge: SceneBridge;
  session: XRSession | null;
  enterVR: () => void;
  pending?: boolean;
}) {
  const host = useRef<HTMLDivElement>(null);
  const runtime = useRef<ReturnType<typeof createSpatialScene> | null>(null);
  const current = useRef(bridge);
  current.current = bridge;
  const [error, setError] = useState('');
  useEffect(() => {
    let cancelled = false;
    import('@/lib/xr-scene').then(async ({ createSpatialScene }) => {
      if (cancelled || !host.current) return;
      try {
        runtime.current = createSpatialScene(host.current, {
          read: () => current.current.read(),
          send: (t) => current.current.send(t),
          view: (v) => current.current.view(v),
          canvas: (doc) => current.current.canvas(doc),
          voice: () => current.current.voice(),
          listening: () => current.current.listening(),
          voiceState: () => current.current.voiceState(),
          toggleVoiceOutput: () => current.current.toggleVoiceOutput(),
          notice: (t) => current.current.notice(t),
          leave: () => current.current.leave(),
        });
        if (session) await runtime.current.enter(session);
      } catch (e) {
        setError(
          `空间渲染未能启动：${e instanceof Error ? e.message : '请使用支持 WebGL 的浏览器'}`,
        );
        if (session) void session.end().catch(() => {});
      }
    });
    return () => {
      cancelled = true;
      runtime.current?.dispose();
      runtime.current = null;
    };
  }, [session]);
  useEffect(() => {
    runtime.current?.refresh();
  }, [bridge]);
  return (
    <section
      className="spatial-scene"
      aria-label={session ? 'VR 头显空间' : '三维空间预览'}
    >
      <div ref={host} className="spatial-scene-canvas" />
      {error ? (
        <div className="scene-error">
          <p>{error}</p>
          <Button onClick={() => bridge.leave()}>返回工作台</Button>
        </div>
      ) : (
        <div className="scene-toolbar quiet-scene-toolbar">
          <Button
            variant="ghost"
            aria-label="返回工作台"
            onClick={() => (session ? void session.end() : bridge.leave())}
          >
            <X size={18} />
          </Button>
          <span>{session ? 'VR 空间' : 'VR 空间预览'}</span>
        </div>
      )}
      {!session && (
        <button className="vr-entry" onClick={enterVR} disabled={pending}>
          <Glasses size={20} />
          {pending ? '正在进入…' : '进入 VR'}
        </button>
      )}
      {!session && (
        <p className="vr-preview-note">
          浏览器预演 · 鼠标代替捏合，拖动空白环顾空间
        </p>
      )}
    </section>
  );
}
