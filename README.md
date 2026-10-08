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

Global skills sync the same way. The **Skills** page lists the skills in each tool's folder. Drag one into **Shared skills**, or choose **Share**, and every tool using the shared set gets it. On each tool's page, choose what that tool's skills folder gets: Shared, Shared + extra, Own or Off. A skill is a folder with a `SKILL.md` plus text files.

| Tool | Skills folder |
| --- | --- |
| Codex | `~/.codex/skills/` |
| Claude Code | `~/.claude/skills/` |
| OpenCode | `~/.config/opencode/skills/` |
| Antigravity | `~/.gemini/antigravity/skills/` |
| Command Code | `~/.commandcode/skills/` |

Skills you don't share are never touched. Unsharing a skill removes the copies Harness Sync made, never the original. Project instruction files, MCP definitions and settings are not synced.

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
