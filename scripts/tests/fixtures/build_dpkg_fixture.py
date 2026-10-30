#!/usr/bin/env python3
"""Build a network-free package test image from an explicit public-binary
allowlist. No host configuration, credentials or package database are copied.
This is Ubuntu dpkg userspace, not a full Debian distribution qualification.
"""
import argparse,os,re,shutil,subprocess
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--directory',type=Path,required=True);a=p.parse_args();root=a.directory/'rootfs';root.mkdir(parents=True,exist_ok=True)
programs=['/bin/sh','/usr/bin/dpkg','/usr/bin/dpkg-deb','/usr/bin/dpkg-query','/usr/bin/dpkg-split','/usr/bin/diff','/usr/sbin/start-stop-daemon','/usr/bin/update-alternatives','/usr/bin/tar','/usr/bin/gzip','/usr/bin/mkdir','/usr/bin/rm','/usr/bin/sync','/usr/bin/sleep','/usr/bin/cat','/usr/bin/id','/sbin/ldconfig']
files=set(programs)
for program in programs:
 result=subprocess.run(['ldd',program],text=True,capture_output=True)
 files.update(re.findall(r'(/[^\s()]+)',result.stdout))
for source in sorted(files):
 target=root/source.lstrip('/');target.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(source,target,follow_symlinks=True)
for name in ['tmp','var/lib/dpkg/info','var/lib/dpkg/updates','var/lib/dpkg/triggers','var/log','etc','opt']: (root/name).mkdir(parents=True,exist_ok=True)
(root/'tmp').chmod(0o1777);(root/'var/lib/dpkg/status').write_text('');(root/'etc/passwd').write_text('root:x:0:0:root:/root:/bin/sh\nnobody:x:65534:65534:nobody:/nonexistent:/bin/sh\n');(root/'etc/group').write_text('root:x:0:\nnogroup:x:65534:\n');(root/'converyn-test-image').write_text('isolated-dpkg-fixture\n')
(a.directory/'Dockerfile').write_text('FROM scratch\nCOPY rootfs /\nENV PATH=/usr/sbin:/usr/bin:/sbin:/bin\nCMD ["/bin/sh"]\n')
packages=a.directory/'packages';packages.mkdir(exist_ok=True)
for version in ['1.0','2.0']:
 package=a.directory/('package-'+version);control=package/'DEBIAN';control.mkdir(parents=True,exist_ok=True);payload=package/'opt/converyn-fixture';payload.mkdir(parents=True,exist_ok=True);(payload/'version').write_text(version+'\n')
 (control/'control').write_text('Package: converyn-fixture\nVersion: '+version+'\nArchitecture: all\nMaintainer: Converyn integration fixture <fixture@localhost>\nDescription: Isolated native package transaction test\n')
 (control/'postinst').write_text('#!/bin/sh\nset -eu\nmkdir -p /var/lib/converyn-fixture\nprintf "'+version+'\\n" >> /var/lib/converyn-fixture/install-events\nsync\n');(control/'postinst').chmod(0o755)
 subprocess.run(['dpkg-deb','--build','--root-owner-group',str(package),str(packages/('converyn-fixture-'+version+'.deb'))],check=True,capture_output=True)
print('Built allowlisted dpkg filesystem and two test packages')
