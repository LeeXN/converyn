import {useCallback,useState} from 'react';
import {api,type Credential} from './api';
import {usePoll} from './use-poll';
import {t} from './i18n';
export function Secrets({credential}:{credential:Credential}) {
 const [revision,setRevision]=useState(0),[busy,setBusy]=useState(false),[error,setError]=useState(''),[notice,setNotice]=useState('');
 const load=useCallback((signal:AbortSignal)=>api<{secrets:{name:string;version:string}[]}>(credential,'/secrets',undefined,signal),[credential]);
 const poll=usePoll(load,revision);
 return <><header className="page-header"><h1>{t('Secret 管理')}</h1></header><p>{t('一次录入，任务按需获取。新版本不会覆盖已有版本。')}</p>
 {(error||poll.error)&&<p role="alert" className="alert">{error||poll.error}</p>}{notice&&<p role="status">{notice}</p>}
 <section className="panel"><h2>{t('新增 Secret 版本')}</h2><form autoComplete="off" onSubmit={async e=>{
  e.preventDefault();const form=e.currentTarget;const data=new FormData(form);const file=data.get('file');if(!(file instanceof File)||!file.size)return;
  setBusy(true);setError('');setNotice('');
  try {await api(credential,'/secrets',{name:data.get('name'),version:data.get('version'),value:await file.text()});form.reset();setRevision(v=>v+1);setNotice(t('Secret 版本已保存。'));}
  catch(error){setError(String(error));}finally{setBusy(false);}
 }}><fieldset disabled={busy}><label>{t('名称')}<input name="name" required pattern="[A-Za-z0-9][A-Za-z0-9_.-]*" maxLength={128}/></label><label>{t('版本')}<input name="version" required defaultValue="v1" maxLength={128}/></label><label>{t('私有内容文件')}<input name="file" type="file" required/></label><button>{busy?t('正在保存…'):t('保存版本')}</button></fieldset></form></section>
 <section className="panel"><h2>{t('已有版本')}</h2><div className="table-wrap"><table><thead><tr><th>{t('名称')}</th><th>{t('版本')}</th></tr></thead><tbody>{poll.data?.secrets.map(row=><tr key={row.name+'/'+row.version}><td>{row.name}</td><td>{row.version}</td></tr>)}</tbody></table></div></section></>;
}
