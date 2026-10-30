# CI integration templates

[English](README.md) | [简体中文](README.zh-CN.md)


These files are opt-in examples. They are not active workflows in this repository and have not executed a real CI deployment. Copy [GitHub Actions](github-actions.yaml) to your application repository's `.github/workflows/`, [GitLab CI](gitlab-ci.yaml) to `.gitlab-ci.yml`, or use [Jenkinsfile](Jenkinsfile).

## Prepare the runner

Use a dedicated runner/agent labelled `converyn-deploy`, reachable Converyn HTTPS, and the reviewed matching Converyn binary. Provision a private client config and all referenced CA/certificate/key/token files outside checkout. The config is 0600. Give its dedicated identity maintainer access only to the intended workflow; the platform administrator creates the initial target binding. No super administrator credential is needed for normal releases.

GitHub uses the environment variable `CONVERYN_CONFIG_PATH` to select that runner file; GitLab and Jenkins provide `CONVERYN_CONFIG` as protected runner settings. The templates run only through explicit manual deployment paths. Restrict deployment jobs to trusted revisions and dedicated runners. See [GitHub environments](https://docs.github.com/en/actions/reference/workflows-and-actions/deployments-and-environments), [GitLab CI](https://docs.gitlab.com/ci/yaml/) and [Jenkins Pipeline](https://www.jenkins.io/doc/book/pipeline/jenkinsfile/).

## Commands and rollback

[deploy.sh](deploy.sh) is shared by all templates. Its stable request ID deduplicates retries of the same release. It validates, registers, previews, waits and prints receipts/logs; a failure stops the job. A restore is an explicit release of the old version, not an automatic action on an unknown failure.

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

## Kubernetes variant

Set the source to `example/external-kubernetes/deployment.yaml`, workflow UID to `external-k8s-web-demo`, target to `qa`, and `RELEASE_PARAMETER=release_version`. Prepare the external target and controller connection once using the [Kubernetes guide](../../docs/en/how-to/external-kubernetes.md). Runner credentials access Converyn; Kubernetes credentials remain on the controllers. Do not commit either.

Set `RELEASE_IMAGE` to a digest allowed by the Kubernetes example when upgrading the container, and restore its previous digest for rollback.
