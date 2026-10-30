import { useEffect, useState } from 'react';
import { t } from './i18n';

const sizes = [10, 20, 50, 100];
export type PagePosition = { limit: number; cursors: string[]; index: number };
const initial = (): PagePosition => ({ limit: 20, cursors: [''], index: 0 });
export const paginationKey = (cluster: string, person: string, page: string) => 'converyn.pagination.' + JSON.stringify([cluster, person, page]);
export function readPosition(key: string): PagePosition {
  try {
    const value = JSON.parse(sessionStorage.getItem(key) ?? 'null') as PagePosition | null;
    if (value && sizes.includes(value.limit) && Number.isInteger(value.index) && value.index >= 0 && Array.isArray(value.cursors) && value.cursors.length <= 10000 && value.index < value.cursors.length && value.cursors[0] === '' && value.cursors.every(cursor => typeof cursor === 'string' && cursor.length < 16384)) return value;
  } catch { /* A disabled store or an old cursor must not prevent reading the list. */ }
  return initial();
}
/** Keep the server's cursor boundaries instead of deriving offsets from live data. */
export function nextPosition(position: PagePosition, cursor: string | number | null | undefined): PagePosition {
  if (cursor == null || String(cursor) === position.cursors[position.index]) return position;
  return { ...position, cursors: [...position.cursors.slice(0, position.index + 1), String(cursor)], index: position.index + 1 };
}
export function usePagination(key: string) {
  const [position, setPosition] = useState(() => readPosition(key));
  useEffect(() => {
    try { sessionStorage.setItem(key, JSON.stringify(position)); } catch { /* Navigation remains usable without persistence. */ }
  }, [key, position]);
  // Two clicks queued before React renders must not skip a page or reuse a stale cursor.
  const change = (next: PagePosition) => setPosition(current => current === position ? next : current);
  return {
    ...position,
    cursor: position.cursors[position.index],
    first: () => change({ ...initial(), limit: position.limit }),
    previous: () => { if (position.index > 0) change({ ...position, index: position.index - 1 }); },
    next: (cursor: string | number | null | undefined) => change(nextPosition(position, cursor)),
    resize: (limit: number) => { if (sizes.includes(limit) && limit !== position.limit) change({ ...initial(), limit }); },
  };
}
export function listEndpoint(page: string, cursor: string, limit: number) {
  const params = new URLSearchParams({ limit: String(limit) });
  if (cursor) params.set('after', cursor);
  return '/' + page + '?' + params;
}
export function Pagination({ pager, count, next, busy, fresh, bottom = false, total }: {
  pager: ReturnType<typeof usePagination>;
  count?: number;
  next?: string | number | null;
  busy: boolean;
  fresh: boolean;
  bottom?: boolean;
  total?: number;
}) {
  if (count === 0 && pager.index === 0 && !busy && next == null) return null;
  const start = pager.index * pager.limit + 1;
  const canNext = count !== undefined && count > 0 && next != null && String(next) !== pager.cursor && fresh;
  return <nav className="pagination" aria-label={t(bottom ? '下方分页' : '上方分页')} aria-busy={busy}>
    <div className="pagination-summary"><strong>{t('第 {page} 页', { page: pager.index + 1 })}</strong><span>{count === undefined ? t('尚未取得本页数据') : count === 0 ? t('本页暂无记录') : t('第 {start}–{end} 条', { start, end: start + count - 1 })}</span>{total !== undefined ? <span>{t('共 {count} 条', { count: total })}</span> : count !== undefined && <span>{t('本页 {count} 条', { count })}</span>}</div>
    <div className="pagination-controls"><label>{t('每页')}<select aria-label={t('每页条数')} value={pager.limit} disabled={busy} onChange={e => pager.resize(Number(e.target.value))}>{sizes.map(size => <option key={size} value={size}>{size}</option>)}</select></label><div className="pagination-buttons"><button className="secondary" disabled={busy || pager.index === 0} onClick={pager.first}>{t('首页')}</button><button className="secondary" disabled={busy || pager.index === 0} onClick={pager.previous}>{t('上一页')}</button><button className="secondary" disabled={busy || !canNext} onClick={() => pager.next(next)}>{t('下一页')}</button></div></div>
  </nav>;
}
