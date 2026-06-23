import { useRef, useState, useCallback, useEffect, useLayoutEffect } from 'react';

const MIN_SCALE = 0.2;
const MAX_SCALE = 4;

interface ZoomState {
  scale: number;
  tx: number;
  ty: number;
}

interface Props {
  height: number;
  children: React.ReactNode;
}

export default function ZoomFrame({ height, children }: Props) {
  const [zoom, setZoom] = useState<ZoomState>({ scale: 1, tx: 0, ty: 0 });
  const dragging = useRef<{ startX: number; startY: number; startTx: number; startTy: number } | null>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const contentRef = useRef<HTMLDivElement>(null);
  const homeZoom = useRef<ZoomState>({ scale: 1, tx: 0, ty: 0 });

  // Compute centered initial position synchronously before first paint
  useLayoutEffect(() => {
    const container = containerRef.current;
    const content = contentRef.current;
    if (!container || !content) return;

    const cw = container.clientWidth;
    const ch = container.clientHeight;
    const contentH = content.clientHeight;
    if (!contentH) return;

    let scale = 1;
    let tx = 0;
    let ty = 0;

    if (contentH > ch) {
      scale = (ch / contentH) * 0.9;
      tx = (cw * (1 - scale)) / 2;
      ty = (ch - contentH * scale) / 2;
    } else {
      ty = (ch - contentH) / 2;
    }

    const home: ZoomState = { scale, tx, ty };
    homeZoom.current = home;
    setZoom(home);
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      setZoom(prev => {
        const factor = e.deltaY < 0 ? 1.1 : 0.9;
        const newScale = Math.min(Math.max(prev.scale * factor, MIN_SCALE), MAX_SCALE);
        const rect = el.getBoundingClientRect();
        const mx = e.clientX - rect.left;
        const my = e.clientY - rect.top;
        const newTx = mx - (mx - prev.tx) * (newScale / prev.scale);
        const newTy = my - (my - prev.ty) * (newScale / prev.scale);
        return { scale: newScale, tx: newTx, ty: newTy };
      });
    };
    el.addEventListener('wheel', onWheel, { passive: false });
    return () => el.removeEventListener('wheel', onWheel);
  }, []);

  const handleMouseDown = useCallback((e: React.MouseEvent) => {
    if (e.button !== 0) return;
    dragging.current = { startX: e.clientX, startY: e.clientY, startTx: zoom.tx, startTy: zoom.ty };
  }, [zoom.tx, zoom.ty]);

  const handleMouseMove = useCallback((e: React.MouseEvent) => {
    if (!dragging.current) return;
    const { startX, startY, startTx, startTy } = dragging.current;
    const dx = e.clientX - startX;
    const dy = e.clientY - startY;
    setZoom(prev => ({ ...prev, tx: startTx + dx, ty: startTy + dy }));
  }, []);

  const stopDrag = useCallback(() => { dragging.current = null; }, []);

  const resetZoom = useCallback(() => setZoom(homeZoom.current), []);

  const h = homeZoom.current;
  const isDefault =
    Math.abs(zoom.scale - h.scale) < 0.001 &&
    Math.abs(zoom.tx - h.tx) < 0.5 &&
    Math.abs(zoom.ty - h.ty) < 0.5;

  return (
    <div style={{ position: 'relative' }}>
      <div
        ref={containerRef}
        style={{
          height,
          overflow: 'hidden',
          position: 'relative',
          cursor: dragging.current ? 'grabbing' : 'grab',
          userSelect: 'none',
        }}
        onMouseDown={handleMouseDown}
        onMouseMove={handleMouseMove}
        onMouseUp={stopDrag}
        onMouseLeave={stopDrag}
      >
        <div
          ref={contentRef}
          style={{
            transform: `translate(${zoom.tx}px, ${zoom.ty}px) scale(${zoom.scale})`,
            transformOrigin: '0 0',
            width: '100%',
          }}
        >
          {children}
        </div>
      </div>
      {!isDefault && (
        <button
          onClick={resetZoom}
          style={{
            position: 'absolute',
            bottom: 10,
            right: 10,
            background: 'rgba(255,255,255,.08)',
            border: '1px solid rgba(255,255,255,.12)',
            color: 'rgba(255,255,255,.6)',
            fontSize: 10,
            padding: '3px 8px',
            borderRadius: 4,
            cursor: 'pointer',
          }}
        >
          reset view
        </button>
      )}
    </div>
  );
}
