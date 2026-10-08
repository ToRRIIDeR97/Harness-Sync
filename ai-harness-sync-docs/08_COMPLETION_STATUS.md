# Implementation and verification status

## 2026-10-08: ignore Python caches in skills

Skills that had run a Python script were refused with "…pyc is not UTF-8 text". `__pycache__` folders and `.pyc` files are now ignored like hidden files: not read, synced or removed. The two-computer skills test now includes both; `cargo test --lib` passes 17 tests.

## 2026-10-08: skill presets and Skills page redesign

Skills now work like instructions. The sync file is version 5, with `sharedSkills`, a per-tool `skillMode` and `skillExtras`, and a library pruned to referenced skills. Version 3 and 4 files are migrated. Skill edits are part of the editor draft and saved with `save_presets`; the `add_skill` and `remove_skill` commands are gone. Unsharing deletes only copies the app wrote. The Skills page is grouped by tool, with drag-and-drop or a Share button into Shared skills. Each tool page has a Skills mode picker and a checklist for extras. Fixed: the save bar left a 28px strip where content scrolled under it, the save buttons wrapped apart in narrow windows, descriptions were cut mid-word, and the instructions preview could collapse.

Verified:

- `cargo test --lib` passes 17 tests. New or changed: skill sets per mode, version 4 migration, uploads, refusal of shared skills with no content, library pruning, tool modes picking skills, and a two-computer share/unshare run where the original folder stays while written copies are removed.
- `cargo check` has no warnings. `npm run build` passes, including `tsc`.
- UI in the browser pane with stand-in status data (not the native window), dark mode, 1100 and 760 px wide. Checked the tool groups, badges, Share, "Use this version", dropping a skill into Shared skills (fired as DragEvents from script), the save payload (`sharedSkills`, `skillSources`, per-tool skill fields), the tool page with both mode pickers, and the save bar resting at the bottom.

Not yet verified: drag-and-drop with a real mouse in the Tauri window, a sandbox `tauri dev` run (blocked because the installed app was running and the app allows one instance), and the earlier open items below.

## 2026-10-07: skill sync

Global skills sync through the same file. The sync file is now version 4 with a `skills` map and a per-tool `skills` switch; version 3 files still load. New `skills.rs`, `add_skill` and `remove_skill` commands, a Skills page and a per-tool "Synced skills" switch.

Verified:

- `cargo test --lib` passes 14 tests, three new: name and path validation (rejects `..`, absolute, hidden, backslash and drive-prefixed paths), add/apply/outside-edit/replace/remove across two sandbox computers (unchanged copies deleted, edited copies kept, unrelated local skills untouched, `.DS_Store` ignored), and skills-off plus symlinked-skill refusal. Preset saves keep skills.
- `cargo check` has no warnings. `npm run build` passes.
- macOS run of `npm run tauri dev` against a sandbox home with a version 4 file: the launch apply wrote `review` (two files) into Claude Code's skills folder, skipped Codex (skills off) and left a local-only skill unchanged.

Not yet verified: the Skills page and switch visually, adding and removing from the UI, skill folder paths for OpenCode, Antigravity and Command Code against those tools' own documentation, and Windows.

## 2026-10-06: shared instruction presets and redesign

The app was rebuilt around one goal: global instructions shared across tools and computers through one Drive file. Removed: GitHub sync, per-device Drive channels, profiles, project aliases, artifact projections, config and MCP editors, thread browsing, usage readers, encrypted export, SQLite storage, Material UI, the old smoke tests and, at the user's request, file backups. The UI follows the chosen A + C design.

Verified:

- `cargo test --lib` passes 11 tests, including capturing this computer's files as presets: path overrides, rendering of all four modes, create/save round trip, stale-revision refusal leaving the file byte-identical, rejection of malformed, old, newer and unknown-tool files, conflict-copy detection, a two-computer apply with a tool-specific preset, outside-edit reporting without writes, empty-preset protection, symlink refusal (Unix), atomic write and date formatting.
- `cargo check` reports no warnings. `npm run build` passes.
- Manual run on Windows in dark mode against a sandbox home (`HARNESS_SYNC_SANDBOX`): first run, start from Codex, native save dialog created the file; the three other installed tools received the text and the uninstalled one was skipped. Setting Antigravity to Own and saving wrote only its own text. Editing the sync file as "Laptop" was applied within seconds and shown in the UI; Antigravity kept its preset. Settings rendered correctly.
- Second-computer takeover in a separate sandbox home: opening the same file showed "Which instructions should win?" without writing anything; choosing this computer's Codex text made it the shared preset, detected Claude Code as Shared + extra, and left the laptop's files unchanged. Running the first sandbox afterwards replaced its files with the laptop's instructions.

Not yet verified: the tray popover (shell access was not available to click the tray icon), light mode visually, the Settings version of "make this computer the main one", a live two-computer Google Drive round trip, macOS and Linux, packaging.

## Limits

- Concurrent saves on two computers within Drive's sync delay can still produce a Drive conflict copy; the app reports it but does not merge.
- Outside edits to tool files are replaced on the next sync with no backup.
- MCP definitions and project skills are not synced. Skills with binary files (images, PDFs) cannot be added.
- Every computer needs the version 5 app once the file is saved by it; older versions refuse the file.
