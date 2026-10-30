helm_install traefik /opt/converyn-example/charts/traefik.tgz ingress-system /opt/converyn-example/values/traefik.json
k -n ingress-system rollout status deployment/traefik --timeout=300s
