# System design

## Components

Tauri 2 desktop app. The React UI calls narrow Rust commands and has no filesystem or shell access. Rust modules:

| Module | Responsibility |
| --- | --- |
| `tools.rs` | The five supported tools, their global instruction and skills paths (with `CODEX_HOME`, `CLAUDE_CONFIG_DIR`, `XDG_CONFIG_HOME` overrides) and installation detection |
| `document.rs` | Sync file format, validation, create, revision-checked save, Drive conflict-copy detection |
| `skills.rs` | Reading, validating, hashing, writing and removing skill folders; listing skills found only on this computer |
| `apply.rs` | Rendering presets per tool, comparing and replacing tool files, capturing this computer's files as presets |
| `state.rs` | Per-computer state in app data: sync-file path, computer name, last applied hashes per tool and per tool skill, last applied file hash |
| `fsutil.rs` | Size-limited reads, atomic writes, hashing, timestamps |
| `lib.rs` | Commands, background worker, tray icon and status popover, autostart, notifications |

## Sync file

```json
{
  "version": 4,
  "revision": 12,
  "updatedAt": "2026-10-06T10:00:00Z",
  "updatedBy": "DESKTOP",
  "shared": "…",
  "tools": {
    "antigravity": { "mode": "custom", "text": "…" },
    "claude": { "mode": "append", "text": "…" },
    "command-code": { "mode": "off", "text": "" },
    "codex": { "mode": "shared", "text": "", "skills": false }
  },
  "skills": {
    "review": { "files": { "SKILL.md": "…", "scripts/check.sh": "…" } }
  }
}
```

Tools absent from `tools` use the shared preset. `append` renders the shared text, a blank line, then the tool text. Rendered text always ends with a newline. Tool files are limited to 1 MB and the sync file to 8 MB.

Version 3 files are read as version 4 with no skills; the next save writes version 4. Older apps refuse version 4 files, so every computer needs this version before skills are added.

## Skills

Each tool has one global skills folder: `$CODEX_HOME/skills`, `$CLAUDE_CONFIG_DIR/skills` (default `~/.claude/skills`), `$XDG_CONFIG_HOME/opencode/skills`, `~/.gemini/antigravity/skills` and `~/.commandcode/skills`. A skill is a subfolder with a `SKILL.md`. The sync file stores each skill's non-hidden files as UTF-8 text keyed by `/`-separated relative path. Names are limited to letters, digits, `-`, `_` and `.`; paths may not contain empty, `.`-prefixed (including `..`), `\` or `:` components. A skill holds at most 200 files of up to 1 MB each.

`add_skill` reads a folder from one tool and stores it as the next revision, replacing any synced skill of the same name. `remove_skill` deletes it from the file. Both are revision-checked like `save_presets`.

For every installed tool whose preset has `skills` on (the default), applying compares each synced skill's hash with the local folder. A differing folder is made identical: files are written atomically and extra non-hidden files removed. A copy that changed since the last apply is reported as edited outside. When a skill leaves the sync file, a copy whose hash still matches the last apply is deleted; a changed copy is kept and no longer tracked. Skills that were never synced are listed as "only here" and never written. Symlinked skill folders or files are refused.

## Saving and applying

The editor remembers the revision it loaded. `save_presets` rereads the file and refuses when the revision changed, then writes revision + 1 through a temporary sibling and rename, then applies locally.

Applying compares each installed, managed tool's file with its rendered preset. A differing file is replaced atomically. No backup is kept. Empty presets, symlinks and non-regular files are never written.

## Making a computer the main one

Opening an existing file only stages it. The user then joins with either the file's presets (this computer's files are replaced) or this computer's files (`capture`): the chosen tool's file becomes the shared preset; other installed tools become Shared when identical, Shared + extra when their file starts with the shared text, and Own otherwise. Missing, empty or Off tools keep the file's setting. The result is saved as a new revision, so every other computer follows. Settings offers the same capture later, guarded by the loaded revision.

## Background behavior

A worker applies once at launch, then checks the sync file every 5 seconds and applies when its bytes change. Tool files are not rewritten between file changes; differences are reported as outside edits until the next change or Sync now. A mutex serializes the worker with all commands. A notification reports presets applied from another computer.

Left-clicking the tray icon toggles a small undecorated `tray` window (`index.html#tray`) beside the icon; it hides when it loses focus. Right-click keeps the Open and Quit menu.

Debug builds accept `HARNESS_SYNC_SANDBOX=<folder>`: tools resolve under `<folder>/home` (ignoring `CODEX_HOME` and similar) and app data under `<folder>/app-data`, so testing never touches real tools. Release builds ignore it.

Google Drive for desktop owns sign-in and transport. The app cannot confirm upload or download.
