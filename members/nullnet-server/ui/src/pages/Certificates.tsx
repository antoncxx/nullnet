import { useState } from 'react';
import Layout from '../components/Layout';
import { useApi } from '../hooks/useApi';
import type { CertJson } from '../types';

function formatExpiry(unix: number | null): { text: string; color: string } {
  if (unix === null) return { text: 'unknown', color: 'var(--t2)' };
  const ms = unix * 1000;
  const days = Math.floor((ms - Date.now()) / 86_400_000);
  const date = new Date(ms).toLocaleDateString();
  if (days < 0) return { text: `expired (${date})`, color: 'var(--red, #e5484d)' };
  if (days < 30) return { text: `${days}d · ${date}`, color: 'var(--amber)' };
  return { text: `${days}d · ${date}`, color: 'var(--t1)' };
}

export default function Certificates() {
  const { data: certs, loading, refetch } = useApi<CertJson[]>('/api/certificates', 10000);
  const [deleting, setDeleting] = useState<Set<string>>(new Set());

  const [domain, setDomain] = useState('');
  const [fullchain, setFullchain] = useState('');
  const [key, setKey] = useState('');
  const [submitting, setSubmitting] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);

  const list = certs ?? [];
  const existing = new Set(list.map(c => c.domain));
  const isRenew = existing.has(domain.trim());

  async function readInto(file: File | undefined, set: (v: string) => void) {
    if (file) set(await file.text());
  }

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    setFormError(null);
    setSubmitting(true);
    try {
      const res = await fetch('/api/certificates', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ domain: domain.trim(), fullchain_pem: fullchain, key_pem: key }),
      });
      if (!res.ok) {
        const body = await res.json().catch(() => ({ error: `HTTP ${res.status}` }));
        throw new Error(body.error ?? `HTTP ${res.status}`);
      }
      setDomain(''); setFullchain(''); setKey('');
      refetch();
    } catch (err) {
      setFormError(String(err instanceof Error ? err.message : err));
    } finally {
      setSubmitting(false);
    }
  }

  async function remove(d: string) {
    if (!confirm(`Delete certificate for ${d}? Clearing the last cert needs a proxy restart to fully take effect.`)) return;
    setDeleting(prev => new Set(prev).add(d));
    try {
      await fetch(`/api/certificates/${encodeURIComponent(d)}`, { method: 'DELETE' });
      refetch();
    } finally {
      setDeleting(prev => { const next = new Set(prev); next.delete(d); return next; });
    }
  }

  const canSubmit = domain.trim() !== '' && fullchain.includes('BEGIN CERTIFICATE') && key.includes('PRIVATE KEY') && !submitting;

  return (
    <Layout page="certificates">
      <div className="content">
        <div className="hero-row">
          <span className="hero-num">{list.length}</span>
          <span className="hero-label">installed TLS certificates</span>
        </div>

        <div className="card">
          <div className="card-head">
            <span className="card-label">Certificates</span>
            <span style={{ fontSize: 11, color: 'var(--t2)' }}>private keys encrypted at rest</span>
          </div>
          <table className="tbl">
            <thead>
              <tr>
                <th>Domain</th>
                <th>Expires</th>
                <th></th>
              </tr>
            </thead>
            <tbody>
              {loading && (
                <tr><td colSpan={3} style={{ color: 'var(--t2)', padding: '20px 16px' }}>Loading…</td></tr>
              )}
              {list.map(c => {
                const exp = formatExpiry(c.expires_at);
                return (
                  <tr key={c.domain}>
                    <td style={{ fontWeight: 500, fontFamily: "'JetBrains Mono',monospace" }}>{c.domain}</td>
                    <td style={{ fontFamily: "'JetBrains Mono',monospace", fontSize: 11, color: exp.color }}>{exp.text}</td>
                    <td>
                      <button className="teardown-btn" onClick={() => remove(c.domain)} disabled={deleting.has(c.domain)}>
                        {deleting.has(c.domain) ? '…' : 'Delete'}
                      </button>
                    </td>
                  </tr>
                );
              })}
              {!loading && list.length === 0 && (
                <tr><td colSpan={3} style={{ color: 'var(--t2)', padding: '20px 16px' }}>No certificates installed</td></tr>
              )}
            </tbody>
          </table>
        </div>

        <div className="card" style={{ marginTop: 16 }}>
          <div className="card-head">
            <span className="card-label">{isRenew ? 'Renew / replace certificate' : 'Add certificate'}</span>
            {isRenew && <span style={{ fontSize: 11, color: 'var(--amber)' }}>overwrites existing «{domain.trim()}»</span>}
          </div>
          <form onSubmit={submit} style={{ padding: 16, display: 'flex', flexDirection: 'column', gap: 12 }}>
            <label style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
              <span style={{ fontSize: 12, color: 'var(--t2)' }}>Domain (exact or *.wildcard)</span>
              <input
                value={domain}
                onChange={e => setDomain(e.target.value)}
                placeholder="app.example.com"
                spellCheck={false}
                style={{ fontFamily: "'JetBrains Mono',monospace", padding: '6px 8px' }}
              />
            </label>

            <label style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
              <span style={{ fontSize: 12, color: 'var(--t2)' }}>
                Fullchain PEM (leaf + intermediates) ·{' '}
                <input type="file" accept=".pem,.crt,.cer" onChange={e => readInto(e.target.files?.[0], setFullchain)} style={{ fontSize: 11 }} />
              </span>
              <textarea
                value={fullchain}
                onChange={e => setFullchain(e.target.value)}
                placeholder="-----BEGIN CERTIFICATE-----"
                spellCheck={false}
                rows={5}
                style={{ fontFamily: "'JetBrains Mono',monospace", fontSize: 11, padding: '6px 8px', resize: 'vertical' }}
              />
            </label>

            <label style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
              <span style={{ fontSize: 12, color: 'var(--t2)' }}>
                Private key PEM ·{' '}
                <input type="file" accept=".pem,.key" onChange={e => readInto(e.target.files?.[0], setKey)} style={{ fontSize: 11 }} />
              </span>
              <textarea
                value={key}
                onChange={e => setKey(e.target.value)}
                placeholder="-----BEGIN PRIVATE KEY-----"
                spellCheck={false}
                rows={5}
                style={{ fontFamily: "'JetBrains Mono',monospace", fontSize: 11, padding: '6px 8px', resize: 'vertical' }}
              />
            </label>

            {formError && <span style={{ color: 'var(--red, #e5484d)', fontSize: 12 }}>{formError}</span>}

            <div>
              <button type="submit" className="teardown-btn" disabled={!canSubmit}>
                {submitting ? 'Saving…' : isRenew ? 'Replace certificate' : 'Add certificate'}
              </button>
            </div>
          </form>
        </div>
      </div>
    </Layout>
  );
}
