import { useState } from 'react';
import type { SessionJson } from '../../types';
import { spRow, spKey, SpSep } from './panelStyles';

const PREVIEW_LIMIT = 5;

function groupBySubnet(sessions: SessionJson[]) {
  const map = new Map<string, SessionJson[]>();
  for (const s of sessions) {
    const prefix = s.client_ip.split('.').slice(0, 3).join('.');
    if (!map.has(prefix)) map.set(prefix, []);
    map.get(prefix)!.push(s);
  }
  return [...map.entries()]
    .map(([prefix, group]) => ({
      label: prefix + '.x',
      sessions: group.sort((a, b) => b.created_at - a.created_at),
    }))
    .sort((a, b) => b.sessions.length - a.sessions.length);
}

function formatTime(unix: number): string {
  return new Date(unix * 1000).toLocaleTimeString([], { hour12: false });
}

export default function InternetPanel({ sessions }: { sessions: SessionJson[] }) {
  const [expanded, setExpanded] = useState(new Set<string>());

  if (sessions.length === 0) {
    return (
      <div style={{ color: 'var(--t2)', fontSize: 11, textAlign: 'center', paddingTop: 24 }}>
        No active clients
      </div>
    );
  }

  const groups = groupBySubnet(sessions);

  return (
    <>
      <div style={spRow}>
        <div style={spKey}>Summary</div>
        <div style={{ fontSize: 12, color: 'var(--t0)', display: 'flex', gap: 8, alignItems: 'baseline' }}>
          <span>
            <span style={{ color: 'var(--cyan)', fontFamily: "'JetBrains Mono',monospace" }}>{sessions.length}</span>
            {' '}client{sessions.length !== 1 ? 's' : ''}
          </span>
          <span style={{ color: 'var(--t3)' }}>·</span>
          <span>
            <span style={{ color: 'var(--blue)', fontFamily: "'JetBrains Mono',monospace" }}>{groups.length}</span>
            {' '}subnet{groups.length !== 1 ? 's' : ''}
          </span>
        </div>
      </div>

      <SpSep />

      {groups.map(({ label, sessions: groupSessions }) => {
        const isExpanded = expanded.has(label);
        const shown = isExpanded ? groupSessions : groupSessions.slice(0, PREVIEW_LIMIT);
        const hasMore = groupSessions.length > PREVIEW_LIMIT;

        function toggleExpand() {
          setExpanded(prev => {
            const next = new Set(prev);
            if (isExpanded) next.delete(label); else next.add(label);
            return next;
          });
        }

        return (
          <div key={label} style={{ marginBottom: 14 }}>
            <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 5 }}>
              <span style={{ fontSize: 10, fontFamily: "'JetBrains Mono',monospace", color: 'var(--blue)', fontWeight: 600 }}>
                ⬡ {label}
              </span>
              <span style={{ fontSize: 9.5, color: 'var(--t2)', fontFamily: "'JetBrains Mono',monospace" }}>
                {groupSessions.length}
              </span>
            </div>

            {shown.map(s => (
              <div key={s.id} style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '4px 0', borderBottom: '1px solid rgba(255,255,255,.03)' }}>
                <div style={{ width: 6, height: 6, borderRadius: '50%', background: 'var(--blue)', flexShrink: 0 }} />
                <div style={{ flex: 1, minWidth: 0 }}>
                  <div style={{ fontSize: 11, fontFamily: "'JetBrains Mono',monospace", color: 'var(--t0)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' as const }}>
                    {s.client_ip}
                  </div>
                  <div style={{ fontSize: 9.5, color: 'var(--t2)' }}>{s.service}</div>
                </div>
                <div style={{ flexShrink: 0, textAlign: 'right' as const }}>
                  <div style={{ fontSize: 9.5, fontFamily: "'JetBrains Mono',monospace", color: 'var(--cyan)' }}>
                    net {s.network_id}
                  </div>
                  <div style={{ fontSize: 9, color: 'var(--t2)' }}>{formatTime(s.created_at)}</div>
                </div>
              </div>
            ))}

            {hasMore && (
              <button
                onClick={toggleExpand}
                style={{ display: 'block', width: '100%', padding: '5px 0', background: 'none', border: 'none', borderTop: '1px solid var(--t3)', color: 'var(--t2)', fontSize: 10, cursor: 'pointer', textAlign: 'left' as const, fontFamily: 'inherit' }}
              >
                {isExpanded ? 'Show less' : `+ ${groupSessions.length - PREVIEW_LIMIT} more`}
              </button>
            )}
          </div>
        );
      })}
    </>
  );
}
