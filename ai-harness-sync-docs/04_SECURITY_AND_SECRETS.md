# Security and secrets

## Local boundary

React has no filesystem, shell or network access. Rust writes only the five fixed global instruction paths, folders named after synced skills inside the five fixed global skills folders, the user-chosen sync file and the app data directory.

## Sync file trust

The sync file is plaintext protected by Google Drive permissions. Anyone who can edit it controls the instructions and skills every connected computer's AI tools receive. Skills can include scripts that an AI tool may run, so a synced skill is as trusted as code you install yourself. Keep it in a private folder. The `updatedBy` name is informational, not authentication.

Instruction and skill text is not screened for secrets. Do not put credentials in presets or synced skills.

A shared skill edited in any tool's skills folder on a connected computer is uploaded to the sync file automatically when it does not conflict. Anything that can write to those folders, including an AI tool, can therefore change that skill on every connected computer. The version it replaces is kept in `backups/library/<skill>` in app data on the uploading computer.

## Writing tool files

Replaced instruction files are not backed up. A skill copy is saved under `backups/<tool>/<skill>` in app data before it is replaced, unless it is byte-identical to what Harness Sync last applied; if that backup cannot be written the copy is left alone. The five newest backups per copy are kept, and they are plain files that are never synced. Automatic syncs never replace a file or skill copy edited outside the app; the app warns about it, can copy that file's text into the unsaved draft so it is kept, and Sync now or saving replaces it. A skill edit that is safe to upload is uploaded by Sync now or saving, not replaced. Writes are atomic. Symlinked or oversized files, unknown tools and malformed or newer-version sync files are refused. An empty preset never blanks a file.

Skill names and file paths from the sync file are validated before any write so they cannot leave the tool's skills folder (no `..`, absolute, hidden or drive-prefixed parts). Replacing a synced skill removes extra files in that skill's folder only; hidden entries and Python caches are never read, synced or removed. When a skill leaves a tool's set, a copy is deleted only if Harness Sync created or replaced it and it is byte-identical to what was last applied. The folder a skill was shared from is never deleted. Skills outside a tool's set are never written or deleted. The UI never sends file paths or contents. It names skills and tools, and Rust reads only `<tool skills folder>/<valid skill name>`.

## Remaining validation

Live two-computer Drive delivery and native dialogs on macOS and Linux are untested. Signed distribution is deferred.
