use crate::document::{Skill, SyncDocument};
use crate::fsutil;
use crate::state::LocalState;
use crate::tools::{self, Locations};
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const MAX_FILES: usize = 200;
const ENTRY_FILE: &str = "SKILL.md";

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SkillState {
    InSync,
    /// The tool's copy differs and will be replaced on the next apply.
    Differs,
    Updated,
    Error,
    /// In the tool's folder but not part of its skill set.
    Local,
}

/// A skill in one tool's folder, or one the tool should get.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct LocalSkill {
    pub name: String,
    pub description: String,
    pub files: usize,
    /// Part of this tool's skill set.
    pub wanted: bool,
    pub state: SkillState,
    /// The copy changed since Harness Sync last wrote or confirmed it.
    pub edited_outside: bool,
    pub message: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ToolSkills {
    pub tool: &'static str,
    pub folder: String,
    pub skills: Vec<LocalSkill>,
}

/// A skill stored in the sync file.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct LibrarySkill {
    pub name: String,
    pub description: String,
    pub files: usize,
}

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct SkillsReport {
    pub library: Vec<LibrarySkill>,
    pub tools: Vec<ToolSkills>,
}

/// Folder names a skill may use: letters, digits, `-`, `_` and `.`, not starting with `.`.
pub fn valid_name(name: &str) -> bool {
    (1..=64).contains(&name.len())
        && !name.starts_with('.')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

fn valid_path(path: &str) -> bool {
    path.split('/').all(|part| {
        !part.is_empty()
            && !part.starts_with('.')
            && !part.chars().any(|c| matches!(c, '\\' | ':' | '\0') || c.is_control())
    })
}

/// Rejects anything that could write outside the skill folder.
pub fn validate(name: &str, skill: &Skill) -> Result<(), String> {
    if !valid_name(name) {
        return Err(format!("\"{name}\" is not a valid skill name"));
    }
    if !skill.files.contains_key(ENTRY_FILE) {
        return Err(format!("Skill \"{name}\" has no {ENTRY_FILE}"));
    }
    if skill.files.len() > MAX_FILES {
        return Err(format!("Skill \"{name}\" has more than {MAX_FILES} files"));
    }
    if let Some(bad) = skill.files.keys().find(|path| !valid_path(path)) {
        return Err(format!("Skill \"{name}\" has an invalid file path \"{bad}\""));
    }
    if skill.files.values().any(|text| text.contains('\0') || text.len() as u64 > fsutil::MAX_FILE_BYTES) {
        return Err(format!("Skill \"{name}\" has a file that is not text or is larger than 1 MB"));
    }
    Ok(())
}

pub fn hash(skill: &Skill) -> String {
    fsutil::hash(&serde_json::to_vec(&skill.files).unwrap_or_default())
}

/// The `description` from SKILL.md front matter.
pub fn description(skill: &Skill) -> String {
    let Some(text) = skill.files.get(ENTRY_FILE) else {
        return String::new();
    };
    let mut lines = text.lines();
    if lines.next().map(str::trim) != Some("---") {
        return String::new();
    }
    lines
        .take_while(|line| line.trim() != "---")
        .find_map(|line| line.strip_prefix("description:"))
        .map(|value| value.trim().trim_matches(|c| c == '"' || c == '\'').chars().take(400).collect())
        .unwrap_or_default()
}

/// Lists the regular, non-hidden files under `dir` as `/`-separated relative paths.
fn list_files(dir: &Path, prefix: &str, out: &mut Vec<String>) -> Result<(), String> {
    let entries = fs::read_dir(dir).map_err(|error| error.to_string())?;
    for entry in entries {
        let entry = entry.map_err(|error| error.to_string())?;
        let name = entry.file_name().into_string().map_err(|_| "has a file name that is not UTF-8".to_string())?;
        if name.starts_with('.') {
            continue;
        }
        let relative = if prefix.is_empty() { name } else { format!("{prefix}/{name}") };
        let kind = entry.file_type().map_err(|error| error.to_string())?;
        if kind.is_symlink() {
            return Err(format!("contains a symbolic link ({relative}), so Harness Sync leaves it alone"));
        } else if kind.is_dir() {
            list_files(&entry.path(), &relative, out)?;
        } else if kind.is_file() {
            out.push(relative);
        }
        if out.len() > MAX_FILES {
            return Err(format!("has more than {MAX_FILES} files"));
        }
    }
    Ok(())
}

fn join(dir: &Path, relative: &str) -> PathBuf {
    relative.split('/').fold(dir.to_path_buf(), |path, part| path.join(part))
}

/// Reads a skill folder. `Ok(None)` means it does not exist.
pub fn read(dir: &Path) -> Result<Option<Skill>, String> {
    match fs::symlink_metadata(dir) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err("is a symbolic link, so Harness Sync leaves it alone".into())
        }
        Ok(metadata) if !metadata.is_dir() => return Err("is not a folder".into()),
        Ok(_) => {}
    }
    let mut paths = Vec::new();
    list_files(dir, "", &mut paths)?;
    let mut files = BTreeMap::new();
    for relative in paths {
        let text = fsutil::read_text(&join(dir, &relative))
            .map_err(|error| format!("{relative} {error}"))?
            .unwrap_or_default();
        files.insert(relative, text);
    }
    Ok(Some(Skill { files }))
}

/// Makes `dir` hold exactly the skill's files. Hidden files are left alone.
fn write(dir: &Path, skill: &Skill) -> Result<(), String> {
    if fs::symlink_metadata(dir).is_ok_and(|metadata| !metadata.is_dir()) {
        return Err("is not a folder".into());
    }
    let mut existing = Vec::new();
    if dir.is_dir() {
        list_files(dir, "", &mut existing)?;
    }
    for (relative, text) in &skill.files {
        fsutil::write_atomic(&join(dir, relative), text)?;
    }
    for relative in existing.iter().filter(|path| !skill.files.contains_key(*path)) {
        fs::remove_file(join(dir, relative)).map_err(|error| error.to_string())?;
    }
    prune_empty(dir);
    Ok(())
}

/// Removes the skill's files, then any folders left empty.
fn remove(dir: &Path, skill: &Skill) -> Result<(), String> {
    for relative in skill.files.keys() {
        fs::remove_file(join(dir, relative)).map_err(|error| error.to_string())?;
    }
    prune_empty(dir);
    let _ = fs::remove_dir(dir);
    Ok(())
}

fn prune_empty(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            prune_empty(&entry.path());
            let _ = fs::remove_dir(entry.path());
        }
    }
}

/// Skill folder names in a tool's skills folder, skipping hidden entries and invalid names.
fn local_names(dir: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| valid_name(name))
        .collect();
    names.sort();
    names
}

fn local(name: &str, skill: Option<&Skill>, wanted: bool, state: SkillState, message: Option<String>) -> LocalSkill {
    LocalSkill {
        name: name.to_owned(),
        description: skill.map(description).unwrap_or_default(),
        files: skill.map(|skill| skill.files.len()).unwrap_or_default(),
        wanted,
        state,
        edited_outside: false,
        message,
    }
}

/// Compares every installed tool's skills folder with its skill set. With `write`, replaces
/// differing copies and deletes unchanged copies Harness Sync wrote that left the set.
pub fn run(
    document: Option<&SyncDocument>,
    locations: &Locations,
    state: &mut LocalState,
    write_files: bool,
) -> SkillsReport {
    let library = document
        .map(|doc| {
            doc.skills
                .iter()
                .map(|(name, skill)| LibrarySkill {
                    name: name.clone(),
                    description: description(skill),
                    files: skill.files.len(),
                })
                .collect()
        })
        .unwrap_or_default();
    let mut report = SkillsReport { library, tools: Vec::new() };
    for tool in tools::TOOLS.iter().filter(|tool| locations.installed(tool)) {
        let dir = locations.skills_dir(tool.id);
        let wanted = document.and_then(|doc| doc.skill_set(tool.id));
        let mut found: BTreeMap<String, LocalSkill> = BTreeMap::new();

        for name in local_names(&dir) {
            if wanted.as_ref().is_some_and(|set| set.contains_key(name.as_str())) {
                continue;
            }
            let entry = match read(&dir.join(&name)) {
                Ok(Some(skill)) if skill.files.contains_key(ENTRY_FILE) => local(&name, Some(&skill), false, SkillState::Local, None),
                Ok(_) => continue,
                Err(error) => local(&name, None, false, SkillState::Error, Some(format!("{name} {error}"))),
            };
            found.insert(name, entry);
        }

        for (name, skill) in wanted.iter().flatten() {
            let key = format!("{}/{name}", tool.id);
            let target = dir.join(name);
            let wanted_hash = hash(skill);
            let current = match read(&target) {
                Ok(current) => current,
                Err(error) => {
                    let message = Some(format!("{} {error}", target.display()));
                    found.insert(name.to_string(), local(name, Some(skill), true, SkillState::Error, message));
                    continue;
                }
            };
            let current_hash = current.as_ref().map(hash);
            let mut entry = local(name, Some(skill), true, SkillState::InSync, None);
            if current_hash.as_deref() != Some(wanted_hash.as_str()) {
                entry.edited_outside = current_hash.is_some()
                    && state.applied_skills.get(&key).is_some_and(|applied| Some(applied) != current_hash.as_ref());
                entry.state = SkillState::Differs;
                if write_files {
                    match write(&target, skill) {
                        Ok(()) => {
                            entry.state = SkillState::Updated;
                            entry.edited_outside = false;
                            state.written_skills.insert(key.clone());
                        }
                        Err(error) => {
                            entry.state = SkillState::Error;
                            entry.message = Some(format!("Could not write {}: {error}", target.display()));
                        }
                    }
                }
            }
            if matches!(entry.state, SkillState::InSync | SkillState::Updated) {
                state.applied_skills.insert(key, wanted_hash);
            }
            found.insert(name.to_string(), entry);
        }

        // Skills that left this tool's set: delete unchanged copies Harness Sync wrote, forget the rest.
        if let (Some(set), true) = (&wanted, write_files) {
            let prefix = format!("{}/", tool.id);
            let gone: Vec<(String, String)> = state
                .applied_skills
                .iter()
                .filter_map(|(key, applied)| {
                    let name = key.strip_prefix(&prefix)?;
                    (!set.contains_key(name)).then(|| (name.to_owned(), applied.clone()))
                })
                .collect();
            for (name, applied) in gone {
                let key = format!("{prefix}{name}");
                let target = dir.join(&name);
                if state.written_skills.contains(&key) {
                    if let Ok(Some(copy)) = read(&target) {
                        if hash(&copy) == applied && remove(&target, &copy).is_ok() {
                            found.remove(&name);
                        }
                    }
                }
                state.applied_skills.remove(&key);
                state.written_skills.remove(&key);
            }
        }

        report.tools.push(ToolSkills {
            tool: tool.id,
            folder: dir.to_string_lossy().into_owned(),
            skills: found.into_values().collect(),
        });
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document;

    fn skill(files: &[(&str, &str)]) -> Skill {
        Skill { files: files.iter().map(|(path, text)| (path.to_string(), text.to_string())).collect() }
    }

    fn machine(label: &str, installed: &[&str]) -> (Locations, LocalState) {
        let locations = tools::test_locations(&fsutil::temp_dir(label).join("home"));
        for id in installed {
            fs::create_dir_all(locations.config_dir(id)).unwrap();
        }
        (locations, LocalState::default())
    }

    fn find<'a>(report: &'a SkillsReport, tool: &str, name: &str) -> Option<&'a LocalSkill> {
        let tool = report.tools.iter().find(|entry| entry.tool == tool)?;
        tool.skills.iter().find(|skill| skill.name == name)
    }

    fn state_of(report: &SkillsReport, tool: &str, name: &str) -> Option<SkillState> {
        find(report, tool, name).map(|skill| skill.state)
    }

    fn share(path: &Path, revision: u64, names: &[&str], uploads: BTreeMap<String, Skill>, tools: BTreeMap<String, document::ToolPreset>) -> SyncDocument {
        let presets = document::Presets {
            shared: "Rules".into(),
            tools,
            shared_skills: names.iter().map(|name| name.to_string()).collect(),
            ..Default::default()
        };
        document::save(path, revision, presets, uploads, "desk").unwrap()
    }

    #[test]
    fn validates_names_and_paths() {
        let ok = skill(&[("SKILL.md", "x"), ("scripts/run.py", "print()")]);
        assert!(validate("my-skill", &ok).is_ok());
        assert!(validate("../up", &ok).is_err());
        assert!(validate(".hidden", &ok).is_err());
        assert!(validate("a/b", &ok).is_err());
        assert!(validate("x", &skill(&[("README.md", "x")])).is_err());
        for bad in ["../x", "/abs", "a//b", "a\\b", "C:x", ".git/config"] {
            assert!(validate("x", &skill(&[("SKILL.md", "x"), (bad, "x")])).is_err(), "{bad}");
        }
        assert_eq!(description(&skill(&[("SKILL.md", "---\nname: x\ndescription: \"Does things\"\n---\nBody")])), "Does things");
    }

    #[test]
    fn syncs_adds_and_removes_skills() {
        let drive = fsutil::temp_dir("skills-drive").join("harness-sync.json");
        let (desk, mut desk_state) = machine("skills-desk", &["claude", "codex"]);
        let source = desk.skills_dir("claude").join("review");
        fs::create_dir_all(source.join("scripts")).unwrap();
        fs::write(source.join("SKILL.md"), "---\ndescription: Reviews code\n---\n").unwrap();
        fs::write(source.join("scripts/check.sh"), "echo ok\n").unwrap();
        fs::write(source.join(".DS_Store"), "junk").unwrap();

        let created = document::create(&drive, "Rules".into(), "desk").unwrap();
        let listed = run(Some(&created), &desk, &mut desk_state, false);
        let found = find(&listed, "claude", "review").unwrap();
        assert_eq!((found.state, found.wanted), (SkillState::Local, false));
        assert_eq!(found.description, "Reviews code");
        assert!(listed.library.is_empty());

        let local = read(&source).unwrap().unwrap();
        assert_eq!(local.files.len(), 2);
        let saved = share(&drive, created.revision, &["review"], [("review".to_string(), local.clone())].into(), BTreeMap::new());
        let report = run(Some(&saved), &desk, &mut desk_state, true);
        assert_eq!(report.library.len(), 1);
        assert_eq!(state_of(&report, "claude", "review"), Some(SkillState::InSync));
        assert_eq!(state_of(&report, "codex", "review"), Some(SkillState::Updated));
        assert_eq!(read(&desk.skills_dir("codex").join("review")).unwrap().unwrap(), local);

        // Another computer receives it; its own skill with another name is untouched.
        let (laptop, mut laptop_state) = machine("skills-laptop", &["opencode"]);
        let mine = laptop.skills_dir("opencode").join("mine");
        fs::create_dir_all(&mine).unwrap();
        fs::write(mine.join("SKILL.md"), "mine").unwrap();
        let (from_drive, _) = document::load(&drive).unwrap();
        run(Some(&from_drive), &laptop, &mut laptop_state, true);
        assert_eq!(read(&laptop.skills_dir("opencode").join("review")).unwrap().unwrap(), local);

        // An outside edit is reported, then replaced, including extra files.
        let target = laptop.skills_dir("opencode").join("review");
        fs::write(target.join("extra.md"), "extra").unwrap();
        let checked = run(Some(&from_drive), &laptop, &mut laptop_state, false);
        let found = find(&checked, "opencode", "review").unwrap();
        assert_eq!(found.state, SkillState::Differs);
        assert!(found.edited_outside);
        run(Some(&from_drive), &laptop, &mut laptop_state, true);
        assert!(!target.join("extra.md").exists());

        // Unsharing deletes unchanged copies Harness Sync wrote, keeps edited ones and the original.
        fs::write(desk.skills_dir("codex").join("review/SKILL.md"), "edited").unwrap();
        let removed = share(&drive, saved.revision, &[], BTreeMap::new(), BTreeMap::new());
        run(Some(&removed), &laptop, &mut laptop_state, true);
        let report = run(Some(&removed), &desk, &mut desk_state, true);
        assert!(!target.exists());
        assert!(source.join("SKILL.md").exists());
        assert_eq!(state_of(&report, "claude", "review"), Some(SkillState::Local));
        assert_eq!(fs::read_to_string(desk.skills_dir("codex").join("review/SKILL.md")).unwrap(), "edited");
        assert!(mine.join("SKILL.md").exists());
        assert!(desk_state.applied_skills.is_empty());
        assert!(desk_state.written_skills.is_empty());
    }

    #[test]
    fn tool_modes_pick_skills() {
        let drive = fsutil::temp_dir("skills-modes").join("harness-sync.json");
        let (desk, mut state) = machine("skills-modes-desk", &["claude", "codex", "opencode"]);
        let created = document::create(&drive, "Rules".into(), "desk").unwrap();
        let mut tools = BTreeMap::new();
        let extras = vec!["extra".to_string()];
        tools.insert("claude".into(), document::ToolPreset { skill_mode: document::Mode::Append, skill_extras: extras.clone(), ..Default::default() });
        tools.insert("opencode".into(), document::ToolPreset { skill_mode: document::Mode::Custom, skill_extras: extras, ..Default::default() });
        tools.insert("codex".into(), document::ToolPreset { skill_mode: document::Mode::Off, ..Default::default() });
        let uploads = [("base".to_string(), skill(&[("SKILL.md", "base")])), ("extra".to_string(), skill(&[("SKILL.md", "extra")]))].into();
        let saved = share(&drive, created.revision, &["base"], uploads, tools);
        let report = run(Some(&saved), &desk, &mut state, true);
        assert_eq!(state_of(&report, "claude", "base"), Some(SkillState::Updated));
        assert_eq!(state_of(&report, "claude", "extra"), Some(SkillState::Updated));
        assert_eq!(state_of(&report, "opencode", "base"), None);
        assert_eq!(state_of(&report, "opencode", "extra"), Some(SkillState::Updated));
        assert!(report.tools.iter().find(|tool| tool.tool == "codex").unwrap().skills.is_empty());
        assert!(!desk.skills_dir("codex").join("base").exists());
    }

    #[test]
    fn skills_off_and_symlinks_are_left_alone() {
        let drive = fsutil::temp_dir("skills-off").join("harness-sync.json");
        let (desk, mut state) = machine("skills-off-desk", &["claude", "codex"]);
        let created = document::create(&drive, "Rules".into(), "desk").unwrap();
        let mut tools = BTreeMap::new();
        tools.insert("codex".into(), document::ToolPreset { skill_mode: document::Mode::Off, ..Default::default() });
        let saved = share(&drive, created.revision, &["tip"], [("tip".to_string(), skill(&[("SKILL.md", "tip")]))].into(), tools);

        #[cfg(unix)]
        {
            let elsewhere = desk.home.join("elsewhere");
            fs::create_dir_all(&elsewhere).unwrap();
            fs::create_dir_all(desk.skills_dir("claude")).unwrap();
            std::os::unix::fs::symlink(&elsewhere, desk.skills_dir("claude").join("tip")).unwrap();
        }
        let report = run(Some(&saved), &desk, &mut state, true);
        assert_eq!(state_of(&report, "codex", "tip"), None);
        assert!(!desk.skills_dir("codex").join("tip").exists());
        #[cfg(unix)]
        {
            assert_eq!(state_of(&report, "claude", "tip"), Some(SkillState::Error));
            assert_eq!(fs::read_dir(desk.home.join("elsewhere")).unwrap().count(), 0);
        }
    }
}
