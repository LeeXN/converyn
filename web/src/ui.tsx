import type { ReactNode } from 'react';
import { fieldLabel, formatDate, t, term } from './i18n';

export function Badge({ value }: { value: string }) {
  return <span className={'badge ' + value}><i />{term(value)}</span>;
}
export function Icon({ name = 'applications' }: { name?: string }) {
  const paths: Record<string, string> = {
    applications: 'M4 4h6v6H4zM14 4h6v6h-6zM4 14h6v6H4zM14 14h6v6h-6z',
    runs: 'm8 5 11 7-11 7z', nodes: 'M4 4h16v6H4zM4 14h16v6H4zM7 7h.01M7 17h.01',
    targets: 'M12 3v4m0 10v4M3 12h4m10 0h4M18 12a6 6 0 1 1-12 0 6 6 0 0 1 12 0',
    audit: 'M6 3h12v18H6zM9 8h6M9 12h6M9 16h4', controlplane: 'M12 3v6M4 21v-6h16v6M12 15V9M8 3h8v6H8z',
    refresh: 'M20 8a8 8 0 1 0 0 8M20 3v5h-5', close: 'm6 6 12 12M6 18 18 6',
    menu: 'M4 6h16M4 12h16M4 18h16', logout: 'M9 4H4v16h5m6-13 5 5-5 5M8 12h12',
  };
  return <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.65" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d={paths[name] ?? paths.applications}/></svg>;
}
export function Empty({ title, children }: { title?: string; children?: ReactNode }) {
  return <div className="empty"><Icon/><h3>{title ?? t('暂无记录')}</h3><p>{children ?? t('当前范围内没有可显示的数据。')}</p></div>;
}
const enumFields = new Set(['action', 'type', 'code', 'status', 'condition', 'outcome', 'phase', 'kind', 'role', 'scope', 'mode']);
/** Keep identifiers and user configuration verbatim; localize only schema labels and enum values. */
export function Fields({ value, name = '', literal = false }: { value: unknown; name?: string; literal?: boolean }) {
  if (value == null) return <span className="muted">{t('未提供')}</span>;
  if (!literal && typeof value === 'number' && name.endsWith('_bytes')) return <span>{new Intl.NumberFormat(undefined, { maximumFractionDigits: 1 }).format(value / (1024 ** (value >= 1024 ** 3 ? 3 : value >= 1024 ** 2 ? 2 : value >= 1024 ? 1 : 0)))} {value >= 1024 ** 3 ? 'GiB' : value >= 1024 ** 2 ? 'MiB' : value >= 1024 ? 'KiB' : 'B'}</span>;
  if (!literal && typeof value === 'number' && ['cpu_used_basis_points', 'utilization_percent', 'temperature_millicelsius'].includes(name)) return <span>{(value / (name === 'cpu_used_basis_points' ? 100 : name === 'temperature_millicelsius' ? 1000 : 1)).toFixed(1)}{name === 'temperature_millicelsius' ? ' °C' : '%'}</span>;
  if (typeof value !== 'object') return <span className="field-value">{
    !literal && name.endsWith('_ms') && typeof value === 'number' ? formatDate(value) :
    typeof value === 'boolean' || (!literal && typeof value === 'string' && enumFields.has(name)) ? term(value, true) : String(value)
  }</span>;
  if (Array.isArray(value)) return value.length ? <ul className="field-list">{value.map((item, i) => <li key={i}><Fields value={item} name={name} literal={literal}/></li>)}</ul> : <span className="muted">{t('未配置')}</span>;
  const entries = Object.entries(value);
  return entries.length ? <dl className="fields">{entries.map(([key, v]) => <div key={key} className={v !== null && typeof v === 'object' ? 'field-group' : undefined}><dt>{literal ? key : fieldLabel(key)}</dt><dd><Fields value={v} name={key} literal={literal || ['parameters', 'fixed', 'variables', 'labels', 'provenance'].includes(key)}/></dd></div>)}</dl> : <span className="muted">{t('未配置')}</span>;
}
export function Details({ value, label }: { value: unknown; label?: string }) {
  return <details className="record-details"><summary>{label ?? t('记录详情')}</summary><Fields value={value}/></details>;
}
