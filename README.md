# Harness Sync

Edit your global AI instructions once and keep them the same in Codex, Claude Code, OpenCode, Antigravity and Command Code, on every computer.

## How it works

- A **shared preset** holds the instructions most tools use.
- Each tool can use the shared preset, the shared preset plus its own additions, its own preset, or be left unmanaged.
- All presets live in one `harness-sync.json` file in a Google Drive for desktop folder. Any computer running Harness Sync can edit it.
- When the file changes, each computer writes the presets to its tools' global instruction files. Replaced files are not backed up.

| Tool | Global instructions file |
| --- | --- |
| Codex | `~/.codex/AGENTS.md` |
| Claude Code | `~/.claude/CLAUDE.md` |
| OpenCode | `~/.config/opencode/AGENTS.md` |
| Antigravity | `~/.gemini/GEMINI.md` |
| Command Code | `~/.commandcode/AGENTS.md` |

Global skills are synced too. Each synced skill folder (a `SKILL.md` plus text files) is copied into every installed tool's skills folder unless you turn skills off for that tool:

| Tool | Skills folder |
| --- | --- |
| Codex | `~/.codex/skills/` |
| Claude Code | `~/.claude/skills/` |
| OpenCode | `~/.config/opencode/skills/` |
| Antigravity | `~/.gemini/antigravity/skills/` |
| Command Code | `~/.commandcode/skills/` |

Add a skill from the **Skills** page. Skills that are only on one computer are listed there and are never touched. Project instruction files, MCP definitions and settings are not synced.

## Set up

1. Install and sign in to Google Drive for desktop on each computer.
2. On the first computer, choose which tool's current instructions start the shared preset, then **Create sync file** and save it in a private Drive folder.
3. On each other computer, choose **No, I set it up already**, pick the same file, then **Use the synced ones**. Choose **Use this computer's** instead to make that computer the main one; you can also do this later in Settings.
4. Keep Harness Sync running (it stays in the tray). Turn on **Start Harness Sync when I sign in** if you like.

## Develop

```sh
cd apps/desktop
npm install
npm run tauri dev
```

Run `cargo test --lib` in `apps/desktop/src-tauri`. See the [documentation](ai-harness-sync-docs/00_README.md).
