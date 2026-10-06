use std::ffi::OsString;
use std::path::{Path, PathBuf};

pub struct Tool {
    pub id: &'static str,
    pub name: &'static str,
    command: &'static str,
}

pub const TOOLS: [Tool; 5] = [
    Tool { id: "codex", name: "Codex", command: "codex" },
    Tool { id: "claude", name: "Claude Code", command: "claude" },
    Tool { id: "opencode", name: "OpenCode", command: "opencode" },
    Tool { id: "antigravity", name: "Antigravity", command: "agy" },
    Tool { id: "command-code", name: "Command Code", command: "commandcode" },
];

pub fn find(id: &str) -> Option<&'static Tool> {
    TOOLS.iter().find(|tool| tool.id == id)
}

/// Where each tool keeps its global instructions on this computer.
#[derive(Clone)]
pub struct Locations {
    pub home: PathBuf,
    pub codex_home: Option<PathBuf>,
    pub claude_dir: Option<PathBuf>,
    pub xdg_config: Option<PathBuf>,
    pub path_env: Option<OsString>,
}

fn absolute(value: Option<OsString>) -> Option<PathBuf> {
    value.map(PathBuf::from).filter(|path| path.is_absolute())
}

impl Locations {
    pub fn from_env(home: PathBuf) -> Self {
        Self {
            home,
            codex_home: absolute(std::env::var_os("CODEX_HOME")),
            claude_dir: absolute(std::env::var_os("CLAUDE_CONFIG_DIR")),
            xdg_config: absolute(std::env::var_os("XDG_CONFIG_HOME")),
            path_env: std::env::var_os("PATH"),
        }
    }

    /// Ignores the tool directory overrides, so every path stays inside `home`.
    pub fn sandboxed(home: PathBuf) -> Self {
        Self { home, codex_home: None, claude_dir: None, xdg_config: None, path_env: None }
    }

    pub fn config_dir(&self, id: &str) -> PathBuf {
        match id {
            "codex" => self.codex_home.clone().unwrap_or_else(|| self.home.join(".codex")),
            "claude" => self.claude_dir.clone().unwrap_or_else(|| self.home.join(".claude")),
            "opencode" => self
                .xdg_config
                .clone()
                .unwrap_or_else(|| self.home.join(".config"))
                .join("opencode"),
            "antigravity" => self.home.join(".gemini"),
            _ => self.home.join(".commandcode"),
        }
    }

    pub fn instructions(&self, id: &str) -> PathBuf {
        let file = match id {
            "claude" => "CLAUDE.md",
            "antigravity" => "GEMINI.md",
            _ => "AGENTS.md",
        };
        self.config_dir(id).join(file)
    }

    pub fn installed(&self, tool: &Tool) -> bool {
        self.config_dir(tool.id).is_dir() || self.command_exists(tool.command)
    }

    fn command_exists(&self, name: &str) -> bool {
        let Some(paths) = &self.path_env else {
            return false;
        };
        let names: Vec<String> = if cfg!(windows) {
            ["exe", "cmd", "bat", "ps1"]
                .iter()
                .map(|ext| format!("{name}.{ext}"))
                .collect()
        } else {
            vec![name.to_owned()]
        };
        std::env::split_paths(paths)
            .any(|dir| names.iter().any(|candidate| Path::new(&dir).join(candidate).is_file()))
    }
}

#[cfg(test)]
pub fn test_locations(home: &Path) -> Locations {
    Locations::sandboxed(home.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_follow_overrides() {
        let home = PathBuf::from(if cfg!(windows) { r"C:\home" } else { "/home/u" });
        let mut locations = test_locations(&home);
        assert_eq!(locations.instructions("codex"), home.join(".codex/AGENTS.md"));
        assert_eq!(locations.instructions("claude"), home.join(".claude/CLAUDE.md"));
        assert_eq!(locations.instructions("opencode"), home.join(".config/opencode/AGENTS.md"));
        assert_eq!(locations.instructions("antigravity"), home.join(".gemini/GEMINI.md"));
        assert_eq!(locations.instructions("command-code"), home.join(".commandcode/AGENTS.md"));
        let custom = home.join("custom");
        locations.codex_home = Some(custom.clone());
        assert_eq!(locations.instructions("codex"), custom.join("AGENTS.md"));
    }
}
