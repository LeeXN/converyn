//! Instance capabilities and workflow-scoped, additive role bindings.
use super::personnel::Registry;
use crate::domain::valid_id;
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Viewer,
    Executor,
    Maintainer,
    PlatformOperator,
    AccessAdmin,
    SuperAdmin,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    View,
    Preview,
    Execute,
    Maintain,
    Control,
    Platform,
    Access,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "uid",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Subject {
    Person(String),
    Group(String),
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Group {
    pub uid: String,
    pub name: String,
    pub members: BTreeSet<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub uid: String,
    pub subject: Subject,
    pub role: Role,
    /// None grants all workflows (and is required for platform roles).
    pub workflow: Option<String>,
    /// Empty means all presets. Maintainers must have all presets.
    #[serde(default)]
    pub presets: BTreeSet<String>,
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Access {
    pub revision: u64,
    pub groups: BTreeMap<String, Group>,
    pub bindings: BTreeMap<String, Binding>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Change {
    PutGroup {
        group: Group,
        expected_revision: u64,
    },
    DeleteGroup {
        uid: String,
        expected_revision: u64,
    },
    PutBinding {
        binding: Binding,
        expected_revision: u64,
    },
    DeleteBinding {
        uid: String,
        expected_revision: u64,
    },
}
impl Registry {
    fn assigned_bindings_for<'a>(&'a self, uid: &'a str) -> impl Iterator<Item = &'a Binding> {
        self.access
            .bindings
            .values()
            .filter(move |b| match &b.subject {
                Subject::Person(person) => person == uid,
                Subject::Group(group) => self
                    .access
                    .groups
                    .get(group)
                    .is_some_and(|g| g.members.contains(uid)),
            })
    }
    pub fn bindings_for<'a>(&'a self, uid: &'a str) -> impl Iterator<Item = &'a Binding> {
        let active = self.identities.get(uid).is_some_and(|p| !p.disabled);
        self.assigned_bindings_for(uid).filter(move |_| active)
    }
    /// Disabling a privileged identity must not make it editable by a lesser role.
    pub fn assigned_super_admin(&self, uid: &str) -> bool {
        self.assigned_bindings_for(uid)
            .any(|b| b.role == Role::SuperAdmin)
    }
    pub fn allows(
        &self,
        uid: &str,
        action: Action,
        workflow: Option<&str>,
        preset: Option<&str>,
    ) -> bool {
        self.bindings_for(uid).any(|b| {
            if b.role == Role::SuperAdmin {
                return true;
            }
            match action {
                Action::Platform => return b.role == Role::PlatformOperator,
                Action::Access => return b.role == Role::AccessAdmin,
                _ => {}
            }
            let role = match action {
                Action::View => matches!(b.role, Role::Viewer | Role::Executor | Role::Maintainer),
                Action::Preview | Action::Execute => {
                    matches!(b.role, Role::Executor | Role::Maintainer)
                }
                Action::Maintain | Action::Control => b.role == Role::Maintainer,
                _ => false,
            };
            role && b.workflow.as_deref().is_none_or(|w| Some(w) == workflow)
                && (b.presets.is_empty() || preset.is_some_and(|p| b.presets.contains(p)))
        })
    }
    pub fn super_admin(&self, uid: &str) -> bool {
        self.bindings_for(uid).any(|b| b.role == Role::SuperAdmin)
    }
    pub fn active_super_admins(&self) -> usize {
        self.identities
            .keys()
            .filter(|uid| self.super_admin(uid))
            .count()
    }
    pub fn access_change(&mut self, change: Change) -> Result<()> {
        let expected = match &change {
            Change::PutGroup {
                expected_revision, ..
            }
            | Change::DeleteGroup {
                expected_revision, ..
            }
            | Change::PutBinding {
                expected_revision, ..
            }
            | Change::DeleteBinding {
                expected_revision, ..
            } => *expected_revision,
        };
        ensure!(self.access.revision == expected, "access_revision_conflict");
        let had_admin = self.active_super_admins() > 0;
        let mut next = self.clone();
        match change {
            Change::PutGroup { group, .. } => {
                ensure!(
                    valid_id(&group.uid)
                        && !group.name.trim().is_empty()
                        && group.name.len() <= 256
                        && group.members.len() <= 10000,
                    "invalid_group"
                );
                ensure!(
                    group
                        .members
                        .iter()
                        .all(|uid| next.identities.contains_key(uid)),
                    "unknown_group_member"
                );
                ensure!(
                    next.access.groups.contains_key(&group.uid) || next.access.groups.len() < 1000,
                    "group_limit"
                );
                next.access.groups.insert(group.uid.clone(), group);
            }
            Change::DeleteGroup { uid, .. } => {
                ensure!(
                    !next
                        .access
                        .bindings
                        .values()
                        .any(|b| b.subject == Subject::Group(uid.clone())),
                    "group_has_bindings"
                );
                ensure!(next.access.groups.remove(&uid).is_some(), "group_not_found");
            }
            Change::PutBinding { binding: b, .. } => {
                ensure!(
                    valid_id(&b.uid)
                        && b.workflow.as_ref().is_none_or(|w| valid_id(w))
                        && b.presets.len() <= 1000
                        && b.presets.iter().all(|p| valid_id(p)),
                    "invalid_binding"
                );
                ensure!(
                    match &b.subject {
                        Subject::Person(p) => next.identities.contains_key(p),
                        Subject::Group(g) => next.access.groups.contains_key(g),
                    },
                    "unknown_subject"
                );
                let platform = matches!(
                    b.role,
                    Role::PlatformOperator | Role::AccessAdmin | Role::SuperAdmin
                );
                ensure!(
                    !platform || (b.workflow.is_none() && b.presets.is_empty()),
                    "platform_role_requires_instance_scope"
                );
                ensure!(
                    b.presets.is_empty() || (b.workflow.is_some() && b.role != Role::Maintainer),
                    "preset_scope_requires_one_workflow_and_non_maintainer_role"
                );
                ensure!(
                    next.access.bindings.contains_key(&b.uid) || next.access.bindings.len() < 10000,
                    "binding_limit"
                );
                next.access.bindings.insert(b.uid.clone(), b);
            }
            Change::DeleteBinding { uid, .. } => {
                ensure!(
                    next.access.bindings.remove(&uid).is_some(),
                    "binding_not_found"
                );
            }
        }
        ensure!(
            !had_admin || next.active_super_admins() > 0,
            "last_super_admin_cannot_be_removed"
        );
        next.access.revision += 1;
        *self = next;
        Ok(())
    }
    pub fn grant_initial(&mut self, uid: &str, role: Role) {
        let id = format!("initial-{uid}");
        self.access.bindings.insert(
            id.clone(),
            Binding {
                uid: id,
                subject: Subject::Person(uid.into()),
                role,
                workflow: None,
                presets: BTreeSet::new(),
            },
        );
        self.access.revision += 1;
    }
}
