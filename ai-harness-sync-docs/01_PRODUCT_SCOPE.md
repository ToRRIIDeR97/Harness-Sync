# Product scope

## Objective

Keep global AI instructions identical across Codex, Claude Code, OpenCode, Antigravity and Command Code, on every computer the user works on, by editing them once.

## Included

- One **shared preset**: the global instructions most tools use.
- Global skills, chosen like instructions: one shared skill set, built by dragging skills from any tool's folder on any computer. Each tool can use the shared set, the shared set plus extra skills, only its own skills, or leave its skills folder unmanaged.
- Per-tool choice: use the shared preset, the shared preset plus tool-specific additions, the tool's own preset, or leave the tool unmanaged.
- One `harness-sync.json` file in a Google Drive for desktop folder holding all presets. Any connected computer can edit it.
- Automatic application at launch and whenever the file changes. Replaced tool files are not backed up, by the user's choice.
- Making any computer the main one: when joining, or later from Settings, its instruction files replace the presets every computer follows.
- Reporting of tool files edited outside the app, stale saves and Google Drive conflict copies.
- A tray status popover, tray operation and optional start at sign-in.

## Excluded

- Project instruction files (repository `AGENTS.md`, `CLAUDE.md`). They are project-specific.
- Project skills, binary skill files, prompts, agents, MCP definitions and harness settings. Deferred.
- Skills that were never added to the sync file. They are listed, never changed.
- Threads, credentials, caches and harness databases.
- GitHub or any other transport. Google Drive for desktop moves the file.
- Copying outside edits back into presets. Outside edits are reported, then replaced on the next sync.

## Acceptance

Editing the shared preset on one computer updates every tool that uses it, on that computer and on any other computer running the app with the same file. A tool-specific preset reaches the same tool on every computer instead of the shared preset. A shared skill appears in the skills folder of every tool using the shared set on every computer. Unsharing deletes only unchanged copies the app wrote, never the original. Stale saves are refused. Invalid files change nothing. Status never claims cloud delivery.
