import { useEffect, useState } from 'react';
import type {
  crates_core_model_domain_Work as Work,
  crates_core_orchestration_machine_types_Attempt as Attempt,
} from './api-types';
import { Badge, Details, Fields } from './ui';
import { LogViewer } from './log-viewer';
import type { Credential } from './api';
import { t } from './i18n';
import { elapsedMilliseconds, durationText, isTerminal } from './run-time';
import { DefinitionCode } from './workflow-steps';

export function Elapsed({ submitted, finished, status }: { submitted?: number | null; finished?: number | null; status: string }) {
  const [now, setNow] = useState(Date.now());
  const running = !isTerminal(status) && finished == null;
  useEffect(() => {
    if (!running) return;
    setNow(Date.now());
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, [running]);
  const elapsed = elapsedMilliseconds(submitted, finished, status, now);
  return <span className="elapsed">{durationText(elapsed)}{running && elapsed !== undefined ? ' · ' + t('进行中') : ''}</span>;
}

export function WorkDetails({ credential, run, work, attempt, tab, onTab }: { tab: string; onTab: (tab: string) => void; credential: Credential; run: string; work: Work; attempt?: Attempt }) {
  const [copied, setCopied] = useState('');
  const effective = attempt?.materialized?.work ?? work;
  const noLog = Object.values(work.outputs ?? {}).some(output => output.secret);
  const results = Object.fromEntries(Object.entries(attempt?.receipt?.outputs ?? {}).map(([name, output]) => [name, output.kind === 'secret' ? t('敏感输出，仅供已授权步骤使用') : output.value]));
  const summary = Object.fromEntries(Object.entries(effective.operation).filter(([key])=>!['environment','script_sha256','sha256','artifact'].includes(key)));
  const environment = 'environment' in effective.operation ? effective.operation.environment : undefined;
  const receipt = attempt?.receipt;
  const native = receipt?.native;
  const { operation: _operation, ...evidence } = native ?? {};
  async function copy() {
    try { await navigator.clipboard.writeText(work.id); setCopied('已复制'); }
    catch { setCopied('复制失败，请手动复制编号'); }
  }
  return <><div className="tabs" role="tablist" aria-label={t('步骤详情')}>{(['result','logs','script'] as const).map((value, i) => <button role="tab" key={value} aria-selected={tab === value} onClick={() => onTab(value)}>{t(['执行结果','日志','脚本'][i])}</button>)}</div>
    <section hidden={tab !== 'result'} className="panel work-detail" id="work-detail" tabIndex={-1} aria-labelledby="work-detail-title">
      <header className="work-detail-heading"><h2 id="work-detail-title">{work.step_id} / {work.node_uid}</h2><Badge value={attempt?.status ?? 'pending'}/></header>
      <div className="work-evidence-grid">
        <section><h3>{t('执行内容')}</h3><Fields value={summary}/>{environment&&<Details label={t("本次执行变量")} value={environment}/>}</section>
        <section><h3>{t('执行结果')}</h3>
          <Fields value={{ status: attempt?.status ?? 'pending', ...(receipt ? { outcome: receipt.outcome, ...(receipt.exit_code != null ? { exit_code: receipt.exit_code } : {}) } : {}), ...(attempt?.reason ? { reason: attempt.reason } : {}), ...(receipt?.diagnostic ? { diagnostic: receipt.diagnostic } : {}) }}/>
          {native && <Fields value={Object.fromEntries(Object.entries(evidence).filter(([, value]) => value != null))}/>}
          {receipt?.execution && <Details label={t("执行身份")} value={{ execution: receipt.execution }}/>}
          {!receipt && <p className="muted">{t('尚未收到回执')}</p>}
        </section>
      </div>
      {Object.keys(work.inputs ?? {}).length > 0 && <section><h3>{t('步骤输入')}</h3><Fields value={{ references: work.inputs, input_sha256: attempt?.materialized?.input_sha256 ?? t('等待前序输出') }}/></section>}
      {Object.keys(work.outputs ?? {}).length > 0 && <section><h3>{t('步骤输出')}</h3><Fields value={Object.keys(results).length ? results : { status: t('尚未产生输出') }}/></section>}
      <div className="work-id"><span>{t('工作项编号')} <code>{work.id}</code></span><button className="secondary" onClick={() => void copy()}>{t('复制工作项编号')}</button><span role="status">{copied && t(copied)}</span></div>
      <Details label={t('技术详情')} value={{ work, attempt }}/>
      {receipt && <details className="record-details"><summary>{t('原始回执')}</summary><pre>{JSON.stringify(receipt, null, 2)}</pre></details>}
    </section>
    {tab === 'script' && <section className="script-panel">      {(effective.operation.kind === 'exec' || effective.operation.kind === 'file') && <DefinitionCode
        credential={credential} run={run} work={work.id} kind={effective.operation.kind} inline
        sha={effective.operation.kind === 'exec' ? effective.operation.script_sha256 : effective.operation.sha256}/>}
{effective.operation.kind !== 'exec' && effective.operation.kind !== 'file' && <p className="muted">{t('此步骤使用原生操作，没有脚本正文。')}</p>}</section>}
    {tab === 'logs' && (noLog ? <section className="panel"><h3>{t('日志')}</h3><p>{t('此步骤产生敏感输出，不公开标准输出或错误日志。')}</p></section> : effective.operation.kind === 'exec' || receipt?.logs
      ? !attempt?.grant && !receipt ? <p className="muted" role="status">{t('步骤尚未开始，等待执行授权后提供日志。')}</p> : <LogViewer credential={credential} run={run} work={work.id} title={work.step_id + ' / ' + work.node_uid} receipt={receipt ?? undefined}/>
      : <section className="panel"><h3>{t('日志')}</h3><p className="muted">{t('此步骤使用原生回执提供执行证据，无脚本输出。')}</p></section>)}
  </>;
}
