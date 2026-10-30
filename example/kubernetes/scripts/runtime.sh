set -euo pipefail
modprobe overlay
modprobe br_netfilter
modprobe nfs
sysctl --system >/dev/null
systemctl daemon-reload
systemctl enable --now containerd.service
systemctl enable kubelet.service
test "$(/usr/local/bin/kubeadm version -o short)" = v1.36.4
# A simple systemd service can be active before its gRPC socket is listening.
# Observe readiness; do not restart the daemon or replay installation on timeout.
deadline=$((SECONDS + 60))
until /usr/local/bin/ctr version >/dev/null 2>&1; do
  if test "$SECONDS" -ge "$deadline"; then
    echo 'containerd API did not become ready within 60 seconds' >&2
    exit 1
  fi
  sleep 2
done
if test -n "${IMAGE_ARCHIVE_SHA256:-}"; then
  printf '%s  %s\n' "$IMAGE_ARCHIVE_SHA256" /opt/converyn-example/images.tar | sha256sum --check --status
  /usr/local/bin/ctr --namespace k8s.io images import --digests /opt/converyn-example/images.tar >/dev/null
  # CRI drops a tag when resolving repo:tag@digest. OCI imports may retain only
  # that longer name, so add the canonical metadata alias without changing bytes.
  python3 - <<'CONVERYN_CANONICAL_IMAGES'
import re
import subprocess

CTR = ['/usr/local/bin/ctr', '--namespace', 'k8s.io', 'images']
MANAGED = 'io.cri-containerd.image=managed'


def canonical_digest_alias(reference):
    match = re.fullmatch(r'([^@\s]+):([\w][\w.-]{0,127})@(sha256:[a-f0-9]{64})',
                         reference, flags=re.ASCII)
    if not match or '/' in match[2]:
        return None
    # The tag cannot contain '/', so a registry's :port is never removed.
    repository, _, digest = match.groups()
    return repository + '@' + digest, digest


def read_images():
    lines = subprocess.check_output(CTR + ['list'], universal_newlines=True).splitlines()
    if not lines or lines[0].split()[:3] != ['REF', 'TYPE', 'DIGEST']:
        raise ValueError('unexpected ctr image metadata header')
    images = {}
    for line in lines[1:]:
        if not line.strip():
            continue
        fields = line.split(None, 6)
        if len(fields) != 7:
            raise ValueError('unexpected ctr image metadata row')
        images[fields[0]] = (fields[2], MANAGED in {label.strip() for label in fields[6].split(',')})
    return images


def verify_alias(images, source, target, digest, target_required=False):
    if images.get(source, (None,))[0] != digest:
        raise ValueError('image source target digest differs from its name: ' + source)
    if target in images and images[target][0] != digest:
        raise ValueError('canonical image alias already has a different digest: ' + target)
    if target_required and target not in images:
        raise ValueError('canonical image alias was not created: ' + target)


def normalize_images():
    images = read_images()
    aliases = []
    for source in images:
        alias = canonical_digest_alias(source)
        if alias:
            aliases.append((source, *alias))
    # Validate every existing source/target before performing any mutations.
    for source, target, digest in aliases:
        verify_alias(images, source, target, digest)
    for source, target, digest in aliases:
        if target not in images:
            subprocess.check_output(CTR + ['tag', source, target], universal_newlines=True)
            images = read_images()
        verify_alias(images, source, target, digest, target_required=True)
        if not images[target][1]:
            subprocess.check_output(CTR + ['label', target, MANAGED], universal_newlines=True)
            images = read_images()
            verify_alias(images, source, target, digest, target_required=True)
            if not images[target][1]:
                raise ValueError('canonical image alias lacks the CRI managed label: ' + target)


normalize_images()
CONVERYN_CANONICAL_IMAGES
fi
printf 'Container runtime available\n'
