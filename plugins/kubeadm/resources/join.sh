transaction_preflight
test ! -e /etc/kubernetes/kubelet.conf
"$KUBEADM" join --config "$CONFIG_PATH" >/dev/null 2>&1
verify_node
