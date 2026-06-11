export const spRow = { marginBottom: 12 };
export const spKey = { fontSize: 10, color: 'var(--t2)', marginBottom: 3, letterSpacing: '.04em', fontWeight: 500 };
export const spVal = { fontSize: 12, color: 'var(--t0)' };
export const spCode = { fontSize: 11, color: 'var(--cyan)', fontFamily: "'JetBrains Mono',monospace" };
export function SpSep() {
  return <hr style={{ border: 'none', borderTop: '1px solid var(--t3)', margin: '12px 0' }} />;
}

export function SpSection({ children }: { children: React.ReactNode }) {
  return (
    <div style={{ fontSize: 10, fontWeight: 600, color: 'var(--t2)', letterSpacing: '.04em', marginBottom: 8 }}>
      {children}
    </div>
  );
}
