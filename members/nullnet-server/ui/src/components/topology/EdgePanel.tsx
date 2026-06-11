import type { GraphEdgeJson } from '../../types';
import { spRow, spKey, spCode } from './panelStyles';

interface Props {
  edge: GraphEdgeJson;
}

export default function EdgePanel({ edge }: Props) {
  return (
    <>
      <div style={spRow}>
        <div style={spKey}>Type</div>
        <span className={`badge ${edge.via_proxy ? 'b-amber' : 'b-blue'}`}>
          {edge.via_proxy ? 'Proxied' : 'Direct'}
        </span>
      </div>
      <div style={spRow}>
        <div style={spKey}>From</div>
        <div style={spCode}>{edge.from}</div>
      </div>
      <div style={spRow}>
        <div style={spKey}>To</div>
        <div style={spCode}>{edge.to}</div>
      </div>
      {edge.via_proxy && (
        <div style={spRow}>
          <div style={spKey}>Via Proxy</div>
          <div style={{ ...spCode, color: '#fbbf24' }}>{edge.via_proxy}</div>
        </div>
      )}
      <div style={spRow}>
        <div style={spKey}>Net ID</div>
        <div style={spCode}>{edge.net_id}</div>
      </div>
      {edge.setup_ms > 0 && (
        <div style={spRow}>
          <div style={spKey}>Setup Time</div>
          <div style={spCode}>{edge.setup_ms}ms</div>
        </div>
      )}
    </>
  );
}
