mod apply;
mod document;
mod fsutil;
mod skills;
mod state;
mod tools;

use apply::{ToolState, ToolStatus};
use document::{Presets, Skill, SyncDocument, ToolPreset};
use serde::Serialize;
use state::LocalState;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, Rect, State, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt as AutostartManagerExt};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_notification::NotificationExt;
use tools::Locations;

const CHECK_INTERVAL: Duration = Duration::from_secs(5);
const POPOVER_WIDTH: f64 = 340.0;
const POPOVER_HEIGHT: f64 = 460.0;

/// When the popover last hid itself after losing focus, so the tray click that caused it does not reopen it.
static POPOVER_HIDDEN_AT: Mutex<Option<Instant>> = Mutex::new(None);

struct App {
    locations: Locations,
    state_path: PathBuf,
    /// Serializes every read-modify-write of local state, tool files and the sync file.
    gate: Mutex<Runtime>,
}

#[derive(Default)]
struct Runtime {
    last_error: Option<String>,
    last_checked: Option<String>,
    /// A sync file the user picked but has not yet decided how to join.
    pending: Option<PathBuf>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PendingFile {
    name: String,
    updated_by: String,
    updated_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SyncFile {
    path: String,
    name: String,
    conflict_copies: Vec<String>,
}

/// The sync file without skill contents, which the UI does not need.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DocumentView {
    revision: u64,
    updated_at: String,
    updated_by: String,
    shared: String,
    tools: BTreeMap<String, ToolPreset>,
    shared_skills: Vec<String>,
}

impl From<SyncDocument> for DocumentView {
    fn from(document: SyncDocument) -> Self {
        Self {
            revision: document.revision,
            updated_at: document.updated_at,
            updated_by: document.updated_by,
            shared: document.shared,
            tools: document.tools,
            shared_skills: document.shared_skills,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Status {
    device_name: String,
    sync_file: Option<SyncFile>,
    document: Option<DocumentView>,
    error: Option<String>,
    last_checked: Option<String>,
    pending: Option<PendingFile>,
    tools: Vec<ToolStatus>,
    skills: skills::SkillsReport,
}

impl App {
    /// Reads the sync file and compares or applies it. Caller holds the gate.
    fn refresh(&self, runtime: &mut Runtime, write: bool) -> Result<Status, String> {
        let mut local = LocalState::load(&self.state_path);
        let loaded = local.sync_file.as_deref().map(document::load);
        let document = match &loaded {
            Some(Ok((document, _))) => Some(document),
            _ => None,
        };
        runtime.last_error = match &loaded {
            Some(Err(error)) => Some(error.clone()),
            _ => None,
        };
        let tools = apply::run(document, &self.locations, &mut local, write);
        let skills = skills::run(document, &self.locations, &mut local, write);
        if write {
            if let Some(Ok((_, hash))) = &loaded {
                local.last_file_hash = Some(hash.clone());
            }
        }
        local.save(&self.state_path)?;
        runtime.last_checked = Some(fsutil::now_rfc3339());
        Ok(Status {
            device_name: local.device(),
            sync_file: local.sync_file.as_ref().map(|path| SyncFile {
                path: path.to_string_lossy().into_owned(),
                name: path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                conflict_copies: document::conflict_copies(path),
            }),
            document: loaded.and_then(Result::ok).map(|(document, _)| document.into()),
            error: runtime.last_error.clone(),
            last_checked: runtime.last_checked.clone(),
            pending: runtime.pending.as_deref().and_then(|path| {
                let (document, _) = document::load(path).ok()?;
                Some(PendingFile {
                    name: path.file_name()?.to_string_lossy().into_owned(),
                    updated_by: document.updated_by,
                    updated_at: document.updated_at,
                })
            }),
            tools,
            skills,
        })
    }

    /// Reads the skills the editor chose to upload from this computer's tool folders.
    fn read_skill_sources(&self, presets: &Presets) -> Result<BTreeMap<String, Skill>, String> {
        presets
            .skill_sources
            .iter()
            .map(|(name, tool)| {
                tools::find(tool).ok_or("Unknown tool")?;
                if !skills::valid_name(name) {
                    return Err(format!("\"{name}\" is not a valid skill name"));
                }
                let dir = self.locations.skills_dir(tool).join(name);
                let skill = skills::read(&dir)
                    .map_err(|error| format!("{} {error}", dir.display()))?
                    .ok_or_else(|| format!("{} was not found", dir.display()))?;
                skills::validate(name, &skill)?;
                Ok((name.clone(), skill))
            })
            .collect()
    }

    fn with_gate<T>(&self, work: impl FnOnce(&mut Runtime) -> Result<T, String>) -> Result<T, String> {
        let mut runtime = self.gate.lock().map_err(|_| "Harness Sync is shutting down")?;
        work(&mut runtime)
    }

    fn connect(&self, runtime: &mut Runtime, path: PathBuf) -> Result<Status, String> {
        let mut local = LocalState::load(&self.state_path);
        local.sync_file = Some(path);
        local.last_file_hash = None;
        local.save(&self.state_path)?;
        self.refresh(runtime, true)
    }

    /// Replaces the presets in `path` with this computer's instruction files.
    fn use_this_computer(&self, path: &std::path::Path, seed: &str, expected_revision: Option<u64>) -> Result<(), String> {
        let (current, _) = document::load(path)?;
        let presets = apply::capture(&current, &self.locations, seed)?;
        let device = LocalState::load(&self.state_path).device();
        document::save(path, expected_revision.unwrap_or(current.revision), presets, BTreeMap::new(), &device)?;
        Ok(())
    }
}

/// Polls the sync file and applies it whenever its bytes change.
fn background(app: AppHandle) {
    std::thread::spawn(move || {
        let mut first = true;
        loop {
            let state = app.state::<App>();
            let result = state.with_gate(|runtime| {
                let local = LocalState::load(&state.state_path);
                let Some(path) = local.sync_file.clone() else {
                    return Ok(None);
                };
                let changed = match document::load(&path) {
                    Ok((_, hash)) => first || local.last_file_hash.as_deref() != Some(&hash),
                    Err(error) => runtime.last_error.as_deref() != Some(&error),
                };
                if !changed {
                    return Ok(None);
                }
                state.refresh(runtime, true).map(Some)
            });
            first = false;
            if let Ok(Some(status)) = result {
                notify_remote_update(&app, &status);
                let _ = app.emit("status-changed", ());
            }
            std::thread::sleep(CHECK_INTERVAL);
        }
    });
}

fn notify_remote_update(app: &AppHandle, status: &Status) {
    let Some(document) = &status.document else {
        return;
    };
    let updated = status.tools.iter().filter(|tool| tool.state == ToolState::Updated).count();
    let skills = status
        .skills
        .tools
        .iter()
        .flat_map(|tool| &tool.skills)
        .filter(|skill| skill.state == skills::SkillState::Updated)
        .map(|skill| skill.name.as_str())
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    if updated + skills == 0 || document.updated_by == status.device_name {
        return;
    }
    let plural = |count: usize, word: &str| format!("{count} {word}{}", if count == 1 { "" } else { "s" });
    let applied = match (updated, skills) {
        (0, _) => plural(skills, "skill"),
        (_, 0) => plural(updated, "tool"),
        _ => format!("{} and {}", plural(updated, "tool"), plural(skills, "skill")),
    };
    let _ = app
        .notification()
        .builder()
        .title("Instructions updated")
        .body(format!("Applied presets from {} to {applied}.", document.updated_by))
        .show();
}

#[tauri::command]
fn get_status(app: State<'_, App>) -> Result<Status, String> {
    app.with_gate(|runtime| app.refresh(runtime, false))
}

#[tauri::command]
fn apply_now(app: State<'_, App>) -> Result<Status, String> {
    app.with_gate(|runtime| app.refresh(runtime, true))
}

#[tauri::command]
fn read_tool_instructions(app: State<'_, App>, tool: String) -> Result<String, String> {
    tools::find(&tool).ok_or("Unknown tool")?;
    let path = app.locations.instructions(&tool);
    Ok(fsutil::read_text(&path)
        .map_err(|error| format!("{} {error}", path.display()))?
        .unwrap_or_default())
}

#[tauri::command]
async fn create_sync_file(
    handle: AppHandle,
    seed_tool: Option<String>,
) -> Result<Option<Status>, String> {
    let picker = handle.clone();
    let chosen = tauri::async_runtime::spawn_blocking(move || {
        picker
            .dialog()
            .file()
            .set_title("Save the sync file in a Google Drive folder")
            .set_file_name("harness-sync.json")
            .add_filter("Harness Sync file", &["json"])
            .blocking_save_file()
    })
    .await
    .map_err(|error| error.to_string())?;
    let Some(chosen) = chosen else {
        return Ok(None);
    };
    let path = chosen.into_path().map_err(|error| error.to_string())?;
    let app = handle.state::<App>();
    let shared = match seed_tool {
        Some(tool) => read_tool_instructions(app.clone(), tool)?,
        None => String::new(),
    };
    app.with_gate(|runtime| {
        let device = LocalState::load(&app.state_path).device();
        document::create(&path, shared, &device)?;
        app.connect(runtime, path).map(Some)
    })
}

#[tauri::command]
async fn open_sync_file(handle: AppHandle) -> Result<Option<Status>, String> {
    let picker = handle.clone();
    let chosen = tauri::async_runtime::spawn_blocking(move || {
        picker
            .dialog()
            .file()
            .set_title("Open your Harness Sync file")
            .add_filter("Harness Sync file", &["json"])
            .blocking_pick_file()
    })
    .await
    .map_err(|error| error.to_string())?;
    let Some(chosen) = chosen else {
        return Ok(None);
    };
    let path = chosen.into_path().map_err(|error| error.to_string())?;
    document::load(&path)?;
    let app = handle.state::<App>();
    app.with_gate(|runtime| {
        runtime.pending = Some(path);
        app.refresh(runtime, false).map(Some)
    })
}

/// Joins the picked sync file. With `seed_tool`, this computer's instructions replace the file's
/// presets first; without it, the file's presets replace this computer's instructions.
#[tauri::command]
fn join_sync_file(app: State<'_, App>, seed_tool: Option<String>) -> Result<Status, String> {
    app.with_gate(|runtime| {
        let path = runtime.pending.clone().ok_or("Choose a sync file first")?;
        if let Some(seed) = seed_tool {
            app.use_this_computer(&path, &seed, None)?;
        }
        runtime.pending = None;
        app.connect(runtime, path)
    })
}

#[tauri::command]
fn cancel_join(app: State<'_, App>) -> Result<Status, String> {
    app.with_gate(|runtime| {
        runtime.pending = None;
        app.refresh(runtime, false)
    })
}

/// Makes this computer's instruction files the presets every computer follows.
#[tauri::command]
fn use_this_computer(app: State<'_, App>, seed_tool: String, expected_revision: u64) -> Result<Status, String> {
    app.with_gate(|runtime| {
        let path = LocalState::load(&app.state_path).sync_file.ok_or("Connect a sync file first")?;
        app.use_this_computer(&path, &seed_tool, Some(expected_revision))?;
        app.refresh(runtime, true)
    })
}

#[tauri::command]
fn disconnect(app: State<'_, App>) -> Result<Status, String> {
    app.with_gate(|runtime| {
        let mut local = LocalState::load(&app.state_path);
        local.sync_file = None;
        local.last_file_hash = None;
        local.save(&app.state_path)?;
        app.refresh(runtime, false)
    })
}

#[tauri::command]
fn save_presets(app: State<'_, App>, presets: Presets, expected_revision: u64) -> Result<Status, String> {
    app.with_gate(|runtime| {
        let local = LocalState::load(&app.state_path);
        let path = local.sync_file.clone().ok_or("Connect a sync file first")?;
        let uploads = app.read_skill_sources(&presets)?;
        document::save(&path, expected_revision, presets, uploads, &local.device())?;
        app.refresh(runtime, true)
    })
}

#[tauri::command]
fn set_device_name(app: State<'_, App>, name: String) -> Result<Status, String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 64 {
        return Err("Use a name between 1 and 64 characters".into());
    }
    app.with_gate(|runtime| {
        let mut local = LocalState::load(&app.state_path);
        local.device_name = Some(name.to_owned());
        local.save(&app.state_path)?;
        app.refresh(runtime, false)
    })
}

#[tauri::command]
fn open_main(app: AppHandle) {
    hide_popover(&app);
    show_main(&app);
}

#[tauri::command]
fn get_autostart(app: AppHandle) -> Result<bool, String> {
    app.autolaunch().is_enabled().map_err(|error| error.to_string())
}

#[tauri::command]
fn set_autostart(app: AppHandle, enabled: bool) -> Result<bool, String> {
    let manager = app.autolaunch();
    let result = if enabled { manager.enable() } else { manager.disable() };
    result.map_err(|error| error.to_string())?;
    manager.is_enabled().map_err(|error| error.to_string())
}

fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn hide_popover(app: &AppHandle) {
    if let Some(popover) = app.get_webview_window("tray") {
        let _ = popover.hide();
    }
}

/// Shows the status popover beside the tray icon, or hides it when open.
fn toggle_popover(app: &AppHandle, icon: Rect) {
    let recently_hidden = POPOVER_HIDDEN_AT
        .lock()
        .ok()
        .and_then(|at| *at)
        .is_some_and(|at| at.elapsed() < Duration::from_millis(300));
    let popover = match app.get_webview_window("tray") {
        Some(popover) if popover.is_visible().unwrap_or(false) => {
            let _ = popover.hide();
            return;
        }
        Some(_) if recently_hidden => return,
        Some(popover) => popover,
        None => match WebviewWindowBuilder::new(app, "tray", WebviewUrl::App("index.html#tray".into()))
            .title("Harness Sync")
            .inner_size(POPOVER_WIDTH, POPOVER_HEIGHT)
            .resizable(false)
            .decorations(false)
            .skip_taskbar(true)
            .always_on_top(true)
            .visible(false)
            .build()
        {
            Ok(popover) => popover,
            Err(_) => return show_main(app),
        },
    };
    let scale = popover.scale_factor().unwrap_or(1.0);
    let position = icon.position.to_physical::<f64>(scale);
    let size = icon.size.to_physical::<f64>(scale);
    let (width, height, gap) = (POPOVER_WIDTH * scale, POPOVER_HEIGHT * scale, 8.0 * scale);
    let mut x = position.x + size.width / 2.0 - width / 2.0;
    let mut y = position.y - height - gap;
    if let Ok(Some(monitor)) = app.monitor_from_point(position.x, position.y) {
        let (left, top) = (monitor.position().x as f64, monitor.position().y as f64);
        let (right, bottom) = (left + monitor.size().width as f64, top + monitor.size().height as f64);
        // Menu bar at the top (macOS, some Linux desktops): open below the icon.
        if position.y < top + (bottom - top) / 2.0 {
            y = position.y + size.height + gap;
        }
        x = x.clamp(left + gap, right - width - gap);
    }
    let _ = popover.set_position(PhysicalPosition::new(x, y));
    let _ = popover.show();
    let _ = popover.set_focus();
    let _ = popover.emit("status-changed", ());
}

fn setup_tray(app: &mut tauri::App) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open Harness Sync", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Harness Sync", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &quit])?;
    let mut tray = TrayIconBuilder::new()
        .menu(&menu)
        .tooltip("Harness Sync")
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_main(app),
            "quit" => app.exit(0),
            _ => (),
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                rect,
                ..
            } = event
            {
                toggle_popover(tray.app_handle(), rect);
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let tray_available = Arc::new(AtomicBool::new(false));
    let setup_tray_available = tray_available.clone();
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| show_main(app)))
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, None))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .setup(move |app| {
            if setup_tray(app).is_ok() {
                setup_tray_available.store(true, Ordering::Relaxed);
            }
            // Development builds can run against a fake home folder so testing never touches real tools.
            #[cfg(debug_assertions)]
            let sandbox = std::env::var_os("HARNESS_SYNC_SANDBOX").map(PathBuf::from);
            #[cfg(not(debug_assertions))]
            let sandbox: Option<PathBuf> = None;
            let (locations, data) = match sandbox {
                Some(root) => (Locations::sandboxed(root.join("home")), root.join("app-data")),
                None => (Locations::from_env(app.path().home_dir()?), app.path().app_local_data_dir()?),
            };
            app.manage(App {
                locations,
                state_path: data.join("state.json"),
                gate: Mutex::new(Runtime::default()),
            });
            background(app.handle().clone());
            Ok(())
        })
        .on_window_event(move |window, event| {
            if window.label() == "tray" {
                if let tauri::WindowEvent::Focused(false) = event {
                    let _ = window.hide();
                    if let Ok(mut at) = POPOVER_HIDDEN_AT.lock() {
                        *at = Some(Instant::now());
                    }
                }
                return;
            }
            if window.label() == "main" && tray_available.load(Ordering::Relaxed) {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_status,
            apply_now,
            read_tool_instructions,
            create_sync_file,
            open_sync_file,
            join_sync_file,
            cancel_join,
            use_this_computer,
            disconnect,
            save_presets,
            set_device_name,
            open_main,
            get_autostart,
            set_autostart,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Harness Sync");
}
