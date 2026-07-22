import { useEffect, useState } from 'react';
import Layout from '../components/Layout';
import { useApi, useApiText } from '../hooks/useApi';
import { useStack } from '../StackContext';

type Status =
  | { kind: 'idle' }
  | { kind: 'saving' }
  | { kind: 'ok' }
  | { kind: 'error'; msg: string };

const STARTER_TEMPLATE = `# New stack — define one [[services]] block per service.
[[services]]
name = "example"
docker_container = "example"
port = 8080
timeout = 60
`;

export default function Config() {
  const { stack, setStack } = useStack();
  const { text, loading, error, refetch } = useApiText(`/api/config/${stack}`);
  const { data: stacks, refetch: refetchStacks } = useApi<string[]>('/api/stacks', 10000);

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

  const notFound = !loading && !!error && error.includes('404');
  const dirty = content !== original;

  async function postConfig(body: string): Promise<{ ok: boolean; error?: string }> {
    try {
      const res = await fetch(`/api/config/${stack}`, {
        method: 'POST',
        headers: { 'Content-Type': 'text/plain' },
        body,
      });
      const data = await res.json().catch(() => ({ ok: res.ok, error: `HTTP ${res.status}` }));
      return { ok: res.ok && data.ok, error: data.error };
    } catch (e) {
      return { ok: false, error: String(e) };
    }
  }

  async function save() {
    setStatus({ kind: 'saving' });
    const r = await postConfig(content);
    if (r.ok) {
      setOriginal(content);
      setStatus({ kind: 'ok' });
    } else {
      setStatus({ kind: 'error', msg: r.error ?? 'unknown error' });
    }
  }

  async function createStack() {
    setStatus({ kind: 'saving' });
    const r = await postConfig(STARTER_TEMPLATE);
    if (r.ok) {
      setContent(STARTER_TEMPLATE);
      setOriginal(STARTER_TEMPLATE);
      setStatus({ kind: 'ok' });
      refetch();
      refetchStacks();
    } else {
      setStatus({ kind: 'error', msg: r.error ?? 'unknown error' });
    }
  }

  async function removeStack() {
    if (!confirm(`Delete stack "${stack}"? Its services are torn down immediately.`)) return;
    const res = await fetch(`/api/config/${stack}`, { method: 'DELETE' });
    if (res.ok) {
      const others = (stacks ?? []).filter(s => s !== stack);
      refetchStacks();
      setStack(others[0] ?? '');
    } else {
      const data = await res.json().catch(() => ({ error: `HTTP ${res.status}` }));
      setStatus({ kind: 'error', msg: data.error ?? `HTTP ${res.status}` });
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

        {notFound && (
          <div className="cfg-empty">
            <div style={{ color: 'var(--t2)', fontSize: 13 }}>
              Stack <span style={{ fontFamily: "'JetBrains Mono',monospace", color: 'var(--t1)' }}>{stack}</span> doesn't exist yet.
            </div>
            <button className="save-btn" onClick={createStack} disabled={status.kind === 'saving'}>
              {status.kind === 'saving' ? 'Creating…' : 'Create stack'}
            </button>
            {status.kind === 'error' && (
              <span className="cfg-err">
                <span className="badge b-red">Error</span>
                <span className="cfg-err-msg">{status.msg}</span>
              </span>
            )}
          </div>
        )}

        {error && !notFound && (
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
              <button className="teardown-btn" onClick={removeStack}>
                Delete stack
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
