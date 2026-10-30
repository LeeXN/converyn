#!/usr/bin/env python3
"""Render field-level Markdown from the checked-in OpenAPI contract."""
from pathlib import Path
import json,re,argparse
parser=argparse.ArgumentParser();parser.add_argument('--check',action='store_true');args=parser.parse_args()
root=Path(__file__).resolve().parents[1];doc=json.loads((root/'docs/schema/openapi.json').read_text());schemas=doc['components']['schemas']
LANG = 'zh-CN'
def tr(zh, en): return en if LANG == 'en' else zh
def type_file(): return 'api-types.md'

def short(key):return key.rsplit('_',1)[-1]
def label(schema):
 if '$ref' in schema:
  key=schema['$ref'].rsplit('/',1)[-1];return f'[{short(key)}]({type_file()}#{key.lower()})'
 if 'const' in schema:return '`'+json.dumps(schema['const'],ensure_ascii=False)+'`'
 if 'oneOf' in schema or 'anyOf' in schema:return tr(' 或 ', ' or ').join(label(v) for v in schema.get('oneOf',schema.get('anyOf',[])))
 t=schema.get('type')
 if isinstance(t,list):return tr(' 或 ', ' or ').join(label({'type':v}) for v in t)
 if t=='array':return '['+', '.join(label(v) for v in schema['prefixItems'])+']' if 'prefixItems' in schema else label(schema.get('items',{}))+'[]'
 if t=='object':return 'map<string, '+label(schema['additionalProperties'])+'>' if isinstance(schema.get('additionalProperties'),dict) else 'object'
 return {'string':'string','integer':'integer','number':'number','boolean':'boolean','null':'null'}.get(t,tr('任意 JSON', 'any JSON'))
def fields(schema):
 while '$ref' in schema:schema=schemas[schema['$ref'].rsplit('/',1)[-1]]
 return schema

def permission(path, method):
 if path.startswith('/internal/'):return tr('仅控制器 peer 身份，协议和插件清单必须匹配。','Controller peer identity with matching protocol and plugin manifest.')
 if '/prototype/' in path:return tr('仅回环开发测试接口。','Loopback development interface only.')
 if path in ['/','/assets/app.js','/assets/index.css','/assets/version.json','/health/live','/openapi.json']:return tr('HTTPS 监听仍检查客户端证书。','The HTTPS listener still checks the client certificate.')
 if path=='/api/v1/identity/enroll':return tr('一次性节点引导凭据。','One-use Node enrollment credentials.')
 if path.endswith('/inputs'):return tr('仅对应 Node 的当前身份与已提交 grant；管理员不能读取明文。响应禁止缓存。','Only the matching current Node identity and committed grant; human administrators cannot read plaintext. Responses are not cacheable.')
 if '/agents/' in path and '/artifacts/' in path:return tr('仅对应 Node 的当前身份及本节点进行中的 Run 分配；支持断点续传。','Only the current Node identity and its ongoing Run assignment; supports byte-range resume.')
 if path.startswith('/api/v1/artifacts'):return tr('平台运维权限；上传文件在所有当前投票成员核验落盘后登记。','Platform-operator permission; files are registered after durable verification on all current voters.')
 if '/agents/' in path:return tr('对应节点身份；管理身份可读取诊断信息，写入仍要求节点证明。','Corresponding Node identity; administrators may read diagnostics, but writes require Node proof.')
 if path in ['/api/v1/identities','/api/v1/credentials']:return tr('具名管理员；引导凭据仅能创建首个管理员。返回内容不包含私钥或令牌。','Named administrator; bootstrap credentials may only create the first administrator. Responses never expose private keys or tokens.')
 if method=='post' and path.startswith('/api/v1/runs/') and (path.endswith('/pause') or path.endswith('/cancel')):return tr('任务提交者或管理员，服务端和状态机检查实际身份。','The submitter or an administrator; the server and state machine check the authenticated identity.')
 if (method=='get' and (path.startswith('/api/v1/workflows') or path.startswith('/api/v1/runs') or path.startswith('/api/v1/nodes/') or path in ['/api/v1/executions','/api/v1/audit','/api/v1/nodes','/api/v1/targets','/api/v1/plugins','/api/v1/identity/me'])) or path.endswith('/preview') or path.endswith('/runs'):return tr('已启用的执行者或管理员；目标和参数由已登记流程限定，无人工审批。','An enabled executor or administrator; registered workflows constrain targets and parameters, with no approval step.')
 return tr('管理员。','Administrator.')

def table(schema):
 schema=fields(schema)
 if 'oneOf' in schema:
  parts=[]
  for variant in schema['oneOf']:
   value=fields(variant);tags=[(k,v['const']) for k,v in value.get('properties',{}).items() if 'const' in v]
   parts.append((tr('区分字段 ', 'Discriminator ')+', '.join(f'`{k}={v}`' for k,v in tags)+'\n\n' if tags else label(variant)+'\n\n')+table(value) if value.get('type')=='object' else label(value)+'\n\n')
  return ''.join(parts)
 if schema.get('type')!='object' or not schema.get('properties'):return tr('类型：', 'Type: ')+label(schema)+'.\n\n'
 out=[tr('| 字段 | JSON 类型 | 必填 / 默认 |', '| Field | JSON type | Required / default |')+'\n| --- | --- | --- |\n']
 for key,value in schema['properties'].items():
  required=tr('必填', 'required') if key in schema.get('required',[]) else tr('可省略', 'optional')
  if 'default' in value:required+=tr('；默认 `', '; default `')+json.dumps(value['default'],ensure_ascii=False)+'`'
  out.append(f'| `{key}` | {label(value)} | {required} |\n')
 if schema.get('additionalProperties') is False:out.append(tr('\n拒绝额外字段。\n', '\nAdditional fields are rejected.\n'))
 return ''.join(out)+'\n'
head='# Converyn HTTP API\n\n[English](../../en/reference/api-reference.md) | [简体中文](../../zh-CN/reference/api-reference.md)\n\n依据 [OpenAPI](../../schema/openapi.json) 生成。完整结构见[字段表](api-types.md)，操作模型见[资源说明](resources.md)，执行路径见[使用指南](../how-to/runs.md)，流程定义见[编写指南](../how-to/workflows.md)。\n\n## 身份与执行\n\n业务 API 使用 `/api/v1`。HTTPS 使用客户端证书，并要求 `Authorization: Bearer <个人令牌>` 与 `x-converyn-cluster: <控制面名称>`。新人员默认无业务权限；平台角色与流程授权决定操作能力及流程／预设范围。任务、日志及归档沿用原执行的授权范围。详见[人员与流程授权](../how-to/access.md)。无密码登录和人工审批。\n\nOP 在环境文件或 Git 检出目录维护流程，通过 CLI 登记不可变快照。执行只接收流程修订、预设目标、开放参数及请求 ID。服务端编译 Plan，提交时检查实际摘要和目标快照。旧 catalog、审批、脚本上传和直接 Plan 上传入口已移除。\n\n预览不是执行。提交成功返回 Run ID，使用原 Run 查询实际终态和回执。参数仅影响本次执行；显式启用持续核对时沿用该次快照。网络重试复用原请求 ID，不另建任务代替未知结果处置。\n\n## 一致性、限制和日志\n\n普通读取确认多数派，Follower 转发保留原始身份。业务变更与命令审计同事务提交；审计自动记录人员、证书指纹、流程修订和结果，不是全部 HTTP 访问日志。\n\n请求体最多 4 MiB，流程包最多 1 MiB，单定义最多 64 KiB，最终开放参数快照最多 256 KiB。流程请求 ID 最多 124 字符。`*_ms` 为 Unix 毫秒，`*_seconds` 为秒。\n\n错误包含 code、message、retryable、request_id。429 应退避；503 或断连可能是提交结果未知，应先查询原请求再重试。日志分块为 64 KiB；SSE 和下载持续复核身份。原始日志独立于审计和备份存储。\n\n## 路由\n\n'
en_head='# Converyn HTTP API\n\n[English](../../en/reference/api-reference.md) | [简体中文](../../zh-CN/reference/api-reference.md)\n\nGenerated from [OpenAPI](../../schema/openapi.json). See [types](api-types.md), [resource responsibilities](resources.md), [usage](../how-to/runs.md) and [workflow authoring](../how-to/workflows.md).\n\n## Identity and execution\n\nBusiness APIs use `/api/v1`, client certificates, `Authorization: Bearer <personal token>` and `x-converyn-cluster`. New people have no business grants. Platform roles and workflow/preset bindings determine capabilities. Runs, logs and archives follow their original execution scope. See [workflow access](../how-to/access.md). There is no password login or approval step.\n\nOP maintains source files or Git checkouts and registers immutable snapshots with CLI. Execution accepts a workflow revision, preset target, declared overrides and request ID; the server compiles the Plan and rechecks its digest and target snapshot. Legacy catalog, approval, script upload and direct Plan upload endpoints are removed.\n\nPreview is not execution. Follow the returned Run and actual receipts. Overrides apply to this execution; opted-in reconciliation retains its snapshot. Retries reuse the original request ID.\n\n## Consistency, limits and logs\n\nReads confirm quorum; followers preserve the authenticated identity. Business changes and command audit commit together. Audit automatically records the person, credential, workflow revision and outcome; it is not a complete HTTP access log.\n\nBodies are limited to 4 MiB, workflow packages to 1 MiB, definitions to 64 KiB and final parameter snapshots to 256 KiB. Workflow request IDs have at most 124 characters. `_ms` uses Unix milliseconds; `_seconds` uses seconds.\n\nErrors contain code, message, retryable and request_id. Back off on 429. After 503 or disconnection, query the original request before retrying. Logs use 64 KiB chunks. Streaming reads revalidate identity. Raw logs are separate from audit and logical backups.\n\n## Routes\n\n'
for LANG in ['en', 'zh-CN']:
 route_lines=[en_head if LANG == 'en' else head]
 for path,methods in doc['paths'].items():
  for method,op in methods.items():
   route_lines += [f'### `{method.upper()} {path}`\n\n',permission(path,method)+'\n\n']
   if op.get('tags')==['internal']:route_lines += [tr('内部协议或受限本地测试接口；访问边界见本接口权限。', "Internal protocol or restricted local test interface; see this route's access rules.")+'\n\n']
   params=op.get('parameters',[])
   if params:
    route_lines += [tr('| 参数 | 位置 | 类型 | 要求 |','| Parameter | Location | Type | Requirement |')+'\n| --- | --- | --- | --- |\n']
    for v in params:route_lines += [f'| `{v["name"]}` | {v["in"]} | {label(v["schema"])} | '+(tr('必填','required') if v.get('required') else tr('可选','optional'))+' |\n']
    route_lines += ['\n']
   body=op.get('requestBody')
   if body:
    route_lines += [tr('请求体：','Request body:')+'\n\n']
    for media,value in body['content'].items():
     route_lines += [tr('媒体类型','Media type')+' `'+media+'`.\n\n',table(value['schema']) if media=='application/json' else tr('二进制分块；按声明的长度和偏移上传。','Binary chunk; upload using the declared length and offset.')+'\n\n']
   else:route_lines += [tr('无JSON请求体。','No JSON request body.')+'\n\n']
   for status,response in op.get('responses',{}).items():
    if not status.startswith('2'):continue
    route_lines += [tr('成功响应 HTTP ','Successful response HTTP ')+status+':\n\n']
    for media,value in response.get('content',{}).items():
     route_lines += [tr('媒体类型','Media type')+' `'+media+'`.\n\n',table(value['schema']) if media=='application/json' else tr('流式/文本返回，按Content-Type读取。','Stream/text response; honor Content-Type.')+'\n\n']
 route_lines += [tr('## 维护与验证','## Maintenance')+'\n\n`python3 tools/generate_api.py`\n\n`python3 tools/render_api_docs.py --check`\n']
 title=tr('# Converyn API 完整字段表','# Converyn API type reference')
 type_lines=[title+'\n\n[English](../../en/reference/api-types.md) | [简体中文](../../zh-CN/reference/api-types.md)\n\n'+tr('由 OpenAPI 契约生成。各类型以源模块和名称标识，列出字段、JSON 类型、必填项和默认值。路由权限见 [API 参考](api-reference.md)。','Generated from OpenAPI. Types are identified by source module and name, with fields, JSON types, required values and defaults. See the [API reference](api-reference.md) for route authorization.')+'\n\n']
 for key,schema in sorted(schemas.items()):type_lines += [f'<a id="{key.lower()}"></a>\n\n### {short(key)}\n\n`{key}`\n\n',table(schema)]
 for name,content in [('api-reference.md',''.join(route_lines)),('api-types.md',''.join(type_lines))]:
  path=root/'docs'/LANG/'reference'/name
  if args.check:assert path.read_text()==content, name+' is stale'
  else:path.write_text(content)
print(f'Rendered bilingual documentation: {sum(len(v) for v in doc["paths"].values())} operations and {len(schemas)} field tables per language')
