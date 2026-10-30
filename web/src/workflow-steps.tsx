import { useEffect, useState } from 'react';
import { api, type Credential } from './api';
import type { crates_core_model_domain_Step as Step, crates_core_orchestration_workflow_Parameter as Parameter } from './api-types';
import { Fields } from './ui';
import { Dialog, CodeBlock } from './dialog';
import { t, term } from './i18n';

export function DefinitionCode({ content, sha, kind, credential, run, work, inline = false }: {
  content?: string; sha: string; kind: string; credential?: Credential; run?: string; work?: string; inline?: boolean;
}) {
  const [open, setOpen] = useState(inline), [loaded, setLoaded] = useState<string>(), [error, setError] = useState(''), [retry, setRetry] = useState(0);
  const label = t(run && kind === 'exec' ? '本次执行脚本（含变量）' : kind === 'exec' ? '原始脚本' : '配置文件');
  useEffect(() => {
    if (!open || content !== undefined || loaded !== undefined || !credential || !run || !work) return;
    const abort = new AbortController(); setError('');
    void api<{ sha256: string; content: string; rendered_content: string }>(credential,
      '/runs/' + encodeURIComponent(run) + '/work/' + encodeURIComponent(work) + '/definition', undefined, abort.signal)
      .then(value => { if (abort.signal.aborted) return; if (value.sha256 !== sha) throw Error(t('定义摘要不匹配')); setLoaded(value.rendered_content); })
      .catch(e => { if (!abort.signal.aborted) setError(String(e)); });
    return () => abort.abort();
  }, [open, content, loaded, credential, run, work, sha, retry]);
  const body = <><p className="definition-digest">{label} · SHA-256 · {sha}</p>{content !== undefined || loaded !== undefined
    ? <CodeBlock content={content ?? loaded ?? ''}/>
    : error ? <div role="alert" className="alert">{error}<button className="secondary" onClick={() => setRetry(v => v + 1)}>{t('重试')}</button></div>
    : <p role="status">{t(credential && run && work ? '读取正文…' : '未提供脚本正文')}</p>}</>;
  return inline ? body : <><button className="secondary" type="button" onClick={() => setOpen(true)}>{t(kind === 'exec' ? '查看脚本' : '查看配置文件')}</button>{open && <Dialog wide title={label} onClose={() => setOpen(false)}>{body}</Dialog>}</>;
}

const stageNames: Record<string, string> = {
  '00-preflight': '主机检查', '01-runtime': '运行时与配置', '02-control-plane': '初始化控制面',
  '02b-secondary-control-planes': '加入其他控制节点', '03-workers': '加入工作节点',
  '04-calico': '容器网络', '05-storage': '持久存储', '06-cert-manager': '证书服务',
  '07-ingress': '入口服务', '08-harbor': '镜像仓库', '09-monitoring': '监控', '10-verify': '功能验收',
};
export function WorkflowSteps({ steps, definitions = {}, parameters = {}, values = {}, provenance = {} }: {
  steps: Step[]; definitions?: Record<string, string>; parameters?: Record<string, Parameter>;
  values?: Record<string, unknown>; provenance?: Record<string, string>;
}) {
  const [selected, setSelected] = useState<Step>();
  let groups: { id: string; steps: string[] }[] = [];
  try {
    const parsed: unknown = JSON.parse(provenance.step_groups ?? 'null');
    if (Array.isArray(parsed) && parsed.every(g => typeof g?.id === 'string' && Array.isArray(g.steps) && g.steps.every((s: unknown) => typeof s === 'string'))) groups = parsed;
  } catch { /* Stage groups are optional. */ }
  const known = new Set(groups.flatMap(g => g.steps));
  const rest = steps.filter(s => !known.has(s.id));
  if (rest.length) groups.push({ id: '', steps: rest.map(s => s.id) });
  const byId = new Map(steps.map(s => [s.id, s]));
  const op = selected?.operation, sha = op?.kind === 'exec' ? op.script_sha256 : op?.kind === 'file' ? op.sha256 : undefined;
  const parameter = selected && Object.entries(parameters).find(([, p]) => p.bindings.some(b => b.step_id === selected.id && b.path === '/content'));
  const content = parameter && typeof values[parameter[0]] === 'string' ? String(values[parameter[0]]) : sha ? definitions[sha] : undefined;
  return <section className="workflow-steps"><header className="section-heading"><h3>{t('流程步骤')}</h3><span className="status-chip">{steps.length}</span></header><p className="muted">{t('无需执行预览即可查看已登记步骤和原始脚本。本次变量渲染结果请在执行预览中查看。')}</p>{groups.map(group =>
    <details className="step-group" key={group.id || 'steps'} open={groups.length === 1}>
      <summary>{stageNames[group.id] ? t(stageNames[group.id]) : group.id || t('执行步骤')} <span className="muted">· {group.steps.length}</span></summary>
      <ol className="step-list">{group.steps.map(id => {
        const step = byId.get(id); if (!step) return null;
        return <li className="workflow-step" key={id}><button type="button" className="step-trigger" onClick={() => setSelected(step)}><span><strong>{step.id}</strong><small>{term(step.operation.kind)}{'path' in step.operation ? ' · ' + step.operation.path : ''}</small></span><span>{t('查看详情')} →</span></button></li>;
      })}</ol>
    </details>)}
    {selected && op && <Dialog wide title={t('步骤详情') + ' · ' + selected.id} onClose={() => setSelected(undefined)}>
      {sha && <DefinitionCode key={selected.id} content={content} sha={sha} kind={op.kind} inline/>}
      <details className="record-details" open={!sha}><summary>{t('步骤信息')}</summary>
      <Fields value={{ phase: selected.phase, scope: selected.scope, hosts: selected.hosts, node_order: selected.node_order, max_parallel: selected.max_parallel, ...('timeout_seconds' in op ? { timeout_seconds: op.timeout_seconds } : {}) }}/>
      {Object.keys(selected.inputs ?? {}).length > 0 && <Fields value={{ inputs: selected.inputs, resolution: t('授权前固定实际输入') }}/>}
      {Object.keys(selected.outputs ?? {}).length > 0 && <Fields value={{ outputs: selected.outputs }}/>}
      </details>
      <details className="record-details"><summary>{t('操作配置')}</summary><Fields value={op}/></details>
    </Dialog>}
  </section>;
}
