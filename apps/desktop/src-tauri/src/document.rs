use crate::fsutil;
use crate::skills;
use crate::tools;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub const VERSION: u32 = 5;
/// Version 3 (no skills) and 4 (every skill shared, per-tool on/off) files are migrated on read.
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

fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ToolPreset {
    pub mode: Mode,
    #[serde(default)]
    pub text: String,
    /// Which skills the tool gets, with the same meaning as `mode` has for instructions.
    #[serde(default, skip_serializing_if = "is_default")]
    pub skill_mode: Mode,
    /// Skills from the library added for `append`, or the whole set for `custom`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skill_extras: Vec<String>,
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
    /// Skills every tool in `shared` or `append` skill mode gets.
    #[serde(default)]
    pub shared_skills: Vec<String>,
    /// The content of every skill referenced by `shared_skills` or a tool's `skill_extras`.
    #[serde(default)]
    pub skills: BTreeMap<String, Skill>,
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Presets {
    pub shared: String,
    pub tools: BTreeMap<String, ToolPreset>,
    #[serde(default)]
    pub shared_skills: Vec<String>,
    /// Skills to read from this computer before saving: skill name to the tool whose copy is used.
    #[serde(default)]
    pub skill_sources: BTreeMap<String, String>,
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

    /// The skills a tool's skills folder should hold, or `None` when its skills are unmanaged.
    pub fn skill_set(&self, id: &str) -> Option<BTreeMap<&str, &Skill>> {
        let preset = self.tools.get(id);
        let extras = preset.map(|preset| preset.skill_extras.as_slice()).unwrap_or_default();
        let names: Vec<&String> = match preset.map(|preset| preset.skill_mode).unwrap_or_default() {
            Mode::Off => return None,
            Mode::Shared => self.shared_skills.iter().collect(),
            Mode::Append => self.shared_skills.iter().chain(extras).collect(),
            Mode::Custom => extras.iter().collect(),
        };
        Some(
            names
                .into_iter()
                .filter_map(|name| self.skills.get_key_value(name))
                .map(|(name, skill)| (name.as_str(), skill))
                .collect(),
        )
    }

    fn referenced_skills(&self) -> BTreeSet<&String> {
        self.shared_skills
            .iter()
            .chain(self.tools.values().flat_map(|preset| &preset.skill_extras))
            .collect()
    }
}

fn validate_skills(document: &SyncDocument) -> Result<(), String> {
    document.skills.iter().try_for_each(|(name, skill)| skills::validate(name, skill))?;
    match document.referenced_skills().into_iter().find(|name| !skills::valid_name(name)) {
        Some(name) => Err(format!("\"{name}\" is not a valid skill name")),
        None => Ok(()),
    }
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

/// Version 4 shared every skill with every tool unless the tool's `skills` switch was off.
fn migrate_v4(value: &mut serde_json::Value) {
    let names: Vec<serde_json::Value> = value
        .get("skills")
        .and_then(|skills| skills.as_object())
        .map(|skills| skills.keys().cloned().map(serde_json::Value::String).collect())
        .unwrap_or_default();
    value["sharedSkills"] = serde_json::Value::Array(names);
    if let Some(tools) = value.get_mut("tools").and_then(|tools| tools.as_object_mut()) {
        for preset in tools.values_mut().filter_map(|preset| preset.as_object_mut()) {
            if preset.remove("skills") == Some(serde_json::Value::Bool(false)) {
                preset.insert("skillMode".into(), "off".into());
            }
        }
    }
}

pub fn parse(content: &str) -> Result<SyncDocument, String> {
    let mut value: serde_json::Value =
        serde_json::from_str(content).map_err(|error| format!("The sync file is not valid JSON: {error}"))?;
    match value.get("version").and_then(|version| version.as_u64()) {
        Some(4) if value.is_object() => migrate_v4(&mut value),
        Some(v) if (OLDEST_VERSION..=u64::from(VERSION)).contains(&v) => {}
        Some(v) if v > u64::from(VERSION) => {
            return Err("The sync file was written by a newer Harness Sync. Update this app.".into())
        }
        _ => return Err("This is not a Harness Sync file".into()),
    }
    let mut document: SyncDocument = serde_json::from_value(value)
        .map_err(|error| format!("The sync file is malformed: {error}"))?;
    validate_presets(&document.shared, &document.tools)?;
    validate_skills(&document)?;
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
        shared_skills: Vec::new(),
        skills: BTreeMap::new(),
    };
    validate_presets(&document.shared, &document.tools)?;
    write(path, &document)?;
    Ok(document)
}

/// Saves new presets if the file is still at `expected_revision`.
///
/// `uploads` are skills read from this computer; they replace library entries of the same name.
/// Every referenced skill must then be in the library, and unreferenced entries are dropped.
pub fn save(
    path: &Path,
    expected_revision: u64,
    presets: Presets,
    uploads: BTreeMap<String, Skill>,
    device: &str,
) -> Result<SyncDocument, String> {
    validate_presets(&presets.shared, &presets.tools)?;
    let (mut document, _) = load(path)?;
    if document.revision != expected_revision {
        return Err(format!(
            "{} changed the presets while you were editing. Your edits were not saved; reload to see their version.",
            document.updated_by
        ));
    }
    let mut seen = BTreeSet::new();
    document.shared = presets.shared;
    document.shared_skills = presets.shared_skills.into_iter().filter(|name| seen.insert(name.clone())).collect();
    document.tools = presets
        .tools
        .into_iter()
        .map(|(id, mut preset)| {
            let mut seen = BTreeSet::new();
            preset.skill_extras.retain(|name| seen.insert(name.clone()));
            (id, preset)
        })
        .filter(|(_, preset)| *preset != ToolPreset::default())
        .collect();
    document.skills.extend(uploads);
    let referenced: BTreeSet<String> = document.referenced_skills().into_iter().cloned().collect();
    if let Some(missing) = referenced.iter().find(|name| !document.skills.contains_key(*name)) {
        return Err(format!("The {missing} skill is not in the sync file. Add it from a tool on this computer."));
    }
    document.skills.retain(|name, _| referenced.contains(name));
    validate_skills(&document)?;
    document.revision += 1;
    document.updated_at = fsutil::now_rfc3339();
    document.updated_by = device.to_owned();
    write(path, &document)?;
    Ok(document)
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
                .map(|(id, mode, text)| (id.to_string(), ToolPreset { mode: *mode, text: text.to_string(), ..Default::default() }))
                .collect(),
            ..Default::default()
        }
    }

    fn skill(text: &str) -> Skill {
        Skill { files: [("SKILL.md".to_string(), text.to_string())].into() }
    }

    fn document() -> SyncDocument {
        SyncDocument {
            version: VERSION,
            revision: 1,
            updated_at: String::new(),
            updated_by: String::new(),
            shared: "Shared rules\n".into(),
            tools: BTreeMap::new(),
            shared_skills: Vec::new(),
            skills: BTreeMap::new(),
        }
    }

    #[test]
    fn render_modes() {
        let mut document = document();
        document.tools.insert("claude".into(), ToolPreset { mode: Mode::Append, text: "Claude extra".into(), ..Default::default() });
        document.tools.insert("antigravity".into(), ToolPreset { mode: Mode::Custom, text: "Gemini only".into(), ..Default::default() });
        document.tools.insert("command-code".into(), ToolPreset { mode: Mode::Off, ..Default::default() });
        assert_eq!(document.render("codex").as_deref(), Some("Shared rules\n"));
        assert_eq!(document.render("claude").as_deref(), Some("Shared rules\n\nClaude extra\n"));
        assert_eq!(document.render("antigravity").as_deref(), Some("Gemini only\n"));
        assert_eq!(document.render("command-code"), None);
    }

    #[test]
    fn skill_sets_follow_modes() {
        let mut document = document();
        for name in ["a", "b", "c"] {
            document.skills.insert(name.into(), skill(name));
        }
        document.shared_skills = vec!["a".into(), "gone".into()];
        let extras = vec!["b".into()];
        document.tools.insert("claude".into(), ToolPreset { skill_mode: Mode::Append, skill_extras: extras.clone(), ..Default::default() });
        document.tools.insert("opencode".into(), ToolPreset { skill_mode: Mode::Custom, skill_extras: extras, ..Default::default() });
        document.tools.insert("antigravity".into(), ToolPreset { skill_mode: Mode::Off, ..Default::default() });
        let names = |id: &str| document.skill_set(id).map(|set| set.into_keys().collect::<Vec<_>>());
        assert_eq!(names("codex"), Some(vec!["a"]));
        assert_eq!(names("claude"), Some(vec!["a", "b"]));
        assert_eq!(names("opencode"), Some(vec!["b"]));
        assert_eq!(names("antigravity"), None);
    }

    #[test]
    fn save_round_trip_and_stale_revision() {
        let dir = fsutil::temp_dir("doc");
        let path = dir.join("harness-sync.json");
        let created = create(&path, "Hello".into(), "desk").unwrap();
        assert_eq!(created.revision, 1);
        assert!(create(&path, "Again".into(), "desk").is_err());

        let saved = save(&path, 1, presets("Hi", &[("antigravity", Mode::Custom, "G"), ("codex", Mode::Shared, "")]), BTreeMap::new(), "laptop").unwrap();
        assert_eq!(saved.revision, 2);
        assert_eq!(saved.updated_by, "laptop");
        // Default presets are not stored.
        assert!(!saved.tools.contains_key("codex"));
        assert_eq!(load(&path).unwrap().0, saved);

        // Sharing a skill needs its content; uploads provide it.
        let mut shared = presets("Hi", &[]);
        shared.shared_skills = vec!["greet".into(), "greet".into()];
        assert!(save(&path, 2, shared.clone(), BTreeMap::new(), "laptop").unwrap_err().contains("greet"));
        let uploads: BTreeMap<_, _> = [("greet".to_string(), skill("hi")), ("unused".to_string(), skill("x"))].into();
        let with_skill = save(&path, 2, shared.clone(), uploads, "laptop").unwrap();
        assert_eq!(with_skill.shared_skills, vec!["greet".to_string()]);
        assert_eq!(with_skill.skills.keys().collect::<Vec<_>>(), vec!["greet"]);
        // Later saves keep library content without re-uploading.
        let kept = save(&path, 3, shared, BTreeMap::new(), "laptop").unwrap();
        assert_eq!(kept.skills["greet"], skill("hi"));
        let removed = save(&path, 4, presets("Hi", &[]), BTreeMap::new(), "laptop").unwrap();
        assert!(removed.skills.is_empty());

        let before = std::fs::read(&path).unwrap();
        let error = save(&path, 1, presets("Lost", &[]), BTreeMap::new(), "desk").unwrap_err();
        assert!(error.contains("laptop"));
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }

    #[test]
    fn migrates_version_4() {
        let doc = parse(r#"{"version":4,"revision":2,"updatedAt":"","updatedBy":"a","shared":"x",
            "tools":{"codex":{"mode":"shared","skills":false},"claude":{"mode":"custom","text":"c"}},
            "skills":{"review":{"files":{"SKILL.md":"r"}}}}"#).unwrap();
        assert_eq!(doc.version, VERSION);
        assert_eq!(doc.shared_skills, vec!["review".to_string()]);
        assert_eq!(doc.preset("codex").skill_mode, Mode::Off);
        assert_eq!(doc.preset("claude").skill_mode, Mode::Shared);
        assert!(doc.skill_set("claude").unwrap().contains_key("review"));
    }

    #[test]
    fn rejects_bad_files() {
        assert!(parse("not json").is_err());
        assert!(parse(r#"{"version":2,"entries":{}}"#).is_err());
        assert!(parse(r#"{"version":6}"#).unwrap_err().contains("newer"));
        assert!(parse(r#"{"version":3,"revision":1,"updatedAt":"","updatedBy":"","shared":"","tools":{"vim":{"mode":"shared"}}}"#).is_err());
        assert!(parse(r#"{"version":3,"revision":1,"updatedAt":"","updatedBy":"","shared":"","tools":{"codex":{"mode":"weird"}}}"#).is_err());
        let ok = parse(r#"{"version":3,"revision":7,"updatedAt":"","updatedBy":"a","shared":"x"}"#).unwrap();
        assert_eq!(ok.preset("codex"), ToolPreset::default());
        assert_eq!(ok.version, VERSION);
        assert!(parse(r#"{"version":5,"revision":1,"updatedAt":"","updatedBy":"","shared":"","skills":{"../x":{"files":{"SKILL.md":"x"}}}}"#).is_err());
        assert!(parse(r#"{"version":5,"revision":1,"updatedAt":"","updatedBy":"","shared":"","skills":{"x":{"files":{"SKILL.md":"x","../../evil":"x"}}}}"#).is_err());
        assert!(parse(r#"{"version":5,"revision":1,"updatedAt":"","updatedBy":"","shared":"","sharedSkills":["../x"]}"#).is_err());
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
