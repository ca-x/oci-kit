use std::path::{Path, PathBuf};

use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

use crate::staging::{StagingRoot, relative_path};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub schema_version: u32,
    pub remove: Vec<Removal>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Removal {
    pub path: String,
    pub reason: String,
}

#[derive(Debug, Serialize)]
pub struct PlannedRemoval {
    pub path: PathBuf,
    pub reason: String,
    pub kind: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CleanupPlan {
    pub root: PathBuf,
    pub applied: bool,
    pub remove: Vec<PlannedRemoval>,
}

pub fn clean(root: &Path, policy: &Policy, apply: bool) -> Result<CleanupPlan> {
    ensure!(
        policy.schema_version == 1,
        "unsupported cleanup policy version"
    );
    let root = StagingRoot::open(root)?;
    let mut remove = Vec::new();
    for rule in &policy.remove {
        ensure!(
            !rule.reason.trim().is_empty(),
            "every removal needs a reason"
        );
        let path = relative_path(Path::new(&rule.path))?;
        remove.push(PlannedRemoval {
            kind: root.kind(&path)?.map(str::to_owned),
            path,
            reason: rule.reason.clone(),
        });
    }
    remove.sort_by(|left, right| {
        left.path
            .components()
            .count()
            .cmp(&right.path.components().count())
            .then_with(|| left.path.cmp(&right.path))
    });
    let mut selected: Vec<PlannedRemoval> = Vec::new();
    for removal in remove {
        if !selected
            .iter()
            .any(|parent| removal.path.starts_with(&parent.path))
        {
            selected.push(removal);
        }
    }
    if apply {
        for removal in &selected {
            root.check_path(&removal.path)?;
        }
        for removal in &selected {
            root.remove(&removal.path)?;
        }
    }
    Ok(CleanupPlan {
        root: root.path().into(),
        applied: apply,
        remove: selected,
    })
}
