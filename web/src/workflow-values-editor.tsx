import { useEffect, useId, useRef, useState } from 'react';
import { locked, parseValue, replace, rows } from './workflow-values';
import { t } from './i18n';

function ValueInput({ value, disabled, label, onChange }: { value: unknown; disabled?: boolean; label?: string; onChange: (value: unknown) => void }) {
  const kind = value === null ? 'null' : typeof value;
  const [draft, setDraft] = useState(String(value));
  useEffect(() => setDraft(String(value)), [value]);
  if (kind === 'boolean') return <select aria-label={label} disabled={disabled} value={String(value)} onChange={e => onChange(e.target.value === 'true')}><option value="true">true</option><option value="false">false</option></select>;
  if (kind === 'object') return <input aria-label={label} disabled value={JSON.stringify(value)}/>;
  if (kind === 'null') return <input aria-label={label} disabled={disabled} value="" placeholder="null" onChange={e => onChange(e.target.value)}/>;
  return <input aria-label={label} disabled={disabled} required={kind === 'number'} type={kind === 'number' ? 'number' : 'text'} step="any" value={draft} onChange={e => {
    setDraft(e.target.value);
    if (kind !== 'number' || (e.target.value.trim() !== '' && e.target.validity.valid)) onChange(kind === 'number' ? parseValue(e.target.value, kind) : e.target.value);
  }}/>;
}
export function WorkflowValuesEditor({ values, defaults, immutable, onChange }: {
  values: Record<string, unknown>; defaults: Record<string, unknown>; immutable: string[];
  onChange: (values: Record<string, unknown>, path?: string[]) => void;
}) {
  const [name, setName] = useState(''), [text, setText] = useState(''), [kind, setKind] = useState('string');
  const [error, setError] = useState<{ field: 'name' | 'value'; message: string }>();
  const nameInput = useRef<HTMLInputElement>(null), valueInput = useRef<HTMLInputElement>(null), id = useId();
  const entries = rows(values), defaultsByPath = new Set(rows(defaults).map(row => JSON.stringify(row.path)));
  function fail(field: 'name' | 'value', message: string) {
    setError({ field, message });
    (field === 'name' ? nameInput : valueInput).current?.focus();
  }
  function add() {
    const key = name.trim(), path = key.split('.');
    if (!key) return fail('name', t('请输入变量名'));
    if (!/^[A-Za-z_][A-Za-z0-9_]*(\.[A-Za-z0-9_-]+)*$/.test(key) || locked(path, immutable))
      return fail('name', t('变量名无效或不可修改'));
    if (entries.some(row => path.every((part, index) => row.path[index] === part)))
      return fail('name', t('变量已存在'));
    let value: unknown;
    try { value = parseValue(text, kind); }
    catch { return fail('value', t('请输入有效数字')); }
    try {
      onChange(replace(values, path, value), path);
      setName(''); setText(kind === 'boolean' ? 'false' : ''); setError(undefined);
      nameInput.current?.focus();
    } catch { fail('name', t('变量路径与现有变量冲突')); }
  }
  return <section className="workflow-values-editor">
    {entries.length > 0 && <div className="workflow-values-list">
      <div className="variable-list-heading"><strong>{t('流程变量')}</strong><span>{entries.length}</span></div>
      <div className="table-wrap"><table><thead><tr><th>{t('变量名')}</th><th>{t('本次值')}</th><th>{t('状态')}</th></tr></thead><tbody>
        {entries.map(row => {
          const fixed = locked(row.path, immutable), added = !defaultsByPath.has(JSON.stringify(row.path));
          return <tr key={JSON.stringify(row.path)}>
            <td className="variable-name"><code>{row.path.join('.')}</code><small>{row.value === null ? t('空值') : typeof row.value === 'number' ? t('数字') : typeof row.value === 'boolean' ? t('布尔值') : typeof row.value === 'string' ? t('文本') : 'JSON'}</small></td>
            <td className="variable-value"><ValueInput label={row.path.join('.')} value={row.value} disabled={fixed} onChange={value => onChange(replace(values, row.path, value), row.path)}/></td>
            <td className="variable-status"><span className={'variable-state ' + (fixed ? 'fixed' : added ? 'added' : '')} title={fixed ? t('固定值由流程定义，执行时不能修改。') : undefined}>{t(fixed ? '不可变' : added ? '本次新增' : '可变')}</span></td>
          </tr>;
        })}
      </tbody></table></div>
    </div>}
    <form className="variable-composer" aria-labelledby={id + '-title'} onSubmit={e => { e.preventDefault(); add(); }}>
      <div className="variable-composer-heading"><strong id={id + '-title'}>{t('新增本次变量')}</strong><small>{t('仅本次执行生效')}</small></div>
      <div className="variable-composer-grid">
        <label>{t('变量名')}<input ref={nameInput} name="variable-name" value={name} autoComplete="off" spellCheck={false} aria-describedby={id + '-hint' + (error?.field === 'name' ? ' ' + id + '-error' : '')} aria-invalid={error?.field === 'name' || undefined} onChange={e => { setName(e.target.value); setError(undefined); }} placeholder={t('例如 APP_VERSION')}/></label>
        <label>{t('类型')}<select name="variable-type" value={kind} onChange={e => { setKind(e.target.value); setText(e.target.value === 'boolean' ? 'false' : ''); setError(undefined); }}><option value="string">{t('文本')}</option><option value="number">{t('数字')}</option><option value="boolean">{t('布尔值')}</option><option value="null">{t('空值')}</option></select></label>
        <label className="variable-composer-value">{t('本次值')}{kind === 'boolean'
          ? <select name="variable-value" value={text || 'false'} onChange={e => setText(e.target.value)}><option value="false">false</option><option value="true">true</option></select>
          : <input ref={valueInput} name="variable-value" type={kind === 'number' ? 'number' : 'text'} step={kind === 'number' ? 'any' : undefined} value={text} disabled={kind === 'null'} placeholder={kind === 'null' ? t('无需填写') : undefined} aria-describedby={error?.field === 'value' ? id + '-error' : undefined} aria-invalid={error?.field === 'value' || undefined} onChange={e => { setText(e.target.value); setError(undefined); }}/>}</label>
        <button className="variable-add" type="submit" disabled={!name.trim()}>{t('新增变量')}</button>
      </div>
      <p className="variable-composer-help" id={id + '-hint'}>{t('支持 image.tag 这样的嵌套名称。')}{' '}{t('添加后才会用于本次执行。')}</p>
      {error && <p className="variable-error" id={id + '-error'} role="alert">{error.message}</p>}
    </form>
  </section>;
}

/** File bindings retain their server-side field allowlist and scalar rules. */
export function ConfigurationFields({ content, paths, onChange }: { content: string; paths: string[]; onChange: (content: string) => void }) {
  let document: unknown;
  try { document = JSON.parse(content); } catch { return <textarea aria-label={t('本次文件内容')} value={content} onChange={e => onChange(e.target.value)}/>; }
  return <div className="parameter-grid">{rows(document).filter(row => !paths.length || paths.includes('/' + row.path.map(p => p.replaceAll('~', '~0').replaceAll('/', '~1')).join('/'))).map(row =>
    <label key={JSON.stringify(row.path)}>{row.path.join('.')}<ValueInput value={row.value} onChange={value => onChange(JSON.stringify(replace(document, row.path, value), null, 2) + '\n')}/></label>)}</div>;
}
