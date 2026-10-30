helm_install csi-nfs /opt/converyn-example/charts/csi-nfs.tgz kube-system /opt/converyn-example/values/csi-nfs.json
k apply -f /opt/converyn-example/manifests/storage-class.json
k -n kube-system rollout status daemonset/csi-nfs-node --timeout=300s
k get csidriver nfs.csi.k8s.io >/dev/null
