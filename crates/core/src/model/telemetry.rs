//! Optional, bounded host observations. These never participate in node identity.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const INVENTORY_LIMIT: usize = 64 * 1024;
pub const RESOURCE_LIMIT: usize = 16 * 1024;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub collected_ms: u64,
    pub data: BTreeMap<String, serde_json::Value>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub errors: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub truncated: bool,
}
impl Snapshot {
    pub fn valid(&self, limit: usize) -> bool {
        serde_json::to_vec(self).is_ok_and(|b| b.len() <= limit)
    }
    pub fn bound(&mut self, limit: usize) {
        while !self.valid(limit) {
            self.truncated = true;
            // Kubernetes mount/device arrays must not evict small CPU and memory
            // metrics merely because those keys sort after `filesystems`.
            let largest = self
                .data
                .iter()
                .filter(|(_, value)| value.as_array().is_some_and(|v| !v.is_empty()))
                .max_by_key(|(_, value)| serde_json::to_vec(value).map_or(0, |v| v.len()))
                .map(|(key, _)| key.clone());
            if let Some(key) = largest {
                let rows = self.data.get_mut(&key).unwrap().as_array_mut().unwrap();
                rows.truncate(rows.len() / 2);
                self.errors.insert(
                    key,
                    "observation size limit exceeded; some entries omitted".into(),
                );
                continue;
            }
            if let Some((key, _)) = self.data.pop_last() {
                self.errors
                    .insert(key, "observation size limit exceeded; omitted".into());
            } else if self.errors.pop_last().is_none() {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn large_mount_arrays_preserve_basic_resource_metrics() {
        let mut snapshot = Snapshot {
            collected_ms: 1,
            data: BTreeMap::from([
                ("cpu_used_basis_points".into(), json!(1200)),
                ("memory_total_bytes".into(), json!(32_u64 << 30)),
                ("memory_available_bytes".into(), json!(8_u64 << 30)),
                ("load".into(), json!(["0.1", "0.2", "0.3"])),
                ("filesystems".into(), json!((0..512).map(|i| json!({"mount":format!("/var/lib/kubelet/pods/{i}/{}", "x".repeat(160))})).collect::<Vec<_>>())),
            ]),
            ..Snapshot::default()
        };
        snapshot.bound(RESOURCE_LIMIT);
        assert!(snapshot.valid(RESOURCE_LIMIT));
        assert!(snapshot.truncated);
        assert_eq!(snapshot.data["memory_total_bytes"], json!(32_u64 << 30));
        assert_eq!(snapshot.data["memory_available_bytes"], json!(8_u64 << 30));
        assert_eq!(snapshot.data["cpu_used_basis_points"], 1200);
        assert_eq!(snapshot.data["load"], json!(["0.1", "0.2", "0.3"]));
        assert!(!snapshot.data["filesystems"].as_array().unwrap().is_empty());
        assert!(snapshot.errors.contains_key("filesystems"));
        let before = snapshot.clone();
        snapshot.bound(RESOURCE_LIMIT);
        assert_eq!(snapshot, before);
    }
}
