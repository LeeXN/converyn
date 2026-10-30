import { useEffect, useId, useRef, useState, type ReactNode } from 'react';
import { createPortal } from 'react-dom';
import { t } from './i18n';

/** A portal keeps tables from constraining readers; closing restores the originating row. */
export function Dialog({ title, children, onClose, wide = false }: { title: string; children: ReactNode; onClose: () => void; wide?: boolean }) {
  const ref = useRef<HTMLDialogElement>(null), close = useRef(onClose), label = useId();
  close.current = onClose;
  const [full, setFull] = useState(false);
  useEffect(() => {
    const dialog = ref.current!, origin = document.activeElement as HTMLElement | null;
    const x = window.scrollX, y = window.scrollY, overflow = document.body.style.overflow;
    document.body.style.overflow = 'hidden';
    dialog.showModal();
    window.scrollTo(x, y);
    return () => {
      dialog.close(); document.body.style.overflow = overflow;
      if (origin?.isConnected) origin.focus({ preventScroll: true });
      window.scrollTo(x, y);
    };
  }, []);
  return createPortal(<dialog ref={ref} className={'console-dialog' + (wide ? ' wide' : '') + (full ? ' fullscreen' : '')}
    aria-labelledby={label} onCancel={e => { e.preventDefault(); close.current(); }}
    onClick={e => { if (e.target === e.currentTarget) close.current(); }}>
    <header className="dialog-heading"><h2 id={label}>{title}</h2><div className="button-row">
      {wide && <button type="button" className="secondary" onClick={() => setFull(v => !v)}>{t(full ? '退出全屏' : '全屏阅读')}</button>}
      <button type="button" className="secondary dialog-close" onClick={() => close.current()} aria-label={t('关闭')}>{t('关闭')}</button>
    </div></header><div className="dialog-body">{children}</div>
  </dialog>, document.body);
}

export function CodeBlock({ content }: { content: string }) {
  const [wrap, setWrap] = useState(false), [copied, setCopied] = useState('');
  return <div className="code-document"><div className="code-toolbar"><label className="checkbox-label"><input type="checkbox" checked={wrap} onChange={e => setWrap(e.target.checked)}/>{t('自动换行')}</label>
    <button className="secondary" type="button" onClick={async () => { try { await navigator.clipboard.writeText(content); setCopied('已复制'); } catch { setCopied('复制失败，请手动复制'); } }}>{t('复制内容')}</button><span role="status">{copied && t(copied)}</span></div>
    <pre tabIndex={0} className={wrap ? 'code-content wrap' : 'code-content'}>{content}</pre></div>;
}

export function CodeViewer({ content, label }: { content: string; label: string }) {
  const [open, setOpen] = useState(false);
  return <><button type="button" className="secondary" onClick={() => setOpen(true)}>{label}</button>{open && <Dialog wide title={label} onClose={() => setOpen(false)}><CodeBlock content={content}/></Dialog>}</>;
}
