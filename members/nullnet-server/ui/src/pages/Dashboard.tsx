import Layout from '../components/Layout';
import { useApi } from '../hooks/useApi';
import { useStack } from '../StackContext';
import type { SessionJson, ServiceJson, NodeJson, PoolJson, GraphJson } from '../types';
import { buildTopoGraph, layoutNodes, svgDims, edgePath, inetEdgePath } from '../components/topology/layout';
import { NODE_W, NODE_H, INET_W, INET_H, INTERNET_ID } from '../components/topology/types';

export default function Dashboard() {
  const { stack } = useStack();
  const { data: sessions } = useApi<SessionJson[]>('/api/sessions', 5000);
  const { data: services } = useApi<ServiceJson[]>(`/api/services/${stack}`, 5000);
  const { data: nodes } = useApi<NodeJson[]>('/api/nodes', 5000);
  const { data: pool } = useApi<PoolJson>('/api/pool', 5000);
  const { data: graph } = useApi<GraphJson>(`/api/graph/${stack}`, 5000);

  const totalSvc = services?.length ?? 0;
  const onlineSvc = services?.filter(s => s.registered).length ?? 0;
  const sessionCount = sessions?.length ?? 0;
  const nodeCount = nodes?.length ?? 0;
  const poolPct = pool ? ((pool.in_use / pool.total) * 100).toFixed(1) : '—';
  const poolUsed = pool ? `${pool.in_use.toLocaleString()} of ${pool.total.toLocaleString()} IDs` : '—';

  return (
    <Layout
      page="dashboard"
      topbarRight={
        <span style={{ fontSize: 11, color: 'var(--t1)', display: 'flex', alignItems: 'center', gap: 5 }}>
          <span className="live-dot"></span>live · 5s
        </span>
      }
    >
      <div className="content">
        <div className="stats">
          <div className="stat glass">
            <div className="stat-label">Services Online</div>
            <div className="stat-value">
              {onlineSvc}<span className="denom">/{totalSvc}</span>
            </div>
            <div className="stat-sub">{totalSvc - onlineSvc} unregistered</div>
          </div>
          <div className="stat glass">
            <div className="stat-label">Active Sessions</div>
            <div className="stat-value" style={{ color: 'var(--cyan)' }}>{sessionCount}</div>
            <div className="stat-sub">isolated networks</div>
          </div>
          <div className="stat glass">
            <div className="stat-label">Connected Nodes</div>
            <div className="stat-value" style={{ color: 'var(--green)' }}>{nodeCount}</div>
            <div className="stat-sub">agent nodes</div>
          </div>
          <div className="stat glass">
            <div className="stat-label">Pool Used</div>
            <div className="stat-value">
              {pool ? <>{poolPct}<span className="denom">%</span></> : '—'}
            </div>
            <div className="stat-sub">{poolUsed}</div>
          </div>
        </div>

        <div className="bot-grid">
          <div className="card glass">
            <div className="card-head">
              <span className="card-label">Topology</span>
              <span style={{ fontSize: 10, color: 'var(--t2)', display: 'flex', alignItems: 'center', gap: 5 }}>
                <span className="live-dot" />live · 5s
              </span>
            </div>
            <div style={{ background: 'rgba(0,0,0,.25)', padding: 12 }}>
              {!graph && (
                <div style={{ color: 'var(--t2)', fontSize: 11, padding: '40px 0', textAlign: 'center' }}>loading topology…</div>
              )}
              {graph && (() => {
                const { nodes: topoNodes, edges: topoEdges } = buildTopoGraph(graph);
                const pos = layoutNodes(topoNodes, topoEdges);
                const { w, h } = svgDims(pos, topoNodes);
                return (
                  <svg viewBox={`0 0 ${w} ${h}`} xmlns="http://www.w3.org/2000/svg" style={{ width: '100%', display: 'block', fontFamily: "'Plus Jakarta Sans',sans-serif" }}>
                    <defs>
                      <filter id="db-gT"><feGaussianBlur stdDeviation="2.5" result="b"/><feMerge><feMergeNode in="b"/><feMergeNode in="SourceGraphic"/></feMerge></filter>
                      <marker id="db-arr" markerWidth="6" markerHeight="6" refX="5" refY="3" orient="auto">
                        <path d="M0,0 L0,6 L6,3 z" fill="rgba(255,255,255,.2)" />
                      </marker>
                      <marker id="db-arr-proxy" markerWidth="6" markerHeight="6" refX="5" refY="3" orient="auto">
                        <path d="M0,0 L0,6 L6,3 z" fill="rgba(251,191,36,.35)" />
                      </marker>
                      <marker id="db-arr-inet" markerWidth="6" markerHeight="6" refX="5" refY="3" orient="auto">
                        <path d="M0,0 L0,6 L6,3 z" fill="rgba(91,156,246,.35)" />
                      </marker>
                    </defs>

                    {/* Internet → proxy edges */}
                    {topoEdges.filter(e => e.isInternetEdge).map((e, i) => {
                      const fp = pos.get(e.from); const tp = pos.get(e.to);
                      if (!fp || !tp) return null;
                      return <path key={`ie-${i}`} d={inetEdgePath(fp, tp)} fill="none" stroke="rgba(91,156,246,.3)" strokeWidth="1" strokeDasharray="4 3" markerEnd="url(#db-arr-inet)" pointerEvents="none" />;
                    })}

                    {/* Service / proxy edges */}
                    {topoEdges.filter(e => !e.isInternetEdge).map((e, i) => {
                      const fp = pos.get(e.from); const tp = pos.get(e.to);
                      if (!fp || !tp) return null;
                      return <path key={i} d={edgePath(fp, tp)} fill="none"
                        stroke={e.isProxyHop ? 'rgba(251,191,36,.3)' : 'rgba(255,255,255,.15)'}
                        strokeWidth="1"
                        strokeDasharray={e.isProxyHop ? '4 3' : undefined}
                        markerEnd={e.isProxyHop ? 'url(#db-arr-proxy)' : 'url(#db-arr)'}
                        pointerEvents="none" />;
                    })}

                    {/* Nodes */}
                    {topoNodes.map(n => {
                      const p = pos.get(n.id);
                      if (!p) return null;

                      if (n.kind === 'internet') return (
                        <g key={INTERNET_ID}>
                          <rect x={p.x} y={p.y} width={INET_W} height={INET_H} rx="11"
                            fill="rgba(91,156,246,.05)" stroke="rgba(91,156,246,.2)"
                            strokeWidth="1" strokeDasharray="4 3" filter="url(#db-gT)" />
                          <text x={p.x + INET_W / 2} y={p.y + INET_H / 2 + 4}
                            textAnchor="middle" fill="rgba(91,156,246,.7)" fontSize="9.5" fontWeight="600" pointerEvents="none">
                            ⬡ internet
                          </text>
                        </g>
                      );

                      if (n.kind === 'proxy') return (
                        <g key={n.id}>
                          <rect x={p.x} y={p.y} width={NODE_W} height={NODE_H} rx="8"
                            fill="rgba(251,191,36,.06)" stroke="rgba(251,191,36,.4)"
                            strokeWidth="1" strokeDasharray="5 3" filter="url(#db-gT)" />
                          <circle cx={p.x + 15} cy={p.y + 19} r="3.5" fill="#fbbf24" />
                          <text x={p.x + 27} y={p.y + 16} fill="rgba(251,191,36,.9)" fontSize="9.5" fontWeight="500" pointerEvents="none">proxy</text>
                          <text x={p.x + 27} y={p.y + 28} fill="rgba(255,255,255,.4)" fontSize="8" fontFamily="'JetBrains Mono',monospace" pointerEvents="none">{n.id}</text>
                        </g>
                      );

                      const color = n.registered ? '#34d399' : '#f87171';
                      const strokeColor = n.registered ? 'rgba(52,211,153,.3)' : 'rgba(248,113,113,.2)';
                      return (
                        <g key={n.id}>
                          <rect x={p.x} y={p.y} width={NODE_W} height={NODE_H} rx="8"
                            fill="rgba(255,255,255,.04)" stroke={strokeColor} strokeWidth="1" filter="url(#db-gT)" />
                          <circle cx={p.x + 15} cy={p.y + 19} r="3.5" fill={color} />
                          <text x={p.x + 27} y={p.y + 16} fill="rgba(255,255,255,.85)" fontSize="9.5" fontWeight="500" pointerEvents="none">{n.id}</text>
                          <text x={p.x + 27} y={p.y + 28} fill="rgba(255,255,255,.3)" fontSize="8" pointerEvents="none">
                            {n.registered ? `${n.active_replica_count}/${n.replica_count} active` : 'unregistered'}
                            {n.entry_point ? ' · entry' : ''}
                          </text>
                        </g>
                      );
                    })}
                  </svg>
                );
              })()}
            </div>
          </div>

          <div className="card glass">
            <div className="card-head">
              <span className="card-label">Recent Sessions</span>
            </div>
            {sessions && sessions.length > 0 ? (
              sessions.slice(0, 6).map(s => (
                <div key={s.id} className="ev">
                  <div className="ev-accent" style={{ background: 'var(--green)' }}></div>
                  <span className="ev-time">{new Date(s.created_at * 1000).toLocaleTimeString()}</span>
                  <span className="ev-text">
                    <b>NET {s.id}</b> — {s.service} ← {s.client_ip}
                  </span>
                </div>
              ))
            ) : (
              <div style={{ padding: '20px 18px', color: 'var(--t2)', fontSize: 11 }}>
                {sessions === null ? 'Loading…' : 'No active sessions'}
              </div>
            )}
          </div>
        </div>

        <div className="card glass" style={{ marginTop: 12 }}>
          <div className="card-head">
            <span className="card-label">Services</span>
            <span style={{ fontSize: 10, color: 'var(--t2)' }}>{onlineSvc}/{totalSvc} online</span>
          </div>
          <table className="tbl">
            <thead>
              <tr>
                <th>Name</th>
                <th>Status</th>
                <th>Replicas</th>
                <th>Dependencies</th>
                <th>Timeout</th>
              </tr>
            </thead>
            <tbody>
              {services?.map(svc => (
                <tr key={svc.name}>
                  <td style={{ fontWeight: 500 }}>{svc.name}</td>
                  <td>
                    <span className={`badge ${svc.registered ? 'b-green' : 'b-dim'}`}>
                      {svc.registered ? 'Online' : 'Offline'}
                    </span>
                  </td>
                  <td style={{ fontFamily: "'JetBrains Mono',monospace", color: 'var(--t1)' }}>
                    {svc.replicas.length}
                  </td>
                  <td>
                    {svc.proxy_dependencies.flat().map((d, i) => (
                      <span key={`${d}-${i}`} className="dep-tag">{d}</span>
                    ))}
                  </td>
                  <td style={{ fontFamily: "'JetBrains Mono',monospace", color: 'var(--t2)', fontSize: 11 }}>
                    {svc.timeout_secs ? `${svc.timeout_secs}s` : '—'}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>
    </Layout>
  );
}
