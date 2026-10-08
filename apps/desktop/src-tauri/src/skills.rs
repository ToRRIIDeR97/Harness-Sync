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
    /// Found in the tool's folder but not synced.
    Local,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SkillCopy {
    pub tool: &'static str,
    pub state: SkillState,
    /// The copy changed since Harness Sync last wrote it.
    pub edited_outside: bool,
    pub message: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SkillStatus {
    pub name: String,
    pub description: String,
    pub synced: bool,
    pub files: usize,
    pub copies: Vec<SkillCopy>,
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
        .map(|value| value.trim().trim_matches(|c| c == '"' || c == '\'').chars().take(200).collect())
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

fn copy(tool: &'static str, state: SkillState, message: Option<String>) -> SkillCopy {
    SkillCopy { tool, state, edited_outside: false, message }
}

/// Compares every installed tool's skills folder with the synced skills. With `write`, replaces
/// differing copies and removes skills that left the sync file when the local copy is unchanged.
pub fn run(
    document: Option<&SyncDocument>,
    locations: &Locations,
    state: &mut LocalState,
    write_files: bool,
) -> Vec<SkillStatus> {
    let mut skills: BTreeMap<String, SkillStatus> = BTreeMap::new();
    let synced = document.map(|doc| &doc.skills);
    for (name, skill) in synced.into_iter().flatten() {
        skills.insert(
            name.clone(),
            SkillStatus {
                name: name.clone(),
                description: description(skill),
                synced: true,
                files: skill.files.len(),
                copies: Vec::new(),
            },
        );
    }
    for tool in tools::TOOLS.iter().filter(|tool| locations.installed(tool)) {
        let dir = locations.skills_dir(tool.id);
        let managed = document.is_some_and(|doc| doc.preset(tool.id).skills);

        // Skills only on this computer, which the user can add.
        for name in local_names(&dir) {
            if managed && synced.is_some_and(|synced| synced.contains_key(&name)) {
                continue;
            }
            let (local, found) = match read(&dir.join(&name)) {
                Ok(Some(skill)) if skill.files.contains_key(ENTRY_FILE) => (Some(skill), copy(tool.id, SkillState::Local, None)),
                Ok(_) => continue,
                Err(error) => (None, copy(tool.id, SkillState::Error, Some(format!("{name} {error}")))),
            };
            let status = skills.entry(name.clone()).or_insert_with(|| SkillStatus {
                name: name.clone(),
                description: String::new(),
                synced: false,
                files: 0,
                copies: Vec::new(),
            });
            if let (Some(local), false) = (&local, status.synced) {
                if status.description.is_empty() {
                    status.description = description(local);
                }
                status.files = status.files.max(local.files.len());
            }
            status.copies.push(found);
        }

        let (Some(synced), true) = (synced, managed) else {
            continue;
        };
        for (name, skill) in synced {
            let key = format!("{}/{name}", tool.id);
            let target = dir.join(name);
            let wanted = hash(skill);
            let current = match read(&target) {
                Ok(current) => current,
                Err(error) => {
                    let found = copy(tool.id, SkillState::Error, Some(format!("{} {error}", target.display())));
                    skills.get_mut(name).expect("synced skill").copies.push(found);
                    continue;
                }
            };
            let current_hash = current.as_ref().map(hash);
            let mut found = copy(tool.id, SkillState::InSync, None);
            if current_hash.as_deref() != Some(wanted.as_str()) {
                found.edited_outside = current_hash.is_some()
                    && state.applied_skills.get(&key).is_some_and(|applied| Some(applied) != current_hash.as_ref());
                found.state = SkillState::Differs;
                if write_files {
                    match write(&target, skill) {
                        Ok(()) => {
                            found.state = SkillState::Updated;
                            found.edited_outside = false;
                        }
                        Err(error) => {
                            found.state = SkillState::Error;
                            found.message = Some(format!("Could not write {}: {error}", target.display()));
                        }
                    }
                }
            }
            if matches!(found.state, SkillState::InSync | SkillState::Updated) {
                state.applied_skills.insert(key, wanted);
            }
            skills.get_mut(name).expect("synced skill").copies.push(found);
        }

        // Skills removed from the sync file: delete copies nobody changed, forget the rest.
        if write_files {
            let prefix = format!("{}/", tool.id);
            let gone: Vec<(String, String)> = state
                .applied_skills
                .iter()
                .filter_map(|(key, applied)| {
                    let name = key.strip_prefix(&prefix)?;
                    (!synced.contains_key(name)).then(|| (name.to_owned(), applied.clone()))
                })
                .collect();
            for (name, applied) in gone {
                let target = dir.join(&name);
                if let Ok(Some(local)) = read(&target) {
                    if hash(&local) == applied {
                        let _ = remove(&target, &local);
                    }
                }
                state.applied_skills.remove(&format!("{prefix}{name}"));
            }
        }
    }
    skills.into_values().collect()
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

    fn copy_state(statuses: &[SkillStatus], name: &str, tool: &str) -> Option<SkillState> {
        let status = statuses.iter().find(|status| status.name == name)?;
        Some(status.copies.iter().find(|copy| copy.tool == tool)?.state)
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
        assert_eq!(copy_state(&listed, "review", "claude"), Some(SkillState::Local));
        assert!(!listed[0].synced);
        assert_eq!(listed[0].description, "Reviews code");

        let local = read(&source).unwrap().unwrap();
        assert_eq!(local.files.len(), 2);
        let saved = document::save_skill(&drive, created.revision, "review", Some(local.clone()), "desk").unwrap();
        let statuses = run(Some(&saved), &desk, &mut desk_state, true);
        assert_eq!(copy_state(&statuses, "review", "claude"), Some(SkillState::InSync));
        assert_eq!(copy_state(&statuses, "review", "codex"), Some(SkillState::Updated));
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
        let found = &checked.iter().find(|s| s.name == "review").unwrap().copies[0];
        assert_eq!(found.state, SkillState::Differs);
        assert!(found.edited_outside);
        run(Some(&from_drive), &laptop, &mut laptop_state, true);
        assert!(!target.join("extra.md").exists());

        // Removing it deletes unchanged copies and keeps edited ones.
        fs::write(desk.skills_dir("codex").join("review/SKILL.md"), "edited").unwrap();
        let removed = document::save_skill(&drive, saved.revision, "review", None, "desk").unwrap();
        run(Some(&removed), &laptop, &mut laptop_state, true);
        run(Some(&removed), &desk, &mut desk_state, true);
        assert!(!target.exists());
        // Hidden files such as .DS_Store are not part of a skill and stay.
        assert!(!desk.skills_dir("claude").join("review/SKILL.md").exists());
        assert!(!desk.skills_dir("claude").join("review/scripts").exists());
        assert_eq!(fs::read_to_string(desk.skills_dir("codex").join("review/SKILL.md")).unwrap(), "edited");
        assert!(mine.join("SKILL.md").exists());
        assert!(desk_state.applied_skills.is_empty());
    }

    #[test]
    fn skills_off_and_symlinks_are_left_alone() {
        let drive = fsutil::temp_dir("skills-off").join("harness-sync.json");
        let (desk, mut state) = machine("skills-off-desk", &["claude", "codex"]);
        let created = document::create(&drive, "Rules".into(), "desk").unwrap();
        let mut presets = document::Presets { shared: "Rules".into(), tools: BTreeMap::new() };
        presets.tools.insert("codex".into(), document::ToolPreset { skills: false, ..Default::default() });
        let saved = document::save(&drive, created.revision, presets, "desk").unwrap();
        let saved = document::save_skill(&drive, saved.revision, "tip", Some(skill(&[("SKILL.md", "tip")])), "desk").unwrap();

        #[cfg(unix)]
        {
            let elsewhere = desk.home.join("elsewhere");
            fs::create_dir_all(&elsewhere).unwrap();
            fs::create_dir_all(desk.skills_dir("claude")).unwrap();
            std::os::unix::fs::symlink(&elsewhere, desk.skills_dir("claude").join("tip")).unwrap();
        }
        let statuses = run(Some(&saved), &desk, &mut state, true);
        assert_eq!(copy_state(&statuses, "tip", "codex"), None);
        assert!(!desk.skills_dir("codex").join("tip").exists());
        #[cfg(unix)]
        {
            assert_eq!(copy_state(&statuses, "tip", "claude"), Some(SkillState::Error));
            assert_eq!(fs::read_dir(desk.home.join("elsewhere")).unwrap().count(), 0);
        }
    }
}
