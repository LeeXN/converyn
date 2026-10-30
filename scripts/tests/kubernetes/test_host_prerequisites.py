#!/usr/bin/env python3
"""Exercise host guards and package selection without touching the host."""
from _paths import EXAMPLE, TOOLS
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = EXAMPLE


class HostPrerequisites(unittest.TestCase):
    def guard(self, name, kernel="6.12.10", distro="rocky", version="8.8",
              cgroup=True, selinux="Permissive"):
        source = (ROOT / name).read_text()
        # Stop before packages or runtime prerequisites can mutate the machine.
        stop = '\nif test "$ID" = ubuntu;' if name == "bootstrap-node.sh" else '\ntest "$(wc -l'
        source = source.split(stop, 1)[0]
        source = source.replace(". /etc/os-release", f"ID={distro}\nVERSION_ID={version}")
        with tempfile.TemporaryDirectory() as directory:
            controllers = Path(directory) / "cgroup.controllers"
            if cgroup:
                controllers.touch()
            source = source.replace("/sys/fs/cgroup/cgroup.controllers", str(controllers))
            mocks = (f'id() {{ echo 0; }}\n'
                     f'uname() {{ if test "$1" = -m; then echo x86_64; else echo {kernel}; fi; }}\n'
                     f'getenforce() {{ echo {selinux}; }}\n')
            return subprocess.run(["bash"], input=mocks + source, text=True, capture_output=True)

    def test_supported_host_and_kernel_matrix(self):
        for name in ["bootstrap-node.sh", "scripts/preflight.sh"]:
            for distro, version in [("rocky", "8.8"), ("rocky", "8"), ("ubuntu", "24.04")]:
                for kernel in ["5.10.0", "6.12.10"]:
                    with self.subTest(name=name, distro=distro, version=version, kernel=kernel):
                        result = self.guard(name, kernel, distro, version)
                        self.assertEqual(result.returncode, 0, result.stderr)

    def test_old_kernels_are_rejected_before_package_installation(self):
        for name in ["bootstrap-node.sh", "scripts/preflight.sh"]:
            for kernel in ["4.18.0-477.el8.x86_64", "5.4.300", "5.9.16"]:
                with self.subTest(name=name, kernel=kernel):
                    result = self.guard(name, kernel)
                    self.assertNotEqual(result.returncode, 0)
                    self.assertIn("Linux >= 5.10", result.stderr)

    def test_cgroup_v1_enforcing_selinux_and_unknown_os_are_rejected(self):
        for name in ["bootstrap-node.sh", "scripts/preflight.sh"]:
            for kwargs, message in [({"cgroup": False}, "cgroup v2"),
                                    ({"selinux": "Enforcing"}, "SELinux"),
                                    ({"distro": "rocky", "version": "9.0"}, "Supported hosts")]:
                with self.subTest(name=name, kwargs=kwargs):
                    result = self.guard(name, **kwargs)
                    self.assertNotEqual(result.returncode, 0)
                    self.assertIn(message, result.stderr)

    def test_package_manager_selection_for_nodes_nfs_and_proxy(self):
        for name, marker, rocky_package, ubuntu_package in [
            ("bootstrap-node.sh", "\n# Reload", "conntrack-tools", "nfs-common"),
            ("bootstrap-nfs.sh", "\ninstall -d", "nfs-utils", "nfs-kernel-server"),
            ("bootstrap-api-proxy.sh", "\nprintf", "haproxy", "haproxy"),
        ]:
            source = (ROOT / name).read_text()
            block = 'if test "$ID" = ubuntu;' + source.split('if test "$ID" = ubuntu;', 1)[1]
            block = block.split(marker, 1)[0]
            for distro, manager, package in [("rocky", "dnf", rocky_package),
                                             ("ubuntu", "apt-get", ubuntu_package)]:
                with self.subTest(name=name, distro=distro):
                    mocks = f'ID={distro}\napt-get() {{ echo apt-get "$@"; }}\ndnf() {{ echo dnf "$@"; }}\nrpm() {{ return 1; }}\n'
                    result = subprocess.run(["bash"], input=mocks + block, text=True, capture_output=True)
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertIn(manager + " install", result.stdout)
                    self.assertIn(package, result.stdout)
                    self.assertNotIn(("apt-get" if distro == "rocky" else "dnf") + " ", result.stdout)

    def test_installed_virtual_package_providers_skip_dnf(self):
        source = (ROOT / "bootstrap-node.sh").read_text()
        block = 'if test "$ID" = ubuntu;' + source.split('if test "$ID" = ubuntu;', 1)[1]
        block = block.split("\n# Reload", 1)[0]
        mocks = ('ID=rocky\nrpm() { test "$1" = -q && test "$2" = --whatprovides; }\n'
                 'dnf() { echo "unexpected network package installation" >&2; return 77; }\n')
        result = subprocess.run(["bash"], input=mocks + block, text=True, capture_output=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stderr, "")

    def test_proxy_refuses_unmanaged_configuration_without_installed_package(self):
        source = (ROOT / "bootstrap-api-proxy.sh").read_text()
        guard = source.split("# Do not take over", 1)[1].split('\nif test "$ID" = ubuntu;', 1)[0]
        guard = "# Do not take over" + guard
        for distro in ["rocky", "ubuntu"]:
            for owned in [False, True]:
                with self.subTest(distro=distro, owned=owned), tempfile.TemporaryDirectory() as directory:
                    config = Path(directory) / "haproxy.cfg"
                    config.write_text("# Managed Converyn Kubernetes API proxy\n" if owned else "# unrelated\n")
                    body = guard.replace("/etc/haproxy/haproxy.cfg", str(config))
                    mocks = f'ID={distro}\nrpm() {{ return 1; }}\ndpkg-query() {{ return 1; }}\n'
                    result = subprocess.run(["bash"], input=mocks + body, text=True, capture_output=True)
                    self.assertEqual(result.returncode == 0, owned, result.stderr)
                    self.assertEqual(config.read_text(), "# Managed Converyn Kubernetes API proxy\n" if owned else "# unrelated\n")


if __name__ == "__main__":
    unittest.main()
