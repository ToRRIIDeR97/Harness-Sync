# UI specification

Chosen 2026-10-06: variant A's sidebar with variant C's per-tool view. Big controls (56px buttons, 60px rows), 16–17px body text, short labels. IBM Plex Sans and Mono are bundled. Light and dark follow the system.

## First run

"Is this your first computer?" with two large buttons:

- **Yes, start here** → "Start from which instructions?": one button per tool with instructions (showing its line count) and **Start empty**. Warn that other tools' instructions will be replaced. Then the native save dialog.
- **No, I set it up already** → the native open dialog, then "Which instructions should win?": **Use the synced ones** (this computer's are replaced) or **Use this computer's** (pick the tool with the main instructions; other computers follow). Cancel leaves everything unchanged.

Below: chips for the tools found on this computer.

## Main window

Sidebar: status card (opens Settings), **Shared preset**, one row per tool with a mode pill (Shared, Shared + extra, Own, Off, or Not installed) and an unsaved dot, **Settings** at the bottom.

- **Shared preset**: "Goes to …", full-height editor.
- **Tool**: "<Tool> gets", state line, four large mode buttons, then the composed file: a read-only "Shared" section with **Edit shared preset**, and/or an editable "<Tool> only" section. Off shows that the file is left alone. Warn when the file was edited outside the app and will be replaced.
- **Save bar**: Unsaved changes / All changes saved, **Discard**, **Save to all computers**. When another computer saved meanwhile: "<name> saved a newer version", **Load their version**, saving disabled.
- **Settings**: status with **Sync now**, conflict copies, sync file path with **Choose another** and **Disconnect**, **Make this computer the main one** (pick a tool; disabled while there are unsaved edits), this computer's name, **Start when I sign in** switch. **Choose another** shows the same "Which instructions should win?" screen.

## Status headline

| Situation | Title | Detail |
| --- | --- | --- |
| Normal | All synced | Updated (time) |
| Sync file unreadable | Can't read sync file | The error |
| A tool write failed | Needs attention | Tool names |
| Drive conflict copy | Check Google Drive | Conflict copy found |
| Edited outside the app | Changed outside the app | Tool names |
| Differences pending | Waiting to sync | Choose Sync now |

## Tray popover

Left-click the tray icon: large status icon, title and detail, logos of synced tools, **Edit instructions** and **Sync now**. Closes when it loses focus.
