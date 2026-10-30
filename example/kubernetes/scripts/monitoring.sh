require_private_password_file "$GRAFANA_PASSWORD_FILE"
k create namespace monitoring --dry-run=client -o json | k apply -f -
k -n monitoring create secret generic grafana-admin --from-literal=admin-user=admin --from-file=admin-password="$GRAFANA_PASSWORD_FILE" --dry-run=client -o json | k apply -f - >/dev/null
helm_install monitoring /opt/converyn-example/charts/monitoring.tgz monitoring /opt/converyn-example/values/monitoring.json
k -n monitoring rollout status statefulset/prometheus-monitoring-kube-prometheus-prometheus --timeout=600s
k -n monitoring rollout status statefulset/alertmanager-monitoring-kube-prometheus-alertmanager --timeout=300s
k -n monitoring rollout status deployment/monitoring-grafana --timeout=300s
