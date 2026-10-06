# Implementation plan

The 2026-10-06 scope replaces the profile/publisher/review design with shared instruction presets in one Drive file. See [product scope](01_PRODUCT_SCOPE.md).

## Sequence

1. **Back up and trim.** Archive the previous source outside the repository. Delete GitHub sync, thread browsing and transfer, profiles, project aliases, portable artifact projections, config/MCP editors, usage readers, encrypted export and SQLite storage. Remove dependencies only those modules used.
2. **Tool registry (`tools.rs`).** Five tools with fixed global instruction paths and detection:

   | Tool | Global instructions | Override |
   | --- | --- | --- |
   | Codex | `~/.codex/AGENTS.md` | `CODEX_HOME` |
   | Claude Code | `~/.claude/CLAUDE.md` | `CLAUDE_CONFIG_DIR` |
   | OpenCode | `~/.config/opencode/AGENTS.md` | `XDG_CONFIG_HOME` |
   | Antigravity | `~/.gemini/GEMINI.md` | none |
   | Command Code | `~/.commandcode/AGENTS.md` | none |

   A tool counts as installed when its configuration directory exists or its command is on `PATH`.
3. **Sync document (`document.rs`).** Version 3 JSON: `revision`, `updatedAt`, `updatedBy`, `shared` text and a per-tool `{ mode, text }` map. Modes: `shared`, `append` (shared text followed by the tool's extra text), `custom` (tool text only) and `off` (not managed). Validate size (1 MB), version and tool IDs. Write through a temporary sibling and rename. Saving requires the revision the editor loaded; a newer file on disk rejects the save.
4. **Apply (`apply.rs`).** Render each managed tool's text. When an installed tool's file differs, replace it atomically (no backups, by the user's choice on 2026-10-06). Skip symlinks and non-regular files. Record the applied hash per tool so later outside edits can be reported.
5. **Local state (`state.rs`).** A small JSON file in app data: selected sync-file path, device name, last applied hashes, last seen file hash.
6. **Background worker.** Apply at launch, then poll the sync file every 5 seconds. A changed file is applied automatically. Tool files are not rewritten between file changes; outside edits are only reported.
7. **Commands (`lib.rs`).** `get_status`, `create_sync_file` (native save dialog, optionally seeded from one tool's current instructions), `open_sync_file` (native open dialog; stages the file), `join_sync_file` (use the file's presets or this computer's), `cancel_join`, `use_this_computer`, `disconnect`, `save_presets`, `apply_now`, `set_device_name`, `open_main`, autostart get/set. Left-click on the tray icon toggles a status popover window. Emit `status-changed` after background work. Keep tray, single instance and autostart.
8. **UI.** Chosen design: variant A's sidebar combined with variant C's per-tool view, plus first run, settings and the tray popover. See the UI specification.
9. **Documentation and tests.** Rewrite scope, design, security and status docs. Remove obsolete smoke tests and docs.

## Acceptance checks

- Saving the shared preset updates every tool in `shared` or `append` mode on this computer and bumps the file revision.
- A second computer that opens the same file writes the same rendered text, including a tool-specific preset instead of the shared one.
- A tool in `off` mode and tools that are not installed are never written.
- A save based on an older revision is rejected without changing the file.
- Malformed, oversized or future-version files change nothing locally.
- A Drive conflict copy beside the sync file is reported.
- Symlinked instruction files are skipped with an explanation.
- No GitHub, thread or project-file code remains reachable.

## Deferred

Skills and MCP definitions, signed updates and a live two-computer Drive test.
