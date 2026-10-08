# System design

## Components

Tauri 2 desktop app. The React UI calls narrow Rust commands and has no filesystem or shell access. Rust modules:

| Module | Responsibility |
| --- | --- |
| `tools.rs` | The five supported tools, their global instruction and skills paths (with `CODEX_HOME`, `CLAUDE_CONFIG_DIR`, `XDG_CONFIG_HOME` overrides) and installation detection |
| `document.rs` | Sync file format, validation, create, revision-checked save, Drive conflict-copy detection |
| `skills.rs` | Reading, validating, hashing, writing and removing skill folders; listing skills found only on this computer |
| `apply.rs` | Rendering presets per tool, comparing and replacing tool files, capturing this computer's files as presets |
| `state.rs` | Per-computer state in app data: sync-file path, computer name, last applied hashes per tool and per tool skill, which skill copies it wrote, last applied file hash |
| `fsutil.rs` | Size-limited reads, atomic writes, hashing, timestamps |
| `lib.rs` | Commands, background worker, tray icon and status popover, autostart, notifications |

## Sync file

```json
{
  "version": 5,
  "revision": 12,
  "updatedAt": "2026-10-06T10:00:00Z",
  "updatedBy": "DESKTOP",
  "shared": "…",
  "tools": {
    "antigravity": { "mode": "custom", "text": "…" },
    "claude": { "mode": "append", "text": "…" },
    "command-code": { "mode": "off", "text": "" },
    "codex": { "mode": "shared", "text": "", "skillMode": "off" },
    "opencode": { "mode": "shared", "text": "", "skillMode": "append", "skillExtras": ["research"] }
  },
  "sharedSkills": ["review"],
  "skills": {
    "research": { "files": { "SKILL.md": "…" } },
    "review": { "files": { "SKILL.md": "…", "scripts/check.sh": "…" } }
  }
}
```

Tools absent from `tools` use the shared preset. `append` renders the shared text, a blank line, then the tool text. Rendered text always ends with a newline. Tool files are limited to 1 MB and the sync file to 8 MB.

Older files are migrated on read and written as version 5 on the next save. Version 3 has no skills. In version 4 every skill was shared and a tool's `"skills": false` becomes `"skillMode": "off"`. Older apps refuse version 5 files, so every computer needs this version once the file is saved.

## Skills

Each tool has one global skills folder: `$CODEX_HOME/skills`, `$CLAUDE_CONFIG_DIR/skills` (default `~/.claude/skills`), `$XDG_CONFIG_HOME/opencode/skills`, `~/.gemini/antigravity/skills` and `~/.commandcode/skills`. A skill is a subfolder with a `SKILL.md`. The sync file stores each skill's files as UTF-8 text, ignoring hidden entries, `__pycache__` folders and `.pyc` files, keyed by `/`-separated relative path. Names are limited to letters, digits, `-`, `_` and `.`; paths may not contain empty, `.`-prefixed (including `..`), `\` or `:` components. A skill holds at most 200 files of up to 1 MB each.

Skills mirror instructions. `sharedSkills` is the shared set. Each tool's `skillMode` picks what its folder gets: `shared` (the shared set, the default), `append` (the shared set plus `skillExtras`), `custom` (only `skillExtras`) or `off` (unmanaged). `skills` is the library holding the content of every referenced skill.

Skill changes are part of the editor draft and are saved with `save_presets`. The draft's `skillSources` maps each skill to upload to the tool whose folder supplies it: a skill new to the file, or "Use this version" for a copy changed outside the app. `lib.rs` reads those folders, then `document::save` replaces library entries with the uploads, refuses a reference with no content and drops entries nothing references. The Status sent to the UI omits skill contents and carries a report: the library, plus each installed tool's folder with every local or wanted skill and its state.

For every installed tool whose skill mode is not `off`, applying compares each wanted skill's hash with the local folder. A differing folder is made identical: files are written atomically and extra files removed, leaving ignored ones. A copy that changed since the last apply is reported as edited outside. When a skill leaves a tool's set, the copy is deleted only if Harness Sync created or replaced it (`writtenSkills`) and its hash still matches the last apply. Originals that were shared from that folder, and changed copies, are kept and no longer tracked. Skills outside a tool's set are listed but never written. Symlinked skill folders or files are refused.

The Skills page lists each installed tool's folder. Dragging a skill into **Shared skills** (HTML drag and drop, which needs `dragDropEnabled: false` on the window) or choosing **Share** adds it to the shared set. Each tool's page has a second mode picker for skills and a checklist for its extras.

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
