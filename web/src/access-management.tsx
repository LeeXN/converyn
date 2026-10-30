import { useCallback, useState } from 'react';
import { api, type Credential } from './api';
import { usePoll } from './use-poll';
import { t } from './i18n';
import { Empty } from './ui';
type Person = {uid:string; name:string; disabled:boolean; revision:number};
type Group = {uid:string; name:string; members:string[]};
type Binding = {uid:string; subject:{kind:'person'|'group';uid:string}; role:string; workflow:string|null; presets:string[]};
type Access = {revision:number; groups:Record<string,Group>; bindings:Record<string,Binding>; workflows:{uid:string;name:string;presets:string[]}[]};
const roles = [
 ['viewer','流程查看者','查看流程、任务、结果与日志'],
 ['executor','流程执行者','预览、执行流程，控制本人任务'],
 ['maintainer','流程维护者','登记、启停流程，控制范围内全部任务'],
 ['platform_operator','平台运维','管理节点、节点组、平台连接和控制面'],
 ['access_admin','身份与授权管理员','管理人员、用户组、凭据与授权'],
 ['super_admin','超级管理员','完整管理权限'],
] as const;
export function AccessManagement({credential,superAdmin}:{credential:Credential;superAdmin:boolean}) {
 const [revision,setRevision]=useState(0),[error,setError]=useState(''),[busy,setBusy]=useState(false),[notice,setNotice]=useState('');
 const [role,setRole]=useState('executor'),[workflow,setWorkflow]=useState(''),[subject,setSubject]=useState('');
 const [editingGroup,setEditingGroup]=useState<Group>();
 const load=useCallback(async(signal:AbortSignal)=>{
  const [access,people]=await Promise.all([api<Access>(credential,'/access',undefined,signal),api<{identities:Record<string,Person>}>(credential,'/identities',undefined,signal)]);
  return {access,people:people.identities};
 },[credential]);
 const poll=usePoll(load,revision), data=poll.data;
 async function change(endpoint:string,body:unknown) {
  if(!poll.fresh||busy)return;
  setBusy(true);setError('');setNotice('');
  try {await api(credential,endpoint,body);setRevision(v=>v+1);setNotice(t('更改已保存'));}
  catch(e){setError(String(e));} finally {setBusy(false);}
 }
 const platform=['platform_operator','access_admin','super_admin'].includes(role);
 const selected=data?.access.workflows.find(w=>w.uid===workflow);
 return <><header className="page-header"><div><span className="eyebrow">CONVERYN</span><h1>{t('访问管理')}</h1></div><button className="secondary" onClick={()=>setRevision(v=>v+1)}>{t('刷新')}</button></header>
 <p className="page-intro">{t('人员通过用户组或直接授权获得能力。新人员默认无法访问业务流程。')}</p>
 {(error||poll.error)&&<p className="alert" role="alert">{error||poll.error}</p>}{notice&&<p role="status" className="success-notice">{notice}</p>}
 {!data?<p>{t('正在读取…')}</p>:<fieldset disabled={busy||!poll.fresh} className="management-layout">
 <section className="panel"><h2>{t('人员')}</h2><form className="inline-form" onSubmit={e=>{e.preventDefault();const f=new FormData(e.currentTarget);void change('/identities',{action:'add_identity',identity:{uid:'user-'+crypto.randomUUID(),name:String(f.get('name')).trim(),disabled:false,revision:1}});e.currentTarget.reset();}}><label>{t('姓名')}<input name="name" required maxLength={256}/></label><button>{t('添加人员')}</button></form><p className="muted">{t('添加人员后，由 CLI 签发个人证书，再授予流程或平台权限。')}</p>
 <div className="table-wrap"><table><thead><tr><th>{t('姓名')}</th><th>{t('状态')}</th><th>{t('操作')}</th></tr></thead><tbody>{Object.values(data.people).map(p=><tr key={p.uid}><td>{p.name}<small>{p.uid}</small></td><td>{p.disabled?t('已停用'):t('已启用')}</td><td><button className="secondary" onClick={()=>{if(confirm(t('确认更改人员启用状态？')))void change('/identities',{action:'update',uid:p.uid,name:p.name,disabled:!p.disabled,expected_revision:p.revision});}}>{p.disabled?t('启用'):t('停用')}</button><button className="secondary" onClick={()=>{const name=prompt(t('姓名'),p.name);if(name?.trim())void change('/identities',{action:'update',uid:p.uid,name:name.trim(),disabled:p.disabled,expected_revision:p.revision});}}>{t('重命名')}</button></td></tr>)}</tbody></table></div></section>
 <section className="panel"><h2>{t('用户组')}</h2><form key={editingGroup?.uid??'new'} className="management-form" onSubmit={e=>{e.preventDefault();const f=new FormData(e.currentTarget);void change('/access',{action:'put_group',expected_revision:data.access.revision,group:{uid:editingGroup?.uid??'group-'+crypto.randomUUID(),name:String(f.get('name')).trim(),members:f.getAll('members')}});setEditingGroup(undefined);}}><label>{t('组名称')}<input name="name" required defaultValue={editingGroup?.name??''}/></label><label>{t('成员')}<select name="members" multiple defaultValue={editingGroup?.members??[]}>{Object.values(data.people).map(p=><option value={p.uid} key={p.uid}>{p.name}</option>)}</select></label><small>{t('可按 Ctrl 或 Command 选择多个成员。')}</small><button>{editingGroup?t('保存用户组'):t('创建用户组')}</button></form>
 {Object.values(data.access.groups).map(g=><div className="management-row" key={g.uid}><div><strong>{g.name}</strong><small>{g.members.map(id=>data.people[id]?.name??id).join(', ')||t('暂无成员')}</small></div><button className="secondary" onClick={()=>setEditingGroup(g)}>{t('编辑')}</button><button className="secondary" onClick={()=>{if(confirm(t('确认删除用户组？请先移除该组的授权。')))void change('/access',{action:'delete_group',uid:g.uid,expected_revision:data.access.revision});}}>{t('删除')}</button></div>)}</section>
 <section className="panel"><h2>{t('授予权限')}</h2><form className="management-form" onSubmit={e=>{e.preventDefault();const f=new FormData(e.currentTarget);const [kind,...id]=subject.split(':');const presets=platform||role==='maintainer'?[]:f.getAll('presets');const description=[data.people[id.join(':')]?.name??data.access.groups[id.join(':')]?.name,t(roles.find(r=>r[0]===role)![1]),platform?t('当前实例'):workflow||t('全部流程'),presets.length?presets.join(', '):t('全部预设')].join(' · ');if(confirm(t('确认授予以下权限：')+'\n'+description))void change('/access',{action:'put_binding',expected_revision:data.access.revision,binding:{uid:'binding-'+crypto.randomUUID(),subject:{kind,uid:id.join(':')},role,workflow:platform?null:workflow||null,presets}});}}>
 <label>{t('人员或用户组')}<select required value={subject} onChange={e=>setSubject(e.target.value)}><option value="">{t('请选择')}</option><optgroup label={t('人员')}>{Object.values(data.people).filter(p=>!p.disabled).map(p=><option value={'person:'+p.uid} key={p.uid}>{p.name}</option>)}</optgroup><optgroup label={t('用户组')}>{Object.values(data.access.groups).map(g=><option value={'group:'+g.uid} key={g.uid}>{g.name}</option>)}</optgroup></select></label>
 <label>{t('角色')}<select value={role} onChange={e=>setRole(e.target.value)}>{roles.filter(r=>superAdmin||r[0]!=='super_admin').map(r=><option value={r[0]} key={r[0]}>{t(r[1])}</option>)}</select><small>{t(roles.find(r=>r[0]===role)![2])}</small></label>
 {!platform&&<><label>{t('流程')}<select value={workflow} onChange={e=>setWorkflow(e.target.value)}><option value="">{t('全部流程')}</option>{data.access.workflows.map(w=><option value={w.uid} key={w.uid}>{w.name} · {w.uid}</option>)}</select></label>{role!=='maintainer'&&selected&&<label>{t('预设范围')}<select key={workflow} name="presets" multiple>{selected.presets.map(p=><option key={p}>{p}</option>)}</select><small>{t('不选择表示全部预设，包括后续新增的预设。')}</small></label>}</>}
 <button>{t('查看并确认授权')}</button></form></section>
 <section className="panel"><h2>{t('现有授权')}</h2>{!Object.keys(data.access.bindings).length?<Empty/>:<div className="table-wrap"><table><thead><tr><th>{t('人员或用户组')}</th><th>{t('角色')}</th><th>{t('范围')}</th><th>{t('操作')}</th></tr></thead><tbody>{Object.values(data.access.bindings).map(b=><tr key={b.uid}><td>{b.subject.kind==='person'?data.people[b.subject.uid]?.name??b.subject.uid:data.access.groups[b.subject.uid]?.name??b.subject.uid}<small>{b.subject.kind==='group'?t('通过用户组授权'):t('直接授权')}</small></td><td>{t(roles.find(r=>r[0]===b.role)?.[1]??b.role)}</td><td>{['super_admin','platform_operator','access_admin'].includes(b.role)?t('当前实例'):b.workflow??t('全部流程')}<small>{b.presets.join(', ')||t('全部预设')}</small></td><td><button className="secondary" disabled={!superAdmin&&b.role==='super_admin'} onClick={()=>{if(confirm(t('撤销后将立即失去相应访问权限，确认撤销？')))void change('/access',{action:'delete_binding',uid:b.uid,expected_revision:data.access.revision});}}>{t('撤销')}</button></td></tr>)}</tbody></table></div>}</section>
 </fieldset>}</>;
}

export function MyAccess({credential}:{credential:Credential}) {
 const load=useCallback((signal:AbortSignal)=>api<{revision:number;bindings:Binding[];platform:boolean;access:boolean;super_admin:boolean}>(credential,'/access/me',undefined,signal),[credential]);
 const poll=usePoll(load,0);
 return <><header className="page-header"><h1>{t('我的权限')}</h1></header><p className="page-intro">{t('直接授权与用户组授权累加。权限由服务器统一校验。')}</p>{poll.error&&<p className="alert" role="alert">{poll.error}</p>}{poll.data&&<section className="panel"><h2>{t('有效授权')}</h2>{poll.data.bindings.length?<div className="table-wrap"><table><thead><tr><th>{t('角色')}</th><th>{t('范围')}</th><th>{t('来源')}</th></tr></thead><tbody>{poll.data.bindings.map(b=><tr key={b.uid}><td>{t(roles.find(r=>r[0]===b.role)?.[1]??b.role)}</td><td>{['super_admin','platform_operator','access_admin'].includes(b.role)?t('当前实例'):b.workflow??t('全部流程')}<small>{b.presets.join(', ')||t('全部预设')}</small></td><td>{b.subject.kind==='group'?t('通过用户组授权'):t('直接授权')}<small>{b.subject.uid}</small></td></tr>)}</tbody></table></div>:<Empty title={t('暂无有效授权')}>{t('请联系管理员登记流程或为你授予访问权限。')}</Empty>}</section>}</>;
}
