//! Best-effort Linux assets and current resources. No external daemon or identity writes.
use converyn_core::model::telemetry::{INVENTORY_LIMIT, RESOURCE_LIMIT, Snapshot};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{BufReader, Read},
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
    time::{Duration, Instant},
};

mod mounts;

#[derive(Clone, Default)]
pub struct Cache {
    pub inventory: Option<Snapshot>,
    pub resources: Option<Snapshot>,
    pub checked_ms: Option<u64>,
}
pub fn start() -> Arc<RwLock<Cache>> {
    let cache = Arc::new(RwLock::new(Cache::default()));
    let target = cache.clone();
    tokio::spawn(async move {
        let mut collector = Collector::new(PathBuf::from("/"));
        let mut last = Instant::now() - Duration::from_secs(300);
        loop {
            let static_due = last.elapsed() >= Duration::from_secs(300);
            let result = tokio::task::spawn_blocking(move || {
                let inventory = static_due.then(|| collector.inventory());
                let resources = collector.resources();
                (collector, inventory, resources)
            })
            .await;
            let Ok((c, mut inventory, mut resources)) = result else {
                return;
            };
            collector = c;
            let has_nvidia = resources
                .data
                .get("gpus")
                .and_then(Value::as_array)
                .is_some_and(|v| v.iter().any(|g| g["vendor"] == "NVIDIA"));
            let nvidia = if has_nvidia {
                nvidia_command(Path::new("nvidia-smi")).await
            } else {
                Ok(String::new())
            };
            enrich(&mut inventory, &mut resources, nvidia);
            resources.bound(RESOURCE_LIMIT);
            if let Some(v) = &mut inventory {
                v.bound(INVENTORY_LIMIT);
            }
            if let Ok(mut state) = target.write() {
                if let Some(v) = inventory {
                    state.checked_ms = Some(v.collected_ms);
                    // Preserve the last changed time when a check finds the same assets.
                    if state.inventory.as_ref().is_none_or(|old| {
                        old.data != v.data || old.errors != v.errors || old.truncated != v.truncated
                    }) {
                        state.inventory = Some(v);
                    }
                }
                state.resources = Some(resources);
            }
            if static_due {
                last = Instant::now();
            }
            tokio::time::sleep(Duration::from_secs(20)).await;
        }
    });
    cache
}
fn now() -> u64 {
    crate::executors::host::now_ms()
}
fn snapshot() -> Snapshot {
    Snapshot {
        collected_ms: now(),
        ..Default::default()
    }
}
fn serial(s: String) -> Option<String> {
    let v = s.trim().to_owned();
    let lower = v.to_ascii_lowercase();
    if v.is_empty()
        || [
            "none",
            "unknown",
            "not specified",
            "default string",
            "system serial number",
            "to be filled by o.e.m.",
            "n/a",
        ]
        .contains(&lower.as_str())
        || v.chars().all(|c| c == '0' || c == '-' || c.is_whitespace())
    {
        None
    } else {
        Some(v)
    }
}
pub struct Collector {
    root: PathBuf,
    previous_cpu: Option<(u64, u64)>,
}
impl Collector {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            previous_cpu: None,
        }
    }
    fn path(&self, p: &str) -> PathBuf {
        self.root.join(p.trim_start_matches('/'))
    }
    fn read(&self, p: &str) -> Result<String, String> {
        let mut b = String::new();
        fs::File::open(self.path(p))
            .and_then(|f| f.take(1024 * 1024).read_to_string(&mut b))
            .map_err(|e| e.to_string())?;
        Ok(b.trim().to_owned())
    }
    fn field(&self, s: &mut Snapshot, key: &str, path: &str) {
        match self.read(path) {
            Ok(v) if !v.is_empty() => {
                s.data.insert(key.into(), json!(v));
            }
            Ok(_) => {
                s.errors.insert(key.into(), "empty system field".into());
            }
            Err(e) => {
                s.errors.insert(key.into(), e);
            }
        }
    }
    fn entries(&self, path: &str) -> Vec<String> {
        let mut v = fs::read_dir(self.path(path))
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .filter_map(|e| e.file_name().into_string().ok())
            .collect::<Vec<_>>();
        v.sort();
        v
    }
    fn memory(&self, s: &mut Snapshot) {
        match self.read("/proc/meminfo") {
            Ok(v) => {
                for (source, key) in [
                    ("MemTotal:", "memory_total_bytes"),
                    ("MemAvailable:", "memory_available_bytes"),
                ] {
                    if let Some(n) = v
                        .lines()
                        .find(|l| l.starts_with(source))
                        .and_then(|l| l.split_whitespace().nth(1))
                        .and_then(|n| n.parse::<u64>().ok())
                    {
                        s.data.insert(key.into(), json!(n.saturating_mul(1024)));
                    } else {
                        s.errors
                            .insert(key.into(), "kernel field unavailable".into());
                    }
                }
            }
            Err(e) => {
                s.errors.insert("memory".into(), e);
            }
        }
    }
    fn gpus(&self, s: &mut Snapshot, metrics: bool) {
        if let Err(e) = fs::read_dir(self.path("/sys/bus/pci/devices")) {
            s.errors.insert("gpus".into(), e.to_string());
        }
        let mut devices = BTreeMap::new();
        for address in self.entries("/sys/bus/pci/devices") {
            let base = format!("/sys/bus/pci/devices/{address}");
            if self
                .read(&format!("{base}/class"))
                .is_ok_and(|c| c.starts_with("0x03"))
            {
                devices.insert(address, base);
            }
        }
        for card in self.entries("/sys/class/drm").into_iter().filter(|n| {
            n.strip_prefix("card")
                .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
        }) {
            let p = format!("/sys/class/drm/{card}/device");
            if let Ok(real) = fs::canonicalize(self.path(&p)) {
                let id = real
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned();
                devices.entry(id).or_insert(p);
            }
        }
        let mut rows = Vec::new();
        for (address, base) in devices {
            let vendor_id = self.read(&format!("{base}/vendor")).ok();
            let vendor = match vendor_id.as_deref() {
                Some("0x10de") => "NVIDIA",
                Some("0x1002") => "AMD",
                Some("0x8086") => "Intel",
                _ => "unknown",
            };
            let driver = fs::read_link(self.path(&format!("{base}/driver")))
                .ok()
                .and_then(|p| p.file_name().map(|s| s.to_string_lossy().into_owned()));
            let model = self
                .read(&format!("{base}/product_name"))
                .ok()
                .or_else(|| self.read(&format!("{base}/device")).ok());
            let mut row = json!({"address":address,"vendor":vendor,"vendor_id":vendor_id,"model":model,"driver":driver});
            let fields: &[(&str, &str)] = if metrics {
                &[
                    ("memory_used_bytes", "mem_info_vram_used"),
                    ("utilization_percent", "gpu_busy_percent"),
                ]
            } else {
                &[
                    ("memory_total_bytes", "mem_info_vram_total"),
                    ("serial", "serial"),
                    ("uuid", "unique_id"),
                ]
            };
            let mut errors = BTreeMap::new();
            for (key, file) in fields {
                match self.read(&format!("{base}/{file}")) {
                    Ok(v) => {
                        row[*key] = if key.ends_with("bytes") || key.ends_with("percent") {
                            v.parse::<u64>().ok().map_or(Value::Null, |n| json!(n))
                        } else {
                            serial(v).map_or(Value::Null, |v| json!(v))
                        };
                        if row[*key].is_null() {
                            errors.insert(*key, "driver returned unavailable value".to_string());
                        }
                    }
                    Err(e) => {
                        errors.insert(*key, format!("driver does not expose {file}: {e}"));
                    }
                }
            }
            if metrics {
                let temp = self
                    .entries(&format!("{base}/hwmon"))
                    .into_iter()
                    .find_map(|h| {
                        self.read(&format!("{base}/hwmon/{h}/temp1_input"))
                            .ok()
                            .and_then(|v| v.parse::<i64>().ok())
                    });
                if let Some(t) = temp {
                    row["temperature_millicelsius"] = json!(t);
                } else {
                    errors.insert(
                        "temperature_millicelsius",
                        "driver temperature unavailable".into(),
                    );
                }
            }
            row["errors"] = json!(errors);
            rows.push(row);
        }
        s.data.insert("gpus".into(), json!(rows));
    }
    fn filesystems(&self, s: &mut Snapshot) {
        let mut rows = Vec::new();
        let selected = fs::File::open(self.path("/proc/self/mountinfo"))
            .and_then(|file| mounts::select(BufReader::new(file)));
        match selected {
            Err(error) => {
                s.errors.insert("filesystems".into(), error.to_string());
            }
            Ok(mounts) => {
                for mount in mounts {
                    let path = self.path(&mount.target);
                    let Ok(path) = std::ffi::CString::new(path.as_os_str().as_encoded_bytes())
                    else {
                        continue;
                    };
                    let mut info = std::mem::MaybeUninit::<libc::statvfs>::uninit();
                    // SAFETY: path is NUL terminated and info points to writable storage.
                    if unsafe { libc::statvfs(path.as_ptr(), info.as_mut_ptr()) } == 0 {
                        let info = unsafe { info.assume_init() };
                        rows.push(json!({"mount":mount.target,"kind":mount.kind,"total_bytes":info.f_blocks.saturating_mul(info.f_frsize),"available_bytes":info.f_bavail.saturating_mul(info.f_frsize),"used_bytes":info.f_blocks.saturating_sub(info.f_bfree).saturating_mul(info.f_frsize)}));
                    } else {
                        s.errors.insert(
                            format!("filesystem:{}", mount.target),
                            std::io::Error::last_os_error().to_string(),
                        );
                    }
                }
            }
        }
        s.data.insert("filesystems".into(), json!(rows));
    }
    pub fn inventory(&self) -> Snapshot {
        let mut s = snapshot();
        self.field(&mut s, "hostname", "/proc/sys/kernel/hostname");
        self.field(&mut s, "kernel", "/proc/sys/kernel/osrelease");
        s.data
            .insert("architecture".into(), json!(std::env::consts::ARCH));
        match self.read("/etc/os-release") {
            Ok(v) => {
                s.data.insert(
                    "os".into(),
                    json!(
                        v.lines()
                            .find_map(|l| l.strip_prefix("PRETTY_NAME="))
                            .unwrap_or("unknown")
                            .trim_matches('"')
                    ),
                );
            }
            Err(e) => {
                s.errors.insert("os".into(), e);
            }
        }
        for (key, file) in [
            ("vendor", "sys_vendor"),
            ("model", "product_name"),
            ("serial", "product_serial"),
        ] {
            let v = self.read(&format!("/sys/class/dmi/id/{file}"));
            match v.and_then(|v| serial(v).ok_or("system value unavailable or placeholder".into()))
            {
                Ok(v) => {
                    s.data.insert(key.into(), json!(v));
                }
                Err(e) => {
                    s.errors.insert(key.into(), e);
                }
            }
        }
        if !s.data.contains_key("model")
            && let Ok(v) = self.read("/sys/firmware/devicetree/base/model")
        {
            s.data
                .insert("model".into(), json!(v.trim_end_matches('\0')));
        }
        let cpu = self.read("/proc/cpuinfo");
        if let Ok(v) = &cpu {
            let pairs = v
                .lines()
                .filter_map(|l| l.split_once(':'))
                .map(|(k, v)| (k.trim(), v.trim()))
                .collect::<Vec<_>>();
            if let Some((_, m)) = pairs
                .iter()
                .find(|(k, _)| ["model name", "Processor", "Hardware"].contains(k))
            {
                s.data.insert("cpu_model".into(), json!(m));
            }
            s.data.insert(
                "cpu_logical".into(),
                json!(pairs.iter().filter(|(k, _)| *k == "processor").count()),
            );
        } else {
            s.errors.insert("cpu".into(), cpu.unwrap_err());
        }
        let mut cores = BTreeSet::new();
        for cpu in self
            .entries("/sys/devices/system/cpu")
            .into_iter()
            .filter(|v| {
                v.strip_prefix("cpu")
                    .is_some_and(|v| !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit()))
            })
        {
            if let (Ok(package), Ok(core)) = (
                self.read(&format!(
                    "/sys/devices/system/cpu/{cpu}/topology/physical_package_id"
                )),
                self.read(&format!("/sys/devices/system/cpu/{cpu}/topology/core_id")),
            ) && core != "-1"
            {
                cores.insert((package, core));
            }
        }
        if !cores.is_empty() {
            s.data.insert("cpu_physical".into(), json!(cores.len()));
        } else {
            s.errors
                .insert("cpu_physical".into(), "kernel topology unavailable".into());
        }
        let hint = format!(
            "{} {}",
            s.data
                .get("vendor")
                .and_then(Value::as_str)
                .unwrap_or_default(),
            s.data
                .get("model")
                .and_then(Value::as_str)
                .unwrap_or_default()
        )
        .to_lowercase();
        let virtualized = [
            ("vmware", "vmware"),
            ("kvm", "kvm"),
            ("qemu", "qemu"),
            ("virtualbox", "virtualbox"),
            ("virtual machine", "hyper-v"),
            ("xen", "xen"),
        ]
        .into_iter()
        .find(|(needle, _)| hint.contains(needle))
        .map(|(_, kind)| kind.to_string())
        .or_else(|| self.read("/sys/hypervisor/type").ok());
        if let Some(v) = virtualized {
            s.data.insert("virtualization".into(), json!(v));
        } else {
            s.errors.insert(
                "virtualization".into(),
                "not identified by kernel or firmware".into(),
            );
        }
        if !s.data.contains_key("serial")
            && let Ok(v) = self.read("/sys/firmware/devicetree/base/serial-number")
            && let Some(v) = serial(v.trim_end_matches('\0').into())
        {
            s.data.insert("serial".into(), json!(v));
            s.errors.remove("serial");
        }
        self.memory(&mut s);
        for (key, path) in [("disks", "/sys/block"), ("interfaces", "/sys/class/net")] {
            if let Err(e) = fs::read_dir(self.path(path)) {
                s.errors.insert(key.into(), e.to_string());
            }
        }
        let mut disks = Vec::new();
        for name in self
            .entries("/sys/block")
            .into_iter()
            .filter(|n| !n.starts_with("loop") && !n.starts_with("ram"))
        {
            let base = format!("/sys/block/{name}");
            let mut row = json!({"name":name});
            let mut errors = BTreeMap::new();
            for (key, file) in [
                ("model", "device/model"),
                ("serial", "device/serial"),
                ("size_bytes", "size"),
                ("rotational", "queue/rotational"),
            ] {
                match self.read(&format!("{base}/{file}")) {
                    Ok(v) => {
                        row[key] = match key {
                            "size_bytes" => v
                                .parse::<u64>()
                                .ok()
                                .map_or(Value::Null, |n| json!(n.saturating_mul(512))),
                            "rotational" => json!(v == "1"),
                            _ => serial(v).map_or(Value::Null, |v| json!(v)),
                        };
                        if row[key].is_null() {
                            errors.insert(key, "value unavailable or placeholder".to_string());
                        }
                    }
                    Err(e) => {
                        errors.insert(key, e);
                    }
                }
            }
            row["type"] = json!(if name.starts_with("nvme") {
                "nvme"
            } else if row["rotational"] == true {
                "hdd"
            } else if row["rotational"] == false {
                "ssd_or_virtual"
            } else {
                "unknown"
            });
            row["errors"] = json!(errors);
            disks.push(row);
        }
        s.data.insert("disks".into(), json!(disks));
        let interfaces=self.entries("/sys/class/net").into_iter().map(|name| {
            let base=format!("/sys/class/net/{name}"); let speed=self.read(&format!("{base}/speed")).ok().and_then(|v|v.parse::<u64>().ok());
            json!({"name":name,"mac":self.read(&format!("{base}/address")).ok(),"speed_mbps":speed,"speed_reason":if speed.is_none(){Some("link speed not exposed or link down")}else{None}})
        }).collect::<Vec<_>>();
        s.data.insert("interfaces".into(), json!(interfaces));
        if self.root == Path::new("/") {
            match addresses() {
                Ok(v) => {
                    s.data.insert("addresses".into(), json!(v));
                }
                Err(e) => {
                    s.errors.insert("addresses".into(), e);
                }
            }
        } else {
            s.errors.insert(
                "addresses".into(),
                "network namespace addresses are not read from fixture roots".into(),
            );
        }
        // Current usage belongs only in resource samples; otherwise it would
        // make unchanged assets appear to change at every five-minute check.
        s.data.remove("memory_available_bytes");
        self.filesystems(&mut s);
        if let Some(rows) = s.data.get_mut("filesystems").and_then(Value::as_array_mut) {
            for row in rows {
                if let Some(row) = row.as_object_mut() {
                    row.remove("available_bytes");
                    row.remove("used_bytes");
                }
            }
        }
        self.gpus(&mut s, false);
        s.bound(INVENTORY_LIMIT);
        s
    }
    pub fn resources(&mut self) -> Snapshot {
        let mut s = snapshot();
        self.memory(&mut s);
        self.filesystems(&mut s);
        self.gpus(&mut s, true);
        match self.read("/proc/loadavg") {
            Ok(v) => {
                s.data.insert(
                    "load".into(),
                    json!(v.split_whitespace().take(3).collect::<Vec<_>>()),
                );
            }
            Err(e) => {
                s.errors.insert("load".into(), e);
            }
        }
        let cpu = self.read("/proc/stat").ok().and_then(|v| {
            let n = v
                .lines()
                .next()?
                .split_whitespace()
                .skip(1)
                .take(8)
                .map(str::parse::<u64>)
                .collect::<Result<Vec<_>, _>>()
                .ok()?;
            if n.len() < 4 {
                return None;
            }
            Some((n.iter().sum::<u64>(), n[3] + n.get(4).copied().unwrap_or(0)))
        });
        match (self.previous_cpu, cpu) {
            (Some((total, idle)), Some((t, i))) if t > total && i >= idle => {
                s.data.insert(
                    "cpu_used_basis_points".into(),
                    json!((t - total).saturating_sub(i - idle).saturating_mul(10000) / (t - total)),
                );
            }
            _ => {
                s.errors.insert(
                    "cpu_used_basis_points".into(),
                    "waiting for two valid CPU samples".into(),
                );
            }
        }
        self.previous_cpu = cpu;
        s.bound(RESOURCE_LIMIT);
        s
    }
}
fn addresses() -> Result<Vec<Value>, String> {
    let mut head = std::ptr::null_mut();
    // SAFETY: libc allocates a linked list, freed below exactly once.
    if unsafe { libc::getifaddrs(&mut head) } != 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    let mut rows = Vec::new();
    let mut p = head;
    while !p.is_null() {
        let item = unsafe { &*p };
        if !item.ifa_addr.is_null() {
            let name = unsafe { std::ffi::CStr::from_ptr(item.ifa_name) }.to_string_lossy();
            let addr = unsafe {
                match (*item.ifa_addr).sa_family as i32 {
                    libc::AF_INET => Some(
                        std::net::Ipv4Addr::from(
                            (*(item.ifa_addr as *const libc::sockaddr_in))
                                .sin_addr
                                .s_addr
                                .to_ne_bytes(),
                        )
                        .to_string(),
                    ),
                    libc::AF_INET6 => Some(
                        std::net::Ipv6Addr::from(
                            (*(item.ifa_addr as *const libc::sockaddr_in6))
                                .sin6_addr
                                .s6_addr,
                        )
                        .to_string(),
                    ),
                    _ => None,
                }
            };
            if let Some(address) = addr {
                rows.push(json!({"interface":name,"address":address}));
            }
        }
        p = item.ifa_next;
    }
    unsafe { libc::freeifaddrs(head) };
    rows.sort_by_key(Value::to_string);
    rows.dedup();
    Ok(rows)
}
async fn nvidia_command(path: &Path) -> Result<String, String> {
    use tokio::io::AsyncReadExt;
    let mut command = tokio::process::Command::new(path);
    command.args(["--query-gpu=pci.bus_id,name,uuid,serial,memory.total,memory.used,utilization.gpu,temperature.gpu","--format=csv,noheader,nounits"]).kill_on_drop(true);
    command
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    let mut child = command
        .spawn()
        .map_err(|e| format!("nvidia-smi unavailable: {e}"))?;
    tokio::time::timeout(Duration::from_secs(2), async {
        let mut output = Vec::new();
        child
            .stdout
            .take()
            .ok_or("nvidia-smi stdout unavailable")?
            .take(65537)
            .read_to_end(&mut output)
            .await
            .map_err(|e| e.to_string())?;
        if output.len() > 65536 {
            return Err("nvidia-smi output exceeds 64 KiB".into());
        }
        let status = child.wait().await.map_err(|e| e.to_string())?;
        if !status.success() {
            return Err(format!("nvidia-smi query failed: {status}"));
        }
        String::from_utf8(output).map_err(|e| e.to_string())
    })
    .await
    .map_err(|_| "nvidia-smi timed out after 2 seconds".to_string())?
}
fn enrich(
    inventory: &mut Option<Snapshot>,
    resources: &mut Snapshot,
    result: Result<String, String>,
) {
    for (snapshot, metrics) in inventory
        .iter_mut()
        .map(|v| (v, false))
        .chain(std::iter::once((resources, true)))
    {
        let Some(rows) = snapshot.data.get_mut("gpus").and_then(Value::as_array_mut) else {
            continue;
        };
        for row in rows.iter_mut().filter(|v| v["vendor"] == "NVIDIA") {
            match &result {
                Err(e) => {
                    row["errors"]["nvidia_smi"] = json!(e);
                }
                Ok(text) => {
                    let address = row["address"].as_str().unwrap_or_default().to_lowercase();
                    if let Some(cols) = text
                        .lines()
                        .map(|l| l.split(',').map(str::trim).collect::<Vec<_>>())
                        .find(|v| v.len() == 8 && v[0].to_lowercase().ends_with(&address))
                    {
                        for (key, index) in if metrics {
                            vec![
                                ("memory_used_bytes", 5),
                                ("utilization_percent", 6),
                                ("temperature_millicelsius", 7),
                            ]
                        } else {
                            vec![
                                ("model", 1),
                                ("uuid", 2),
                                ("serial", 3),
                                ("memory_total_bytes", 4),
                            ]
                        } {
                            let value = cols[index];
                            let parsed = if index >= 4 {
                                value.parse::<u64>().ok().map(|v| {
                                    json!(v.saturating_mul(if index <= 5 {
                                        1024 * 1024
                                    } else if index == 7 {
                                        1000
                                    } else {
                                        1
                                    }))
                                })
                            } else {
                                serial(value.into())
                                    .filter(|v| !v.starts_with('['))
                                    .map(|v| json!(v))
                            };
                            if let Some(v) = parsed {
                                row[key] = v;
                                row["errors"].as_object_mut().unwrap().remove(key);
                            } else {
                                row["errors"][key] = json!(format!("nvidia-smi: {value}"));
                            }
                        }
                    } else {
                        row["errors"]["nvidia_smi"] = json!("device not returned by driver query");
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn put(root: &Path, name: &str, text: &str) {
        let p = root.join(name.trim_start_matches('/'));
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }
    #[test]
    fn vm_assets_placeholder_serial_hotplug_and_missing_fields() {
        let d = tempfile::tempdir().unwrap();
        let r = d.path();
        put(r, "/sys/class/dmi/id/sys_vendor", "VMware, Inc.");
        put(
            r,
            "/sys/class/dmi/id/product_name",
            "VMware Virtual Platform",
        );
        put(
            r,
            "/sys/class/dmi/id/product_serial",
            "To Be Filled By O.E.M.",
        );
        put(
            r,
            "/proc/cpuinfo",
            "processor : 0\nmodel name : Test x86 CPU\n\nprocessor : 1\nmodel name : Test x86 CPU",
        );
        for cpu in [0, 1] {
            put(
                r,
                &format!("/sys/devices/system/cpu/cpu{cpu}/topology/physical_package_id"),
                "0",
            );
            put(
                r,
                &format!("/sys/devices/system/cpu/cpu{cpu}/topology/core_id"),
                "0",
            );
        }
        put(r, "/proc/meminfo", "MemTotal: 100 kB\nMemAvailable: 25 kB");
        fs::create_dir_all(r.join("sys/bus/pci/devices")).unwrap();
        let c = Collector::new(r.into());
        let first = c.inventory();
        assert_eq!(first.data["virtualization"], "vmware");
        assert_eq!(first.data["cpu_logical"], 2);
        assert_eq!(first.data["cpu_physical"], 1);
        assert!(!first.data.contains_key("serial"));
        assert!(first.errors.contains_key("serial"));
        assert_eq!(first.data["gpus"], json!([]));
        put(r, "/sys/block/nvme0n1/size", "100");
        put(r, "/sys/block/nvme0n1/queue/rotational", "0");
        put(r, "/sys/block/nvme0n1/device/serial", "disk-1");
        let second = c.inventory();
        assert_eq!(second.data["disks"][0]["size_bytes"], 51200);
        assert_eq!(second.data["disks"][0]["serial"], "disk-1");
        fs::remove_dir_all(r.join("sys/block/nvme0n1")).unwrap();
        assert_eq!(c.inventory().data["disks"], json!([]));
    }
    #[test]
    fn arm_cpu_gpu_classification_drm_dedup_and_driver_unavailable() {
        use std::os::unix::fs::symlink;
        let d = tempfile::tempdir().unwrap();
        let r = d.path();
        put(
            r,
            "/proc/cpuinfo",
            "processor : 0\nProcessor : ARMv8 Processor rev 1",
        );
        put(r, "/sys/firmware/devicetree/base/model", "Test ARM Board\0");
        for (id, class, vendor) in [
            ("0000:01:00.0", "0x030200", "0x10de"),
            ("0000:02:00.0", "0x030000", "0x1002"),
            ("0000:03:00.0", "0x040300", "0x10de"),
        ] {
            put(r, &format!("/sys/bus/pci/devices/{id}/class"), class);
            put(r, &format!("/sys/bus/pci/devices/{id}/vendor"), vendor);
        }
        for card in ["card0", "card1"] {
            let p = r.join(format!("sys/class/drm/{card}"));
            fs::create_dir_all(&p).unwrap();
            symlink(r.join("sys/bus/pci/devices/0000:01:00.0"), p.join("device")).unwrap();
        }
        let c = Collector::new(r.into());
        let s = c.inventory();
        assert_eq!(s.data["cpu_model"], "ARMv8 Processor rev 1");
        assert_eq!(s.data["model"], "Test ARM Board");
        assert_eq!(s.data["gpus"].as_array().unwrap().len(), 2);
        let mut inventory = Some(s);
        let mut resources = snapshot();
        c.gpus(&mut resources, true);
        enrich(
            &mut inventory,
            &mut resources,
            Ok("00000000:01:00.0, NVIDIA Test, GPU-test, N/A, 1024, 32, N/A, 50\n".into()),
        );
        assert_eq!(
            inventory.as_ref().unwrap().data["gpus"][0]["model"],
            "NVIDIA Test"
        );
        assert_eq!(
            resources.data["gpus"][0]["memory_used_bytes"],
            32 * 1024 * 1024
        );
        assert!(resources.data["gpus"][0]["utilization_percent"].is_null());
        assert!(resources.data["gpus"][0]["errors"]["utilization_percent"].is_string());
    }
    #[test]
    fn cpu_deltas_and_observation_bounds() {
        let d = tempfile::tempdir().unwrap();
        let r = d.path();
        let mut c = Collector::new(r.into());
        put(r, "/proc/stat", "cpu 10 0 10 80 0 0 0 0\n");
        assert!(c.resources().errors.contains_key("cpu_used_basis_points"));
        put(r, "/proc/stat", "cpu 20 0 20 160 0 0 0 0\n");
        assert_eq!(c.resources().data["cpu_used_basis_points"], 2000);
        let mut s = snapshot();
        s.data
            .insert("huge".into(), json!("x".repeat(INVENTORY_LIMIT)));
        s.bound(INVENTORY_LIMIT);
        assert!(s.valid(INVENTORY_LIMIT));
        assert!(s.truncated);
        assert!(s.errors.contains_key("huge"));
    }
    #[test]
    fn host_mount_filter_is_shared_by_inventory_and_resources_before_statvfs() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let mut table = String::new();
        for i in 0..4000 {
            table.push_str(&format!(
                "10 1 0:45 / /var/lib/kubelet/pods/{i}/volumes/claim rw - nfs4 server:/claim rw\n"
            ));
        }
        table.push_str(
            "1 0 8:1 / / rw - xfs /dev/sda1 rw\n2 1 8:1 /tmp/private /tmp rw - xfs /dev/sda1 rw\n",
        );
        put(root, "/proc/self/mountinfo", &table);
        put(
            root,
            "/proc/meminfo",
            "MemTotal: 1024 kB\nMemAvailable: 512 kB",
        );
        put(root, "/proc/stat", "cpu 10 0 10 80 0 0 0 0\n");
        put(root, "/proc/loadavg", "0.1 0.2 0.3 1/10 1");
        let mut collector = Collector::new(root.into());
        let inventory = collector.inventory();
        let resources = collector.resources();
        for sample in [&inventory, &resources] {
            let rows = sample.data["filesystems"].as_array().unwrap();
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0]["mount"], "/");
            assert!(!sample.truncated);
            assert!(!sample.errors.keys().any(|k| k.starts_with("filesystem:")));
        }
        assert!(
            !inventory.data["filesystems"][0]
                .as_object()
                .unwrap()
                .contains_key("used_bytes")
        );
        assert!(resources.data["filesystems"][0]["used_bytes"].is_u64());
        assert_eq!(resources.data["memory_total_bytes"], 1024 * 1024);
    }

    #[tokio::test]
    async fn missing_tool_timeout_and_bounded_output() {
        use std::os::unix::fs::PermissionsExt;
        assert!(
            nvidia_command(Path::new("/nonexistent/converyn-nvidia-smi"))
                .await
                .unwrap_err()
                .contains("unavailable")
        );
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("fake-smi");
        fs::write(&p, "#!/bin/sh\nexec sleep 10\n").unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(0o700)).unwrap();
        let now = Instant::now();
        assert!(nvidia_command(&p).await.unwrap_err().contains("timed out"));
        assert!(now.elapsed() < Duration::from_secs(4));
        fs::write(&p, "#!/bin/sh\nhead -c 70000 /dev/zero\n").unwrap();
        assert!(nvidia_command(&p).await.unwrap_err().contains("64 KiB"));
    }
}
