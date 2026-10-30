import type { Credential } from './api';

const key = 'converyn.session';
/** A tab-scoped session survives refresh; identity and expiry are rechecked before mounting pages. */
export function readSession(): Credential | undefined {
  try {
    const value = JSON.parse(sessionStorage.getItem(key) ?? 'null');
    if (value && typeof value.cluster === 'string' && value.cluster.trim() && typeof value.token === 'string' && value.token.trim() && typeof value.expires === 'number' && value.expires > Date.now()) {
      return { cluster: value.cluster, token: value.token };
    }
  } catch { /* Corrupt or unavailable storage must not prevent a fresh login. */ }
  clearSession();
}
export function saveSession(credential: Credential, expires: number) {
  try { sessionStorage.setItem(key, JSON.stringify({ ...credential, expires })); }
  catch { /* The current in-memory login remains usable when storage is unavailable. */ }
}
export function clearSession() {
  try { sessionStorage.removeItem(key); } catch { /* Storage may be disabled by browser policy. */ }
}
