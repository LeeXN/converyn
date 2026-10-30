//! Workflow-local labels and ordered host selections. Enrollment remains global.
use crate::model::domain::{MAX_NODES, valid_id};
use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Host {
    pub uid: String,
    pub address: String,
    pub hostname: String,
    #[serde(default)]
    pub values: super::values::Values,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Inventory {
    pub nodes: Vec<Host>,
    #[serde(default)]
    pub groups: BTreeMap<String, Vec<String>>,
}

impl Inventory {
    /// Restrict public topology and Agent context to a frozen target selection.
    pub fn restricted(&self, ids: &BTreeSet<String>) -> Self {
        let mut inventory = self.clone();
        inventory.nodes.retain(|n| ids.contains(&n.uid));
        inventory.groups.retain(|_, members| {
            let declared_empty = members.is_empty();
            members.retain(|id| ids.contains(id));
            declared_empty || !members.is_empty()
        });
        inventory
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(
            (1..=MAX_NODES).contains(&self.nodes.len()),
            "invalid_inventory_node_count"
        );
        let mut ids = BTreeSet::new();
        let mut addresses = BTreeSet::new();
        for node in &self.nodes {
            ensure!(
                valid_id(&node.uid) && valid_id(&node.hostname) && ids.insert(node.uid.clone()),
                "invalid_or_duplicate_inventory_node"
            );
            let address: std::net::IpAddr = node.address.parse()?;
            ensure!(addresses.insert(address), "duplicate_inventory_address");
            super::values::validate(&node.values)?;
        }
        ensure!(
            self.groups.len() <= 64 && !self.groups.contains_key("all"),
            "invalid_inventory_groups"
        );
        for (name, members) in &self.groups {
            ensure!(valid_id(name), "invalid_inventory_group");
            let unique: BTreeSet<_> = members.iter().collect();
            ensure!(
                unique.len() == members.len() && members.iter().all(|id| ids.contains(id)),
                "invalid_inventory_group_members"
            );
        }
        for node in &self.nodes {
            self.context(&node.uid)?;
        }
        Ok(())
    }

    pub fn context(&self, uid: &str) -> Result<String> {
        let node = self
            .nodes
            .iter()
            .find(|n| n.uid == uid)
            .ok_or_else(|| anyhow::anyhow!("node_not_in_inventory"))?;
        let mut groups = self.groups.clone();
        groups.insert(
            "all".into(),
            self.nodes.iter().map(|n| n.uid.clone()).collect(),
        );
        let labels: Vec<_> = groups
            .iter()
            .filter(|(_, members)| members.iter().any(|member| member == uid))
            .map(|(name, _)| name)
            .collect();
        let nodes: Vec<_> = self
            .nodes
            .iter()
            .map(|n| serde_json::json!({"uid":n.uid,"address":n.address,"hostname":n.hostname}))
            .collect();
        let text = serde_json::to_string(
            &serde_json::json!({"node":{"uid":node.uid,"address":node.address,"hostname":node.hostname,"groups":labels},"nodes":nodes,"groups":groups}),
        )?;
        ensure!(text.len() <= 65536, "inventory_context_exceeds_64KiB");
        Ok(text)
    }

    pub fn select(&self, expression: &str) -> Result<Vec<String>> {
        let (name, index) = match expression.split_once('[') {
            Some((name, rest)) => (
                name,
                Some(
                    rest.strip_suffix(']')
                        .ok_or_else(|| anyhow::anyhow!("invalid_host_selector"))?,
                ),
            ),
            None => (expression, None),
        };
        let members = if name == "all" {
            self.nodes.iter().map(|n| n.uid.clone()).collect()
        } else {
            self.groups
                .get(name)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("unknown_host_group:{name}"))?
        };
        let Some(index) = index else {
            return Ok(members);
        };
        if index == "-1" {
            return Ok(vec![
                members
                    .last()
                    .ok_or_else(|| anyhow::anyhow!("host_selector_out_of_bounds"))?
                    .clone(),
            ]);
        }
        if let Some((start, end)) = index.split_once('-') {
            ensure!(
                !start.is_empty()
                    && !end.is_empty()
                    && start.bytes().chain(end.bytes()).all(|c| c.is_ascii_digit()),
                "invalid_host_selector"
            );
            let start: usize = start.parse()?;
            let end: usize = end.parse()?;
            ensure!(
                start <= end && end < members.len(),
                "host_selector_out_of_bounds"
            );
            return Ok(members[start..=end].to_vec());
        }
        ensure!(
            !index.is_empty() && index.bytes().all(|c| c.is_ascii_digit()),
            "invalid_host_selector"
        );
        let i: usize = index
            .parse()
            .map_err(|_| anyhow::anyhow!("invalid_host_selector"))?;
        match members.get(i) {
            Some(id) => Ok(vec![id.clone()]),
            None => bail!("host_selector_out_of_bounds"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PackageFile {
    pub sha256: String,
    #[serde(default)]
    pub executable: bool,
}
pub fn valid_package_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && path.len() <= 256
        && path.split('/').all(|p| {
            !p.is_empty()
                && p != "."
                && p != ".."
                && p.bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
        })
}

pub fn validate_bundle(files: &BTreeMap<String, PackageFile>) -> Result<()> {
    ensure!(files.len() <= 4096, "too_many_bundle_files");
    for path in files.keys() {
        ensure!(valid_package_path(path), "invalid_bundle_file");
        for (index, _) in path.match_indices('/') {
            ensure!(
                !files.contains_key(&path[..index]),
                "bundle_file_directory_conflict"
            );
        }
    }
    Ok(())
}
