use crate::apply::Write;
use crate::document::{Skill, SyncDocument};
use crate::fsutil;
use crate::state::LocalState;
use crate::tools::{self, Locations};
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_FILES: usize = 200;
const ENTRY_FILE: &str = "SKILL.md";
/// Backups kept for each skill copy.
const KEPT_BACKUPS: usize = 5;

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SkillState {
    InSync,
    /// The tool's copy differs and will be replaced on the next apply, or on Sync now if edited outside.
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
    /// An outside edit the background worker uploads by itself, because the sync file still holds
    /// the version last applied here. It is not uploaded if another tool's copy changed differently.
    pub auto_upload: bool,
    /// Where this run saved the copy it replaced, when that copy was not one Harness Sync wrote.
    pub backup: Option<String>,
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

/// Hidden entries and Python bytecode caches, which Python regenerates and are not text.
fn ignored(name: &str) -> bool {
    name.starts_with('.') || name == "__pycache__" || name.ends_with(".pyc")
}

/// Lists the regular files under `dir` that are not ignored as `/`-separated relative paths.
fn list_files(dir: &Path, prefix: &str, out: &mut Vec<String>) -> Result<(), String> {
    let entries = fs::read_dir(dir).map_err(|error| error.to_string())?;
    for entry in entries {
        let entry = entry.map_err(|error| error.to_string())?;
        let name = entry.file_name().into_string().map_err(|_| "has a file name that is not UTF-8".to_string())?;
        if ignored(&name) {
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
        auto_upload: false,
        backup: None,
        message,
    }
}

/// Saves `skill` under `root/<owner>/<name>/<unix time>` and keeps the newest `KEPT_BACKUPS`.
pub fn back_up(root: &Path, owner: &str, name: &str, skill: &Skill) -> Result<PathBuf, String> {
    let parent = root.join(owner).join(name);
    // Seconds and nanoseconds, so names sort by age and a pruned name is never used again.
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    let dir = (0..)
        .map(|n| parent.join(format!("{}-{:09}", now.as_secs(), now.subsec_nanos() + n)))
        .find(|dir| !dir.exists())
        .ok_or("No free backup folder")?;
    for (relative, text) in &skill.files {
        fsutil::write_atomic(&join(&dir, relative), text)?;
    }
    let mut kept: Vec<PathBuf> = fs::read_dir(&parent)
        .map_err(|error| error.to_string())?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    kept.sort();
    for old in kept.iter().rev().skip(KEPT_BACKUPS) {
        let _ = fs::remove_dir_all(old);
    }
    Ok(dir)
}

/// Skills edited in a tool's folder that are safe to upload without asking: the sync file still
/// holds the version Harness Sync last applied to that tool, and every edited copy on this
/// computer is identical. Anything else is a conflict for the user to settle in the app.
pub fn outside_edits(document: &SyncDocument, locations: &Locations, state: &LocalState) -> BTreeMap<String, Skill> {
    // `None` marks a skill with conflicting edits.
    let mut edits: BTreeMap<String, Option<Skill>> = BTreeMap::new();
    for tool in tools::TOOLS.iter().filter(|tool| locations.installed(tool)) {
        let dir = locations.skills_dir(tool.id);
        for (name, skill) in document.skill_set(tool.id).iter().flatten() {
            let Ok(Some(copy)) = read(&dir.join(name)) else {
                continue;
            };
            let (wanted, current) = (hash(skill), hash(&copy));
            let applied = state.applied_skills.get(&format!("{}/{name}", tool.id));
            // In sync, or an untouched older copy the next apply updates.
            if current == wanted || applied == Some(&current) {
                continue;
            }
            let safe = applied == Some(&wanted) && validate(name, &copy).is_ok();
            let agreed = match edits.get(*name) {
                None => safe,
                Some(other) => safe && other.as_ref() == Some(&copy),
            };
            edits.insert(name.to_string(), agreed.then_some(copy));
        }
    }
    edits.into_iter().filter_map(|(name, skill)| Some((name, skill?))).collect()
}

/// Compares every installed tool's skills folder with its skill set. Unless `write_files` is
/// `Never`, replaces differing copies as it allows and deletes unchanged copies Harness Sync wrote
/// that left the set. A replaced copy that Harness Sync did not write itself is first saved under
/// `backups`; if that fails the copy is left alone. With no `backups` folder nothing is saved.
pub fn run(
    document: Option<&SyncDocument>,
    locations: &Locations,
    state: &mut LocalState,
    write_files: Write,
    backups: Option<&Path>,
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
                entry.auto_upload = entry.edited_outside && state.applied_skills.get(&key) == Some(&wanted_hash);
                entry.state = SkillState::Differs;
                let keep = write_files == Write::KeepOutsideEdits && entry.edited_outside;
                if write_files != Write::Never && !keep {
                    // A copy Harness Sync last wrote or confirmed is already in the sync file's history;
                    // any other content exists nowhere else.
                    let unsaved = current
                        .as_ref()
                        .filter(|copy| !copy.files.is_empty() && state.applied_skills.get(&key) != current_hash.as_ref());
                    let saved = match (unsaved, backups) {
                        (Some(copy), Some(root)) => back_up(root, tool.id, name, copy)
                            .map(Some)
                            .map_err(|error| format!("Could not back up {}, so it was left alone: {error}", target.display())),
                        _ => Ok(None),
                    };
                    let written = saved.and_then(|backup| {
                        write(&target, skill).map_err(|error| format!("Could not write {}: {error}", target.display()))?;
                        Ok(backup)
                    });
                    match written {
                        Ok(backup) => {
                            entry.state = SkillState::Updated;
                            entry.edited_outside = false;
                            entry.auto_upload = false;
                            entry.backup = backup.map(|dir| dir.to_string_lossy().into_owned());
                            state.written_skills.insert(key.clone());
                        }
                        Err(message) => {
                            entry.state = SkillState::Error;
                            entry.message = Some(message);
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
        if let (Some(set), true) = (&wanted, write_files != Write::Never) {
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
        fs::create_dir_all(source.join("scripts/__pycache__")).unwrap();
        fs::write(source.join("scripts/__pycache__/check.cpython-314.pyc"), [0xa7u8, 0x0d, 0xff, 0x00]).unwrap();
        fs::write(source.join("stray.pyc"), [0xffu8]).unwrap();

        let created = document::create(&drive, "Rules".into(), "desk").unwrap();
        let listed = run(Some(&created), &desk, &mut desk_state, Write::Never, None);
        let found = find(&listed, "claude", "review").unwrap();
        assert_eq!((found.state, found.wanted), (SkillState::Local, false));
        assert_eq!(found.description, "Reviews code");
        assert!(listed.library.is_empty());

        let local = read(&source).unwrap().unwrap();
        assert_eq!(local.files.len(), 2);
        let saved = share(&drive, created.revision, &["review"], [("review".to_string(), local.clone())].into(), BTreeMap::new());
        let report = run(Some(&saved), &desk, &mut desk_state, Write::Always, None);
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
        run(Some(&from_drive), &laptop, &mut laptop_state, Write::Always, None);
        assert_eq!(read(&laptop.skills_dir("opencode").join("review")).unwrap().unwrap(), local);

        // An outside edit is reported, then replaced, including extra files.
        let target = laptop.skills_dir("opencode").join("review");
        fs::write(target.join("extra.md"), "extra").unwrap();
        let checked = run(Some(&from_drive), &laptop, &mut laptop_state, Write::Never, None);
        let found = find(&checked, "opencode", "review").unwrap();
        assert_eq!(found.state, SkillState::Differs);
        assert!(found.edited_outside);
        run(Some(&from_drive), &laptop, &mut laptop_state, Write::Always, None);
        assert!(!target.join("extra.md").exists());

        // Unsharing deletes unchanged copies Harness Sync wrote, keeps edited ones and the original.
        fs::write(desk.skills_dir("codex").join("review/SKILL.md"), "edited").unwrap();
        let removed = share(&drive, saved.revision, &[], BTreeMap::new(), BTreeMap::new());
        run(Some(&removed), &laptop, &mut laptop_state, Write::Always, None);
        let report = run(Some(&removed), &desk, &mut desk_state, Write::Always, None);
        assert!(!target.exists());
        assert!(source.join("SKILL.md").exists());
        assert_eq!(state_of(&report, "claude", "review"), Some(SkillState::Local));
        assert_eq!(fs::read_to_string(desk.skills_dir("codex").join("review/SKILL.md")).unwrap(), "edited");
        assert!(mine.join("SKILL.md").exists());
        assert!(desk_state.applied_skills.is_empty());
        assert!(desk_state.written_skills.is_empty());
    }

    #[test]
    fn automatic_apply_keeps_edited_skills() {
        let drive = fsutil::temp_dir("skills-keep").join("harness-sync.json");
        let (desk, mut state) = machine("skills-keep-desk", &["claude", "codex"]);
        let created = document::create(&drive, "Rules".into(), "desk").unwrap();
        let saved = share(&drive, created.revision, &["tip"], [("tip".to_string(), skill(&[("SKILL.md", "tip")]))].into(), BTreeMap::new());
        run(Some(&saved), &desk, &mut state, Write::Always, None);
        let edited = desk.skills_dir("codex").join("tip/SKILL.md");
        fs::write(&edited, "mine").unwrap();
        let newer = share(&drive, saved.revision, &["tip"], [("tip".to_string(), skill(&[("SKILL.md", "tip v2")]))].into(), BTreeMap::new());

        let auto = run(Some(&newer), &desk, &mut state, Write::KeepOutsideEdits, None);
        let found = find(&auto, "codex", "tip").unwrap();
        assert_eq!((found.state, found.edited_outside), (SkillState::Differs, true));
        assert_eq!(fs::read_to_string(&edited).unwrap(), "mine");
        assert_eq!(state_of(&auto, "claude", "tip"), Some(SkillState::Updated));

        let explicit = run(Some(&newer), &desk, &mut state, Write::Always, None);
        assert_eq!(state_of(&explicit, "codex", "tip"), Some(SkillState::Updated));
        assert_eq!(fs::read_to_string(&edited).unwrap(), "tip v2");
    }

    #[test]
    fn replaced_edits_are_backed_up() {
        let drive = fsutil::temp_dir("skills-backup").join("harness-sync.json");
        let backups = fsutil::temp_dir("skills-backup-data").join("backups");
        let (desk, mut state) = machine("skills-backup-desk", &["claude", "codex"]);
        // Codex already has its own skill of the same name before the first apply.
        let codex = desk.skills_dir("codex").join("tip");
        fs::create_dir_all(&codex).unwrap();
        fs::write(codex.join("SKILL.md"), "codex original").unwrap();
        let created = document::create(&drive, "Rules".into(), "desk").unwrap();
        let saved = share(&drive, created.revision, &["tip"], [("tip".to_string(), skill(&[("SKILL.md", "tip")]))].into(), BTreeMap::new());

        let first = run(Some(&saved), &desk, &mut state, Write::Always, Some(&backups));
        let replaced = find(&first, "codex", "tip").unwrap();
        assert_eq!(replaced.state, SkillState::Updated);
        let kept = PathBuf::from(replaced.backup.clone().unwrap());
        assert!(kept.starts_with(backups.join("codex").join("tip")));
        assert_eq!(fs::read_to_string(kept.join("SKILL.md")).unwrap(), "codex original");
        // Claude had no copy, so nothing was saved for it.
        assert!(find(&first, "claude", "tip").unwrap().backup.is_none());

        // An outside edit with an extra file is saved whole before Sync now replaces it.
        fs::write(codex.join("SKILL.md"), "mine").unwrap();
        fs::create_dir_all(codex.join("scripts")).unwrap();
        fs::write(codex.join("scripts/run.py"), "print()").unwrap();
        let second = run(Some(&saved), &desk, &mut state, Write::Always, Some(&backups));
        let kept = PathBuf::from(find(&second, "codex", "tip").unwrap().backup.clone().unwrap());
        assert_eq!(fs::read_to_string(kept.join("SKILL.md")).unwrap(), "mine");
        assert_eq!(fs::read_to_string(kept.join("scripts/run.py")).unwrap(), "print()");
        assert_eq!(fs::read_to_string(codex.join("SKILL.md")).unwrap(), "tip");
        assert!(!codex.join("scripts").exists());

        // A copy Harness Sync wrote and nobody edited is replaced without a backup.
        let newer = share(&drive, saved.revision, &["tip"], [("tip".to_string(), skill(&[("SKILL.md", "tip v2")]))].into(), BTreeMap::new());
        let third = run(Some(&newer), &desk, &mut state, Write::Always, Some(&backups));
        assert_eq!(state_of(&third, "codex", "tip"), Some(SkillState::Updated));
        assert!(find(&third, "codex", "tip").unwrap().backup.is_none());

        // Only the newest backups are kept.
        for round in 0..KEPT_BACKUPS + 2 {
            let text = round.to_string();
            back_up(&backups, "codex", "tip", &skill(&[("SKILL.md", text.as_str())])).unwrap();
        }
        let mut left: Vec<PathBuf> = fs::read_dir(backups.join("codex").join("tip")).unwrap().map(|entry| entry.unwrap().path()).collect();
        left.sort();
        assert_eq!(left.len(), KEPT_BACKUPS);
        assert_eq!(fs::read_to_string(left.last().unwrap().join("SKILL.md")).unwrap(), (KEPT_BACKUPS + 1).to_string());
    }

    #[test]
    fn finds_outside_edits_that_are_safe_to_upload() {
        let drive = fsutil::temp_dir("skills-upload").join("harness-sync.json");
        let (desk, mut state) = machine("skills-upload-desk", &["claude", "codex"]);
        let created = document::create(&drive, "Rules".into(), "desk").unwrap();
        let tip = |text: &str| -> BTreeMap<String, Skill> { [("tip".to_string(), skill(&[("SKILL.md", text)]))].into() };
        let saved = share(&drive, created.revision, &["tip"], tip("tip"), BTreeMap::new());
        // A differing copy that was there before the first apply is not this app's to upload.
        let claude = desk.skills_dir("claude").join("tip/SKILL.md");
        fs::create_dir_all(claude.parent().unwrap()).unwrap();
        fs::write(&claude, "older local").unwrap();
        assert!(outside_edits(&saved, &desk, &state).is_empty());

        run(Some(&saved), &desk, &mut state, Write::Always, None);
        assert!(outside_edits(&saved, &desk, &state).is_empty());

        // One tool's copy edited: safe.
        let codex = desk.skills_dir("codex").join("tip/SKILL.md");
        fs::write(&codex, "mine").unwrap();
        let checked = run(Some(&saved), &desk, &mut state, Write::KeepOutsideEdits, None);
        assert!(find(&checked, "codex", "tip").unwrap().auto_upload);
        assert_eq!(outside_edits(&saved, &desk, &state), tip("mine"));

        // Two tools edited differently: a conflict. Edited the same way: safe again.
        fs::write(&claude, "theirs").unwrap();
        assert!(outside_edits(&saved, &desk, &state).is_empty());
        fs::write(&claude, "mine").unwrap();
        assert_eq!(outside_edits(&saved, &desk, &state), tip("mine"));

        // An edit that is not a valid skill is never uploaded.
        fs::write(&claude, "tip").unwrap();
        fs::write(&codex, "bad\0").unwrap();
        assert!(outside_edits(&saved, &desk, &state).is_empty());

        // The sync file moved on while this copy was edited: a conflict for the user to settle.
        fs::write(&codex, "mine").unwrap();
        let newer = share(&drive, saved.revision, &["tip"], tip("tip v2"), BTreeMap::new());
        assert!(outside_edits(&newer, &desk, &state).is_empty());
        let auto = run(Some(&newer), &desk, &mut state, Write::KeepOutsideEdits, None);
        assert!(!find(&auto, "codex", "tip").unwrap().auto_upload);
        assert_eq!(fs::read_to_string(&codex).unwrap(), "mine");
        assert!(outside_edits(&newer, &desk, &state).is_empty());
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
        let report = run(Some(&saved), &desk, &mut state, Write::Always, None);
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
        let report = run(Some(&saved), &desk, &mut state, Write::Always, None);
        assert_eq!(state_of(&report, "codex", "tip"), None);
        assert!(!desk.skills_dir("codex").join("tip").exists());
        #[cfg(unix)]
        {
            assert_eq!(state_of(&report, "claude", "tip"), Some(SkillState::Error));
            assert_eq!(fs::read_dir(desk.home.join("elsewhere")).unwrap().count(), 0);
        }
    }
}
