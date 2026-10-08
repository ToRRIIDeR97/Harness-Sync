use crate::fsutil;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Per-computer settings kept in the app data directory. Never synced.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct LocalState {
    pub sync_file: Option<PathBuf>,
    pub device_name: Option<String>,
    /// Hash of the text Harness Sync last wrote or confirmed for each tool.
    #[serde(default)]
    pub applied: BTreeMap<String, String>,
    /// Hash of each skill Harness Sync last wrote or confirmed, keyed by `tool/skill`.
    #[serde(default)]
    pub applied_skills: BTreeMap<String, String>,
    /// Skill copies Harness Sync created or replaced, keyed by `tool/skill`. Only these are ever deleted.
    #[serde(default)]
    pub written_skills: BTreeSet<String>,
    /// Hash of the sync file bytes last applied.
    pub last_file_hash: Option<String>,
}

pub fn default_device_name() -> String {
    ["COMPUTERNAME", "HOSTNAME"]
        .iter()
        .filter_map(|key| std::env::var(key).ok())
        .map(|name| name.trim().to_owned())
        .find(|name| !name.is_empty())
        .unwrap_or_else(|| "This computer".into())
}

impl LocalState {
    pub fn load(path: &Path) -> Self {
        fsutil::read_text(path)
            .ok()
            .flatten()
            .and_then(|content| serde_json::from_str(&content).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let content = serde_json::to_string_pretty(self).map_err(|error| error.to_string())?;
        fsutil::write_atomic(path, &content)
    }

    pub fn device(&self) -> String {
        self.device_name.clone().unwrap_or_else(default_device_name)
    }
}
