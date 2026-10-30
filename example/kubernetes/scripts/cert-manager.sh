k apply --server-side --field-manager=converyn-community -f /opt/converyn-example/manifests/cert-manager.yaml
for deployment in cert-manager cert-manager-cainjector cert-manager-webhook; do
  k -n cert-manager rollout status "deployment/$deployment" --timeout=300s
done
# Ready Deployments alone do not prove webhook service discovery and admission.
retry 30 k apply -f /opt/converyn-example/manifests/issuers.json
k -n cert-manager wait --for=condition=Ready certificate/community-root --timeout=180s
k wait --for=condition=Ready clusterissuer/community-ca --timeout=180s
