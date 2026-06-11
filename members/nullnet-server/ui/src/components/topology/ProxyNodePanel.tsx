import type { GraphEdgeJson } from '../../types';
import { spRow, spKey, spVal, spCode } from './panelStyles';

interface Props {
  ip: string;
  edges: GraphEdgeJson[];
}

export default function ProxyNodePanel({ ip, edges }: Props) {
  const proxyEdges = edges.filter(e => e.via_proxy === ip);
  const targets = [...new Set(proxyEdges.map(e => e.to))];

  return (
    <>
      <div style={spRow}>
        <div style={spKey}>Type</div>
        <span className="badge b-amber">Proxy entry</span>
      </div>
      <div style={spRow}>
        <div style={spKey}>IP Address</div>
        <div style={spCode}>{ip}</div>
      </div>
      <div style={spRow}>
        <div style={spKey}>Active Tunnels</div>
        <div style={{ ...spVal, color: 'var(--cyan)' }}>{proxyEdges.length}</div>
      </div>
      {targets.length > 0 && (
        <div style={spRow}>
          <div style={spKey}>Routing to</div>
          <div>
            {targets.map(t => {
              const e = proxyEdges.find(e2 => e2.to === t)!;
              return (
                <div key={t} style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '5px 0', borderBottom: '1px solid var(--t3)' }}>
                  <span style={{ fontSize: 11, flex: 1, color: 'var(--t0)' }}>{t}</span>
                  <span className="badge b-blue" style={{ fontSize: '8.5px' }}>net {e.net_id}</span>
                </div>
              );
            })}
          </div>
        </div>
      )}
    </>
  );
}
