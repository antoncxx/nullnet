import { useEffect, useState } from 'react';
import Layout from '../components/Layout';
import { useApiText } from '../hooks/useApi';
import { useStack } from '../StackContext';

type Status =
  | { kind: 'idle' }
  | { kind: 'saving' }
  | { kind: 'ok' }
  | { kind: 'error'; msg: string };

export default function Config() {
  const { stack } = useStack();
  const { text, loading, error } = useApiText(`/api/config/${stack}`);

  const [content, setContent] = useState<string>('');
  const [original, setOriginal] = useState<string>('');
  const [status, setStatus] = useState<Status>({ kind: 'idle' });

  // Sync the editor when the loaded file (or stack) changes.
  useEffect(() => {
    if (text !== null) {
      setContent(text);
      setOriginal(text);
      setStatus({ kind: 'idle' });
    }
  }, [text]);

  const dirty = content !== original;

  async function save() {
    setStatus({ kind: 'saving' });
    try {
      const res = await fetch(`/api/config/${stack}`, {
        method: 'POST',
        headers: { 'Content-Type': 'text/plain' },
        body: content,
      });
      const data = await res.json().catch(() => ({ ok: res.ok, error: `HTTP ${res.status}` }));
      if (res.ok && data.ok) {
        setOriginal(content);
        setStatus({ kind: 'ok' });
      } else {
        setStatus({ kind: 'error', msg: data.error ?? `HTTP ${res.status}` });
      }
    } catch (e) {
      setStatus({ kind: 'error', msg: String(e) });
    }
  }

  return (
    <Layout page="config">
      <div className="content">
        <div className="page-title">Configuration</div>
        <div className="page-sub">
          Live service configuration for stack{' '}
          <span style={{ fontFamily: "'JetBrains Mono',monospace", color: 'var(--t1)' }}>{stack}</span>
          {' '}— edits are validated and applied without a restart.
        </div>

        {loading && <div style={{ color: 'var(--t2)', fontSize: 12 }}>Loading…</div>}

        {error && (
          <div style={{ color: 'var(--red)', fontSize: 12 }}>
            Failed to load config: {error}
          </div>
        )}

        {!loading && !error && (
          <>
            <textarea
              className="cfg-edit"
              value={content}
              spellCheck={false}
              onChange={e => {
                setContent(e.target.value);
                if (status.kind !== 'idle') setStatus({ kind: 'idle' });
              }}
            />
            <div className="cfg-actions">
              <button
                className="save-btn"
                onClick={save}
                disabled={!dirty || status.kind === 'saving'}
              >
                {status.kind === 'saving' ? 'Applying…' : 'Save & apply'}
              </button>

              {status.kind === 'ok' && (
                <span className="badge b-green">✓ Valid — applied live</span>
              )}
              {status.kind === 'error' && (
                <span className="cfg-err">
                  <span className="badge b-red">Parse error</span>
                  <span className="cfg-err-msg">{status.msg}</span>
                </span>
              )}
              {status.kind === 'idle' && dirty && (
                <span style={{ color: 'var(--t2)', fontSize: 11 }}>Unsaved changes</span>
              )}
            </div>
          </>
        )}
      </div>
    </Layout>
  );
}
