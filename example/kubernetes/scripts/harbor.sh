require_private_password_file "$HARBOR_PASSWORD_FILE"
k create namespace harbor --dry-run=client -o json | k apply -f -
# Secret bytes stay in a pipe/local Kubernetes Secret, never command arguments or Plan.
k -n harbor create secret generic harbor-admin --from-file=HARBOR_ADMIN_PASSWORD="$HARBOR_PASSWORD_FILE" --dry-run=client -o json | k apply -f - >/dev/null
k apply -f /opt/converyn-example/manifests/harbor-certificate.json
k -n harbor wait --for=condition=Ready certificate/harbor --timeout=180s
helm_install harbor /opt/converyn-example/charts/harbor.tgz harbor /opt/converyn-example/values/harbor.json
k -n harbor rollout status deployment/harbor-core --timeout=600s
k -n harbor rollout status deployment/harbor-registry --timeout=600s
