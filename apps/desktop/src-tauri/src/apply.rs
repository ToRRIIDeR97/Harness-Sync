use crate::document::{Mode, Presets, SyncDocument, ToolPreset};
use crate::fsutil;
use crate::state::LocalState;
use crate::tools::{self, Locations};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ToolState {
    /// No sync file is connected yet.
    Detected,
    InSync,
    /// The file differs and will be replaced on the next apply.
    Differs,
    Updated,
    /// The rendered preset is empty, so the file was left alone.
    Skipped,
    Off,
    NotInstalled,
    Error,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ToolStatus {
    pub id: &'static str,
    pub name: &'static str,
    pub installed: bool,
    pub path: String,
    pub exists: bool,
    /// Non-empty lines in the current file.
    pub lines: usize,
    pub mode: Mode,
    pub state: ToolState,
    /// The file changed since Harness Sync last wrote it.
    pub edited_outside: bool,
    pub message: Option<String>,
}

/// Compares every tool with the presets. With `write`, replaces differing files.
pub fn run(
    document: Option<&SyncDocument>,
    locations: &Locations,
    state: &mut LocalState,
    write: bool,
) -> Vec<ToolStatus> {
    tools::TOOLS
        .iter()
        .map(|tool| {
            let path = locations.instructions(tool.id);
            let mut status = ToolStatus {
                id: tool.id,
                name: tool.name,
                installed: locations.installed(tool),
                path: path.to_string_lossy().into_owned(),
                exists: false,
                lines: 0,
                mode: document.map(|doc| doc.preset(tool.id).mode).unwrap_or_default(),
                state: ToolState::Detected,
                edited_outside: false,
                message: None,
            };
            let current = match fsutil::read_text(&path) {
                Ok(current) => current,
                Err(error) => {
                    status.exists = true;
                    status.state = ToolState::Error;
                    status.message = Some(format!("{} {error}", path.display()));
                    return status;
                }
            };
            status.exists = current.is_some();
            status.lines = current
                .as_deref()
                .map(|text| text.lines().filter(|line| !line.trim().is_empty()).count())
                .unwrap_or_default();
            let Some(document) = document else {
                return status;
            };
            if !status.installed {
                status.state = ToolState::NotInstalled;
                return status;
            }
            let Some(rendered) = document.render(tool.id) else {
                status.state = ToolState::Off;
                return status;
            };
            let current_text = current.as_deref().unwrap_or("");
            status.edited_outside = current.is_some()
                && current_text != rendered
                && state
                    .applied
                    .get(tool.id)
                    .is_some_and(|applied| *applied != fsutil::hash(current_text.as_bytes()));
            if current_text == rendered {
                status.state = ToolState::InSync;
                state.applied.insert(tool.id.into(), fsutil::hash(rendered.as_bytes()));
                return status;
            }
            if rendered.trim().is_empty() {
                status.state = ToolState::Skipped;
                status.message = Some("The preset is empty, so this file was left unchanged.".into());
                return status;
            }
            if !write {
                status.state = ToolState::Differs;
                return status;
            }
            match fsutil::write_atomic(&path, &rendered) {
                Ok(()) => {
                    status.state = ToolState::Updated;
                    status.exists = true;
                    status.edited_outside = false;
                    status.lines = rendered.lines().filter(|line| !line.trim().is_empty()).count();
                    state.applied.insert(tool.id.into(), fsutil::hash(rendered.as_bytes()));
                }
                Err(error) => {
                    status.state = ToolState::Error;
                    status.message = Some(format!("Could not write {}: {error}", path.display()));
                }
            }
            status
        })
        .collect()
}

/// Presets that reproduce this computer's current instruction files.
///
/// `seed` becomes the shared preset. Every other installed tool keeps the shared preset when its file
/// matches, becomes "shared + extra" when its file starts with the shared text, and otherwise keeps its
/// file as its own preset. Tools that are missing, empty or turned off keep their current setting.
pub fn capture(document: &SyncDocument, locations: &Locations, seed: &str) -> Result<Presets, String> {
    let seed_tool = tools::find(seed).ok_or("Unknown tool")?;
    let read = |id: &str| fsutil::read_text(&locations.instructions(id)).ok().flatten().unwrap_or_default();
    let shared = read(seed);
    if shared.trim().is_empty() {
        return Err(format!("{} has no instructions on this computer", seed_tool.name));
    }
    let base = shared.trim_end();
    let mut presets = BTreeMap::new();
    for tool in tools::TOOLS.iter() {
        let existing = document.preset(tool.id);
        let text = read(tool.id);
        let preset = if tool.id == seed {
            ToolPreset::default()
        } else if !locations.installed(tool) || existing.mode == Mode::Off || text.trim().is_empty() {
            existing
        } else if text.trim_end() == base {
            ToolPreset::default()
        } else if let Some(extra) = text.strip_prefix(base).filter(|rest| rest.starts_with('\n')) {
            ToolPreset { mode: Mode::Append, text: extra.trim_start().to_owned() }
        } else {
            ToolPreset { mode: Mode::Custom, text }
        };
        presets.insert(tool.id.to_owned(), preset);
    }
    Ok(Presets { shared, tools: presets })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{self, Presets, ToolPreset};
    use std::collections::BTreeMap;
    use std::fs;

    struct Machine {
        locations: Locations,
        state: LocalState,
    }

    fn machine(label: &str, installed: &[&str]) -> Machine {
        let root = fsutil::temp_dir(label);
        let locations = tools::test_locations(&root.join("home"));
        for id in installed {
            fs::create_dir_all(locations.config_dir(id)).unwrap();
        }
        Machine { locations, state: LocalState::default() }
    }

    fn read(machine: &Machine, id: &str) -> Option<String> {
        fsutil::read_text(&machine.locations.instructions(id)).unwrap()
    }

    fn state_of(statuses: &[ToolStatus], id: &str) -> ToolState {
        statuses.iter().find(|status| status.id == id).unwrap().state
    }

    #[test]
    fn two_computers_get_the_same_presets() {
        let drive = fsutil::temp_dir("drive").join("harness-sync.json");
        let mut desk = machine("desk", &["codex", "claude", "antigravity", "command-code"]);
        fs::write(desk.locations.instructions("claude"), "Old claude\n").unwrap();

        let created = document::create(&drive, "Be concise.".into(), "desk").unwrap();
        let mut tools = BTreeMap::new();
        tools.insert("antigravity".to_string(), ToolPreset { mode: Mode::Custom, text: "Gemini preset".into() });
        tools.insert("command-code".to_string(), ToolPreset { mode: Mode::Off, text: String::new() });
        let saved = document::save(&drive, created.revision, Presets { shared: "Be concise.".into(), tools }, "desk").unwrap();

        let statuses = run(Some(&saved), &desk.locations, &mut desk.state, true);
        assert_eq!(state_of(&statuses, "claude"), ToolState::Updated);
        assert_eq!(state_of(&statuses, "opencode"), ToolState::NotInstalled);
        assert_eq!(state_of(&statuses, "command-code"), ToolState::Off);
        assert_eq!(read(&desk, "claude").as_deref(), Some("Be concise.\n"));
        assert_eq!(read(&desk, "codex").as_deref(), Some("Be concise.\n"));
        assert_eq!(read(&desk, "antigravity").as_deref(), Some("Gemini preset\n"));
        assert_eq!(read(&desk, "command-code"), None);
        assert_eq!(read(&desk, "opencode"), None);

        // The second computer only has the file.
        let mut laptop = machine("laptop", &["antigravity", "codex"]);
        let (from_drive, _) = document::load(&drive).unwrap();
        run(Some(&from_drive), &laptop.locations, &mut laptop.state, true);
        assert_eq!(read(&laptop, "antigravity").as_deref(), Some("Gemini preset\n"));
        assert_eq!(read(&laptop, "codex").as_deref(), Some("Be concise.\n"));

        // A second run changes nothing.
        let again = run(Some(&from_drive), &laptop.locations, &mut laptop.state, true);
        assert!(again.iter().all(|s| s.state != ToolState::Updated));
    }

    #[test]
    fn capture_makes_this_computer_the_source() {
        let drive = fsutil::temp_dir("capture-drive").join("harness-sync.json");
        let created = document::create(&drive, "Desk rules
".into(), "desk").unwrap();
        let mut laptop = machine("capture", &["codex", "claude", "opencode", "antigravity"]);
        fs::write(laptop.locations.instructions("codex"), "Laptop rules
").unwrap();
        fs::write(laptop.locations.instructions("claude"), "Laptop rules

Claude extra
").unwrap();
        fs::write(laptop.locations.instructions("antigravity"), "Gemini only
").unwrap();

        let presets = capture(&created, &laptop.locations, "codex").unwrap();
        assert_eq!(presets.shared, "Laptop rules
");
        assert_eq!(presets.tools["claude"], ToolPreset { mode: Mode::Append, text: "Claude extra
".into() });
        assert_eq!(presets.tools["antigravity"], ToolPreset { mode: Mode::Custom, text: "Gemini only
".into() });
        // OpenCode has no file yet, so it keeps receiving the shared preset.
        assert_eq!(presets.tools["opencode"], ToolPreset::default());

        let saved = document::save(&drive, created.revision, presets, "laptop").unwrap();
        let statuses = run(Some(&saved), &laptop.locations, &mut laptop.state, true);
        // Every existing file already matches, so only the empty OpenCode file is written.
        assert_eq!(state_of(&statuses, "codex"), ToolState::InSync);
        assert_eq!(state_of(&statuses, "claude"), ToolState::InSync);
        assert_eq!(state_of(&statuses, "antigravity"), ToolState::InSync);
        assert_eq!(read(&laptop, "opencode").as_deref(), Some("Laptop rules
"));
        assert!(capture(&saved, &laptop.locations, "command-code").is_err());
    }

    #[test]
    fn reports_outside_edits_without_writing_when_checking() {
        let mut desk = machine("outside", &["codex"]);
        let drive = fsutil::temp_dir("drive2").join("harness-sync.json");
        let doc = document::create(&drive, "Rules".into(), "desk").unwrap();
        run(Some(&doc), &desk.locations, &mut desk.state, true);
        fs::write(desk.locations.instructions("codex"), "Rules\nmy edit\n").unwrap();

        let checked = run(Some(&doc), &desk.locations, &mut desk.state, false);
        let codex = checked.iter().find(|s| s.id == "codex").unwrap();
        assert_eq!(codex.state, ToolState::Differs);
        assert!(codex.edited_outside);
        assert_eq!(codex.lines, 2);
        assert_eq!(read(&desk, "codex").as_deref(), Some("Rules\nmy edit\n"));
    }

    #[test]
    fn empty_preset_and_symlink_are_left_alone() {
        let mut desk = machine("empty", &["codex", "claude"]);
        fs::write(desk.locations.instructions("codex"), "Keep me\n").unwrap();
        let drive = fsutil::temp_dir("drive3").join("harness-sync.json");
        let doc = document::create(&drive, String::new(), "desk").unwrap();
        let statuses = run(Some(&doc), &desk.locations, &mut desk.state, true);
        assert_eq!(state_of(&statuses, "codex"), ToolState::Skipped);
        assert_eq!(read(&desk, "codex").as_deref(), Some("Keep me\n"));

        #[cfg(unix)]
        {
            let target = desk.locations.home.join("dotfiles.md");
            fs::write(&target, "linked").unwrap();
            std::os::unix::fs::symlink(&target, desk.locations.instructions("claude")).unwrap();
            let doc = document::create(&drive.with_file_name("b.json"), "x".into(), "d").unwrap();
            let statuses = run(Some(&doc), &desk.locations, &mut desk.state, true);
            assert_eq!(state_of(&statuses, "claude"), ToolState::Error);
            assert_eq!(fs::read_to_string(&target).unwrap(), "linked");
        }
    }
}
