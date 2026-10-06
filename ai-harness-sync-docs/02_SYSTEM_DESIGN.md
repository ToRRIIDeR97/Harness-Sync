# System design

## Components

Tauri 2 desktop app. The React UI calls narrow Rust commands and has no filesystem or shell access. Rust modules:

| Module | Responsibility |
| --- | --- |
| `tools.rs` | The five supported tools, their global instruction paths (with `CODEX_HOME`, `CLAUDE_CONFIG_DIR`, `XDG_CONFIG_HOME` overrides) and installation detection |
| `document.rs` | Sync file format, validation, create, revision-checked save, Drive conflict-copy detection |
| `apply.rs` | Rendering presets per tool, comparing and replacing tool files, capturing this computer's files as presets |
| `state.rs` | Per-computer state in app data: sync-file path, computer name, last applied hashes, last applied file hash |
| `fsutil.rs` | Size-limited reads, atomic writes, hashing, timestamps |
| `lib.rs` | Commands, background worker, tray icon and status popover, autostart, notifications |

## Sync file

```json
{
  "version": 3,
  "revision": 12,
  "updatedAt": "2026-10-06T10:00:00Z",
  "updatedBy": "DESKTOP",
  "shared": "…",
  "tools": {
    "antigravity": { "mode": "custom", "text": "…" },
    "claude": { "mode": "append", "text": "…" },
    "command-code": { "mode": "off", "text": "" }
  }
}
```

Tools absent from `tools` use the shared preset. `append` renders the shared text, a blank line, then the tool text. Rendered text always ends with a newline. Files are limited to 1 MB.

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
