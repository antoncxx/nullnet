/** ISO alpha-2 country code → flag emoji (regional-indicator letters). Empty
 *  string for anything that isn't two ASCII letters. */
export function flagEmoji(cc?: string): string {
  if (!cc || !/^[A-Za-z]{2}$/.test(cc)) return '';
  const base = 0x1f1e6;
  const u = cc.toUpperCase();
  return String.fromCodePoint(base + u.charCodeAt(0) - 65, base + u.charCodeAt(1) - 65);
}

const regionNames = new Intl.DisplayNames(['en'], { type: 'region' });

/** Full country name for a flag tooltip; falls back to the raw code. */
export function countryName(cc?: string): string {
  if (!cc) return '';
  try {
    return regionNames.of(cc.toUpperCase()) ?? cc.toUpperCase();
  } catch {
    return cc.toUpperCase();
  }
}
