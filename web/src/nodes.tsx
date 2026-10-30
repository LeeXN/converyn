import { useCallback, useState } from 'react';
import { api, type Credential } from './api';
import { usePoll } from './use-poll';
import { Fields, Empty } from './ui';
import { observationStatus, type ObservationKind } from './node-observation';
import { nodeAddresses } from './node-addresses';
import { t, fieldLabel, formatDate } from './i18n';
import type { crates_core_orchestration_machine_types_NodeObservation as Node } from './api-types';
const show = (v: unknown): string => v == null ? t('未取得') : String(v);
const date = formatDate;
export function NodeAddresses({ addresses }: { addresses: unknown }) {
    const rows = nodeAddresses(addresses);
    if (!rows.length) return <span className="muted">{t('未取得')}</span>;
    return <div className="node-addresses"><code title={rows[0].interface}>{rows[0].address}</code>{rows.length > 1 && <details><summary>{t('另有 {count} 个 IP', { count: rows.length - 1 })}</summary><ul>{rows.slice(1).map(row => <li key={row.address}><code>{row.address}</code><small>{row.interface}</small></li>)}</ul></details>}</div>;
}
const fieldTitle = (key: string) => key === 'filesystems' ? t('宿主机存储挂载点') : fieldLabel(key);
function ObservationAge({ state, kind }: { state: NonNullable<ReturnType<typeof observationStatus>>; kind: ObservationKind }) {
    const keys = kind === 'inventory'
        ? { seconds: '{count} 秒前检查', minutes: '{count} 分钟前检查', hours: '{count} 小时前检查', days: '{count} 天前检查' }
        : { seconds: '{count} 秒前更新', minutes: '{count} 分钟前更新', hours: '{count} 小时前更新', days: '{count} 天前更新' };
    return <>{state.stale && <>{t(kind === 'inventory' ? '资产信息已过期' : '资源信息已过期')}{' · '}</>}{t(keys[state.unit], { count: state.count })}</>;
}
export function ObservationStatus({ node, kind }: { node: Node; kind: ObservationKind }) {
    const state = observationStatus(node, kind);
    if (!state) return <span className="muted">{t('尚未采集')}</span>;
    const omitted = state.omitted.map(key => key === 'filesystems' ? t('存储挂载点') : fieldLabel(key)).join(', ');
    return <span className="observation-status"><span className={'status-chip ' + (state.stale ? 'warning' : 'healthy')} title={date(state.checked)}><ObservationAge state={state} kind={kind}/></span>
      {state.truncated && <span className="status-chip warning" title={t('部分明细因本次上报大小限制被省略。')}>{omitted ? t('{fields}列表不完整', { fields: omitted }) : t('部分采集明细已省略')}</span>}
      {!!state.errors.length && <span className="status-chip warning">{t('部分字段采集失败')}</span>}</span>;
}
function OmittedNotice() { return <p className="notice warning">{t('部分条目因本次上报大小限制被省略，以下为已上报内容。')}</p>; }
function Devices({ title, rows, omitted = false, storage = false }: { title: string; rows: unknown; omitted?: boolean; storage?: boolean }) {
    if (Array.isArray(rows) && rows.some(row => row === null || typeof row !== 'object' || Array.isArray(row))) return <section><h3>{title}</h3>{omitted && <OmittedNotice/>}<Fields value={rows}/></section>;
    const devices = Array.isArray(rows) ? rows as Record<string, unknown>[] : [];
    const columns = Array.from(new Set(devices.flatMap(d => Object.keys(d).filter(k => k !== 'errors'))));
    return <section><h3>{title}</h3>{omitted && <OmittedNotice/>}{devices.length ? <div className="table-wrap"><table><thead><tr>{columns.map(k => <th key={k}>{fieldLabel(k)}</th>)}<th>{t("缺失原因")}</th></tr></thead><tbody>{devices.map((d, i) => <tr key={i}>{columns.map(k => <td key={k}><Fields value={d[k]} name={k}/></td>)}<td>{d.errors ? <Fields value={d.errors}/> : '—'}</td></tr>)}</tbody></table></div> : !omitted && <p>{Array.isArray(rows) ? t(storage ? '未发现宿主机存储挂载点' : '未发现设备') : t("尚未采集")}</p>}</section>;
}
export function ObservationDetails({ node, kind }: { node: Node; kind: ObservationKind }) {
    const s = node[kind], state = observationStatus(node, kind);
    return <section className="panel"><h2>{t(kind === 'inventory' ? '主机资产' : '资源概况')}</h2><ObservationStatus node={node} kind={kind}/><p>{t('采集时间')}{' · '}{date(s?.collected_ms)}{kind === 'inventory' ? t(' · 最近检查 ') + date(node.inventory_checked_ms) : ''}</p>
      {state?.truncated && !state.omitted.length && <p className="notice warning">{t('部分明细因本次上报大小限制被省略。')}</p>}
      {s && state && <><Fields value={Object.fromEntries(Object.entries(s.data).filter(([, v]) => !Array.isArray(v)))}/>
        {Object.entries(s.data).filter(([, v]) => Array.isArray(v)).map(([k, v]) => <Devices key={k} title={fieldTitle(k)} rows={v} omitted={state.omitted.includes(k)} storage={k === 'filesystems'}/>)}
        {state.omitted.filter(k => !Array.isArray(s.data[k])).map(k => <section key={k}><h3>{fieldTitle(k)}</h3><OmittedNotice/></section>)}
        {!!state.errors.length && <section><h3>{t('分项采集错误')}</h3><dl>{state.errors.map(([k, v]) => <div key={k}><dt>{fieldLabel(k)}</dt><dd>{v}</dd></div>)}</dl></section>}</>}
    </section>;
}
export function NodeTable({ nodes, open }: { nodes: Record<string, Node>; open?: (id: string) => void }) {
    if (!Object.keys(nodes).length) return <Empty/>;
    return <div className="table-wrap"><table><thead><tr><th>{t("节点")}</th><th>{t("心跳")}</th><th>IP</th><th>CPU</th><th>{t("内存")}</th><th>GPU</th><th>{t("系统")}</th><th>{t("资产状态")}</th><th>{t("资源状态")}</th></tr></thead><tbody>{Object.entries(nodes).map(([id, n]) => {
        const i = n.inventory?.data ?? {};
        return <tr key={id}><td>{open ? <button title={id} onClick={() => open(id)}>{id}</button> : <strong>{id}</strong>}<small>{show(i.hostname)}</small></td><td>{Date.now() - n.observed_ms < 60000 ? t("在线") : t("离线")}<small>{date(n.observed_ms)}</small></td><td><NodeAddresses addresses={i.addresses}/></td><td>{show(i.cpu_model)}<small>{t('物理核心')} {show(i.cpu_physical)} / {t('逻辑核心')} {show(i.cpu_logical)}</small></td><td>{typeof i.memory_total_bytes === 'number' ? (i.memory_total_bytes / 1024 ** 3).toFixed(1) + ' GiB' : t("未取得")}</td><td>{Array.isArray(i.gpus) ? i.gpus.length ? i.gpus.map(g => show((g as Record<string, unknown>).model)).join(', ') : t("无 GPU") : t("尚未采集")}</td><td><Fields value={i.os}/></td><td><ObservationStatus node={n} kind="inventory"/></td><td><ObservationStatus node={n} kind="resources"/></td></tr>;
    })}</tbody></table></div>;
}
export function NodeDetails({ credential, id, back }: { credential: Credential; id: string; back: () => void }) {
    const [revision, setRevision] = useState(0);
    const load = useCallback((signal: AbortSignal) => api<{ nodes: Record<string, Node> }>(credential, '/nodes/' + encodeURIComponent(id), undefined, signal), [credential, id]);
    const poll = usePoll(load, revision), node = poll.data?.nodes[id];
    const inventory = node?.inventory?.data ?? {};
    return <><a href="#/nodes" className="back-link" onClick={e => { e.preventDefault(); back(); }}>{t('返回节点列表')}</a><header className="page-header"><div><span className="eyebrow">{t('节点详情')}</span><h1>{id}</h1></div><button className="secondary" onClick={() => setRevision(v => v + 1)}>{t('刷新')}</button></header>{poll.error && <p role="alert">{poll.error}</p>}{!node ? <p>{t("读取节点…")}</p> : <>{node.diagnostic&&<p role="alert" className="alert">{node.diagnostic}</p>}<NodeTable nodes={{ [id]: node }}/>{(['inventory', 'resources'] as const).map(kind => <ObservationDetails key={kind} node={node} kind={kind}/>)}<section className="panel"><h2>{t("节点身份")}</h2><dl><dt>{t("硬件摘要")}</dt><dd>{node.hardware_sha256}</dd><dt>{t("启动编号")}</dt><dd>{node.boot_id}</dd><dt>{t("会话编号")}</dt><dd>{node.session_id}</dd><dt>{t("整机序列号")}</dt><dd>{show(inventory.serial)}</dd></dl></section></>}</>;
}
