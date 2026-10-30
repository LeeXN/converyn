import { t } from './i18n';
export type Credential = { token: string; cluster: string };
export type ApiError = { code: string; message: string; retryable: boolean; request_id: string };
export class RequestError extends Error {
  constructor(
    public status: number,
    public detail: ApiError,
  ) {
    super(
      t(({log_storage_disabled:'未配置集中日志存储，可查看回执日志尾部。',log_storage_unavailable:'集中日志存储暂不可用，正在等待恢复。',log_store_identity_changed:'日志存储身份与任务记录不一致，请联系运维人员核对存储。',log_chunk_checksum_mismatch:'日志校验失败，已停止读取，请联系运维人员核对存储。',log_chunk_missing:'日志分块暂时缺失，正在等待恢复。',node_still_in_use:'节点仍属于节点组或被未结束任务使用，请先解除使用关系。',scope_invalid:'节点组范围已失效，请修复范围并重新预览提交。',node_group_members_in_use:'所需成员仍被执行任务占用，暂停或取消处理中也会保留占用。',node_group_referenced_by_workflow:'当前流程仍引用该节点组，请先调整或移除目标绑定。',node_group_revision_conflict:'节点组已更新，请刷新修订后重试。',last_super_admin_cannot_be_removed: '不能移除最后一位超级管理员。',last_admin_cannot_be_disabled: '不能移除最后一位超级管理员。',group_has_bindings:'该用户组仍有授权，请先撤销相关授权。',last_super_admin_credential_cannot_be_revoked:'最后一份有效的超级管理员凭据不能撤销。'} as Record<string,string>)[detail.code] ?? ({401: '登录凭据无效或已失效。', 403: '没有执行此操作的权限。', 404: '请求的资源不存在。', 409: '数据已变化，请刷新后重试。', 422: '请求参数不符合要求。', 429: '请求过于频繁，请稍后重试。', 503: '控制面暂不可用，正在等待恢复。'} as Record<number, string>)[status] ?? '请求未完成，请根据结果代码检查配置或联系运维人员。') +
        ([409,422].includes(status) && detail.message ? ' · '+detail.message : '') +
        ' [' +
        detail.code +
        ']' +
        (detail.request_id ? ' · ' + detail.request_id : '') + (status===403 ? ' · ' + t('请检查流程授权与预设范围，或联系授权管理员。') : ''),
    );
  }
}
export const headers = (c: Credential) => ({
  Authorization: 'Bearer ' + c.token,
  'x-converyn-cluster': c.cluster,
  'Content-Type': 'application/json',
});
export async function api<T>(
  c: Credential,
  path: string,
  body?: unknown,
  signal?: AbortSignal,
): Promise<T> {
  const timeout = AbortSignal.timeout(15000);
  const response = await fetch('/api/v1' + path, {
    method: body === undefined ? 'GET' : 'POST',
    headers: headers(c),
    body: body === undefined ? undefined : JSON.stringify(body),
    signal: signal ? AbortSignal.any([signal, timeout]) : timeout,
    cache: 'no-store',
  });
  const text = await response.text();
  let value: unknown;
  try {
    value = text ? JSON.parse(text) : undefined;
  } catch {
    if (response.ok) throw Error(t("服务器返回了无法解析的数据，请重试读取。"));
  }
  if (!response.ok) {
    throw requestError(response.status, value, response.headers.get('x-request-id') ?? '');
  }
  return value as T;
}

/** HTTP and SSE share the same diagnostics and retry policy. */
export function requestError(status: number, value: unknown, requestId = ''): RequestError {
  const v = value && typeof value === 'object' ? value as Partial<ApiError> & { error?: string } : {};
  return new RequestError(status, {
    code: v.code ?? v.error ?? 'http_' + status,
    message: v.message ?? t('请求失败 (HTTP ') + status + ')',
    retryable: v.retryable ?? [408, 429, 502, 503, 504].includes(status),
    request_id: v.request_id ?? requestId,
  });
}
export async function responseError(response: Response): Promise<RequestError> {
  let value: unknown;
  try { value = await response.json(); } catch { /* Proxies may return a plain error page. */ }
  return requestError(response.status, value, response.headers.get('x-request-id') ?? '');
}
