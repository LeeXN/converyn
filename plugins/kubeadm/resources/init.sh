transaction_preflight
# 初始化是一次性事务，不能把缺失的回执解释成可以再次 init。
test ! -e /etc/kubernetes/admin.conf
test ! -e /etc/kubernetes/manifests/kube-apiserver.yaml
test ! -e /var/lib/etcd/member
"$KUBEADM" init --skip-token-print --upload-certs --config "$CONFIG_PATH" >/dev/null 2>&1
verify_node
