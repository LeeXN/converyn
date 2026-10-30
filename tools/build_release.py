#!/usr/bin/env python3
"""Build one static Linux distribution binary plus a provenance manifest.
Toolchains are explicit inputs; no automatic tool or artifact downloads.
"""
import argparse, hashlib, json, os, platform, re, shutil, subprocess, tempfile
from pathlib import Path

def source_fingerprint(root):
    source=hashlib.sha256()
    for path in sorted([root/'Cargo.toml',root/'Cargo.lock',root/'docs/schema/openapi.json',root/'tools/generate_api.py',root/'rust-toolchain.toml',root/'tools/plugin-build.rs',root/'tools/build_release.py',root/'scripts/ops/install.sh',root/'scripts/ops/init.sh',*root.glob('crates/**/Cargo.toml'),*root.glob('crates/**/build.rs'),*root.glob('crates/**/src/**/*'),*root.glob('plugins/**/Cargo.toml'),*root.glob('plugins/**/build.rs'),*root.glob('plugins/**/src/**/*'),*root.glob('plugins/**/resources/**/*'),*root.glob('web/dist/**/*'),*root.glob('web/src/**/*')]):
        if path.is_file():source.update(str(path.relative_to(root)).encode()+b'\0'+path.read_bytes()+b'\0')
    return source.hexdigest()

p=argparse.ArgumentParser()
p.add_argument('--arch',choices=['amd64','arm64'],required=True)
p.add_argument('--output',type=Path,required=True)
p.add_argument('--runner',type=Path,help='explicit target emulator when build host cannot execute the target binary')
p.add_argument('--build-tag',help='immutable build label; defaults to output directory name')
p.add_argument('--features',default='full',help='explicit Cargo feature set; empty builds the core')
p.add_argument('--zig',type=Path,help='ARM64 C compiler for bundled SQLite/ring')
a=p.parse_args();root=Path(__file__).resolve().parents[1];target={'amd64':'x86_64-unknown-linux-musl','arm64':'aarch64-unknown-linux-musl'}[a.arch]
assert not a.output.exists(),'refusing to replace release output'
env=dict(os.environ)
with tempfile.TemporaryDirectory(prefix='converyn-build-tools-') as tools:
    if a.arch=='arm64' and not a.zig and platform.machine() in ('aarch64','arm64'):
        env.update(CC_aarch64_unknown_linux_musl='musl-gcc',AR_aarch64_unknown_linux_musl='ar',CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER='musl-gcc')
    elif a.arch=='arm64':
        assert a.zig and a.zig.is_file(),'--zig is required for ARM64 C dependencies'
        cc=Path(tools)/'cc';ar=Path(tools)/'ar';zig=str(a.zig.resolve())
        cc.write_text('#!/usr/bin/env python3\nimport os,sys\na=[s.replace("--target=aarch64-unknown-linux-musl","--target=aarch64-linux-musl") for s in sys.argv[1:]]\nos.execv('+repr(zig)+',['+repr(zig)+',"cc","-target","aarch64-linux-musl",*a])\n');cc.chmod(0o700)
        ar.write_text('#!/usr/bin/env python3\nimport os,sys\nos.execv('+repr(zig)+',['+repr(zig)+',"ar",*sys.argv[1:]])\n');ar.chmod(0o700)
        env.update(CC_aarch64_unknown_linux_musl=str(cc),AR_aarch64_unknown_linux_musl=str(ar),CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER='rust-lld')
    source_before=source_fingerprint(root)
    subprocess.run(['cargo','build','-p','converyn','--release','--locked','--no-default-features','--features',a.features,'--target',target],cwd=root,env=env,check=True)
    build_dir=Path(env.get('CARGO_TARGET_DIR',root/'target'))
    if not build_dir.is_absolute():build_dir=root/build_dir
    binary=build_dir/target/'release/converyn';elf=binary.read_bytes()
    assert elf[:6]==b'\x7fELF\x02\x01' and int.from_bytes(elf[18:20],'little')=={'amd64':62,'arm64':183}[a.arch]
    headers=subprocess.check_output(['readelf','-l',str(binary)],text=True);assert 'INTERP' not in headers,'dynamic runtime interpreter found'
    source_after=source_fingerprint(root)
    assert source_before==source_after,'source inputs changed during build; no release output published, rebuild from the final source snapshot'
    runner=[str(a.runner.resolve())] if a.runner else []
    plugins=json.loads(subprocess.check_output([*runner,str(binary),'-o','json','get', 'plugins', '--local'],cwd=root,text=True))
    a.output.mkdir(parents=True);destination=a.output/('converyn-linux-'+a.arch);shutil.copy2(binary,destination);destination.chmod(0o755)
    installer=a.output/'install.sh';shutil.copy2(root/'scripts/ops/install.sh',installer);installer.chmod(0o755)
    initializer=a.output/'init.sh';shutil.copy2(root/'scripts/ops/init.sh',initializer);initializer.chmod(0o755)
    manifest={'build_tag':a.build_tag or a.output.name,'binary':destination.name,'installer':installer.name,'installer_sha256':hashlib.sha256(installer.read_bytes()).hexdigest(),'initializer':initializer.name,'initializer_sha256':hashlib.sha256(initializer.read_bytes()).hexdigest(),'binary_sha256':hashlib.sha256(elf).hexdigest(),'bytes':len(elf),'target':target,'static':True,'source_tree_sha256':source_after,'cargo_lock_sha256':hashlib.sha256((root/'Cargo.lock').read_bytes()).hexdigest(),'features':sorted(filter(None,a.features.split(','))),'version':re.search(r'(?s)\[workspace.package\].*?\bversion\s*=\s*"([^"]+)"',(root/'Cargo.toml').read_text())[1],'schema':int(re.search(r'SCHEMA_VERSION: u32 = (\d+)',(root/'crates/runtime/src/storage/store.rs').read_text())[1]),'protocol':int(re.search(r'PROTOCOL_VERSION: u64 = (\d+)',(root/'crates/runtime/src/controller/network.rs').read_text())[1]),'openapi_sha256':hashlib.sha256((root/'docs/schema/openapi.json').read_bytes()).hexdigest(),'plugins':plugins['plugins'],'plugin_manifest_sha256':plugins['manifest_sha256'],'rustc':subprocess.check_output(['rustc','--version'],cwd=root,text=True).strip()}
    (a.output/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n');print(json.dumps(manifest,indent=2))
