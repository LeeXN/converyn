# CI 集成样例

[English](README.md) | [简体中文](README.zh-CN.md)


这些文件需要主动复制启用，不是本仓库的活动流水线，也没有执行真实 CI 部署。将 [GitHub Actions](github-actions.yaml) 复制到应用仓库的 `.github/workflows/`，将 [GitLab CI](gitlab-ci.yaml) 复制为 `.gitlab-ci.yml`，或使用 [Jenkinsfile](Jenkinsfile)。

## 准备执行环境

使用带 `converyn-deploy` 标签的专用 Runner/Agent，确保能访问 Converyn HTTPS，并安装审核过的匹配二进制。在检出目录之外准备私有客户端配置及其引用的 CA、证书、私钥和令牌文件，配置文件权限为 0600。专用身份只授予目标流程的 maintainer 权限；初始目标绑定由平台管理员创建，正常发布无需超级管理员凭据。

GitHub 通过环境变量 `CONVERYN_CONFIG_PATH` 选择 Runner 上的文件；GitLab 和 Jenkins 通过受保护的 Runner 设置提供 `CONVERYN_CONFIG`。模板只提供显式手动部署入口，将部署限制在受信修订和专用 Runner。参考 [GitHub 环境](https://docs.github.com/en/actions/reference/workflows-and-actions/deployments-and-environments)、[GitLab CI](https://docs.gitlab.com/ci/yaml/) 和 [Jenkins Pipeline](https://www.jenkins.io/doc/book/pipeline/jenkinsfile/)。

## 命令与回退

三个模板共用 [deploy.sh](deploy.sh)，稳定请求 ID 用于同一发布的重试去重。脚本依次校验、登记、预览、等待并显示回执和日志，失败立即停止。回退是显式发布旧版本，不在结果未知时自动执行。

```bash
export CONVERYN_CONFIG=/secure/ci/config.json
export CONVERYN_WORKFLOW_DIR=example/node-demo
export CONVERYN_WORKFLOW_UID=node-site-demo
export CONVERYN_TARGET=default
export DEPLOYMENT_ID=manual-release-001
export RELEASE_VERSION=v2
bash example/ci/deploy.sh
# Restore a previously registered version using a new deployment identifier:
export DEPLOYMENT_ID=manual-restore-001
export RELEASE_VERSION=v1
bash example/ci/deploy.sh
```

## Kubernetes 变体

将源文件设为 `example/external-kubernetes/deployment.yaml`、流程 UID 设为 `external-k8s-web-demo`、目标设为 `qa`，并设置 `RELEASE_PARAMETER=release_version`。按 [Kubernetes 指南](../../docs/zh-CN/how-to/external-kubernetes.md)预先准备外部目标和控制器连接。Runner 凭据访问 Converyn，Kubernetes 凭据保留在控制器，不提交任何一方的凭据。

升级实际容器镜像时同时设置 `RELEASE_IMAGE` 为 Kubernetes 示例允许的固定摘要；回退时恢复旧镜像摘要。
