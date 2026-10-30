import { useCallback, useState } from 'react';
import { api, type Credential } from './api';
import { usePoll } from './use-poll';
import { t } from './i18n';
import { Empty } from './ui';
import { Dialog } from './dialog';
import { NodeAddresses } from './nodes';
import type { crates_core_orchestration_machine_types_NodeObservation as Node } from './api-types';
type NodeGroup = { uid: string; name: string; revision: number; node_uids: string[] };
type RegisteredNode = { uid: string; retired: boolean; fingerprint: string };
async function all<T>(credential: Credential, path: string, key: string, signal: AbortSignal): Promise<Record<string, T>> {
  let after = ''; const result: Record<string, T> = {};
  do {
    const page = await api<Record<string, unknown>>(credential, path + '?limit=100' + (after ? '&after=' + encodeURIComponent(after) : ''), undefined, signal);
    const rows = page[key];
    if (Array.isArray(rows)) for (const row of rows as (T & { uid: string })[]) result[row.uid] = row;
    else Object.assign(result, rows);
    after = typeof page.next_cursor === 'string' ? page.next_cursor : '';
  } while (after);
  return result;
}
export function Infrastructure({ credential }: { credential: Credential }) {
  const [revision, setRevision] = useState(0), [busy, setBusy] = useState(false), [error, setError] = useState(''), [success, setSuccess] = useState('');
  const [tab, setTab] = useState('groups'), [search, setSearch] = useState('');
  const [editing, setEditing] = useState<NodeGroup>(), [action, setAction] = useState<{ group?: NodeGroup; node?: RegisteredNode }>();
  const [dirty, setDirty] = useState(false), [discard, setDiscard] = useState(false), [members, setMembers] = useState<string[]>([]);
  const load = useCallback(async (signal: AbortSignal) => {
    const [groups, nodes, observations] = await Promise.all([
      all<NodeGroup>(credential, '/node-groups', 'node_groups', signal),
      all<RegisteredNode>(credential, '/identity/nodes', 'nodes', signal),
      all<Node>(credential, '/nodes', 'nodes', signal).then(nodes => ({ nodes, error: '' })).catch(e => ({ nodes: {} as Record<string, Node>, error: String(e) })),
    ]);
    return { groups, nodes, observations };
  }, [credential]);
  const poll = usePoll(load, revision), data = poll.data;
  function edit(group: NodeGroup) { setEditing(group); setMembers([...group.node_uids]); setSearch(''); setError(''); setDirty(false); setDiscard(false); }
  function close() { if (busy) return; if (editing && dirty) { setDiscard(true); return; } setEditing(undefined); setAction(undefined); setError(''); setDiscard(false); }
  async function write(path: string, body: unknown) {
    if (busy || !poll.fresh) return;
    setBusy(true); setError(''); setSuccess('');
    try { await api(credential, path, body); setRevision(v => v + 1); setEditing(undefined); setAction(undefined); setDirty(false); setSuccess('操作已保存'); }
    catch (e) { setError(String(e)); }
    finally { setBusy(false); }
  }
  function identity(uid: string) {
    const inventory = data?.observations.nodes[uid]?.inventory?.data;
    return <div className="node-identity"><strong>{uid}</strong>{typeof inventory?.hostname === 'string' && <small>{inventory.hostname}</small>}<NodeAddresses addresses={inventory?.addresses}/></div>;
  }
  const candidates = data ? [...new Set([...Object.values(data.nodes).filter(n => !n.retired).map(n => n.uid), ...members])].sort() : [];
  const visible = candidates.filter(uid => {
    const inventory = data?.observations.nodes[uid]?.inventory?.data;
    return [uid, inventory?.hostname, JSON.stringify(inventory?.addresses)].join(' ').toLowerCase().includes(search.toLowerCase());
  });
  return <><header className="page-header"><div><span className="eyebrow">{t('基础设施')}</span><h1>{t('节点组管理')}</h1></div><div className="button-row"><button className="secondary" onClick={() => setRevision(v => v + 1)}>{t('刷新')}</button><button disabled={!poll.fresh} onClick={() => edit({ uid: '', name: '', revision: 0, node_uids: [] })}>{t('创建节点组')}</button></div></header>
    <p className="page-intro">{t('平台节点组限制流程允许使用的节点范围。流程 inventory 决定部署节点、业务分组和顺序；节点可以加入多个组，组之间不承诺物理隔离。')}</p>
    {poll.error && <p role="alert" className="alert">{poll.error}</p>}{success && <p role="status" className="success-notice">{t(success)}</p>}
    {data && <><div className="summary-strip"><div><span>{t('节点组')}</span><strong>{Object.keys(data.groups).length}</strong></div><div><span>{t('已登记节点')}</span><strong>{Object.keys(data.nodes).length}</strong></div><div><span>{t('可用节点')}</span><strong>{Object.values(data.nodes).filter(n => !n.retired).length}</strong></div></div>
      <div className="tabs" role="tablist" aria-label={t('节点组管理')}><button role="tab" aria-selected={tab === 'groups'} onClick={() => setTab('groups')}>{t('节点组')}</button><button role="tab" aria-selected={tab === 'nodes'} onClick={() => setTab('nodes')}>{t('节点登记')}</button></div>
      {data.observations.error && <p className="notice warning" role="status">{t('节点观测暂不可用，仍可管理已登记节点。')} {data.observations.error}</p>}
      {tab === 'groups' ? <div className="group-grid">{Object.values(data.groups).map(group => <section className="panel group-card" key={group.uid}><header><div><h2>{group.name}</h2><small>{group.uid} · {t('修订')} {group.revision}</small></div><span className="status-chip">{t('{count} 个成员', { count: group.node_uids.length })}</span></header>
        <div className="group-members">{group.node_uids.map(uid => <div key={uid}>{identity(uid)}</div>)}{!group.node_uids.length && <p className="muted">{t('尚未添加成员')}</p>}</div>
        <footer className="button-row"><button className="secondary" disabled={busy || !poll.fresh} onClick={() => edit(group)}>{t('编辑')}</button><button className="danger secondary" disabled={busy || !poll.fresh} onClick={() => { setError(''); setAction({ group }); }}>{t('删除')}</button></footer>
      </section>)}{!Object.keys(data.groups).length && <Empty title={t('尚未创建节点组')}/>}</div> : <section className="panel"><h2>{t('节点登记与成员关系')}</h2><p className="muted">{t('退役会撤销节点接入资格并保留历史，不用于临时下线。')}</p><div className="table-wrap"><table><thead><tr><th>{t('节点')}</th><th>{t('所属节点组')}</th><th>{t('状态')}</th><th>{t('操作')}</th></tr></thead><tbody>{Object.values(data.nodes).map(node => <tr key={node.uid}><td>{identity(node.uid)}</td><td>{Object.values(data.groups).filter(g => g.node_uids.includes(node.uid)).map(g => g.name).join(', ') || t('未加入节点组')}</td><td><span className={'status-chip ' + (node.retired ? '' : 'healthy')}>{t(node.retired ? '已退役' : '已登记')}</span></td><td>{!node.retired && <button className="secondary danger" disabled={busy || !poll.fresh} onClick={() => { setError(''); setAction({ node }); }}>{t('退役节点')}</button>}</td></tr>)}</tbody></table></div></section>}
    </>}
    {editing && data && <Dialog title={t(editing.revision ? '编辑节点组' : '创建节点组')} onClose={close}><form className="group-editor" onChange={e => { if (!(e.nativeEvent.target instanceof HTMLInputElement) || e.nativeEvent.target.type !== 'search') { setDirty(true); setDiscard(false); } }} onSubmit={e => {
      e.preventDefault(); const f = new FormData(e.currentTarget);
      void write('/node-groups/commands', { action: editing.revision ? 'update_node_group' : 'create_node_group', expected_revision: editing.revision, group: { uid: String(f.get('uid')), name: String(f.get('name')), revision: editing.revision + 1, node_uids: members } });
    }}><fieldset disabled={busy}><div className="form-grid"><label>UID<input name="uid" required pattern="[A-Za-z0-9][A-Za-z0-9_.-]*" defaultValue={editing.uid} readOnly={!!editing.revision}/></label><label>{t('展示名称')}<input name="name" required defaultValue={editing.name}/></label></div>
      <header className="member-heading"><h3>{t('选择成员')}</h3><span>{t('已选择 {count} 个节点', { count: members.length })}</span></header><label>{t('搜索节点')}<input type="search" value={search} onChange={e => setSearch(e.target.value)} placeholder={t('节点 ID、主机名或 IP')}/></label>
      <div className="member-picker">{visible.map(uid => <div className="member-option" key={uid}><input type="checkbox" aria-label={uid} checked={members.includes(uid)} onChange={e => setMembers(v => e.target.checked ? [...v, uid] : v.filter(id => id !== uid))}/>{identity(uid)}</div>)}{!visible.length && <p className="muted">{t('没有匹配的节点')}</p>}</div><small className="muted">{t('允许空组，节点可以加入多个组。')}</small>
      {error && <p role="alert" className="alert">{error}</p>}{!poll.fresh && <p role="status" className="notice warning">{t('数据尚未刷新或已过期，执行操作暂不可用。')}</p>}
      <div className="form-actions"><button disabled={!poll.fresh}>{t(busy ? '正在保存…' : '保存')}</button><button type="button" className="secondary" onClick={close}>{t('取消编辑')}</button></div>
      {discard && <div className="notice warning" role="alert"><p>{t('尚有未保存修改，是否放弃？')}</p><button type="button" className="danger secondary" onClick={() => { setDirty(false); setEditing(undefined); setDiscard(false); }}>{t('放弃修改')}</button></div>}
    </fieldset></form></Dialog>}
    {action && <Dialog title={action.group ? t('删除节点组') + ' · ' + action.group.name : t('退役节点') + ' · ' + action.node!.uid} onClose={close}><form onSubmit={e => { e.preventDefault(); const reason = String(new FormData(e.currentTarget).get('reason')).trim(); if (!reason) return; void write(action.group ? '/node-groups/commands' : '/identity/retire', action.group ? { action: 'delete_node_group', uid: action.group.uid, expected_revision: action.group.revision, reason } : { node_uid: action.node!.uid, expected_fingerprint: action.node!.fingerprint, evidence: reason }); }}>
      <p className="notice warning">{t(action.group ? '删除节点组不会删除机器。被流程引用或有任务占用时，必须先解除引用或等待任务结束。' : '退役会撤销节点接入资格并保留历史。请先退出所有节点组，并确认没有未结束任务；这不是临时下线操作。')}</p>
      <fieldset disabled={busy}><label>{t('操作原因')}<textarea name="reason" required maxLength={4096}/></label>{error && <p role="alert" className="alert">{error}</p>}{!poll.fresh && <p className="notice warning">{t('数据尚未刷新或已过期，执行操作暂不可用。')}</p>}<div className="form-actions"><button className="danger" disabled={!poll.fresh}>{t(action.group ? '确认删除' : '确认退役')}</button><button className="secondary" type="button" onClick={close}>{t('取消')}</button></div></fieldset>
    </form></Dialog>}
  </>;
}
