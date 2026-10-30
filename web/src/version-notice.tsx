import { useEffect, useState } from 'react';
import { t } from './i18n';

/** Keep edits intact when a long-lived tab outlives its embedded UI build. */
export function VersionNotice() {
  const [changed, setChanged] = useState(false);
  useEffect(() => {
    const initial = document.querySelector<HTMLMetaElement>('meta[name="converyn-ui-version"]')?.content;
    if (!initial) return;
    const abort = new AbortController();
    let pending = false;
    const check = async () => {
      if (pending || document.hidden) return;
      pending = true;
      try {
        const response = await fetch('/assets/version.json', { cache: 'no-store', signal: abort.signal });
        if (response.ok && (await response.json()).version !== initial) setChanged(true);
      } catch { /* A transient outage does not discard the user's draft. */ }
      finally { pending = false; }
    };
    const timer = setInterval(() => void check(), 60000);
    document.addEventListener('visibilitychange', check);
    return () => { abort.abort(); clearInterval(timer); document.removeEventListener('visibilitychange', check); };
  }, []);
  return changed ? <div role="status" className="alert version-notice"><span>{t('界面已有更新，请保留本次编辑后刷新。')}</span><button onClick={() => location.reload()}>{t('刷新页面')}</button></div> : null;
}
