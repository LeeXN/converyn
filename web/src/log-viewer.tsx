import { useEffect, useRef, useState } from 'react';
import { type Credential, headers, RequestError, requestError, responseError } from './api';
import { t } from './i18n';
import type { crates_core_model_receipt_Receipt as Receipt } from './api-types';
export type LogReceipt = Pick<Receipt, 'logs'>;
/** Reconnect at the last accepted byte offset. Receipt tails never advance that cursor. */
export function LogViewer({ credential, run, work, title, receipt }: {
  credential: Credential; run: string; work: string; title: string; receipt?: LogReceipt;
}) {
  const [stream, setStream] = useState<'stdout' | 'stderr'>('stdout'), [text, setText] = useState(''), [error, setError] = useState(''), [state, setState] = useState('连接中');
  const [fatal, setFatal] = useState(false), [tail, setTail] = useState(false), [suppressed, setSuppressed] = useState(false), [emptyComplete, setEmptyComplete] = useState(false);
  const [follow, setFollow] = useState(false), [retry, setRetry] = useState(0), [downloadError, setDownloadError] = useState('');
  const latest = useRef(receipt); latest.current = receipt;
  const output = useRef<HTMLPreElement>(null), downloads = useRef<AbortController | null>(null);
  const path = '/runs/' + encodeURIComponent(run) + '/logs/' + encodeURIComponent(work) + '/' + stream;
  useEffect(() => { if (follow && output.current) output.current.scrollTop = output.current.scrollHeight; }, [text, follow]);
  useEffect(() => {
    const abort = new AbortController();
    setText(''); setError(''); setDownloadError(''); setSuppressed(false); setEmptyComplete(false); setFatal(false); setTail(false); setState('连接中');
    let offset = 0, delay = 1000, terminal = false;
    const content = new TextDecoder();
    const wait = () => new Promise<void>(resolve => {
      const finish = () => { clearTimeout(timer); abort.signal.removeEventListener('abort', finish); resolve(); };
      const timer = setTimeout(finish, delay); abort.signal.addEventListener('abort', finish, { once: true });
    });
    void (async () => {
      while (!abort.signal.aborted && !terminal) {
        let reader: ReadableStreamDefaultReader<Uint8Array> | undefined;
        try {
          const response = await fetch('/api/v1' + path + '/events?offset=' + offset, { headers: headers(credential), signal: abort.signal, cache: 'no-store' });
          if (abort.signal.aborted) return;
          if (!response.ok) throw await responseError(response);
          reader = response.body?.getReader();
          if (!reader) throw requestError(422, { code: 'log_stream_unsupported', message: t('浏览器不支持日志流'), retryable: false });
          const decoder = new TextDecoder(); let pending = '';
          while (!abort.signal.aborted && !terminal) {
            const result = await reader.read();
            if (abort.signal.aborted) return;
            if (result.done) break;
            pending += decoder.decode(result.value, { stream: true }); pending = pending.replaceAll('\r\n', '\n');
            let end: number;
            while ((end = pending.indexOf('\n\n')) >= 0) {
              const event = pending.slice(0, end); pending = pending.slice(end + 2);
              const data = event.split('\n').filter(l => l.startsWith('data:')).map(l => l.slice(5).trimStart()).join('\n');
              if (!data) continue;
              if (event.includes('event: error')) { const v = JSON.parse(data); throw requestError(v.status, v); }
              const v = JSON.parse(data) as { data: string; offset: number; next_offset: number; complete: boolean; expired: boolean; truncated: boolean; condition?: string; source?: string };
              const bytes = Uint8Array.from(atob(v.data), c => c.charCodeAt(0));
              if (v.offset !== offset || v.next_offset !== offset + bytes.length) throw requestError(422, { code: 'log_cursor_mismatch', message: t('日志游标不一致'), retryable: false });
              const chunk = content.decode(bytes, { stream: !v.complete });
              const first = offset === 0;
              setText(t => (first ? chunk : t + chunk).slice(-2 * 1024 * 1024)); setTail(false);
              setEmptyComplete(v.complete && v.next_offset === 0 && !v.expired && !v.truncated);
              offset = v.next_offset; delay = 1000; setError(''); setFatal(false);
              terminal = v.complete;
              setState(v.expired ? '日志已到期' : v.condition === 'upload_incomplete' ? '上传缺失区间，无法完整读取' : v.complete ? (v.truncated ? '已保存日志完整，原输出有截断' : '已保存日志上传完成') : v.condition === 'upload_pending' ? '等待最终日志上传' : v.source === 'live_fragment' ? '实时片段（最终摘要尚未提交）' : '实时接收');
              if (terminal) break;
            }
          }
          if (!terminal) throw Error(t('连接中断，从确认字节位置重连'));
        } catch (e) {
          if (abort.signal.aborted) return;
          if (e instanceof RequestError && e.detail.code === 'logs_suppressed') { setText(''); setError(''); setSuppressed(true); return; }
          if (e instanceof RequestError && [401, 403].includes(e.status)) { setText(''); setTail(false); setError(t('日志读取权限已失效')); setFatal(true); setState('日志读取失败'); return; }
          const retryable = e instanceof RequestError ? e.detail.retryable : !(e instanceof SyntaxError);
          setError(e instanceof RequestError ? e.message + ' · ' + e.detail.message : String(e)); setFatal(!retryable);
          const fallback = latest.current?.logs?.[stream]?.tail;
          if (offset === 0 && fallback !== undefined) { setText(fallback); setTail(true); }
          setState(retryable ? '正在重连，保留已读取日志' : '日志读取失败');
          if (!retryable) return;
        } finally { if (reader) await reader.cancel().catch(() => {}); }
        if (!terminal) { await wait(); delay = Math.min(delay * 2, 10000); }
      }
    })();
    return () => { abort.abort(); downloads.current?.abort(); };
  }, [credential, path, stream, retry]);
  async function download(receiptTail = false) {
    downloads.current?.abort(); const abort = new AbortController(); downloads.current = abort; setDownloadError('');
    try {
      let blob: Blob;
      if (receiptTail) blob = new Blob([latest.current?.logs?.[stream]?.tail ?? ''], { type: 'text/plain;charset=utf-8' });
      else {
        const r = await fetch('/api/v1' + path + '/download', { headers: headers(credential), signal: abort.signal });
        if (!r.ok) throw await responseError(r);
        blob = await r.blob();
        const expected = r.headers.get('x-converyn-sha256');
        const digest = Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256', await blob.arrayBuffer())), b => b.toString(16).padStart(2, '0')).join('');
        if (!expected || digest !== expected) throw Error(t('日志摘要校验失败'));
      }
      if (abort.signal.aborted) return;
      const url = URL.createObjectURL(blob), a = document.createElement('a'); a.href = url; a.download = run + '-' + work + '-' + stream + (receiptTail ? '-receipt-tail' : '') + '.log'; a.click(); setTimeout(() => URL.revokeObjectURL(url), 1000);
    } catch (e) { if (!abort.signal.aborted) setDownloadError(String(e)); }
  }
  if (suppressed) return <section className="panel"><p role="status">{t('敏感输出生产步骤不提供日志')}</p></section>;
  return <section className="panel log-panel"><h3>{t('任务日志 · ')}{title}</h3><div className="log-controls"><select aria-label={t('日志流')} value={stream} onChange={e => setStream(e.target.value as 'stdout' | 'stderr')}><option value="stdout">{t('标准输出')}</option><option value="stderr">{t('标准错误')}</option></select><button className="secondary" onClick={() => void download()}>{t('下载完整已保存日志')}</button>{receipt?.logs?.[stream]?.tail !== undefined && <button className="secondary" onClick={() => void download(true)}>{t('下载回执尾部')}</button>}<label className="checkbox-label"><input type="checkbox" checked={follow} onChange={e => setFollow(e.target.checked)}/>{t('跟随最新输出')}</label></div>
    <p role="status" className={error ? 'notice warning' : 'muted'}>{t(state)}{t(' · 页面最多显示最近 2 MiB')}</p>
    {tail && <p className="notice warning" role="status">{t('集中存储不可用：原回执尾部，非完整输出')}</p>}
    {fatal && <p role="alert" className="alert">{error}</p>}{error && <details className="record-details"><summary>{t('日志诊断信息')}</summary><p>{error}</p></details>}
    {error && <button className="secondary" onClick={() => setRetry(v => v + 1)}>{t('重新连接')}</button>}{downloadError && <p role="alert" className="alert">{downloadError}</p>}
    <pre ref={output} tabIndex={0} aria-live="polite" className="log-output" onScroll={e => { const p = e.currentTarget; if (p.scrollHeight - p.scrollTop - p.clientHeight > 32) setFollow(false); }}>{text || t(emptyComplete ? '此日志流没有输出' : fatal ? '日志读取失败' : '等待日志输出')}</pre>
  </section>;
}
