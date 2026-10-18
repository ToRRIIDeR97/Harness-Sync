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
    /// Loads local settings. An unreadable file is moved aside and reported, so it is never overwritten.
    pub fn load(path: &Path) -> (Self, Option<String>) {
        let parsed = fsutil::read_text(path)
            .and_then(|content| content.map(|text| serde_json::from_str(&text).map_err(|error| error.to_string())).transpose());
        let error = match parsed {
            Ok(state) => return (state.unwrap_or_default(), None),
            Err(error) => error,
        };
        let name = path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
        let aside = path.with_file_name(format!("{name}.unreadable-{}", fsutil::now_seconds()));
        let warning = match std::fs::rename(path, &aside) {
            Ok(()) => format!(
                "Local settings could not be read and were reset. The old copy is {}.",
                aside.file_name().unwrap_or_default().to_string_lossy()
            ),
            Err(rename) => format!("Local settings could not be read ({error}) or moved aside ({rename}) and were reset."),
        };
        (Self::default(), Some(warning))
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let content = serde_json::to_string_pretty(self).map_err(|error| error.to_string())?;
        fsutil::write_atomic(path, &content)
    }

    pub fn device(&self) -> String {
        self.device_name.clone().unwrap_or_else(default_device_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unreadable_state_is_moved_aside_and_reported() {
        let dir = fsutil::temp_dir("state");
        let path = dir.join("state.json");
        assert!(LocalState::load(&path).1.is_none());
        std::fs::write(&path, "{ not json").unwrap();
        let (state, warning) = LocalState::load(&path);
        assert!(state.sync_file.is_none());
        let warning = warning.unwrap();
        assert!(!path.exists());
        let moved: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|entry| entry.unwrap().file_name().into_string().unwrap()).collect();
        assert_eq!(moved.len(), 1);
        assert!(moved[0].starts_with("state.json.unreadable-") && warning.contains(&moved[0]));
        assert_eq!(std::fs::read_to_string(dir.join(&moved[0])).unwrap(), "{ not json");
    }
}
