# Implementation and verification status

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
- Skills and MCP definitions are not synced yet.
