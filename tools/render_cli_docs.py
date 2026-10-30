#!/usr/bin/env python3
"""Generate bilingual CLI option references from the selected binary's help."""
import argparse
import os
from pathlib import Path
import re
import subprocess

ZH = {'delete': '按修订和原因删除未被引用的节点组。', 'get': '统一查询资源；--local 查看本机内置插件。',
 'status': '显示实例与业务概况。',
 'validate': '校验本地流程源文件。',
 'register': '登记流程快照及其本地制品，或单独上传制品。',
 'create': '创建身份或签发凭据。',
 'update': '提交身份修改或类型化管理变更。',
 'enable': '启用流程或身份。',
 'disable': '停用流程或身份。',
 'rotate': '轮换个人凭据。',
 'revoke': '撤销个人凭据。',
 'export': '导出公有证书、CA 或私有凭据。',
 'replace': '提交带审计证据的成员替换。',
 'retire': '核对指纹与证据后退役 Node 身份。',
 'run': '执行流程；默认提交返回，--wait 等待，--dry-run 只预览。',
 'watch': '等待 Run 的实际证据并返回结果退出码。',
 'logs': '读取、跟踪或导出 Run 日志。',
 'cancel': '请求取消 Run。',
 'pause': '暂停 Run 的新授权。',
 'resume': '恢复 Run 授权或继续原凭据操作。',
 'resolve': '携带证据处置不确定 Work。',
 'compile': '离线编译 PlanInput 或插件固定调用。',
 'prepare': '生成本地插件私有输入。',
 'expand': '将插件展开为本地 Playbook 与脚本。',
 'config': '导入、选择、查看和导出个人 context。',
 'auth': '查询权限、管理授权及生成 GUI 登录指引。',
 'admin': '安装、初始化、备份恢复及平台维护。',
 'task': '流程脚本输入、输出和上下文工具。',
 'server': '启动控制器。',
 'agent': '启动自主 Agent。'}


def visible_commands(text):
    """Discover command rows with or without a help summary."""
    return [(name, description or '') for name, description in
            re.findall(r'^  ([a-z][a-z-]+)(?:[ \t]{2,}(.*))?$', text, re.M)
            if name != 'help']


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--binary', type=Path, required=True)
    p.add_argument('--check', action='store_true')
    a = p.parse_args()
    env = dict(os.environ)
    for variable in ['CONVERYN_TOKEN','CONVERYN_CONFIG','CONVERYN_AUTHORITY','CONVERYN_REQUEST_DIR']:
        env.pop(variable, None)
    def help_text(*command):
        text = subprocess.check_output([str(a.binary.resolve()), *command, '--help'],
                                       env=env, text=True)
        return '\n'.join(line.rstrip() for line in text.replace(a.binary.name, 'converyn').splitlines()) + '\n'
    root = Path(__file__).resolve().parents[1]
    top = help_text()
    entries = visible_commands(top)
    assert {name for name, _ in entries} <= set(ZH), 'update descriptions for changed commands'
    for lang in ['en', 'zh-CN']:
        zh = lang == 'zh-CN'
        parts = [('# CLI 使用参考' if zh else '# CLI reference') + '\n\n',
                 '[English](../../en/reference/cli.md) | [简体中文](../../zh-CN/reference/cli.md)\n\n',
                 ('选项由真实二进制的 `--help` 生成。客户端按 `--config` → `CONVERYN_CONFIG` → `~/.converyn/config.json` 选择文件；`--context` 临时覆盖当前 context。个人配置内嵌证书、私钥和令牌，文件需为 0600。客户端只接受当前便携 context 配置；server/agent/admin renew 使用各自显式指定的服务配置。详见[快速开始](../how-to/installation.md)。\n\n'
                  if zh else 'Options are generated from the actual binary. Clients select `--config`, then `CONVERYN_CONFIG`, then `~/.converyn/config.json`; `--context` temporarily overrides the saved selection. Personal configurations embed certificates, keys and tokens and require mode 0600. Clients accept portable context profiles. Server/agent/admin renew use explicit, separate service configurations. See [getting started](../how-to/installation.md).\n\n'),
                 ('常用命令、参数覆盖、重试、退出码和日志操作见[使用指南](../how-to/runs.md)，流程定义与脚本约定见[编写指南](../how-to/workflows.md)。以下仅维护完整命令与选项。默认输出表格；程序解析请用 `-o json`，`--output` 仍表示文件路径。资源操作统一使用“动词 + 资源”；维护操作位于 `admin`。\n\n'
                  if zh else 'See [usage](../how-to/runs.md) for common commands, values, retries, exit codes and logs, and [authoring](../how-to/workflows.md) for workflow/script contracts. This reference owns complete commands and options. Output defaults to tables; programs should select `-o json`. `--output` still selects a file path. Resource operations use verb + resource; maintenance operations use `admin`.\n\n')]

        for name, desc in entries:
            text = help_text(name)
            parts += ['## `' + name + '`\n\n', (ZH[name] if zh else desc) + '\n\n',
                      '```text\n' + text[text.index('Usage:'):].rstrip() + '\n```\n\n']
            def nested(parent, parent_text, level=3):
                children = [name for name, _ in visible_commands(parent_text.split('Commands:', 1)[1].split('Options:', 1)[0])] if 'Commands:' in parent_text else []
                for sub in children:
                    if sub == 'help': continue
                    command = [*parent, sub]
                    text = help_text(*command)
                    parts.extend(['#' * level + ' `' + ' '.join(command) + '`\n\n```text\n' + text[text.index('Usage:'):].rstrip() + '\n```\n\n'])
                    nested(command, text, level + 1)
            nested([name], text)
        path = root / 'docs' / lang / 'reference' / 'cli.md'
        content = ''.join(parts)
        if a.check:
            assert path.read_text() == content, str(path) + ' is stale'
        else:
            path.write_text(content)
    print('Bilingual CLI references cover', len(entries), 'commands and all visible nested subcommands')


if __name__ == '__main__':
    main()
