use crate::fsutil;
use crate::skills;
use crate::tools;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

pub const VERSION: u32 = 4;
/// Version 3 files have no skills and are read as version 4.
const OLDEST_VERSION: u64 = 3;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// The shared preset.
    #[default]
    Shared,
    /// The shared preset followed by this tool's extra text.
    Append,
    /// This tool's own preset only.
    Custom,
    /// Not managed by Harness Sync.
    Off,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ToolPreset {
    pub mode: Mode,
    #[serde(default)]
    pub text: String,
    /// Whether synced skills are copied into this tool's skills folder.
    #[serde(default = "yes", skip_serializing_if = "is_yes")]
    pub skills: bool,
}

fn yes() -> bool {
    true
}

fn is_yes(value: &bool) -> bool {
    *value
}

impl Default for ToolPreset {
    fn default() -> Self {
        Self { mode: Mode::default(), text: String::new(), skills: true }
    }
}

/// One skill folder: relative `/`-separated paths mapped to their text.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Skill {
    pub files: BTreeMap<String, String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SyncDocument {
    pub version: u32,
    pub revision: u64,
    pub updated_at: String,
    pub updated_by: String,
    pub shared: String,
    #[serde(default)]
    pub tools: BTreeMap<String, ToolPreset>,
    #[serde(default)]
    pub skills: BTreeMap<String, Skill>,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Presets {
    pub shared: String,
    pub tools: BTreeMap<String, ToolPreset>,
}

impl SyncDocument {
    pub fn preset(&self, id: &str) -> ToolPreset {
        self.tools.get(id).cloned().unwrap_or_default()
    }

    /// The text a tool's instructions file should contain, or `None` when unmanaged.
    pub fn render(&self, id: &str) -> Option<String> {
        let preset = self.preset(id);
        let text = match preset.mode {
            Mode::Off => return None,
            Mode::Shared => self.shared.clone(),
            Mode::Custom => preset.text,
            Mode::Append if preset.text.trim().is_empty() => self.shared.clone(),
            Mode::Append if self.shared.trim().is_empty() => preset.text,
            Mode::Append => format!("{}\n\n{}", self.shared.trim_end(), preset.text.trim_start()),
        };
        Some(if text.is_empty() || text.ends_with('\n') {
            text
        } else {
            text + "\n"
        })
    }
}

fn validate_skills(skills: &BTreeMap<String, Skill>) -> Result<(), String> {
    skills.iter().try_for_each(|(name, skill)| skills::validate(name, skill))
}

fn validate_presets(shared: &str, tools: &BTreeMap<String, ToolPreset>) -> Result<(), String> {
    if shared.contains('\0') || tools.values().any(|preset| preset.text.contains('\0')) {
        return Err("Presets cannot contain NUL characters".into());
    }
    if let Some(unknown) = tools.keys().find(|id| tools::find(id).is_none()) {
        return Err(format!("Unknown tool \"{unknown}\" in the sync file"));
    }
    Ok(())
}

pub fn parse(content: &str) -> Result<SyncDocument, String> {
    let value: serde_json::Value =
        serde_json::from_str(content).map_err(|error| format!("The sync file is not valid JSON: {error}"))?;
    match value.get("version").and_then(|version| version.as_u64()) {
        Some(v) if (OLDEST_VERSION..=u64::from(VERSION)).contains(&v) => {}
        Some(v) if v > u64::from(VERSION) => {
            return Err("The sync file was written by a newer Harness Sync. Update this app.".into())
        }
        _ => return Err("This is not a Harness Sync file".into()),
    }
    let mut document: SyncDocument = serde_json::from_value(value)
        .map_err(|error| format!("The sync file is malformed: {error}"))?;
    validate_presets(&document.shared, &document.tools)?;
    validate_skills(&document.skills)?;
    document.version = VERSION;
    Ok(document)
}

/// Loads the document and the hash of its exact bytes.
pub fn load(path: &Path) -> Result<(SyncDocument, String), String> {
    let content = fsutil::read_text_limited(path, fsutil::MAX_SYNC_FILE_BYTES)
        .map_err(|error| format!("The sync file {error}"))?
        .ok_or("The sync file was not found. Check that Google Drive is running.")?;
    Ok((parse(&content)?, fsutil::hash(content.as_bytes())))
}

fn write(path: &Path, document: &SyncDocument) -> Result<(), String> {
    let mut content = serde_json::to_string_pretty(document).map_err(|error| error.to_string())?;
    content.push('\n');
    if content.len() as u64 > fsutil::MAX_SYNC_FILE_BYTES {
        return Err("Presets and skills are too large; the sync file is limited to 8 MB".into());
    }
    fsutil::write_atomic(path, &content)
}

pub fn create(path: &Path, shared: String, device: &str) -> Result<SyncDocument, String> {
    if let Ok(Some(existing)) = fsutil::read_text(path) {
        if parse(&existing).is_ok() {
            return Err("That file is already a sync file. Use Open existing file instead.".into());
        }
    }
    let document = SyncDocument {
        version: VERSION,
        revision: 1,
        updated_at: fsutil::now_rfc3339(),
        updated_by: device.to_owned(),
        shared,
        tools: BTreeMap::new(),
        skills: BTreeMap::new(),
    };
    validate_presets(&document.shared, &document.tools)?;
    write(path, &document)?;
    Ok(document)
}

/// Rewrites the file as the next revision if it is still at `expected_revision`.
fn update(
    path: &Path,
    expected_revision: u64,
    device: &str,
    change: impl FnOnce(&mut SyncDocument),
) -> Result<SyncDocument, String> {
    let (mut document, _) = load(path)?;
    if document.revision != expected_revision {
        return Err(format!(
            "{} changed the presets while you were editing. Your edits were not saved; reload to see their version.",
            document.updated_by
        ));
    }
    change(&mut document);
    validate_skills(&document.skills)?;
    document.revision += 1;
    document.updated_at = fsutil::now_rfc3339();
    document.updated_by = device.to_owned();
    write(path, &document)?;
    Ok(document)
}

/// Saves new presets if the file is still at `expected_revision`. Skills are kept.
pub fn save(
    path: &Path,
    expected_revision: u64,
    presets: Presets,
    device: &str,
) -> Result<SyncDocument, String> {
    validate_presets(&presets.shared, &presets.tools)?;
    update(path, expected_revision, device, |document| {
        document.shared = presets.shared;
        document.tools = presets
            .tools
            .into_iter()
            .filter(|(_, preset)| *preset != ToolPreset::default())
            .collect();
    })
}

/// Adds or replaces one synced skill, or removes it when `skill` is `None`.
pub fn save_skill(
    path: &Path,
    expected_revision: u64,
    name: &str,
    skill: Option<Skill>,
    device: &str,
) -> Result<SyncDocument, String> {
    update(path, expected_revision, device, |document| {
        match skill {
            Some(skill) => document.skills.insert(name.to_owned(), skill),
            None => document.skills.remove(name),
        };
    })
}

/// Copies Google Drive created beside the sync file after conflicting edits.
pub fn conflict_copies(path: &Path) -> Vec<String> {
    let (Some(dir), Some(stem), Some(name)) = (
        path.parent(),
        path.file_stem().and_then(|stem| stem.to_str()),
        path.file_name().and_then(|name| name.to_str()),
    ) else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut copies: Vec<String> = entries
        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
        .filter(|other| {
            other != name
                && other.starts_with(stem)
                && other.to_ascii_lowercase().ends_with(".json")
                && (other.contains('(') || other.to_ascii_lowercase().contains("conflict"))
        })
        .collect();
    copies.sort();
    copies
}

#[cfg(test)]
mod tests {
    use super::*;

    fn presets(shared: &str, tools: &[(&str, Mode, &str)]) -> Presets {
        Presets {
            shared: shared.into(),
            tools: tools
                .iter()
                .map(|(id, mode, text)| (id.to_string(), ToolPreset { mode: *mode, text: text.to_string(), skills: true }))
                .collect(),
        }
    }

    #[test]
    fn render_modes() {
        let mut document = SyncDocument {
            version: VERSION,
            revision: 1,
            updated_at: String::new(),
            updated_by: String::new(),
            shared: "Shared rules\n".into(),
            tools: BTreeMap::new(),
            skills: BTreeMap::new(),
        };
        document.tools.insert("claude".into(), ToolPreset { mode: Mode::Append, text: "Claude extra".into(), skills: true });
        document.tools.insert("antigravity".into(), ToolPreset { mode: Mode::Custom, text: "Gemini only".into(), skills: true });
        document.tools.insert("command-code".into(), ToolPreset { mode: Mode::Off, text: String::new(), skills: true });
        assert_eq!(document.render("codex").as_deref(), Some("Shared rules\n"));
        assert_eq!(document.render("claude").as_deref(), Some("Shared rules\n\nClaude extra\n"));
        assert_eq!(document.render("antigravity").as_deref(), Some("Gemini only\n"));
        assert_eq!(document.render("command-code"), None);
    }

    #[test]
    fn save_round_trip_and_stale_revision() {
        let dir = fsutil::temp_dir("doc");
        let path = dir.join("harness-sync.json");
        let created = create(&path, "Hello".into(), "desk").unwrap();
        assert_eq!(created.revision, 1);
        assert!(create(&path, "Again".into(), "desk").is_err());

        let saved = save(&path, 1, presets("Hi", &[("antigravity", Mode::Custom, "G"), ("codex", Mode::Shared, "")]), "laptop").unwrap();
        assert_eq!(saved.revision, 2);
        assert_eq!(saved.updated_by, "laptop");
        // Default presets are not stored.
        assert!(!saved.tools.contains_key("codex"));
        assert_eq!(load(&path).unwrap().0, saved);

        let skill = Skill { files: [("SKILL.md".to_string(), "hi".to_string())].into() };
        let with_skill = save_skill(&path, 2, "greet", Some(skill.clone()), "laptop").unwrap();
        assert_eq!(with_skill.skills["greet"], skill);
        // Saving presets keeps skills.
        let kept = save(&path, 3, presets("Hi", &[]), "laptop").unwrap();
        assert_eq!(kept.skills["greet"], skill);
        let removed = save_skill(&path, 4, "greet", None, "laptop").unwrap();
        assert!(removed.skills.is_empty());

        let before = std::fs::read(&path).unwrap();
        let error = save(&path, 1, presets("Lost", &[]), "desk").unwrap_err();
        assert!(error.contains("laptop"));
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }

    #[test]
    fn rejects_bad_files() {
        assert!(parse("not json").is_err());
        assert!(parse(r#"{"version":2,"entries":{}}"#).is_err());
        assert!(parse(r#"{"version":5}"#).unwrap_err().contains("newer"));
        assert!(parse(r#"{"version":3,"revision":1,"updatedAt":"","updatedBy":"","shared":"","tools":{"vim":{"mode":"shared"}}}"#).is_err());
        assert!(parse(r#"{"version":3,"revision":1,"updatedAt":"","updatedBy":"","shared":"","tools":{"codex":{"mode":"weird"}}}"#).is_err());
        let ok = parse(r#"{"version":3,"revision":7,"updatedAt":"","updatedBy":"a","shared":"x"}"#).unwrap();
        assert_eq!(ok.preset("codex"), ToolPreset::default());
        assert_eq!(ok.version, VERSION);
        assert!(parse(r#"{"version":4,"revision":1,"updatedAt":"","updatedBy":"","shared":"","skills":{"../x":{"files":{"SKILL.md":"x"}}}}"#).is_err());
        assert!(parse(r#"{"version":4,"revision":1,"updatedAt":"","updatedBy":"","shared":"","skills":{"x":{"files":{"SKILL.md":"x","../../evil":"x"}}}}"#).is_err());
    }

    #[test]
    fn finds_drive_conflict_copies() {
        let dir = fsutil::temp_dir("conflict");
        let path = dir.join("harness-sync.json");
        for name in ["harness-sync.json", "harness-sync (1).json", "other.json", "harness-sync.json.bak"] {
            std::fs::write(dir.join(name), "{}").unwrap();
        }
        assert_eq!(conflict_copies(&path), vec!["harness-sync (1).json".to_string()]);
    }
}
