import { Badge, Details, Empty, Fields } from './ui';
import { t } from './i18n';

type LogId = { index?: number; leader_id?: { term?: number; node_id?: number } };
type MemberStatus = {
  config_fingerprint?: string;
  voters?: number[];
  membership?: { log_id?: LogId; membership?: { nodes?: Record<string, { addr?: string }>; configs?: number[][] } };
  replication?: Record<string, LogId | null> | null;
  operations?: { active?: string | null; operations?: Record<string, { condition: string; diagnostic?: string; revision?: number; intent?: unknown }> };
};
export function ControlPlane({ value }: { value: MemberStatus }) {
  const nodes = Object.entries(value.membership?.membership?.nodes ?? {});
  const operations = Object.entries(value.operations?.operations ?? {});
  return <><div className="stat-grid">
    <div><span>{t('成员数量')}</span><strong>{nodes.length}</strong></div>
    <div><span>{t('投票成员')}</span><strong>{value.voters?.length ?? 0}</strong></div>
    <div><span>{t('成员配置索引')}</span><strong>{value.membership?.log_id?.index ?? '—'}</strong></div>
    <div><span>{t('成员变更')}</span><strong>{value.operations?.active ? t('进行中') : t('无进行中的变更')}</strong></div>
  </div><section className="panel"><h2>{t('控制面成员')}</h2>{nodes.length ? <div className="table-wrap"><table><thead><tr><th>{t('成员编号')}</th><th>{t('地址')}</th><th>{t('角色')}</th><th>{t('已复制日志索引')}</th></tr></thead><tbody>{nodes.map(([id, node]) => <tr key={id}><td>{id}</td><td>{node.addr ?? '—'}</td><td>{value.voters?.includes(Number(id)) ? t('投票成员') : t('学习成员')}</td><td>{value.replication?.[id]?.index ?? t('未报告')}</td></tr>)}</tbody></table></div> : <Empty/>}<p className="muted">{t('成员配置和复制进度不等于节点健康探测；未报告表示当前响应没有该成员的复制数据。')}</p><Details label={t('配置指纹')} value={{ config_fingerprint: value.config_fingerprint }}/></section><section className="panel"><h2>{t('成员变更记录')}</h2>{operations.length ? operations.map(([id, operation]) => <article key={id}><h3>{id}</h3><Badge value={operation.condition}/>{operation.diagnostic && <p>{operation.diagnostic}</p>}<Details value={operation.intent}/></article>) : <Empty title={t('暂无成员变更')}/>}</section></>;
}
export type TargetPreset = { hosts?: string | null; node_group_uid?: string | null; external_target_uid?: string | null; node_uids?: string[]; roles?: string[]; labels?: Record<string, string> };
export type WorkflowInventory = { nodes: { uid: string; address: string; hostname: string }[]; groups: Record<string, string[]> };
export function TargetLocation({ preset }: { preset: TargetPreset }) {
  return <>{preset.hosts && <span className="target-location">{t('流程选择器')} · {preset.hosts}</span>}{preset.node_group_uid && <span className="target-location" title={preset.node_group_uid}>{t('平台节点组')} · {preset.node_group_uid}</span>}{preset.external_target_uid && <span className="target-location" title={preset.external_target_uid}>{t('外部目标')} · {preset.external_target_uid}</span>}</>;
}
export function PresetSummary({ preset, inventory }: { preset?: TargetPreset; inventory?: WorkflowInventory | null }) {
  if (!preset) return null;
  const fields = Object.fromEntries(Object.entries(preset).filter(([, value]) => value != null && (Array.isArray(value) ? value.length > 0 : typeof value === 'object' ? Object.keys(value).length > 0 : true)));
  return <section className="preset-summary"><h3>{t('本次执行目标')}</h3><Fields value={fields}/>{preset.hosts && inventory && <><p>{t('流程分组独立于平台节点组。实际执行节点和顺序以预览为准。')}</p><div className="table-wrap"><table><thead><tr><th>{t('流程分组')}</th><th>{t('节点顺序')}</th></tr></thead><tbody>{Object.entries({ all: inventory.nodes.map(n => n.uid), ...inventory.groups }).map(([group, nodes]) => <tr key={group}><td>{group}</td><td>{nodes.join(' → ') || t('空')}</td></tr>)}</tbody></table></div><p className="muted">{t('平台节点组限定执行范围；Secret 在控制面录入一次，按任务交付。')}</p></>}</section>;
}
export function Targets({ targets }: { targets: Record<string, unknown> }) {
  return <><section className="panel target-guide" aria-labelledby="target-guide-title">
    <h2 id="target-guide-title">{t('把应用发布到已有的 Kubernetes 集群')}</h2>
    <p>{t('外部目标用于标识一个已有的平台，并引用它的连接配置。选择绑定该目标的流程预设后，控制器通过插件调用平台 API 完成发布和检查。')}</p>
    <div className="execution-paths">
      <article><h3>{t('在 VM 或物理机上执行')}</h3><p>{t('写配置、管理服务、运行脚本，由主机上的 Converyn Agent 执行。')}</p><p className="execution-route">{t('目标预设 → 流程 inventory → 平台节点组范围核验 → Agent')}</p></article>
      <article><h3>{t('向 Kubernetes 发布应用')}</h3><p>{t('创建或更新 Deployment、检查发布状态，由控制器调用 Kubernetes API 执行。')}</p><p className="execution-route">{t('目标预设 → 外部目标 → Kubernetes API')}</p></article>
    </div>
    <details className="target-setup"><summary>{t('什么时候需要配置外部目标？')}</summary><p>{t('已有 Kubernetes 集群，并希望通过流程发布应用时，由运维人员完成以下配置：')}</p><ol>
      <li>{t('准备平台连接配置，登记外部目标及其身份。')}</li>
      <li>{t('在流程定义中，把“测试”或“生产”等目标预设绑定到该外部目标。')}</li>
      <li>{t('执行人员选择预设、预览计划并执行，控制器调用平台 API 完成操作。')}</li>
    </ol><p>{t('例如：生产预设 → 生产 Kubernetes 目标 → 该集群的 API。目标登记和访问凭据由运维人员维护。')}</p></details>
  </section>{Object.keys(targets).length ? <><h2 className="registered-targets-title">{t('已登记的外部目标')}</h2><div className="target-grid">{Object.entries(targets).map(([id, target]) => <section className="panel" key={id}><h2>{id}</h2><Fields value={target}/></section>)}</div></> : <Empty title={t('尚未登记外部目标')}><span>{t('这里为空不影响主机流程执行。主机流程通过 inventory 选择节点并绑定平台节点组；需要向 Kubernetes 发布应用时，请先由运维人员登记外部目标并绑定流程。')}</span><a className="empty-link" href="#/workflows">{t('查看流程')}</a></Empty>}</>;
}
