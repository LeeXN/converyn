state=$(/usr/local/bin/helm list --all --namespace tigera-operator --filter '^calico$' -o json | python3 -c '
import json,sys
rows=json.load(sys.stdin)
if rows:
    assert len(rows)==1 and rows[0]["chart"]=="tigera-operator-v3.33.0", "Calico version change requires a separate upgrade plan"
print("existing" if rows else "new")')
if test "$state" = new; then
  # The operator subsequently owns mutations in the admission policies. Reapplying
  # their installation defaults on a retry would conflict with its live fields.
  /usr/local/bin/helm template calico-crds /opt/converyn-example/charts/calico-crds.tgz \
    --api-versions admissionregistration.k8s.io/v1/MutatingAdmissionPolicy | \
    k apply --server-side --field-manager=converyn-community -f -
fi
k wait --for=condition=Established crd/installations.operator.tigera.io --timeout=180s
helm_install calico /opt/converyn-example/charts/calico.tgz tigera-operator /opt/converyn-example/values/calico.json
retry 30 k get tigerastatus/calico >/dev/null
k wait --for=condition=Available tigerastatus/calico --timeout=600s
k -n calico-system rollout status daemonset/calico-node --timeout=600s
k wait --for=condition=Ready nodes --all --timeout=600s
k -n kube-system rollout status deployment/coredns --timeout=180s
