//! Host storage selection precedes statvfs so Pod mounts never consume the sample budget.
use std::{
    collections::BTreeMap,
    io::{self, BufRead},
    path::Path,
};

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Mount {
    pub target: String,
    pub kind: String,
    device: String,
    source: String,
    root: String,
}

fn unescape(value: &str) -> String {
    // Decode backslashes last: a literal "\\040" must not turn into a space.
    value
        .replace("\\040", " ")
        .replace("\\011", "\t")
        .replace("\\012", "\n")
        .replace("\\134", "\\")
}

fn internal(target: &str) -> bool {
    let path = Path::new(target);
    if [
        "/var/lib/kubelet/pods",
        "/var/lib/kubelet/plugins",
        "/var/lib/kubelet/plugins_registry",
        "/run/containerd",
        "/run/docker",
        "/run/containers",
        "/var/lib/containerd/tmpmounts",
        "/var/lib/docker/overlay2",
        "/var/lib/docker/containers",
        "/var/lib/docker/devicemapper/mnt",
        "/var/lib/containers/storage/overlay",
    ]
    .iter()
    .any(|prefix| path.starts_with(prefix))
    {
        return true;
    }
    let parts: Vec<_> = target.split('/').filter(|s| !s.is_empty()).collect();
    // Kubelet root-dir and containerd root are configurable. Recognize their
    // internal layout without treating every business directory named pods as K8s.
    parts
        .iter()
        .any(|s| s.starts_with("io.containerd.snapshotter."))
        || parts.windows(3).any(|p| {
            let runtime_id = p[2].len() >= 32 && p[2].bytes().all(|b| b.is_ascii_hexdigit());
            if runtime_id
                && ((p[0] == "devicemapper" && p[1] == "mnt")
                    || (p[0] == "btrfs" && p[1] == "subvolumes"))
            {
                return true;
            }
            (p[0] == "plugins" && p[1] == "kubernetes.io" && p[2] == "csi")
                || (p[0] == "pods"
                    && p[1].len() >= 16
                    && p[1].bytes().all(|b| b.is_ascii_hexdigit() || b == b'-')
                    && ["volumes", "volume-subpaths"].contains(&p[2]))
        })
}

impl Mount {
    fn parse(line: &str) -> Option<Self> {
        let (left, right) = line.split_once(" - ")?;
        let fields: Vec<_> = left.split_whitespace().collect();
        let fs: Vec<_> = right.split_whitespace().collect();
        if fields.len() < 6 || fs.len() < 3 {
            return None;
        }
        let (major, minor) = fields[2].split_once(':')?;
        let major = major.parse::<u32>().ok()?;
        minor.parse::<u32>().ok()?;
        let kind = fs[0];
        if ![
            "ext2", "ext3", "ext4", "xfs", "btrfs", "zfs", "vfat", "msdos", "exfat", "f2fs",
            "bcachefs", "ntfs3", "nfs", "nfs4", "cifs", "smb3",
        ]
        .contains(&kind)
        {
            return None;
        }
        let target = unescape(fields[4]);
        let root = unescape(fields[3]);
        let source = unescape(fs[1]);
        if !target.starts_with('/')
            || !root.starts_with('/')
            || internal(&target)
            || major == 7
            || ["/dev/loop", "/dev/ram", "/dev/zram"].iter().any(|p| {
                source
                    .strip_prefix(p)
                    .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
            })
        {
            return None;
        }
        Some(Self {
            target,
            root,
            source,
            kind: kind.into(),
            device: fields[2].into(),
        })
    }

    fn identity(&self) -> (String, String, String) {
        // Different exports/datasets/subvolumes remain useful host storage.
        // Plain block filesystems share capacity across their bind aliases.
        let (identity, volume) = match self.kind.as_str() {
            "nfs" | "nfs4" | "cifs" | "smb3" | "zfs" => (self.source.clone(), String::new()),
            "btrfs" => (self.device.clone(), self.root.clone()),
            _ => (self.device.clone(), String::new()),
        };
        (self.kind.clone(), identity, volume)
    }

    fn preference(&self) -> (bool, usize, usize, &str) {
        (
            self.root != "/",
            Path::new(&self.target).components().count(),
            self.target.len(),
            &self.target,
        )
    }
}

pub(super) fn select(reader: impl BufRead) -> io::Result<Vec<Mount>> {
    let mut mounts: BTreeMap<_, Mount> = BTreeMap::new();
    // Stream the whole table instead of clipping it before filtering: host
    // storage may occur after megabytes of container mount entries.
    for line in reader.lines() {
        let Some(candidate) = Mount::parse(&line?) else {
            continue;
        };
        let key = candidate.identity();
        match mounts.get_mut(&key) {
            Some(current) if candidate.preference() < current.preference() => *current = candidate,
            None => {
                mounts.insert(key, candidate);
            }
            _ => {}
        }
    }
    let mut rows: Vec<_> = mounts.into_values().collect();
    rows.sort_by(|a, b| a.target.cmp(&b.target));
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    fn row(device: &str, root: &str, target: &str, kind: &str, source: &str) -> String {
        format!("42 1 {device} {root} {target} rw shared:1 - {kind} {source} rw\n")
    }
    fn selected(text: &str) -> Vec<Mount> {
        select(Cursor::new(text)).unwrap()
    }

    #[test]
    fn real_host_storage_deduplicates_bind_aliases_but_keeps_independent_tmp_and_shares() {
        let data = row("253:0", "/tmp/private", "/tmp", "xfs", "/dev/mapper/root")
            + &row(
                "253:0",
                "/var/tmp/private",
                "/var/tmp",
                "xfs",
                "/dev/mapper/root",
            )
            + &row("253:0", "/", "/", "xfs", "/dev/root")
            + &row("8:1", "/", "/boot", "ext4", "/dev/sda1")
            + &row("8:2", "/", "/boot/efi", "vfat", "/dev/sda2")
            + &row("253:2", "/", "/data", "xfs", "/dev/mapper/crypt-data")
            + &row("9:0", "/", "/archive", "ext4", "/dev/md0")
            + &row("8:3", "/", "/tmp", "ext4", "/dev/sdb1")
            + &row("0:44", "/", "/shared", "nfs4", "server:/data")
            + &row("0:45", "/subdir", "/alias", "nfs4", "server:/data")
            + &row("0:44", "/", "/logs", "nfs4", "server:/logs")
            + &row("0:46", "/", "/smb", "cifs", "//server/business");
        let rows = selected(&data);
        assert_eq!(rows.len(), 9);
        assert!(rows.iter().any(|m| m.target == "/"));
        assert!(rows.iter().any(|m| m.target == "/tmp" && m.device == "8:3"));
        assert!(
            !rows
                .iter()
                .any(|m| m.target == "/var/tmp" || m.target == "/alias")
        );
        assert!(rows.iter().any(|m| m.target == "/logs"));
    }

    #[test]
    fn pod_and_runtime_churn_does_not_change_host_mounts_even_beyond_old_read_limit() {
        let host = row("253:0", "/", "/", "xfs", "/dev/mapper/root");
        let mut data = String::new();
        for i in 0..7000 {
            data += &row(
                "0:55",
                "/",
                &format!(
                    "/custom/kubelet/pods/00000000-0000-0000-0000-{i:012}/volumes/kubernetes.io~csi/volume/mount"
                ),
                "nfs4",
                "server:/pod",
            );
            data += &row(
                "253:9",
                "/",
                &format!("/data/runtime/io.containerd.snapshotter.v1.devmapper/snapshots/{i}"),
                "ext4",
                "/dev/dm-9",
            );
        }
        data += &row("0:2", "/", "/run", "tmpfs", "tmpfs");
        data += &row("0:3", "/", "/containers/root", "overlay", "overlay");
        data += &row("7:0", "/", "/image", "ext4", "/dev/disk/by-label/image");
        data += &host;
        assert!(data.len() > 1024 * 1024);
        assert_eq!(selected(&data), selected(&host));
    }

    #[test]
    fn preserves_datasets_subvolumes_escaped_paths_and_similarly_named_business_dirs() {
        let data = row("0:90", "/@", "/", "btrfs", "/dev/sda")
            + &row("0:90", "/@home", "/home", "btrfs", "/dev/sda")
            + &row("0:91", "/", "/zfs", "zfs", "pool/data")
            + &row("0:92", "/", "/zfs-backup", "zfs", "pool/backup")
            + &row("8:7", "/", "/var/lib/kubelet-backup", "xfs", "/dev/sdc")
            + &row(
                "8:8",
                "/",
                r"/data/team\040share\134040",
                "ext4",
                "/dev/sdd",
            )
            + &row(
                "8:9",
                "/",
                "/business/pods/weekly/volumes",
                "ext4",
                "/dev/sde",
            );
        let rows = selected(&data);
        assert_eq!(rows.len(), 7);
        assert!(rows.iter().any(|m| m.target == r"/data/team share\040"));
        assert!(selected("malformed\n1 2 invalid / / rw - ext4 /dev/sda rw\n").is_empty());
        let reversed = data.lines().rev().collect::<Vec<_>>().join("\n");
        assert_eq!(rows, selected(&reversed));
    }
}
