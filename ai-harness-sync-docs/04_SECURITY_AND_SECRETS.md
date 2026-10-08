# Security and secrets

## Local boundary

React has no filesystem, shell or network access. Rust writes only the five fixed global instruction paths, folders named after synced skills inside the five fixed global skills folders, the user-chosen sync file and the app data directory.

## Sync file trust

The sync file is plaintext protected by Google Drive permissions. Anyone who can edit it controls the instructions and skills every connected computer's AI tools receive. Skills can include scripts that an AI tool may run, so a synced skill is as trusted as code you install yourself. Keep it in a private folder. The `updatedBy` name is informational, not authentication.

Instruction and skill text is not screened for secrets. Do not put credentials in presets or synced skills.

## Writing tool files

Replaced files are not backed up, so an outside edit is lost on the next sync; the app warns when a file was edited outside it. Writes are atomic. Symlinked or oversized files, unknown tools and malformed or newer-version sync files are refused. An empty preset never blanks a file.

Skill names and file paths from the sync file are validated before any write so they cannot leave the tool's skills folder (no `..`, absolute, hidden or drive-prefixed parts). Replacing a synced skill removes extra files in that skill's folder only; hidden entries and Python caches are never read, synced or removed. When a skill leaves a tool's set, a copy is deleted only if Harness Sync created or replaced it and it is byte-identical to what was last applied. The folder a skill was shared from is never deleted. Skills outside a tool's set are never written or deleted. The UI never sends file paths or contents. It names skills and tools, and Rust reads only `<tool skills folder>/<valid skill name>`.

## Remaining validation

Live two-computer Drive delivery and native dialogs on macOS and Linux are untested. Signed distribution is deferred.
