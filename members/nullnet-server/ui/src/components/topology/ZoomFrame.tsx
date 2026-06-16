import { useRef, useState, useCallback } from 'react';

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

  const handleWheel = useCallback((e: React.WheelEvent) => {
    e.preventDefault();
    setZoom(prev => {
      const factor = e.deltaY < 0 ? 1.1 : 0.9;
      const newScale = Math.min(Math.max(prev.scale * factor, MIN_SCALE), MAX_SCALE);
      const rect = containerRef.current!.getBoundingClientRect();
      const mx = e.clientX - rect.left;
      const my = e.clientY - rect.top;
      const newTx = mx - (mx - prev.tx) * (newScale / prev.scale);
      const newTy = my - (my - prev.ty) * (newScale / prev.scale);
      return { scale: newScale, tx: newTx, ty: newTy };
    });
  }, []);

  const handleMouseDown = useCallback((e: React.MouseEvent) => {
    if (e.button !== 0) return;
    dragging.current = { startX: e.clientX, startY: e.clientY, startTx: zoom.tx, startTy: zoom.ty };
  }, [zoom.tx, zoom.ty]);

  const handleMouseMove = useCallback((e: React.MouseEvent) => {
    if (!dragging.current) return;
    const dx = e.clientX - dragging.current.startX;
    const dy = e.clientY - dragging.current.startY;
    setZoom(prev => ({ ...prev, tx: dragging.current!.startTx + dx, ty: dragging.current!.startTy + dy }));
  }, []);

  const stopDrag = useCallback(() => { dragging.current = null; }, []);

  const resetZoom = useCallback(() => setZoom({ scale: 1, tx: 0, ty: 0 }), []);

  const isDefault = zoom.scale === 1 && zoom.tx === 0 && zoom.ty === 0;

  return (
    <div style={{ position: 'relative' }}>
      <div
        ref={containerRef}
        style={{
          height,
          overflow: 'hidden',
          position: 'relative',
          cursor: dragging.current ? 'grabbing' : 'grab',
        }}
        onWheel={handleWheel}
        onMouseDown={handleMouseDown}
        onMouseMove={handleMouseMove}
        onMouseUp={stopDrag}
        onMouseLeave={stopDrag}
      >
        <div
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
