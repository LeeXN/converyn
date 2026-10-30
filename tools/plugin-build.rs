// 编译时固定插件实现及契约源码；目录移动或资源变化都会使内容摘要变化。
use std::{path::{Path,PathBuf},io::Write};
fn sources(path:&Path, out:&mut Vec<PathBuf>) {
    for item in std::fs::read_dir(path).expect("plugin directory") {
        let path=item.expect("source entry").path();
        if path.is_dir() { if path.file_name().unwrap()!="target" { sources(&path,out); } }
        else { out.push(path); }
    }
}
fn main() {
    use sha2::{Digest,Sha256};
    let root=PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let mut files=Vec::new();sources(&root.join("src"),&mut files);
    if root.join("resources").is_dir(){sources(&root.join("resources"),&mut files);}
    files.extend([root.join("Cargo.toml"),root.join("build.rs")]);
    sources(&root.join("../../crates/plugin-api/src"),&mut files);
    files.extend([root.join("../../crates/plugin-api/Cargo.toml"),root.join("../../tools/plugin-build.rs"),root.join("../../Cargo.lock"),root.join("../../Cargo.toml"),root.join("../../rust-toolchain.toml")]);files.sort();
    let mut hash=Sha256::new();
    for p in files { println!("cargo:rerun-if-changed={}",p.display());hash.update(p.strip_prefix(&root).unwrap_or(&p).to_string_lossy().as_bytes());hash.update([0]);hash.update(std::fs::read(&p).unwrap());hash.update([0]); }
    let mut features=std::env::vars().filter(|(k,_)|k.starts_with("CARGO_FEATURE_")).collect::<Vec<_>>();features.sort();for (k,v) in features{hash.update(k.as_bytes());hash.update([0]);hash.update(v.as_bytes());hash.update([0]);}
    let out=PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("identity.rs");
    write!(std::fs::File::create(out).unwrap(),"pub const CONTENT_SHA256: &str = \"{:x}\";",hash.finalize()).unwrap();
}
