# Repository guidance

- Follow the [product scope](ai-harness-sync-docs/01_PRODUCT_SCOPE.md), [implementation sequence](ai-harness-sync-docs/05_IMPLEMENTATION_PLAN.md), and current [coverage and verification record](ai-harness-sync-docs/08_COMPLETION_STATUS.md).
- The desktop app is in `apps/desktop`. Run `npm install`, `npm run dev`, `npm run build`, or `npm run tauri dev` there.
- The [system design](ai-harness-sync-docs/02_SYSTEM_DESIGN.md) and [security model](ai-harness-sync-docs/04_SECURITY_AND_SECRETS.md) own architecture and security facts. Update them when those boundaries change.
- The React UI must not gain unrestricted filesystem or command execution access. Add narrow Rust commands when local operations begin.
- Show only real device and sync state. No sample data should appear as live results.
- Only the five global instruction files, the chosen sync file and app data may be written. Warn before replacing text the user wrote outside the app.
- Preserve unrelated work and test in proportion to the change.
