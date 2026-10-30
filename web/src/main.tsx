import {Secrets} from './secrets';
import { TargetManagement } from './target-management';
import { AccessManagement, MyAccess } from './access-management';
import { Infrastructure } from './infrastructure';
import { NodeTable, NodeDetails } from './nodes';
import type { crates_core_orchestration_machine_types_NodeObservation as Node } from './api-types';
import { useCallback, useEffect, useRef, useState } from 'react';
import { createRoot } from 'react-dom/client';
import { api, type Credential } from './api';
import { usePoll } from './use-poll';
import { Elapsed, WorkDetails } from './run-work';
import { WorkflowSteps } from './workflow-steps';
import { Dialog, CodeViewer } from './dialog';
import { WorkflowValuesEditor, ConfigurationFields } from './workflow-values-editor';
import { override } from './workflow-values';
import { VersionNotice } from './version-notice';
import type { crates_core_model_domain_Work as Work, crates_core_orchestration_machine_types_Attempt as Attempt } from './api-types';
import { Badge, Icon, Details, Fields, Empty } from './ui';
import { ControlPlane, Targets, PresetSummary, TargetLocation, type TargetPreset } from './records';
import { Pagination, paginationKey, listEndpoint, usePagination } from './pagination';
import { t, term, formatDate, useLocale, setLocale, type Locale } from './i18n';
import { readSession, saveSession, clearSession } from './session';
import { RequestError } from './api';
import './style.css';
type Person = {
    uid: string;
    name: string;
    disabled: boolean;
};
type Me = {
    capabilities: {platform:boolean; access:boolean; super_admin:boolean};
    principal: string;
    identity: Person | null;
    credential: {
        uid: string;
        expires_ms: number;
        overlap_until_ms?: number | null;
    } | null;
};
type Parameter = {
    kind: 'string' | 'integer' | 'number' | 'boolean';
    default?: unknown;
    choices: unknown[];
    minimum?: number;
    maximum?: number;
    max_length?: number;
    bindings: { step_id: string; path: string; transform?: 'sha256' | null; allowed_json_paths?: string[] }[];
};
type Definition = {
    inventory?: import('./records').WorkflowInventory | null;
    values?: Record<string, unknown>;
    immutable?: string[];
    node_values?: Record<string, Record<string, unknown>>;
    playbook: { steps: import('./api-types').crates_core_model_domain_Step[] };
    definitions: Record<string, string>;
    uid: string;
    name: string;
    targets: Record<string, TargetPreset>;
    parameters: Record<string, Parameter>;
    fixed: Record<string, unknown>;
    provenance: Record<string, string>;
    reconcile_seconds?: number;
};
type Reconciliation = {
    target: string;
    deployment: {
        condition: string;
        last_run?: string;
    };
    policy: {
        interval_seconds?: number;
        condition: string;
        last_started_ms?: number;
        last_run?: string;
    } | null;
};
type Workflow = {
    actions: {maintain:boolean;execute:string[]};
    reconciliation?: Reconciliation[];
    workflow: {
        uid: string;
        current: string;
        revision: number;
        enabled: boolean;
    };
    definition: Definition;
};
type Execution = {
    actor: {
        uid: string;
        name: string;
    };
    request: {
        workflow_uid: string;
        workflow_revision: string;
        target: string;
    };
    parameters: Record<string, unknown>;
    system: boolean;
    submitted_ms: number;
    deployment_uid?: string;
};
type Run = {
    pause_requested?: boolean;
    cancel_requested?: boolean;
    actions?: {control:boolean;retry_inputs?:boolean};
    submitted_ms?: number | null;
    finished_ms?: number | null;
    id: string;
    status: string;
    revision: number;
    total: number;
    completed: number;
    node_group_uid?: string | null;
    execution?: Execution;
    attempts?: Record<string, Attempt>;
    plan?: {
        work: Work[];
        sha256: string;
    };
};
type Route = {
    page: string;
    id?: string;
};
const sections = [['workflows', '流程', 'applications'], ['runs', '任务', 'runs'], ['nodes', '节点', 'nodes'], ['targets', '外部目标', 'targets'], ['audit', '审计', 'audit'], ['controlplane', '控制面', 'controlplane'], ['profile', '我的权限', 'audit'], ['access', '访问管理', 'audit'], ['secrets', 'Secret 管理', 'audit'], ['infrastructure', '节点组管理', 'nodes']] as const;
function readRoute(): Route {
    const [page, id] = location.hash.split('?')[0].replace(/^#\/?/, '').split('/');
    try {
        return { page: sections.some(s => s[0] === page) ? page : 'workflows', id: id ? decodeURIComponent(id) : undefined };
    }
    catch {
        return { page: 'workflows' };
    }
}
function go(page: string, id?: string) { location.hash = '#/' + page + (id ? '/' + encodeURIComponent(id) : ''); }
function path(id: string) { return encodeURIComponent(id); }
const date = formatDate;
function Login({ connect, notice }: {
    notice?: string;
    connect: (c: Credential, m: Me) => void;
}) {
    const [error, setError] = useState(''), [busy, setBusy] = useState(false);
    const initialCluster = new URLSearchParams(location.search).get('cluster') ?? '';
    return <div className="login-layout"><div className="login-story"><a className="brand" href="#/"><span className="brand-symbol">C</span> CONVERYN</a><div><span className="eyebrow">{t("归衡 · 流程执行台")}</span><h1>{t("运行已有流程，")}<br />{t("每次操作都有记录。")}</h1><p>{t("选择预设目标，填写本次参数，")}<br />{t("跟踪任务、回执与日志。")}</p></div><small>{t("Converyn 流程控制台")}</small></div><main className="login">{notice && <p role="alert" className="alert">{notice}</p>}<h2>{t("连接控制面")}</h2><p>{t("使用运维人员签发的个人客户端证书和配套令牌。")}</p><form onSubmit={async (e) => {
            e.preventDefault();
            const f = new FormData(e.currentTarget);
            const c = { cluster: String(f.get('cluster')).trim(), token: String(f.get('token')).trim() };
            setBusy(true);
            setError('');
            try {
                const m = await api<Me>(c, '/identity/me');
                if (!m.identity)
                    throw Error(t("这是引导凭据，请先通过命令行创建具名管理员。"));
                connect(c, m);
            }
            catch (e) {
                setError(String(e));
            }
            finally {
                setBusy(false);
            }
        }}><fieldset disabled={busy}><label>{t("控制面名称")}<input name="cluster" required autoComplete="off" defaultValue={initialCluster}/></label><label>{t("个人令牌")}<input name="token" type="password" required autoComplete="off"/></label>{error && <p role="alert" className="alert">{error}</p>}<button className="login-submit">{busy ? t("正在核验…") : t("连接")}</button></fieldset></form><details className="login-help"><summary>{t("首次连接指引")}</summary><ol><li>{t("导入管理员交付的个人证书，并信任对应 CA。")}</li><li>{t("填写交付说明中的控制面名称和个人令牌。")}</li><li>{t("连接失败时，在 CLI 运行 converyn auth doctor 检查接入。")}</li></ol></details><p className="login-help">{t("先在浏览器导入个人证书。登录保留在当前标签页，刷新后会重新核验身份；退出或凭据失效后清除。")}</p></main></div>;
}
function WorkflowForm({ credential, item, fresh }: {
    credential: Credential;
    item: Workflow;
    fresh: boolean;
}) {
    const [view, setView] = useState(() => new URLSearchParams(location.hash.split('?')[1] ?? '').get('tab') ?? 'overview');
    const [adminRevision, setAdminRevision] = useState(item.workflow.revision), [enabled,setEnabled]=useState(item.workflow.enabled);
    const [workflowValues, setWorkflowValues] = useState<Record<string, unknown>>(() => structuredClone(item.definition.values ?? {}));
    const [workflowOverrides, setWorkflowOverrides] = useState<Record<string, unknown>>({});
    const [configurationReset, setConfigurationReset] = useState(0);
    const targets = Object.keys(item.definition.targets), [target, setTarget] = useState(targets[0] ?? ''), [values, setValues] = useState<Record<string, string>>(() => Object.fromEntries(Object.entries(item.definition.parameters).map(([n, p]) => [n, p.default == null ? '' : String(p.default)]))), [preview, setPreview] = useState<{
        request: unknown;
        plan: {
            sha256: string;
            work: {
                id: string;
                step_id: string;
                node_uid: string;
                phase: string;
                operation?: Work['operation'];
            }[];
        };
        parameters: unknown;
        rendered_scripts?: Record<string, string>;
    }>(), [busy, setBusy] = useState(false), [error, setError] = useState('');
    const epoch = useRef(0), abort = useRef<AbortController | undefined>(undefined);
    const reconciliation = item.reconciliation?.find(r => r.target === target);
    useEffect(() => () => { epoch.current++; abort.current?.abort(); }, []);
    function invalidate() { epoch.current++; abort.current?.abort(); setPreview(undefined); setError(''); setBusy(false); }
    useEffect(()=>{setEnabled(item.workflow.enabled);setAdminRevision(item.workflow.revision);invalidate();},[item.workflow.enabled,item.workflow.revision]);
    const targetScope = targets.join('\0');
    useEffect(()=>{if(!item.definition.targets[target]){setTarget(targets[0]??'');invalidate();}},[targetScope,target]);

    function parameters() {
        const result: Record<string, unknown> = structuredClone(workflowOverrides);
        for (const [name, p] of Object.entries(item.definition.parameters)) {
            const v = values[name];
            if (v === '' && !(p.kind === 'string' && p.default === ''))
                throw Error(t("请填写 ") + name);
            result[name] = p.kind === 'string' ? v : p.kind === 'boolean' ? v === 'true' : Number(v);
            if (typeof result[name] === 'number') {
                const n = result[name] as number;
                if (!Number.isFinite(n) || (p.kind === 'integer' && !Number.isInteger(n)))
                    throw Error(t("参数不是有效数字：") + name);
                if ((p.minimum != null && n < p.minimum) || (p.maximum != null && n > p.maximum))
                    throw Error(t("参数超出范围：") + name);
            }
            if (typeof result[name] === 'string' && new TextEncoder().encode(String(result[name])).length > Math.min(p.max_length ?? 4096, 65536))
                throw Error(t("参数超过最大长度：") + name);
            if (p.choices?.length && !p.choices.some(choice => choice === result[name]))
                throw Error(t("参数不在可选值中：") + name);
        }
        return result;
    }
    async function check() {
        const invalid = document.querySelector<HTMLInputElement>('.workflow-values-list input:invalid, .configuration-editor input:invalid');
        if (invalid) { invalid.reportValidity(); invalidate(); return; }
        const ticket = ++epoch.current;
        abort.current?.abort();
        const controller = new AbortController();
        abort.current = controller;
        setBusy(true);
        setError('');
        try {
            const result = await api<typeof preview>(credential, '/workflows/' + path(item.workflow.uid) + '/preview', { workflow_revision: item.workflow.current, target, parameters: parameters(), request_id: 'request-' + crypto.randomUUID() }, controller.signal);
            if (ticket === epoch.current) {
                setPreview(result);
                requestAnimationFrame(()=>document.getElementById("execution-preview")?.scrollIntoView({block:"start",behavior:"smooth"}));
            }
        }
        catch (e) {
            if (ticket === epoch.current && !controller.signal.aborted)
                setError(String(e));
        }
        finally {
            if (ticket === epoch.current)
                setBusy(false);
        }
    }
    async function execute() {
        const invalid = document.querySelector<HTMLInputElement>('.workflow-values-list input:invalid, .configuration-editor input:invalid');
        if (invalid) { invalid.reportValidity(); invalidate(); return; }
        if (!preview)
            return;
        const ticket = ++epoch.current;
        const controller = new AbortController();
        abort.current = controller;
        setBusy(true);
        setError('');
        try {
            const result = await api<{
                run_id: string;
            }>(credential, '/workflows/' + path(item.workflow.uid) + '/runs', preview.request, controller.signal);
            if (ticket === epoch.current)
                go('runs', result.run_id);
        }
        catch (e) {
            if (ticket === epoch.current && !controller.signal.aborted)
                setError(String(e) + t("。提交结果未知时请保留本次预览，用相同请求重试或到任务列表查询。"));
        }
        finally {
            if (ticket === epoch.current)
                setBusy(false);
        }
    }
    return <section className="panel workflow-execution"><a href="#/workflows" className="back-link">{t('返回流程列表')}</a><header className="page-header"><h2>{item.definition.name}</h2>{item.actions?.maintain&&<button className="secondary" disabled={!fresh||busy} onClick={async()=>{setBusy(true);setError('');try{await api(credential,'/workflows/'+path(item.workflow.uid)+'/enabled',{enabled:!enabled,expected_revision:adminRevision});setEnabled(!enabled);setAdminRevision(v=>v+1);invalidate();}catch(e){setError(String(e));}finally{setBusy(false);}}}>{enabled?t('停用流程'):t('启用流程')}</button>}</header><div className="tabs" role="tablist" aria-label={t('流程详情')}>{(['overview','steps','execute'] as const).map((tab, i) => <button role="tab" key={tab} aria-selected={view === tab} onClick={() => setView(tab)}>{t(['概览','步骤与脚本','执行配置'][i])}</button>)}</div>
<section hidden={view !== 'overview'}><div className="summary-strip"><div><span>{t('流程步骤')}</span><strong>{item.definition.playbook.steps.length}</strong></div><div><span>{t('目标预设')}</span><strong>{targets.length}</strong></div><div><span>{t('状态')}</span><strong>{t(enabled ? '可执行' : '已停用')}</strong></div></div><p className="muted">{item.workflow.uid} · {item.workflow.current.slice(0, 12)}</p><p>{t('查看步骤与原始脚本，或进入执行配置核对本次参数。')}</p><PresetSummary preset={item.definition.targets[target]} inventory={item.definition.inventory}/><Details label={t('固定配置与来源')} value={{ targets: item.definition.targets, fixed: item.definition.fixed, provenance: item.definition.provenance }}/></section>
<section hidden={view !== 'steps'}><WorkflowSteps steps={item.definition.playbook.steps} definitions={item.definition.definitions} parameters={item.definition.parameters} provenance={item.definition.provenance}/></section>
<section hidden={view !== 'execute'}><ol className="execution-stages" aria-label={t('执行步骤')}><li className={!preview?'active':''}>{t('1 · 配置')}</li><li className={preview?'active':''}>{t('2 · 核对计划')}</li><li>{t('3 · 执行并跟踪')}</li></ol><p>{t("流程 ")}{item.workflow.uid} {t(" · 修订 ")}{item.workflow.current.slice(0, 12)}</p><p>{item.definition.reconcile_seconds ? t("成功后按本次配置持续核对，周期 ") + item.definition.reconcile_seconds + t(" 秒") : t("执行一次，完成后结束")}</p><fieldset disabled={!fresh || !enabled || busy}><label>{t("预设目标")}<select value={target} onChange={e => { invalidate(); setTarget(e.target.value); }}>{targets.map(t => <option key={t}>{t}</option>)}</select></label><section className="run-configuration" onInput={invalidate}><header><h3>{t("本次配置")}</h3><button type="button" className="secondary reset-configuration" onClick={() => { invalidate(); setValues(Object.fromEntries(Object.entries(item.definition.parameters).map(([n, p]) => [n, p.default == null ? '' : String(p.default)]))); setWorkflowValues(structuredClone(item.definition.values ?? {})); setWorkflowOverrides({}); setConfigurationReset(v => v + 1); }}>{t("恢复默认值")}</button></header><WorkflowValuesEditor key={configurationReset} values={workflowValues} defaults={item.definition.values ?? {}} immutable={item.definition.immutable ?? []} onChange={(next, path) => { invalidate(); setWorkflowValues(next); setWorkflowOverrides(previous => path ? override(previous, next, path) : {}); }}/><div className="parameter-grid">{Object.entries(item.definition.parameters).filter(([, p]) => !p.bindings?.some(b => b.path === '/content')).map(([name, p]) => <label key={name}>{name}{p.choices?.length ? <select value={values[name]} onChange={e => { invalidate(); setValues(v => ({ ...v, [name]: e.target.value })); }}><option value="">{t("请选择")}</option>{p.choices.map(v => <option key={String(v)} value={String(v)}>{String(v)}</option>)}</select> : p.kind === 'boolean' ? <select value={values[name]} onChange={e => { invalidate(); setValues(v => ({ ...v, [name]: e.target.value })); }}><option value="">{t("请选择")}</option><option value="true">{term(true)}</option><option value="false">{term(false)}</option></select> : <input value={values[name]} type={p.kind === 'string' ? 'text' : 'number'} step={p.kind === 'integer' ? 1 : 'any'} min={p.minimum} max={p.maximum} maxLength={p.max_length} onChange={e => { invalidate(); setValues(v => ({ ...v, [name]: e.target.value })); }}/>}<small className="parameter-help"><span>{t("默认值")}: {p.default == null ? t("未提供") : String(p.default)}</span>{p.minimum != null && <span>{t("最小值")}: {p.minimum}</span>}{p.maximum != null && <span>{t("最大值")}: {p.maximum}</span>}{p.kind === 'string' && <span>{t("最大长度（字节）")}: {Math.min(p.max_length ?? 4096, 65536)}</span>}{!!p.bindings?.length && <span>{t("应用步骤")}: {[...new Set(p.bindings.map(b => b.step_id))].join(', ')}</span>}</small></label>)}</div>{Object.entries(item.definition.parameters).filter(([, p]) => p.bindings?.some(b => b.path === '/content')).map(([name, p]) => {
 const binding = p.bindings!.find(b => b.path === '/content')!;
 const step = item.definition.playbook.steps.find(s => s.id === binding.step_id);
 const file = step?.operation.kind === 'file' ? step.operation.path : name;
 const changed = values[name] !== String(p.default ?? '');
 return <details className="configuration-editor" key={name}><summary>{file}{changed && <strong> · {t('已临时修改')}</strong>}</summary>
 <div className="configuration-fields"><p className="configuration-fields-label">{t('本次文件内容')}</p><ConfigurationFields content={values[name]} paths={binding.allowed_json_paths ?? []} onChange={content => { invalidate(); setValues(v => ({ ...v, [name]: content })); }}/></div>
 <p className="parameter-help">{t('可修改字段')}: {binding.allowed_json_paths?.join(', ')}</p><p className="parameter-help">{t('仅本次执行生效')} · {new TextEncoder().encode(values[name]).length} / {Math.min(p.max_length ?? 4096, 65536)} B</p>
 <button className="secondary" onClick={() => { invalidate(); setValues(v => ({ ...v, [name]: String(p.default ?? '') })); }}>{t('恢复此项默认值')}</button>
 {changed && <details className="definition-code"><summary>{t('对照默认内容')}</summary><pre tabIndex={0}>{String(p.default ?? '')}</pre></details>}
 </details>;
 })}</section><PresetSummary preset={item.definition.targets[target]} inventory={item.definition.inventory}/><div className="execution-actions"><button className={preview?"secondary":undefined} disabled={!item.actions?.execute.includes(target)} onClick={() => void check()}>{t("预览执行计划")}</button></div>{!item.actions?.execute.includes(target)&&<p className="muted">{t("当前预设仅有查看权限，请联系授权管理员。")}</p>}</fieldset>{!fresh && <p role="status">{t("数据尚未刷新或已过期，执行操作暂不可用。")}</p>}{!enabled && <p>{t("流程已停用。")}</p>}{error && <p role="alert">{error}</p>}{preview && <section id="execution-preview"><h3>{t("执行预览")}</h3><p>{preview.plan.work.length} {t(" 个工作项 · 计划摘要 ")}{preview.plan.sha256.slice(0, 16)}</p><div className="table-wrap"><table><thead><tr><th>{t("参数")}</th><th>{t("本次值")}</th></tr></thead><tbody>{Object.entries(preview.parameters as Record<string, unknown>).map(([name, value]) => <tr key={name}><td>{name}</td><td>{typeof value === 'boolean' ? term(value) : typeof value === 'string' && value.includes('\n') ? <CodeViewer label={t('查看本次内容')} content={value}/> : typeof value === 'object' ? JSON.stringify(value) : String(value)}</td></tr>)}</tbody></table></div><div className="table-wrap"><table><thead><tr><th>{t("步骤")}</th><th>{t("实际节点")}</th><th>{t("阶段")}</th><th>{t("执行内容")}</th></tr></thead><tbody>{preview.plan.work.map(w => <tr key={w.id}><td>{w.step_id}</td><td>{w.node_uid}</td><td>{term(w.phase)}</td><td><Details value={w.operation} label={t("查看详情")}/>{preview.rendered_scripts?.[w.id] && <CodeViewer content={preview.rendered_scripts[w.id]} label={t("查看执行脚本（含变量）")}/>}</td></tr>)}</tbody></table></div><Details value={{ sha256: preview.plan.sha256 }} label={t("计划摘要")}/><div className="execution-actions"><button disabled={!fresh||!enabled||busy||!item.actions?.execute.includes(target)} onClick={()=>void execute()}>{t("执行已预览计划")}</button></div></section>}{reconciliation && <section><h3>{t("持续核对状态")}</h3><Badge value={reconciliation.policy?.interval_seconds == null ? 'disabled' : reconciliation.policy.condition}/><p>{t("上次核对 ")}{date(reconciliation.policy?.last_started_ms)}</p>{(reconciliation.policy?.last_run || reconciliation.deployment.last_run) && <button onClick={() => go('runs', reconciliation.policy?.last_run || reconciliation.deployment.last_run)}>{t("查看最近任务")}</button>}</section>}</section></section>;
}
function RunDetails({ credential, id, me }: {
    credential: Credential;
    id: string;
    me: Me;
}) {
    const [revision, setRevision] = useState(0), [error, setError] = useState(''), [busy, setBusy] = useState(false);
    const [selected, setSelected] = useState(() => new URLSearchParams(location.hash.split('?')[1] ?? '').get('work') ?? '');
    const [workTab, setWorkTab] = useState('result');
    useEffect(() => { const sync = () => setSelected(new URLSearchParams(location.hash.split('?')[1] ?? '').get('work') ?? ''); addEventListener('hashchange', sync); return () => removeEventListener('hashchange', sync); }, []);
    const load = useCallback((signal: AbortSignal) => api<Run>(credential, '/runs/' + path(id), undefined, signal), [credential, id]);
    const poll = usePoll(load, revision);
    const run = poll.data;
    const allowed = run?.actions?.control === true;
    const terminal = run && ['succeeded', 'failed', 'canceled'].includes(run.status);
    async function control(action: 'pause' | 'cancel', resume = false) {
        if (!run || !poll.fresh || busy)
            return;
        if (action === 'cancel' && !confirm(t("取消这次任务？取消不会回滚已完成的修改，已开始的操作需要停止或完成证据才能终结。")))
            return;
        setBusy(true);
        setError('');
        try {
            await api(credential, '/runs/' + path(id) + '/' + action, action === 'pause' ? { paused: !resume, expected_revision: run.revision } : {});
            setRevision(v => v + 1);
        }
        catch (e) {
            setError(String(e));
        }
        finally {
            setBusy(false);
        }
    }
    if (!run)
        return <p role="status">{poll.error || t("读取任务…")}</p>;
    const work = run.plan?.work ?? [];
    const active = work.find(w => w.id === selected);
    function selectWork(workId: string) {
        history.replaceState(null, '', '#/runs/' + path(id) + (workId ? '?work=' + encodeURIComponent(workId) : ''));
        setSelected(workId);
    }
    return <><button onClick={() => go('runs')}>{t("返回任务列表")}</button><section className="panel"><header className="run-heading"><div><span className="eyebrow">{t("任务详情")}</span><h1>{run.execution?.request.workflow_uid??id}</h1><p className="page-intro">{run.execution?.request.target}</p><small className="copy-id">{id}</small></div><Badge value={run.status}/></header><dl className="run-timing"><div><dt>{t("提交时间")}</dt><dd>{date(run.submitted_ms ?? run.execution?.submitted_ms)}</dd></div><div><dt>{t("结束日期")}</dt><dd>{date(run.finished_ms)}</dd></div><div><dt>{t("总用时")}</dt><dd><Elapsed submitted={run.submitted_ms ?? run.execution?.submitted_ms} finished={run.finished_ms} status={run.status}/></dd></div></dl><p>{t("操作人员：")}{run.execution?.actor.name ?? t("系统")}{run.execution?.system ? t(" · 系统核对任务") : ''}</p><Details label={t("任务技术信息")} value={{run_id:id,actor_uid:run.execution?.actor.uid,workflow_revision:run.execution?.request.workflow_revision,plan_sha256:run.plan?.sha256}}/><details className="record-details"><summary>{t("实际参数快照")}</summary><Fields value={run.execution?.parameters} literal/></details>{allowed && run.actions?.retry_inputs && <button disabled={busy||!poll.fresh} onClick={()=>void control('pause',true)}>{t('恢复未执行的输入步骤')}</button>}{run.pause_requested && !terminal && <p className="notice warning" role="status">{t('已请求暂停后续步骤，等待已授权的操作结束。')}</p>}{allowed && !terminal && <fieldset className="run-controls" disabled={!poll.fresh || busy || run.cancel_requested}><button onClick={() => void control('pause', !!run.pause_requested)}>{run.pause_requested ? t('恢复后续步骤') : t('暂停后续步骤')}</button><button onClick={() => void control('cancel')}>{t(run.cancel_requested ? '正在取消…' : '取消任务')}</button><p className="muted control-help">{t('暂停仅阻止后续步骤获得执行授权，已运行的脚本会继续。取消任务不会回滚已完成的修改。')}</p></fieldset>}{(error || poll.error) && <p role="alert">{error || poll.error}</p>}<p className="muted">{t("上次读取 ")}{date(poll.updated)}{poll.error ? t(" · 下次检查 ") + date(poll.next) : ''}</p><div className="table-wrap"><table className="work-table"><thead><tr><th>{t("步骤")}</th><th>{t("节点")}</th><th>{t("阶段")}</th><th>{t("操作")}</th><th>{t("状态")}</th><th>{t("退出码")}</th><th>{t("详情")}</th></tr></thead><tbody>{work.map(w => <tr key={w.id} className={active?.id === w.id ? 'selected' : undefined}><td><button aria-pressed={active?.id === w.id} aria-controls="work-detail" onClick={() => selectWork(w.id)}>{w.step_id}</button></td><td>{w.node_uid}</td><td>{term(w.phase)}</td><td>{term(w.operation.kind)}{'execution' in w.operation && w.operation.execution==='host' ? ' · root' : ''}</td><td><Badge value={run.attempts?.[w.id]?.status ?? 'pending'}/></td><td>{run.attempts?.[w.id]?.receipt?.exit_code ?? "—"}</td><td><button aria-controls="work-detail" onClick={() => selectWork(w.id)}>{t("查看详情")}</button></td></tr>)}</tbody></table></div></section>{active && <Dialog wide title={active.step_id + ' / ' + active.node_uid} onClose={() => selectWork('')}><nav className="step-navigation" aria-label={t('切换步骤')}><button className="secondary" disabled={work.indexOf(active) === 0} onClick={() => selectWork(work[work.indexOf(active) - 1].id)}>{t('上一步')}</button><span>{work.indexOf(active) + 1} / {work.length}</span><button className="secondary" disabled={work.indexOf(active) === work.length - 1} onClick={() => selectWork(work[work.indexOf(active) + 1].id)}>{t('下一步')}</button></nav><WorkDetails key={id + '/' + active.id} credential={credential} run={id} work={active} attempt={run.attempts?.[active.id]} tab={workTab} onTab={setWorkTab}/></Dialog>}</>;

}
function Pages({ credential, me, route }: {
    credential: Credential;
    me: Me;
    route: Route;
}) {
    const [revision, setRevision] = useState(0);
    const query = new URLSearchParams(location.hash.split('?')[1]??'');
    const workflowState = query.get('status')??(route.page==='workflows'?'enabled':'');
    function filter(name:string,value:string){const q=new URLSearchParams(location.hash.split('?')[1]??'');if(value)q.set(name,value);else q.delete(name);location.hash='#/'+route.page+(q.size?'?'+q:'');}
    const filterKey = query.toString();
    const pager = usePagination(paginationKey(credential.cluster, me.identity?.uid ?? me.principal, route.page + '/' + filterKey));
    const paginated = !route.id && ['workflows', 'runs', 'audit', 'nodes', 'targets'].includes(route.page);
    const filters = new URLSearchParams(query); filters.delete('work');
    if(route.page==='workflows'&&!filters.has('status'))filters.set('status','enabled');
    if(route.page==='audit'&&!filters.has('view'))filters.set('view','activity');
    const endpoint = route.page === 'workflows' && route.id ? '/workflows/'+path(route.id) : route.page==='controlplane'?'/controlplane/members':listEndpoint(route.page,pager.cursor,pager.limit)+(filters.size?'&'+filters:'');
    const load = useCallback(async (signal: AbortSignal) => {
        const value = await api<Record<string, unknown>>(credential, endpoint, undefined, signal);
        return value;
    }, [credential, endpoint]);
    const poll = usePoll(load, revision, !['profile','access','infrastructure','secrets'].includes(route.page) && !(['runs', 'nodes'].includes(route.page) && route.id));
    const value = poll.data;
    const next = value?.next_cursor as string | number | null | undefined;
    const rows = value?.[({ workflows: 'workflows', runs: 'runs', nodes: 'nodes', targets: 'targets', audit: 'records' } as Record<string, string>)[route.page]];
    const count = value ? Array.isArray(rows) ? rows.length : rows && typeof rows === 'object' ? Object.keys(rows).length : 0 : undefined;
    const pagination = (bottom = false) => <Pagination pager={pager} count={count} next={next} busy={poll.loading} fresh={poll.fresh} bottom={bottom} total={value?.total_count as number | undefined}/>;
    useEffect(() => { if (paginated) document.querySelector('.page-header')?.scrollIntoView({ block: 'start' }); }, [paginated, pager.cursor, pager.limit]);
    if (route.page==='profile') return <MyAccess credential={credential}/>;
    if (route.page==='access') return <AccessManagement credential={credential} superAdmin={me.capabilities.super_admin}/>;
    if (route.page==='secrets') return <Secrets credential={credential}/>;
    if (route.page==='infrastructure') return <Infrastructure credential={credential}/>;
    if (route.page === 'nodes' && route.id) return <NodeDetails key={route.id} credential={credential} id={route.id} back={() => go('nodes')}/>;
    if (route.page === 'runs' && route.id)
        return <RunDetails key={route.id} credential={credential} id={route.id} me={me}/>;
    return <><header className="page-header"><div><span className="eyebrow">CONVERYN</span><h1>{t(sections.find(s => s[0] === route.page)?.[1] ?? '')}</h1></div><button onClick={() => setRevision(v => v + 1)}><Icon name="refresh"/>{t("刷新")}</button></header><p className="muted">{t("上次更新 ")}{date(poll.updated)}{poll.error ? t(" · 下次检查 ") + date(poll.next) : ''}</p>{poll.error && <p role="alert" className="alert">{poll.error}</p>}{poll.loading && !value && <p>{t("正在读取…")}</p>}{route.page === 'audit' && <section className="audit-intro"><p className="page-intro">{t('业务操作和失败事件默认展示。节点观测是 Agent 状态上报，不等同于配置变更；全部记录始终保留。')}</p><div className="tabs" role="tablist" aria-label={t('审计分类')}>{(['activity','observations','all'] as const).map((view, i) => <button role="tab" key={view} aria-selected={(query.get('view') ?? 'activity') === view} onClick={() => filter('view', view)}>{t(['业务操作与失败','节点观测','全部记录'][i])}</button>)}</div></section>}{route.page === 'workflows' && !route.id && <label className="workflow-filter">{t('流程状态')}<select value={workflowState} onChange={e => filter('status',e.target.value)}><option value="enabled">{t('可执行')}</option><option value="disabled">{t('已停用')}</option><option value="all">{t('全部')}</option></select></label>}{!route.id&&['workflows','runs'].includes(route.page)&&<form className="list-filters" key={filterKey} onSubmit={e=>{e.preventDefault();const f=new FormData(e.currentTarget);const q=new URLSearchParams();for(const [k,v]of f)if(v)q.set(k,String(v));if(route.page==='workflows')q.set('status',workflowState);location.hash='#/'+route.page+(q.size?'?'+q:'');}}>{route.page==='workflows'?<label>{t('搜索流程')}<input name="search" defaultValue={query.get('search')??''} placeholder={t('名称或标识')}/></label>:<><label>{t('流程')}<input name="workflow" defaultValue={query.get('workflow')??''}/></label><label>{t('预设目标')}<input name="preset" defaultValue={query.get('preset')??''}/></label><label>{t('状态')}<select name="status" defaultValue={workflowState}><option value="">{t('全部')}</option>{['pending','preparing','paused','authorized','running','cancel_requested','needs_attention','succeeded','failed','canceled'].map(s=><option value={s} key={s}>{term(s)}</option>)}</select></label><label className="checkbox-label"><input type="checkbox" name="actor" value={me.identity?.uid??''} defaultChecked={!!query.get('actor')}/>{t('仅本人任务')}</label></>}<button className="secondary">{t('筛选')}</button><button type="button" className="secondary" onClick={()=>{location.hash='#/'+route.page;}}>{t('清除')}</button></form>}{paginated && pagination()}{value && paginated && count === 0 && ['workflows', 'runs', 'audit'].includes(route.page) && <Empty title={route.page==='workflows'?t('暂无可用流程'):t('暂无记录')}>{route.page==='workflows'?<>{t('请联系管理员登记流程或为你授予访问权限。')}<code className="empty-link">converyn auth status</code></>:t('调整筛选条件，或从流程页面开始执行。')}</Empty>}{value && route.page === 'workflows'  && (route.id ? <WorkflowForm key={(value as unknown as Workflow).workflow.current} credential={credential} item={value as unknown as Workflow} fresh={poll.fresh}/> : count ? <div className="table-wrap"><table><thead><tr><th>{t("流程")}</th><th>{t("目标预设")}</th><th>{t("状态")}</th><th>{t("修订")}</th><th>{t("操作")}</th></tr></thead><tbody>{((value.workflows ?? []) as Workflow[]).map(item => <tr key={item.workflow.uid}><td><button onClick={() => go('workflows', item.workflow.uid)}>{item.definition.name}</button><small>{item.workflow.uid}</small></td><td>{Object.entries(item.definition.targets).map(([name, preset]) => <div key={name}>{name}<small><TargetLocation preset={preset}/></small></div>)}</td><td>{item.workflow.enabled ? t("可执行") : t("已停用")}</td><td>{item.workflow.revision}</td><td><div className="button-row"><a className="button secondary" href={'#/workflows/'+path(item.workflow.uid)+'?tab=steps'}>{t('查看步骤')}</a><a className="button" href={'#/workflows/'+path(item.workflow.uid)+'?tab=execute'}>{t('配置执行')}</a></div></td></tr>)}</tbody></table></div> : null)}{value && !!count && route.page === 'runs' && <div className="table-wrap"><table><thead><tr><th>{t("任务")}</th><th>{t("操作人员")}</th><th>{t("预设目标")}</th><th>{t("状态")}</th><th>{t("进度")}</th><th aria-sort="descending">{t("执行日期")} ↓</th><th>{t("结束日期")}</th><th>{t("总用时")}</th></tr></thead><tbody>{((value.runs ?? []) as Run[]).map(r => <tr key={r.id}><td><button title={r.id} onClick={() => go('runs', r.id)}>{r.execution?.request.workflow_uid??r.id}</button><small title={r.id}>{r.id.length>20?r.id.slice(0,20)+'…':r.id}</small></td><td title={r.execution?.actor.uid}>{r.execution?.actor.name ?? t("系统")}</td><td>{r.execution?.request.target}</td><td><Badge value={r.status}/></td><td>{r.completed}/{r.total}</td><td className="date-cell">{date(r.submitted_ms ?? r.execution?.submitted_ms)}</td><td className="date-cell">{date(r.finished_ms)}</td><td><Elapsed submitted={r.submitted_ms ?? r.execution?.submitted_ms} finished={r.finished_ms} status={r.status}/></td></tr>)}</tbody></table></div>}{value && !!count && route.page === 'audit' && <div className="table-wrap"><table><thead><tr><th>{t("人员")}</th><th>{t("操作")}</th><th>{t("时间")}</th><th>{t("结果")}</th></tr></thead><tbody>{((value.records ?? []) as Record<string, unknown>[]).map((row, i) => <tr key={i}><td>{String(row.identity_name ?? row.principal ?? t("系统"))}{row.principal !== row.identity_name && <small title={String(row.principal ?? '')}>{String(row.principal ?? '')}</small>}</td><td>{term(row.action ?? row.type)}<Details value={row}/></td><td>{date(row.observed_ms as number)}</td><td>{term(row.code)}</td></tr>)}</tbody></table></div>}{value && route.page === 'nodes' && <NodeTable nodes={(value.nodes ?? {}) as Record<string, Node>} open={id => go('nodes', id)}/>} {value && route.page === 'targets' && <><TargetManagement credential={credential} onSaved={()=>setRevision(v=>v+1)}/><Targets targets={(value.targets ?? {}) as Record<string, unknown>}/></>} {value && route.page === 'controlplane' && <ControlPlane value={value}/>}{paginated && count !== undefined && (count >= 10 || pager.index > 0 || next != null) && pagination(true)}</>;
}
function LanguageSwitch() {
    const locale = useLocale();
    return <select className="language-switch" aria-label={t('界面语言')} value={locale} onChange={e => setLocale(e.target.value as Locale)}><option value="zh-CN">{t("简体中文")}</option><option value="en">English</option></select>;
}
function App() {
    useLocale();
    const [credential, setCredential] = useState<Credential | undefined>(() => readSession()), [me, setMe] = useState<Me>(), [route, setRoute] = useState(readRoute), [menu, setMenu] = useState(false), [retry, setRetry] = useState(0);
    const [authError, setAuthError] = useState('');
    useEffect(() => { const change = () => { setRoute(readRoute()); setMenu(false); }; addEventListener('hashchange', change); return () => removeEventListener('hashchange', change); }, []);
    const logout = useCallback(() => { clearSession(); setCredential(undefined); setMe(undefined); setMenu(false); }, []);
    const load = useCallback(async (signal: AbortSignal) => {
        if (!credential) throw Error(t('未登录'));
        try {
            const current = await api<Me>(credential, '/identity/me', undefined, signal);
            const deadline = current.credential ? Math.min(current.credential.expires_ms, current.credential.overlap_until_ms ?? Infinity) : 0;
            if (!current.identity || current.identity.disabled || deadline <= Date.now()) {
                if (!signal.aborted) { logout(); setAuthError(t('登录已失效，请重新连接。')); }
                throw Error(t('登录已失效，请重新连接。'));
            }
            saveSession(credential, deadline);
            return current;
        } catch (error) {
            if (!signal.aborted && error instanceof RequestError && [401, 403].includes(error.status)) { logout(); setAuthError(t('登录已失效，请重新连接。')); }
            throw error;
        }
    }, [credential, logout]);
    const personal = usePoll(load, retry, !!credential);
    const current = personal.data ?? me;
    const deadline = current?.credential ? Math.min(current.credential.expires_ms, current.credential.overlap_until_ms ?? Infinity) : undefined;
    useEffect(() => {
        if (!deadline) return;
        let timer: ReturnType<typeof setTimeout>;
        const check = () => {
            const remaining = deadline - Date.now();
            if (remaining <= 0) { logout(); setAuthError(t('登录已失效，请重新连接。')); }
            else timer = setTimeout(check, Math.min(remaining, 2147483647));
        };
        check();
        return () => clearTimeout(timer);
    }, [deadline, logout]);
    useEffect(() => {
        if (!menu) return;
        const close = (event: KeyboardEvent) => { if (event.key === 'Escape') { setMenu(false); document.querySelector<HTMLButtonElement>('.menu-toggle')?.focus(); } };
        addEventListener('keydown', close);
        return () => removeEventListener('keydown', close);
    }, [menu]);
    if (!credential) return <><div className="login-language"><LanguageSwitch/></div><Login notice={authError} connect={(c, m) => { saveSession(c, Math.min(m.credential?.expires_ms ?? 0, m.credential?.overlap_until_ms ?? Infinity)); setCredential(c); setMe(m); setAuthError(''); }}/></>;
    if (!current) return <main className="session-restore"><LanguageSwitch/><h1>{t('正在恢复登录…')}</h1><p>{t('正在向控制面核验当前标签页的登录凭据。')}</p>{personal.error && <><p role="alert">{personal.error}</p><button onClick={() => setRetry(v => v + 1)}>{t('重试')}</button></>}<button className="secondary" onClick={logout}>{t('返回登录')}</button></main>;
    const platformPages=['nodes','targets','audit','controlplane','infrastructure','secrets'];
    const navigation = sections.filter(s=>s[0]==='access'?current.capabilities.access:!platformPages.includes(s[0])||current.capabilities.platform);
    const pageAllowed=route.page==='access'?current.capabilities.access:!platformPages.includes(route.page)||current.capabilities.platform;
    return <div className={'layout ' + (menu ? 'nav-open' : '')}><aside id="console-navigation" className="sidebar"><a className="brand" href="#/workflows"><span className="brand-symbol">C</span> CONVERYN</a><nav aria-label={t('主导航')}>{[['工作台',['workflows','runs']],['基础设施',['nodes','infrastructure','targets']],['访问与系统',['secrets','access','audit','controlplane']]].map(([title,pages])=><div className="nav-group" key={String(title)}>{navigation.some(s=>(pages as string[]).includes(s[0]))&&<span className="nav-group-label">{t(String(title))}</span>}{navigation.filter(s=>(pages as string[]).includes(s[0])).map(([page,label,icon])=><a key={page} href={'#/'+page} aria-current={page===route.page?'page':undefined}><Icon name={icon}/>{t(label)}</a>)}</div>)}</nav><div className="sidebar-footer"><p>{current.identity?.name}<small>{current.capabilities.super_admin?t("超级管理员"):t("个人身份")}</small></p><small>{t('凭据有效至：')}{date(deadline)}</small><button className="secondary" onClick={logout}><Icon name="logout"/>{t('退出')}</button></div></aside>{menu && <button className="nav-backdrop" aria-label={t('关闭菜单')} onClick={() => setMenu(false)}/>}<div className="workspace"><header className="topbar"><button className="menu-toggle icon-button" onClick={() => setMenu(v => !v)} aria-label={t('菜单')} aria-expanded={menu} aria-controls="console-navigation"><Icon name={menu ? 'close' : 'menu'}/></button><div className="cluster-heading"><span className="connection-dot"/><span title={credential.cluster}>{credential.cluster}</span></div><div className="topbar-actions"><LanguageSwitch/><a className="identity-name" title={t('我的权限')} href="#/profile">{current.identity?.name}</a></div></header><main className="page-content"><VersionNotice/>{personal.error && <p role="alert" className="alert">{t('身份暂时无法核验，正在重试：')}{personal.error}</p>}{!pageAllowed ? <Empty title={t('需要管理员权限')}/> : <Pages key={route.page + '/' + (route.id ?? '') + '/' + (!route.id ? (location.hash.split('?')[1]??'') : '')} credential={credential} me={current} route={route}/>}</main></div></div>;
}
createRoot(document.getElementById('root')!).render(<App />);
