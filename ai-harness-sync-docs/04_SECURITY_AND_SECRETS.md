# Security and secrets

## Local boundary

React has no filesystem, shell or network access. Rust writes only the five fixed global instruction paths, the user-chosen sync file and the app data directory.

## Sync file trust

The sync file is plaintext protected by Google Drive permissions. Anyone who can edit it controls the instructions every connected computer's AI tools receive. Keep it in a private folder. The `updatedBy` name is informational, not authentication.

Instruction text is not screened for secrets. Do not put credentials in presets.

## Writing tool files

Replaced files are not backed up, so an outside edit is lost on the next sync; the app warns when a file was edited outside it. Writes are atomic. Symlinked or oversized files, unknown tools and malformed or newer-version sync files are refused. An empty preset never blanks a file.

## Remaining validation

Live two-computer Drive delivery and native dialogs on macOS and Linux are untested. Signed distribution is deferred.
